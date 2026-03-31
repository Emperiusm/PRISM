# PRISM Display Engine — Subsystem Design Spec

**Protocol for Remote Interactive Streaming & Multiplexing**

| Field       | Value                          |
|-------------|--------------------------------|
| Version     | 1.0                            |
| Status      | DRAFT                          |
| Date        | 2026-03-30                     |
| Authors     | Ehsan + Claude                 |
| Parent spec | 2026-03-30-prism-architecture-design.md |
| Architecture reqs | R20-R23, R31-R36, R40, R42-R46 |

This document is the complete Display Engine design for PRISM across all phases. It covers the capture-classify-encode-send pipeline, region classification, parallel encoding, degradation ladder, and all hyper-optimizations. The architecture spec defines *what* Display Engine owns; this spec defines *how*.

---

## Table of Contents

1. [Pipeline Architecture](#1-pipeline-architecture)
2. [Capture Stage](#2-capture-stage)
3. [Region Classification Stage](#3-region-classification-stage)
4. [Encode Stage](#4-encode-stage)
5. [Packetize + Send Stage](#5-packetize--send-stage)
6. [Degradation Ladder](#6-degradation-ladder)
7. [Cursor Management](#7-cursor-management)
8. [Multi-Client Display](#8-multi-client-display)
9. [Display Channel Protocol](#9-display-channel-protocol)
10. [Phase Mapping](#10-phase-mapping)
11. [File Layout](#11-file-layout)
12. [Testing Strategy](#12-testing-strategy)
13. [Optimizations Index](#13-optimizations-index)

---

## 1. Pipeline Architecture

### 1.1 Pipeline Stages

Five stages, fully pipelined. Each stage runs on dedicated pinned threads. Frames flow without backpressure — if a downstream stage is slow, upstream drops the frame.

```
Stage 1: Capture        → GPU texture + damage rects
    | (SPSC ring, per monitor)
Stage 2: Classify       → Region map (GPU compute shader)
    | (streaming: regions submitted to encode as found)
Stage 3: Encode         → Bitstream per region (parallel HW encoders)
    | (direct to packetizer — no assembly wait)
Stage 4: Packetize+Send → Self-describing PRISM packets → Transport
    | (direct)
Stage 5: Feedback       → Client quality reports → Degradation ladder
```

### 1.2 Zero-Allocation Ring Buffer (R36)

Capture→Classify handoff uses a lock-free SPSC ring buffer. Pre-allocated slots, no heap allocation during operation.

```rust
pub struct FrameRing<T> {
    buffer: Box<[MaybeUninit<T>]>,
    capacity: usize,
    write_idx: AtomicU32,
    read_idx: AtomicU32,
}

impl<T> FrameRing<T> {
    /// Try to write. Returns None if full (consumer slow — DROP the frame).
    pub fn try_push(&self, item: T) -> Option<()>;
    /// Try to read. Returns None if empty.
    pub fn try_pop(&self) -> Option<T>;
}
```

Ring capacities: Capture→Classify: 4 slots (66ms at 60fps). If full, producer drops — a stale frame is worse than a skipped frame. Degradation ladder sees the drop via metrics.

### 1.3 Work-Stealing Queue for Encoders

Classify→Encode uses `crossbeam_deque::Injector` for work-stealing. Classifier pushes N `EncodeJob`s. Encoder pool (2-3 threads) steals jobs. Natural load balancing.

### 1.4 Streaming Packetization

Encode→Send has no buffer. Each encoder worker calls the packetizer directly after encoding a region. No frame assembly, no waiting for all regions. Slices ship the instant they're encoded.

### 1.5 Thread Pinning (R34, R48)

```
Capture thread:     pinned to GPU NUMA node
Classify thread:    pinned to GPU NUMA node
Encoder threads:    pinned to GPU NUMA node (1 per HW encoder session)
Packetizer calls:   run on encoder threads (inline, no separate thread)
```

Platform-specific pinning:
- **Windows:** Query GPU NUMA via DXGI adapter. `SetThreadGroupAffinity`. Intel hybrid: `SetThreadSelectedCpuSets` for P-cores. `SetThreadPriority(THREAD_PRIORITY_TIME_CRITICAL)`.
- **Linux:** Query `/sys/bus/pci/devices/*/numa_node`. `sched_setaffinity`. `sched_setscheduler(SCHED_FIFO, 50)`.

### 1.6 Frame Types Through the Pipeline

```rust
/// Stage 1 output
pub struct CapturedFrame {
    pub texture: SharedTexture,
    pub damage_rects: Vec<Rect>,
    pub display_id: DisplayId,
    pub capture_time_us: u64,
    pub frame_seq: u32,
    pub is_input_triggered: bool,
    pub is_speculative: bool,
}

/// Stage 2 output (per region, streamed to encoder)
pub struct EncodeJob {
    pub frame_seq: u32,
    pub display_id: DisplayId,
    pub region: ClassifiedRegion,
    pub texture: SharedTexture,
    pub region_rect: Rect,
    pub target_bitrate: u64,
    pub force_keyframe: bool,
    pub quality_tier: QualityTier,
    pub expected_regions: usize,
    pub frame_meta: FrameMetadata,
}

/// Stage 3 output (per region)
pub struct EncodedRegion {
    pub rect: Rect,
    pub encoding: RegionEncoding,
    pub decoder_slot: u8,
    pub slices: Vec<EncodedSlice>,
}

pub struct EncodedSlice {
    pub slice_index: u8,
    pub total_slices: u8,
    pub data: Bytes,
}

pub enum RegionEncoding {
    Video { codec: CodecId, is_keyframe: bool },
    Lossless { format: LosslessFormat },
    DamageRect,
    Unchanged,
}

#[derive(Clone, Copy, PartialEq)]
pub enum QualityTier {
    Normal,
    Preview,  // fast encode, low bitrate, will be replaced (H11)
}

pub struct FrameMetadata {
    pub display_id: DisplayId,
    pub capture_time_us: u64,
    pub is_preview: bool,
    pub replaces_seq: Option<u32>,
    pub total_regions: u8,
}
```

---

## 2. Capture Stage

### 2.1 Platform Capture Trait

```rust
pub trait PlatformCapture: Send + 'static {
    fn start(&mut self, config: CaptureConfig) -> Result<(), CaptureError>;
    fn stop(&mut self);
    fn trigger_capture(&self);
    fn next_frame(&mut self) -> Option<CapturedFrame>;
    fn enumerate_monitors(&self) -> Vec<MonitorInfo>;
    fn create_virtual_display(&mut self, config: DisplayConfig) -> Result<DisplayId, CaptureError>;
    fn destroy_virtual_display(&mut self, id: DisplayId) -> Result<(), CaptureError>;
}

pub struct CaptureConfig {
    pub display_id: DisplayId,
    pub capture_mode: CaptureMode,
    pub cursor: CursorCapture,
}

pub enum CaptureMode {
    FullDesktop,
    Window { hwnd: u64 },
    Virtual { resolution: (u32, u32), refresh_rate: u8 },
}

pub enum CursorCapture {
    Embedded,    // cursor in frame (simple mode clients)
    Separate,    // cursor shape sent separately (client-side rendering)
    None,
}

pub struct MonitorInfo {
    pub display_id: DisplayId,
    pub name: String,
    pub resolution: (u32, u32),
    pub position: (i32, i32),
    pub scale_factor: f32,
    pub refresh_rate: u8,
    pub primary: bool,
    pub is_virtual: bool,
}
```

### 2.2 Windows Capture: Hybrid DDA + WGC

**DDA** captures the full composited desktop (actual pixels the user sees). This is the frame we encode. Always correct.

**WGC** captures individual windows for **metadata only** — which windows exist, where they are, how often they update. We don't use WGC pixels for encoding. This drives Tier 1 region classification without GPU compositing.

```rust
pub enum CaptureStrategy {
    /// Full desktop via DDA + WGC metadata for classification.
    FullDesktop {
        dda: DdaCapture,
        window_tracker: WgcWindowTracker,
    },
    /// Single window via WGC (user chose one app).
    SingleWindow {
        wgc: WgcCapture,
        hwnd: u64,
    },
    /// Virtual display via IDD + DDA.
    VirtualDisplay {
        dda: DdaCapture,
        virtual_id: DisplayId,
    },
}
```

**WGC Window Tracker:**
```rust
pub struct WgcWindowTracker {
    sessions: HashMap<u64, WgcMetadataSession>,
}

pub struct WgcMetadataSession {
    hwnd: u64,
    rect: Rect,
    frames_last_second: AtomicU32,
    last_frame_time: AtomicU64,
}

impl WgcWindowTracker {
    pub fn window_activity(&self) -> Vec<(u64, Rect, UpdateFrequency)>;
}

pub enum UpdateFrequency {
    Static,     // no changes in 5 seconds
    Low,        // 1-10 updates/sec
    High,       // >10 updates/sec
    Unknown,    // newly tracked
}
```

For **per-window capture mode**, WGC is used directly for frame pixels.

### 2.3 Zero-Copy GPU Pipeline (H1)

```
DDA/WGC → ID3D11Texture2D (GPU memory)
  → Shared via CreateSharedHandle()
  → Classifier reads via GPU compute shader
  → Encoder reads directly (zero CPU readback)
  → Encoded bitstream → pinned CPU buffer → QUIC send
```

No pixel data ever touches CPU memory. The only GPU→CPU transfers:
- Classification buffer readback: ~2KB for 4K (~5µs)
- Encoder output bitstream: variable (already compressed)
- Lossless delta blocks: ~1-5KB when using delta compression

### 2.4 GPU Texture Sharing

```rust
pub struct SharedTexture {
    handle: SharedTextureHandle,
    width: u32,
    height: u32,
    format: TextureFormat,
    fence: GpuFence,
}

pub struct GpuFence {
    fence: ID3D11Fence,
    value: AtomicU64,
}
```

Each pipeline stage has its own D3D11 device context. Textures shared via `CreateSharedHandle`. GPU fences synchronize: capture signals after write, consumers wait before read.

**Double-buffered textures** eliminate fence waits between capture and classify: capture writes texture A while classifier reads texture B. Swap each frame. Cost: one extra GPU texture (~33MB for 4K BGRA).

### 2.5 Capture-on-Damage (H3)

DDA: `AcquireNextFrame()` blocks until content changes.
WGC: `FrameArrived` event fires only on change.

Static desktop = 0 captures, 0 encodes, 0 bandwidth. Active typing = 5-10 events/sec.

**Frame pacing:** Adaptive interval matches content update rate. Terminal at 10fps → capture at 12fps (1.2x). Hard cap at target FPS (from degradation level). Prevents encoding more frames than client can decode.

### 2.6 Input-Triggered Capture (R32)

Input events trigger immediate capture (bypass damage-wait). Debounced to 125/sec (8ms minimum interval):

```rust
pub struct InputTriggerCoalescer {
    min_interval: Duration,     // 8ms
    last_trigger: AtomicU64,
    pending: AtomicBool,
}
```

At 60fps display, 125Hz trigger rate = every frame has at most one trigger. Input-triggered capture adds at most 8ms latency.

### 2.7 Speculative IDR on Scene Change (H11)

Win32 event hooks detect window focus changes:

```rust
pub struct Win32EventHooks {
    hook_handle: HWINEVENTHOOK,
    event_tx: mpsc::Sender<WindowEvent>,
}

pub enum WindowEvent {
    ForegroundChanged { hwnd: u64 },
    MinimizeStart { hwnd: u64 },
    MinimizeEnd { hwnd: u64 },
    MoveSizeEnd { hwnd: u64 },
    WindowDestroyed { hwnd: u64 },
}
```

On `ForegroundChanged` (Alt+Tab):
1. Immediate capture (bypass damage-wait)
2. Mark `is_speculative = true`, `QualityTier::Preview`
3. Encode at 50% bitrate (faster encode)
4. Send with `is_preview = true`
5. Client displays immediately
6. Next regular frame with `replaces_seq` silently replaces preview

Pixels on screen in ~13ms instead of ~31ms after Alt+Tab.

### 2.8 Damage Rect Merging

DDA returns fine-grained dirty rects. Merge adjacent rects (threshold: 64px) to reduce encode jobs:

50 small rects → 3-5 merged rects. Merge threshold matches classifier block size.

### 2.9 Multi-Monitor Capture (R40)

Each monitor gets an independent capture instance. Client requests specific monitors during negotiation. Server creates pipeline per requested monitor. Client can request downscaled resolution (4K monitor → 1080p encode).

### 2.10 Virtual Display (R24, R25)

Headless servers: detect "no monitor," create virtual display matching client's request. Windows: Indirect Display Driver (IDD). Linux: virtual KMS. macOS: CGVirtualDisplay (14+). Multiple clients → multiple virtual displays with independent resolutions.

### 2.11 macOS Capture (Phase 3)

ScreenCaptureKit → IOSurface → VideoToolbox. GPU-resident, zero-copy.

### 2.12 Linux Capture (Phase 3)

PipeWire + xdg-desktop-portal → DMA-BUF fd → VAAPI/NVENC. GPU-resident, zero-copy.

---

## 3. Region Classification Stage

### 3.1 Two-Tier Classification

**Tier 1 (Phase 1): Window-level.** Uses WGC update frequency metadata. No pixel analysis. Each window classified as single region.

**Tier 2 (Phase 2+): Sub-window.** GPU compute shader analyzes 64×64 blocks. Identifies text, video, static at block granularity.

Both produce `Vec<ClassifiedRegion>`:

```rust
pub struct ClassifiedRegion {
    pub rect: Rect,
    pub classification: RegionType,
    pub confidence: f32,
    pub decoder_slot: u8,
}

pub enum RegionType {
    Text,       // lossless encoding
    Video,      // lossy H.264/H.265 hardware encode
    Static,     // cached, damage rect updates only
    Uncertain,  // below confidence → lossless (R22)
}
```

### 3.2 Tier 1: Window-Level Classification

```rust
pub struct Tier1Classifier {
    window_tracker: Arc<WgcWindowTracker>,
    history: HashMap<u64, WindowHistory>,
}

struct WindowHistory {
    classification: RegionType,
    stable_since: Instant,
}
```

Classification mapping:
- `UpdateFrequency::Static` → `RegionType::Static`
- `UpdateFrequency::Low` → `RegionType::Text`
- `UpdateFrequency::High` → `RegionType::Video`
- `UpdateFrequency::Unknown` → `RegionType::Uncertain`

Confidence scales with classification stability: full confidence after 5s of stable classification.

Uncovered areas (desktop background) → `Static`.

Decoder slot assignment: Video → slot 0, Text/Uncertain → slot 1, Static → slot 2.

### 3.3 Tier 2: GPU Compute Shader (R42)

```rust
pub struct Tier2Classifier {
    compute_pipeline: ComputePipeline,
    previous_frame: SharedTexture,
    classification_buffer: GpuReadbackBuffer,
    block_size: u32,  // 64 pixels
    tier1: Tier1Classifier,
}

#[repr(u8)]
pub enum BlockClass {
    Unchanged = 0,
    LowMotion = 1,    // <5% pixels changed → Text/UI
    HighMotion = 2,    // >=5% pixels changed → Video
    EdgeCase = 3,      // ambiguous
}
```

GPU compute shader per frame:
1. Sample 16 pixels (4×4 grid) per 64×64 block
2. Compare with previous frame. Threshold: >4 color difference per channel
3. Classify: 0 changed = Unchanged, <5% = LowMotion, ≥5% = HighMotion

Output: one byte per block. 4K = 60×34 = 2040 blocks = 2KB readback. GPU time: ~0.1ms.

**Block merging:** Greedy maximal rectangle algorithm. Scan rows, extend rightward and downward for same-type blocks. 2040 blocks → 3-10 merged regions typically.

**Tier combination:** GPU (Tier 2) is authoritative. Tier 1 breaks ties for EdgeCase blocks using window-level hints. Anything still Uncertain → Text (lossless, R22).

**Region boundary snapping:** Video regions snapped to 16-pixel macroblock alignment. At most 15 pixels expansion per edge — negligible bandwidth.

### 3.4 Static Region Atlas Tracking (R23, R46)

```rust
pub struct StaticAtlasTracker {
    region_hashes: HashMap<RegionKey, u64>,
    static_frame_count: HashMap<RegionKey, u32>,
    cache_threshold: u32,  // 30 frames (0.5s)
}

pub enum StaticDecision {
    Unchanged,        // client uses cached texture → 0 bytes
    SendAndCache,     // send lossless + client caches
    EncodeNormally,   // not cached yet
}
```

Regions must be static for 30 frames before caching. Hash comparison detects content changes. Client-side texture atlas composites cached + fresh in single GPU pass.

### 3.5 Streaming Classification Output

Classifier submits regions to the encode queue as they're found — doesn't wait to find all regions first. First video region starts encoding while classifier finds remaining text regions. Saves ~0.3ms per frame.

---

## 4. Encode Stage

### 4.1 Split Encoder Pool

Dedicated encoder instances per encoding mode. No mode switching between frames.

```rust
pub struct EncoderPool {
    video_encoders: Vec<HwEncoder>,       // lossy, CBR/VBR, UltraLowLatency
    lossless_encoders: Vec<HwEncoder>,    // H.264/H.265 lossless mode
    video_queue: EncodeQueue,             // priority-aware
    lossless_queue: EncodeQueue,
    cpu_lossless_pool: Option<CpuEncoderPool>,  // fallback if no HW lossless
    result_tx: mpsc::Sender<(u32, usize, EncodedRegion)>,  // (frame_seq, expected_regions, region)
}
```

Session allocation strategy:
- 3 GPU sessions: 2 video + 1 lossless
- 2 GPU sessions: 1 video + 1 lossless
- 1 GPU session: 1 video (lossless falls back to CPU)

### 4.2 Priority-Aware Encode Queue

```rust
pub struct EncodeQueue {
    high: crossbeam_deque::Injector<EncodeJob>,    // video, keyframes, speculative
    normal: crossbeam_deque::Injector<EncodeJob>,  // text, damage rects
}

impl EncodeQueue {
    pub fn steal(&self) -> Option<EncodeJob> {
        // Drain high queue first, then normal
    }
}
```

Video regions and keyframes encode first. Text fills gaps between video encodes.

### 4.3 Hardware Encoder Abstraction

```rust
pub struct HwEncoder {
    backend: EncoderBackend,
    config: EncoderConfig,
    d3d_context: ID3D11DeviceContext,
    output_buffer: PinnedOutputBuffer,
    rate_control_hinter: RateControlHinter,
    recorder: Arc<DisplayRecorder>,
}

pub enum EncoderBackend {
    Nvenc(NvencEncoder),      // NVIDIA
    Amf(AmfEncoder),          // AMD
    Qsv(QsvEncoder),          // Intel
    VideoToolbox(VtEncoder),  // macOS
    Vaapi(VaapiEncoder),      // Linux
    Software(SoftwareEncoder), // always available
}

pub struct EncoderConfig {
    pub codec: CodecId,
    pub preset: EncoderPreset,
    pub bitrate_bps: u64,
    pub max_fps: u8,
    pub resolution: (u32, u32),
    pub keyframe_interval: KeyframeInterval,
    pub slice_mode: SliceMode,
    pub color_space: ColorSpace,
}
```

### 4.4 Ultra-Low-Latency Configuration (H2)

```
NVENC: tuning = ULTRA_LOW_LATENCY, lookahead = 0, bFrames = 0,
       gopLength = INFINITE, rc = CBR, minQP = 18
AMF:   usage = ULTRA_LOW_LATENCY, bFrames = 0, rc = CBR
QSV:   targetUsage = 7, lookAhead = 0, bFrames = 0
```

Savings: 33-66ms eliminated by removing lookahead. Single biggest latency win.

### 4.5 Lossless Encoding Strategy

Three-tier approach, no CPU QOI on the hot path:

```rust
fn encode_lossless(&mut self, job: &EncodeJob) -> Result<...> {
    // 1. Delta check: if <10% of blocks changed, send only changed blocks
    //    GPU compute XOR + readback only changed blocks (~1-5KB)
    if let Some(prev) = &self.previous_texture {
        let changed_pct = self.compute_change_percentage(...)?;
        if changed_pct == 0.0 { return Ok(Unchanged); }
        if changed_pct < 0.10 { return self.encode_lossless_delta(job, prev); }
    }

    // 2. Hardware lossless H.264/H.265 (no GPU readback, HW decode on client)
    if self.supports_hw_lossless() {
        return self.encode_lossless_hw(job);  // NV_ENC_PARAMS_RC_LOSSLESS
    }

    // 3. CPU QOI fallback (only if no HW lossless support)
    self.encode_lossless_cpu(job)
}
```

Eliminates 3-4ms GPU readback for text regions in the common case.

### 4.6 Slice-Level Streaming (R33)

H.264/H.265 slices are independently decodable. NVENC `sliceMode` produces 2-8 slices per frame. Each sent as separate datagram.

```
Full-frame: 3ms encode + 10ms network + 2ms decode = 15ms
4-slice:    0.75ms + 10ms + 0.5ms = 11.25ms (pipelined)
Savings: ~4ms
```

### 4.7 Dynamic Bitrate and Resolution

- **Bitrate change:** Cheap. Dynamic NVENC/AMF/QSV parameter. Applies next frame. No reinit.
- **Resolution change:** Expensive. Requires encoder reinit + IDR. Only on degradation level change.
- **Codec change:** Most expensive. Full pipeline reconfiguration. Rare.

Allocation handle provides current bitrate target. Encoder reads it before each frame (~1ns atomic load).

### 4.8 Adaptive Keyframe Interval (R45)

```rust
pub enum KeyframeInterval {
    Fixed(u32),
    Adaptive { min_frames: u32, max_frames: u32 },
    OnDemand,
}
```

Static desktop: keyframe every 30s. Active coding: every 5s. Video: every 2s. Lossy network (>2%): every 1s. Driven by degradation ladder.

### 4.9 Rate Control Warm-Start

Per-window content complexity estimates. On scene change (Alt+Tab), encoder starts with the new window's remembered QP/bitrate instead of converging over 5-10 frames.

```rust
pub struct RateControlHinter {
    window_complexity: HashMap<u64, ComplexityEstimate>,
}

pub struct ComplexityEstimate {
    avg_qp: f32,
    avg_bitrate_bps: u64,
    frame_count: u32,
}
```

### 4.10 Pinned Output Buffers

Pre-allocated pinned memory for encoder output. One per encoder, reused across frames. No allocation, no page faults during encoding.

### 4.11 Encoder Selection at Startup

Priority: NVENC → AMF → QSV → Software. Query via platform-specific APIs. GPU max concurrent sessions queried for pool sizing.

---

## 5. Packetize + Send Stage

### 5.1 Streaming Packetizer

No frame assembler. Each encoder worker calls packetizer directly after encoding a region. Slices ship the instant they're encoded.

```rust
pub struct StreamingPacketizer {
    routing_table: Arc<RoutingTable>,
    mtu_tracker: MtuTracker,
    sequence: AtomicU32,
    header_template: PacketHeaderTemplate,
    recorder: Arc<DisplayRecorder>,
}

impl StreamingPacketizer {
    /// Called by each encoder worker immediately after encoding a region.
    pub fn send_region(
        &self,
        frame_seq: u32,
        expected_regions: usize,
        region: EncodedRegion,
        frame_meta: &FrameMetadata,
    );
}
```

### 5.2 Self-Describing Slice Packets

Each slice carries full metadata — no dependency on a region map packet arriving first. 16 bytes of per-slice metadata:

```rust
// Per-slice metadata in packet payload
struct SlicePayloadHeader {
    decoder_slot: u8,
    slice_index: u8,
    total_slices: u8,
    encoding_type: u8,
    rect_x: i16,
    rect_y: i16,
    rect_w: u16,
    rect_h: u16,
    region_count: u8,       // total regions in this frame
    is_preview: u8,
    replaces_seq: u32,
    // Piggybacked cursor (first slice only)
    cursor_x: u16,          // normalized 0-65535
    cursor_y: u16,          // normalized 0-65535
    cursor_flags: u8,       // bit 0: visible, bit 1: shape_changed
}
```

Overhead: 16 bytes per slice. At 4 slices × 60fps = 3.84 KB/sec. Negligible vs megabytes of frame data.

### 5.3 Send Routing

```
Keyframe slices:  reliable stream (open_uni + write + finish, synchronous)
P-frame slices:   try_send_datagram (non-blocking)
                  WouldBlock → drop (never queue stale)
                  TooLarge → spill to uni stream
```

Keyframe sends are synchronous on the encoder thread. Infrequent (every 2-30s). Blocks for <1ms on LAN. No priority inversion.

### 5.4 Pre-Built Packet Header Template

Static portion of PRISM header pre-built. Only sequence, timestamp, flags, and payload_length patched per packet. Saves ~5 buffer operations per packet.

### 5.5 Cursor Position Piggybacking

Cursor position piggybacked on first slice of each frame instead of separate datagrams. Eliminates 60 separate cursor datagrams/sec. 4 extra bytes per first slice.

---

## 6. Degradation Ladder

### 6.1 Profile-Specific Ladders

Each connection profile has its own degradation philosophy:

**Gaming:** Drops resolution before FPS. Never enables region detection.

| Level | Bitrate | Resolution | FPS | Codec | Regions | FEC |
|-------|---------|-----------|-----|-------|---------|-----|
| 0: optimal | 80 Mbps | 4K | 120 | H.265 | off | 0% |
| 1: reduced_res | 40 Mbps | 1440p | 120 | H.265 | off | 0% |
| 2: reduced_fps | 20 Mbps | 1080p | 60 | H.264 | off | 0% |
| 3: minimum | 8 Mbps | 720p | 30 | H.264 | off | 10% |

**Coding:** Drops FPS before resolution. Always keeps region detection (text sharpness).

| Level | Bitrate | Resolution | FPS | Codec | Regions | FEC |
|-------|---------|-----------|-----|-------|---------|-----|
| 0: optimal | 20 Mbps | 4K | 60 | H.265 | on | 0% |
| 1: reduced_bw | 8 Mbps | 1440p | 60 | H.264 | on | 0% |
| 2: reduced_fps | 4 Mbps | 1080p | 30 | H.264 | on | 0% |
| 3: minimum | 1 Mbps | 720p | 15 | H.264 | on | 15% |

Additional profiles: Media, Mobile, Default (balanced).

All profiles have a final `audio_only` level (0 display).

### 6.2 Evaluation Logic

Consumes `ConnectionQuality.recommendation` from Transport. Maps recommendations to target levels. Does not re-invent quality measurement.

```
QualityRecommendation::Optimal         → try upgrade
QualityRecommendation::ReduceBitrate   → find level for target_bps
QualityRecommendation::ReduceResolution → step down
QualityRecommendation::ReduceFramerate → find lower-fps level
QualityRecommendation::EnableFec       → find level with FEC
QualityRecommendation::ConnectionUnusable → audio_only
```

### 6.3 Hysteresis

- **Downgrade:** 2 seconds sustained below threshold (fast — protect user experience)
- **Upgrade:** 10 seconds sustained above threshold (slow — avoid flapping)
- **Evaluation frequency:** Only when quality score changes (>0.05 delta) or every 500ms periodic. At stable quality: 2 evaluations/sec, not 120.

### 6.4 User Constraints (R19)

```rust
pub struct UserConstraints {
    pub min_resolution: Option<(u32, u32)>,
    pub pin_resolution: Option<(u32, u32)>,
    pub pin_fps: Option<u8>,
    pub min_fps: Option<u8>,
}
```

Degradation ladder respects these as hard limits. "Never below 720p" prevents level 3+ on Gaming profile.

### 6.5 Level Changes

```rust
pub struct LevelChange {
    pub old_level: u8,
    pub new_level: u8,
    pub bitrate_changed: bool,
    pub resolution_changed: bool,     // expensive: encoder reinit + IDR
    pub fps_changed: bool,
    pub codec_changed: bool,          // most expensive: full pipeline reconfig
    pub needs_encoder_reinit: bool,
    pub needs_idr: bool,              // any level change needs fresh keyframe
}
```

### 6.6 BandwidthAware Integration

```rust
impl BandwidthAware for DisplayEngine {
    fn bandwidth_needs(&self) -> BandwidthNeeds {
        let level = current_degradation_level();
        let complexity = current_content_complexity();
        BandwidthNeeds {
            min_bps: complexity.min_bitrate(),
            ideal_bps: level.max_bitrate_mbps * 1_000_000,
            max_bps: level.max_bitrate_mbps * 2_000_000,  // burst for keyframes
            urgency: if pending_keyframe { 1.0 } else { 0.0 },
        }
    }
}
```

### 6.7 Keyframe Hint Integration

Before encoding an IDR, Display Engine hints the arbiter. Arbiter temporarily boosts display allocation for 100ms by reducing lower-priority channels. Eliminates congestion spike from keyframe burst.

---

## 7. Cursor Management

### 7.1 Server-Side Cursor Tracking

```rust
pub struct CursorManager {
    current_shape: Option<CursorShape>,
    last_sent_hash: u64,
    position_tx: broadcast::Sender<CursorPosition>,
}

pub struct CursorShape {
    pub width: u32,
    pub height: u32,
    pub hotspot_x: u32,
    pub hotspot_y: u32,
    pub format: CursorFormat,
    pub data: Bytes,
    pub hash: u64,
}

pub struct CursorPosition {
    pub x: f32,    // normalized 0.0-1.0
    pub y: f32,
    pub visible: bool,
    pub timestamp_us: u64,
}
```

DDA provides cursor shape + position. WGC toggles cursor off in captured frames.

### 7.2 Transmission

- **Shape changes:** Sent on reliable stream (rare — only when cursor changes from arrow to text beam, etc.)
- **Position updates:** Piggybacked on display frame slices (first slice of each frame). No separate datagrams.

### 7.3 Client-Side Rendering

Client renders cursor locally at its own pointer position with zero latency. Server sends position corrections only when client prediction diverges.

---

## 8. Multi-Client Display

### 8.1 Per-Client Encoding

Capture and classification are shared (one per monitor). Encoding is per-client: each client gets its own encoder pool configured for their negotiated codec, resolution, and decoder capabilities.

```
Monitor 1 capture → classify once → regions
    ├── Client A encoder pool (H.265, 4K, 3 decoder slots)
    └── Client B encoder pool (H.264, 1080p, 1 slot = simple mode)
```

Multi-client doesn't multiply capture or classification cost — only encoding.

### 8.2 Per-Client Pipeline

```rust
pub struct DisplayEngine {
    captures: HashMap<DisplayId, MonitorCapture>,
    classifiers: HashMap<DisplayId, Box<dyn RegionClassifier>>,
    pipelines: HashMap<(DisplayId, ClientId), ClientDisplayPipeline>,
}

pub struct ClientDisplayPipeline {
    client_id: ClientId,
    display_id: DisplayId,
    encoder_pool: EncoderPool,
    degradation: DegradationLadder,
    packetizer: StreamingPacketizer,
    client_config: ClientDisplayConfig,
}

pub struct ClientDisplayConfig {
    pub codec: CodecId,
    pub max_resolution: (u32, u32),
    pub max_decoder_instances: u8,
    pub can_composite_regions: bool,
    pub profile: ConnectionProfile,
}
```

### 8.3 Simple Mode (R30)

Low-capability clients (browser, underpowered hardware): single full-frame encoder, no region detection, one decoder slot. Classification skipped entirely. Server encodes the full frame as a single video region.

---

## 9. Display Channel Protocol

### 9.1 Message Types

```rust
pub mod display_msg {
    pub const REGION_MAP: u8 = 0x01;           // optional hint, sent before slices
    pub const SLICE: u8 = 0x02;                // self-describing encoded slice
    pub const CURSOR_SHAPE: u8 = 0x03;         // cursor shape changed (reliable stream)
    pub const CURSOR_POSITION: u8 = 0x04;      // standalone position (fallback)
    pub const IDR_REQUEST: u8 = 0x05;          // client → server: request fresh keyframe
    pub const QUALITY_HINT: u8 = 0x06;         // server → client: quality level info
}
```

### 9.2 Client-Side Frame Gap Detection

```rust
pub struct FrameGapDetector {
    last_received_seq: u32,
    gap_threshold: u32,           // 1 missing frame
    recovery_cooldown: Duration,  // 1 second
    last_idr_request: Instant,
}
```

Client detects sequence gap → sends `IDR_REQUEST` via Control channel → server triggers keyframe within one RTT.

---

## 10. Phase Mapping

| Component | Phase 1 | Phase 2 | Phase 3 | Phase 4 |
|-----------|---------|---------|---------|---------|
| Capture (Windows) | DDA + WGC metadata | No change | No change | No change |
| Capture (macOS) | — | — | ScreenCaptureKit | No change |
| Capture (Linux) | — | — | PipeWire + DMA-BUF | No change |
| Virtual Display | IDD (Windows only) | No change | + Linux KMS, macOS CGVirtualDisplay | No change |
| Classification Tier 1 | Window-level (WGC metadata) | No change | No change | No change |
| Classification Tier 2 | — | GPU compute shader | No change | No change |
| Static Atlas | — | Full tracking + caching | No change | No change |
| Encoder Pool | 1 video encoder, H.264 | + lossless encoder, H.265/AV1 | No change | No change |
| Parallel Encoding | Single encoder | Multi-encoder pool | No change | No change |
| Slice Streaming | Single slice | Multi-slice (2-8) | No change | No change |
| Lossless Encoding | CPU QOI fallback | HW lossless H.264 + delta compression | No change | No change |
| Degradation Ladder | Full (profile-specific) | No change | No change | No change |
| Speculative IDR | Full (Win32 hooks) | No change | + macOS/Linux equivalents | No change |
| Multi-Client | Single client | No change | Multi-client pipelines | No change |
| Simple Mode | Full support | No change | No change | No change |
| Cursor | Shape + piggybacked position | + predictive client-side | No change | No change |

---

## 11. File Layout

```
crates/prism-display/src/
    lib.rs                      # re-exports, DisplayEngine
    engine.rs                   # DisplayEngine, per-client pipelines
    pipeline.rs                 # FramePipeline, FrameRing, pipeline orchestration

    capture/
        mod.rs                  # PlatformCapture trait, CaptureConfig, MonitorInfo
        windows.rs              # CaptureStrategy, HybridCapture, DDA, WGC
        window_tracker.rs       # WgcWindowTracker, WgcMetadataSession, UpdateFrequency
        event_hooks.rs          # Win32EventHooks, WindowEvent, speculative IDR
        input_trigger.rs        # InputTriggerCoalescer
        frame_pacer.rs          # FramePacer, adaptive interval
        virtual_display.rs      # VirtualDisplayManager
        macos.rs                # MacOSCapture (Phase 3)
        linux.rs                # LinuxCapture (Phase 3)

    gpu/
        mod.rs                  # SharedTexture, GpuFence, TextureFormat
        double_buffer.rs        # DoubleBufferedTexture
        compute.rs              # GPU compute shader dispatch (Tier 2)

    classify/
        mod.rs                  # RegionClassifier trait, ClassifiedRegion, RegionType
        tier1.rs                # Tier1Classifier (window-level)
        tier2.rs                # Tier2Classifier (GPU compute, block merge)
        atlas.rs                # StaticAtlasTracker, StaticDecision
        damage.rs               # Damage rect merging

    encode/
        mod.rs                  # EncoderPool, EncodeJob, EncodedRegion
        queue.rs                # EncodeQueue (priority-aware, work-stealing)
        hw_encoder.rs           # HwEncoder, EncoderBackend, EncoderConfig
        nvenc.rs                # NVENC implementation
        amf.rs                  # AMF implementation
        qsv.rs                  # QSV implementation
        software.rs             # Software fallback
        lossless.rs             # Lossless encoding (HW, delta, CPU QOI)
        slice.rs                # Slice splitting, NAL unit parsing
        rate_control.rs         # RateControlHinter, ComplexityEstimate
        config.rs               # EncoderPreset, KeyframeInterval, SliceMode
        pinned_buffer.rs        # PinnedOutputBuffer
        detection.rs            # detect_best_encoder, query_max_sessions

    packetize/
        mod.rs                  # StreamingPacketizer
        slice_packet.rs         # Self-describing slice packet builder
        header_template.rs      # PacketHeaderTemplate
        cursor.rs               # CursorManager, CursorShape, piggybacking

    degradation/
        mod.rs                  # DegradationLadder, DegradationLevel
        profiles.rs             # Gaming, Coding, Media, Mobile level sets
        hysteresis.rs           # Hysteresis, upgrade/downgrade timing
        constraints.rs          # UserConstraints
        bandwidth.rs            # BandwidthAware impl, BandwidthNeeds

    protocol.rs                 # Display channel message types, gap detection
```

---

## 12. Testing Strategy

| Category | What | How |
|----------|------|-----|
| Unit: FrameRing | Push/pop, full-ring drop, empty-ring None | Concurrent producer/consumer |
| Unit: Tier1 | Window frequency → classification mapping | Feed known frequencies, verify types |
| Unit: Tier2 blocks | BlockClass from known pixel data | Synthetic frames with known regions |
| Unit: Block merge | 2040 blocks → merged rectangles | Known block maps, verify rect count |
| Unit: Atlas tracker | Static detection, cache threshold, invalidation | Frame sequence with known hashes |
| Unit: Damage merge | 50 rects → 3-5 merged, threshold behavior | Known rect sets |
| Unit: Macroblock snap | Arbitrary rect → 16px-aligned | Edge cases: 0, 1, 15, 16, 17 pixels |
| Unit: Encode queue | Priority ordering, high before normal | Submit mixed, verify drain order |
| Unit: Lossless routing | Delta <10% → delta, >10% → HW lossless, no HW → CPU | Mock encoder, verify path taken |
| Unit: Slice split | NAL unit parsing, slice boundaries | Known H.264 bitstreams |
| Unit: Rate hinter | EMA convergence, warm-start accuracy | Feed known QP/bitrate sequences |
| Unit: Degradation levels | Profile-specific ladders, level properties | Verify Gaming never has region_detection |
| Unit: Hysteresis | Upgrade hold (10s), downgrade hold (2s), flapping prevention | Time-controlled evaluation |
| Unit: User constraints | Pin resolution, min FPS respected | Verify level clamping |
| Unit: Packet header | Template build, patch dynamic fields | Roundtrip verify |
| Unit: Slice metadata | Self-describing packet, all fields correct | Build + parse roundtrip |
| Unit: Gap detector | Sequence gaps trigger IDR request, cooldown respected | Feed gapped sequences |
| Unit: Input trigger | Debouncing, 8ms minimum interval | High-frequency triggers |
| Integration: Full pipeline | Capture → classify → encode → packetize on localhost | Verify frames arrive at client, decode successfully |
| Integration: Region detection | VS Code + YouTube layout, verify text/video classification | Known window positions + update rates |
| Integration: Degradation | Network degrades, verify level steps down within 2s | Inject RTT increase |
| Integration: Speculative IDR | Alt+Tab → verify preview frame in <20ms | Win32 event + timing |
| Integration: Multi-client | Two clients, different codecs, both receive | Verify per-client encoding |
| Integration: Simple mode | Low-capability client gets single full-frame | Negotiate with can_composite=false |
| Integration: Keyframe recovery | Client sends IDR_REQUEST, verify IDR within 1 RTT | Gap injection |
| Perf: Pipeline latency | Capture → send, verify <10ms on LAN | End-to-end timing |
| Perf: Encoder throughput | Frames/sec at 4K, verify ≥60fps | Benchmark encode loop |
| Perf: Classification | Tier 2 GPU time at 4K | Benchmark, verify <0.2ms |
| Perf: Packetizer | Packets/sec throughput | Benchmark, verify >1000/sec |
| Fuzz: Slice parser | Malformed NAL units | cargo-fuzz |
| Fuzz: Region map | Random block classifications | cargo-fuzz on merge algorithm |

---

## 13. Optimizations Index

| ID | Optimization | Impact | Phase |
|----|-------------|--------|-------|
| H1 | Zero-copy capture-to-encode (GPU-resident) | Eliminates 4ms GPU readback | 1 |
| H2 | Encoder lookahead elimination | Eliminates 33-66ms latency | 1 |
| H3 | Capture-on-damage (not polling) | 0 bandwidth when static | 1 |
| H7 | Scroll prediction metadata | Instant perceived scroll | 2 |
| H9 | GPU compute shader for frame diff | 0.1ms vs 3-4ms CPU diff | 2 |
| H10 | Parallel encoder pool | 47% faster multi-region encode | 2 |
| H11 | Speculative IDR on scene change | 13ms vs 31ms after Alt+Tab | 1 |
| H12 | Adaptive keyframe interval | Bandwidth savings for static content | 1 |
| H13 | Client-side static region atlas | 40-60% bandwidth savings for typical desktop | 2 |
| R32 | Input-triggered capture | Up to 16ms faster response to input | 1 |
| R33 | Slice-level streaming | ~4ms savings from progressive decode | 2 |
| D1 | Double-buffered GPU textures | Eliminates capture→classify fence wait | 1 |
| D2 | Damage rect merging | Fewer encode jobs, better HW utilization | 1 |
| D3 | Adaptive frame pacing | Match content rate, reduce wasted encodes | 1 |
| E1 | Pinned encoder output buffers | No allocation/page faults during encode | 1 |
| E2 | Streaming classify→encode overlap | ~0.3ms savings per frame | 2 |
| E3 | Rate control warm-start per window | Correct bitrate from first frame after switch | 1 |
| E4 | Macroblock boundary snapping | Avoid partial macroblock waste | 2 |
| P1 | Pre-built packet header template | ~5 fewer buffer ops per packet | 1 |
| P2 | Cursor position piggybacking | Eliminate 60 cursor datagrams/sec | 1 |
| P4 | Client-side frame gap detection | Proactive IDR request within 1 RTT | 1 |
| L1 | HW lossless H.264 (no GPU readback) | Eliminates 3-4ms readback for text regions | 2 |
| L2 | GPU delta compression for small changes | ~1-5KB readback vs 14.7MB full region | 2 |

---

*PRISM Display Engine Design v1.0 — CC0 Public Domain*
