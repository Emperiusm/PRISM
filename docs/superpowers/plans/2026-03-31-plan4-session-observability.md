# Plan 4: Session + Observability Implementation

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the `prism-observability` and `prism-session` crates providing frame tracing, client feedback, overlay packets, time-series metrics history, session types, channel ownership registry, ArcSwap routing table, tombstone reconnection, heartbeat monitoring, connection profiles, capability negotiation, bandwidth arbitration with zero-cost allocation handles, per-channel bandwidth tracking, starvation detection, and the ChannelHandler dispatch framework.

**Architecture:** `prism-observability` depends only on `prism-metrics` — it adds frame tracing (adaptive sampling), client feedback types, 128-byte overlay packets, and ring-buffer time-series history. `prism-session` depends on `prism-protocol` (channels, capabilities), `prism-transport` (connections, framing, quality), `prism-security` (SecurityContext, PairingEntry), and `prism-metrics` (recording). Session Manager is control-plane only (R37) — it never touches frame bytes. The RoutingTable uses `arc-swap` for lock-free reads (~5ns per-frame lookup). The BandwidthArbiter uses priority-weighted proportional allocation with `AllocationHandle` (shared atomics, ~1ns per-frame reads). Channel dispatch routes via the `ChannelHandler` trait. All components are testable in isolation — integration wiring (recv loop, connection lifecycle) is deferred to Plan 6.

**Tech Stack:** `arc-swap` (lock-free routing), `tokio` (broadcast, async), `async-trait` (dyn ChannelHandler), `uuid` (ClientId), `serde`/`serde_json` (tombstone persistence), `bytes` (packet handling), `prism-protocol`, `prism-transport`, `prism-security`, `prism-metrics`

**Spec refs:**
- Session+Observability: `docs/superpowers/specs/2026-03-30-session-observability-design.md` (all sections)
- Architecture: `docs/superpowers/specs/2026-03-30-prism-architecture-design.md` (R1, R2, R5-R14, R37-R39, R47)

---

## File Structure

```
PRISM/
  crates/
    prism-observability/
      Cargo.toml
      src/
        lib.rs                  # re-exports
        frame_trace.rs          # FrameTrace, FrameLatencyBreakdown, FrameTracer
        feedback.rs             # ClientFeedback, ClientFeedbackConfig, ClientAlert
        overlay.rs              # OverlayPacket (128-byte binary)
        time_series.rs          # MetricsTimeSeries, TimeSeriesRing, TimeSample

    prism-session/
      Cargo.toml
      src/
        lib.rs                  # re-exports
        error.rs                # SessionError
        types.rs                # SessionState, ClientId, SessionEvent, ArbiterEvent
        control_msg.rs          # Control channel message type constants, ShutdownNotice
        channel.rs              # ChannelRegistry, ChannelOwnership, ChannelGrantResult
        routing.rs              # RoutingTable, RoutingSnapshot, RouteEntry, RoutingMutation
        tombstone.rs            # TombstoneStore, Tombstone, ChannelRecoveryState
        heartbeat.rs            # HeartbeatMonitor, HeartbeatState
        profiles.rs             # ConnectionProfile, DisplayProfile, EncoderPreset, defaults
        negotiation.rs          # CapabilityNegotiator, NegotiationResult, NegotiatedChannel
        dispatch.rs             # ChannelHandler trait, ChannelDispatcher, ChannelError
        arbiter.rs              # BandwidthArbiter, ClientBudget, ChannelAllocation,
                                # AllocationHandle, AllocationResult, BandwidthNeeds,
                                # StarvationDetector, StarvationWarning
        tracker.rs              # ChannelBandwidthTracker (hot-path atomic counters)
```

---

## Task 1: prism-observability Crate Setup + FrameTrace

**Files:**
- Modify: `Cargo.toml` (workspace root)
- Create: `crates/prism-observability/Cargo.toml`
- Create: `crates/prism-observability/src/lib.rs`
- Create: `crates/prism-observability/src/frame_trace.rs`
- Create: placeholder files for other modules

- [ ] **Step 1: Update workspace root Cargo.toml**

Add `"crates/prism-observability"` to members. Add `prism-observability = { path = "crates/prism-observability" }` to workspace.dependencies. Keep all existing deps unchanged.

- [ ] **Step 2: Create crates/prism-observability/Cargo.toml**

```toml
[package]
name = "prism-observability"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
prism-metrics = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
```

- [ ] **Step 3: Create lib.rs and placeholder files**

`crates/prism-observability/src/lib.rs`:
```rust
pub mod frame_trace;
pub mod feedback;
pub mod overlay;
pub mod time_series;
```

Create placeholder files: `feedback.rs`, `overlay.rs`, `time_series.rs` with just a comment.

- [ ] **Step 4: Write failing tests for FrameTrace**

`crates/prism-observability/src/frame_trace.rs`:
```rust
/// End-to-end latency decomposition for a single display frame.
#[derive(Debug, Clone)]
pub struct FrameTrace {
    pub frame_seq: u32,
    pub capture_start_us: u64,
    pub capture_end_us: u64,
    pub classify_end_us: u64,
    pub encode_start_us: u64,
    pub encode_end_us: u64,
    pub send_us: u64,
    pub network_recv_us: Option<u64>,
    pub decode_end_us: Option<u64>,
    pub render_end_us: Option<u64>,
}

/// Computed latency breakdown from a FrameTrace.
#[derive(Debug, Clone)]
pub struct FrameLatencyBreakdown {
    pub capture_us: u64,
    pub classify_us: u64,
    pub encode_us: u64,
    pub send_us: u64,
    pub network_us: Option<u64>,
    pub decode_us: Option<u64>,
    pub render_us: Option<u64>,
    pub total_us: u64,
}

// Implementation goes here

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_trace_breakdown_server_only() {
        let trace = FrameTrace {
            frame_seq: 1,
            capture_start_us: 1000,
            capture_end_us: 1500,
            classify_end_us: 1800,
            encode_start_us: 1800,
            encode_end_us: 3000,
            send_us: 3100,
            network_recv_us: None,
            decode_end_us: None,
            render_end_us: None,
        };
        let breakdown = trace.breakdown();
        assert_eq!(breakdown.capture_us, 500);
        assert_eq!(breakdown.classify_us, 300);
        assert_eq!(breakdown.encode_us, 1200);
        assert_eq!(breakdown.send_us, 100);
        assert!(breakdown.network_us.is_none());
        assert_eq!(breakdown.total_us, 2100); // send_us - capture_start_us
    }

    #[test]
    fn frame_trace_breakdown_full_pipeline() {
        let trace = FrameTrace {
            frame_seq: 2,
            capture_start_us: 0,
            capture_end_us: 500,
            classify_end_us: 700,
            encode_start_us: 700,
            encode_end_us: 2000,
            send_us: 2100,
            network_recv_us: Some(5000),
            decode_end_us: Some(6000),
            render_end_us: Some(6500),
        };
        let breakdown = trace.breakdown();
        assert_eq!(breakdown.network_us, Some(2900));
        assert_eq!(breakdown.decode_us, Some(1000));
        assert_eq!(breakdown.render_us, Some(500));
        assert_eq!(breakdown.total_us, 6500); // render_end - capture_start
    }
}
```

- [ ] **Step 5: Run tests to verify they fail**

Run: `cargo test -p prism-observability`
Expected: FAIL — `breakdown()` method not defined

- [ ] **Step 6: Implement FrameTrace::breakdown()**

Add between the struct definitions and `#[cfg(test)]`:

```rust
impl FrameTrace {
    pub fn breakdown(&self) -> FrameLatencyBreakdown {
        let capture_us = self.capture_end_us.saturating_sub(self.capture_start_us);
        let classify_us = self.classify_end_us.saturating_sub(self.capture_end_us);
        let encode_us = self.encode_end_us.saturating_sub(self.encode_start_us);
        let send_us = self.send_us.saturating_sub(self.encode_end_us);

        let network_us = self.network_recv_us.map(|r| r.saturating_sub(self.send_us));
        let decode_us = match (self.decode_end_us, self.network_recv_us) {
            (Some(d), Some(n)) => Some(d.saturating_sub(n)),
            _ => None,
        };
        let render_us = match (self.render_end_us, self.decode_end_us) {
            (Some(r), Some(d)) => Some(r.saturating_sub(d)),
            _ => None,
        };

        let total_us = self.render_end_us
            .unwrap_or(self.send_us)
            .saturating_sub(self.capture_start_us);

        FrameLatencyBreakdown {
            capture_us, classify_us, encode_us, send_us,
            network_us, decode_us, render_us, total_us,
        }
    }
}
```

- [ ] **Step 7: Run tests to verify they pass**

Run: `cargo test -p prism-observability`
Expected: 2 tests pass

- [ ] **Step 8: Commit**

```bash
git add crates/prism-observability/ Cargo.toml
git commit -m "feat(observability): scaffold crate, FrameTrace with latency breakdown"
```

---

## Task 2: FrameTracer (Adaptive Sampling)

**Files:**
- Modify: `crates/prism-observability/src/frame_trace.rs`

- [ ] **Step 1: Write failing tests**

Append to frame_trace.rs tests:

```rust
    #[test]
    fn tracer_uniform_sampling() {
        let mut tracer = FrameTracer::new();
        // Default: uniform_rate=60, so first trace at frame 60
        let mut traced = 0;
        for _ in 0..120 {
            if tracer.should_trace(5000) { traced += 1; }
        }
        assert_eq!(traced, 2); // frames 60 and 120
    }

    #[test]
    fn tracer_always_traces_slow_frames() {
        let mut tracer = FrameTracer::new();
        tracer.update_threshold(10_000); // 10ms threshold
        // Slow frame should be traced regardless of uniform counter
        assert!(tracer.should_trace(15_000));
    }

    #[test]
    fn tracer_respects_budget() {
        let mut tracer = FrameTracer::new();
        tracer.update_threshold(1); // very low threshold → everything is "slow"
        let mut traced = 0;
        for _ in 0..20 {
            if tracer.should_trace(100) { traced += 1; }
        }
        assert!(traced <= 10, "should respect max_traces_per_second={}", traced);
    }

    #[test]
    fn tracer_resets_budget_each_second() {
        let mut tracer = FrameTracer::new();
        tracer.update_threshold(1);
        // Exhaust budget
        for _ in 0..10 { tracer.should_trace(100); }
        assert!(!tracer.should_trace(100)); // budget exhausted
        // Simulate second boundary
        tracer.reset_second();
        assert!(tracer.should_trace(100)); // budget restored
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p prism-observability -- tracer`
Expected: FAIL — `FrameTracer` not defined

- [ ] **Step 3: Implement FrameTracer**

Add to frame_trace.rs:

```rust
/// Adaptive frame trace sampler. Traces slow frames always, uniform-samples the rest.
pub struct FrameTracer {
    uniform_rate: u32,
    uniform_counter: u32,
    slow_frame_threshold_us: u64,
    traces_this_second: u32,
    max_traces_per_second: u32,
}

impl FrameTracer {
    pub fn new() -> Self {
        Self {
            uniform_rate: 60,
            uniform_counter: 0,
            slow_frame_threshold_us: 20_000, // default 20ms (auto-tuned to p95)
            traces_this_second: 0,
            max_traces_per_second: 10,
        }
    }

    /// Decide whether to trace this frame.
    /// `last_frame_total_us`: total latency of the previous frame.
    pub fn should_trace(&mut self, last_frame_total_us: u64) -> bool {
        if self.traces_this_second >= self.max_traces_per_second {
            return false;
        }
        // Always trace slow frames
        if last_frame_total_us > self.slow_frame_threshold_us {
            self.traces_this_second += 1;
            return true;
        }
        // Uniform sampling for baseline
        self.uniform_counter += 1;
        if self.uniform_counter >= self.uniform_rate {
            self.uniform_counter = 0;
            self.traces_this_second += 1;
            return true;
        }
        false
    }

    /// Update the slow-frame threshold (typically set to p95 latency).
    pub fn update_threshold(&mut self, p95_us: u64) {
        self.slow_frame_threshold_us = p95_us;
    }

    /// Reset the per-second budget (called at second boundaries).
    pub fn reset_second(&mut self) {
        self.traces_this_second = 0;
    }
}

impl Default for FrameTracer {
    fn default() -> Self { Self::new() }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p prism-observability`
Expected: 6 tests pass (2 + 4 new)

- [ ] **Step 5: Commit**

```bash
git add crates/prism-observability/src/frame_trace.rs
git commit -m "feat(observability): FrameTracer adaptive sampling with budget"
```

---

## Task 3: ClientFeedback + ClientAlert + OverlayPacket

**Files:**
- Modify: `crates/prism-observability/src/feedback.rs`
- Modify: `crates/prism-observability/src/overlay.rs`

- [ ] **Step 1: Write failing tests for feedback types**

`crates/prism-observability/src/feedback.rs`:
```rust
use serde::{Deserialize, Serialize};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_feedback_json_roundtrip() {
        let fb = ClientFeedback {
            avg_decode_us: 5000,
            avg_render_us: 2000,
            frames_decoded: 1000,
            frames_dropped: 5,
            frames_late: 10,
            decoder_queue_depth: 2,
        };
        let json = serde_json::to_string(&fb).unwrap();
        let decoded: ClientFeedback = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.frames_dropped, 5);
    }

    #[test]
    fn client_feedback_config_defaults() {
        let config = ClientFeedbackConfig::default();
        assert_eq!(config.normal_interval_ms, 1000);
        assert_eq!(config.stressed_interval_ms, 200);
    }

    #[test]
    fn client_feedback_is_stressed() {
        let config = ClientFeedbackConfig::default();
        assert!(!config.is_stressed(1, 0.01));
        assert!(config.is_stressed(4, 0.01));  // queue depth > threshold
        assert!(config.is_stressed(1, 0.06));  // drop rate > threshold
    }

    #[test]
    fn client_alert_json_roundtrip() {
        let alert = ClientAlert::DecoderOverloaded { queue_depth: 5, drop_rate: 0.15 };
        let json = serde_json::to_string(&alert).unwrap();
        let decoded: ClientAlert = serde_json::from_str(&json).unwrap();
        assert!(matches!(decoded, ClientAlert::DecoderOverloaded { queue_depth: 5, .. }));
    }
}
```

- [ ] **Step 2: Write failing tests for OverlayPacket**

`crates/prism-observability/src/overlay.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlay_packet_size_is_128() {
        assert_eq!(OVERLAY_PACKET_SIZE, 128);
    }

    #[test]
    fn overlay_roundtrip() {
        let packet = OverlayPacket {
            fps: 60,
            degradation_level: 0,
            active_clients: 1,
            transport_type: 0, // QUIC
            codec: *b"h265",
            resolution_w: 2560,
            resolution_h: 1440,
            bitrate_kbps: 5000,
            rtt_us: 5000,
            loss_rate_permille: 1,
            capture_us: 500,
            encode_us: 2000,
            network_us: 3000,
            decode_us: 1000,
            render_us: 500,
            total_us: 7000,
            display_kbps: 4000,
            input_kbps: 10,
            audio_kbps: 128,
            fileshare_kbps: 0,
            total_kbps: 4138,
            available_kbps: 10000,
        };
        let bytes = packet.to_bytes();
        assert_eq!(bytes.len(), OVERLAY_PACKET_SIZE);
        let decoded = OverlayPacket::from_bytes(&bytes).unwrap();
        assert_eq!(decoded.fps, 60);
        assert_eq!(decoded.resolution_w, 2560);
        assert_eq!(decoded.codec, *b"h265");
        assert_eq!(decoded.total_us, 7000);
    }
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p prism-observability`
Expected: FAIL — types not defined

- [ ] **Step 4: Implement feedback types**

Add to `feedback.rs`:

```rust
/// Client performance feedback sent periodically.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientFeedback {
    pub avg_decode_us: u64,
    pub avg_render_us: u64,
    pub frames_decoded: u64,
    pub frames_dropped: u64,
    pub frames_late: u64,
    pub decoder_queue_depth: u8,
}

/// Configuration for client feedback frequency.
#[derive(Debug, Clone)]
pub struct ClientFeedbackConfig {
    pub normal_interval_ms: u64,
    pub stressed_interval_ms: u64,
    pub stress_threshold_queue_depth: u8,
    pub stress_threshold_drop_rate: f32,
}

impl Default for ClientFeedbackConfig {
    fn default() -> Self {
        Self {
            normal_interval_ms: 1000,
            stressed_interval_ms: 200,
            stress_threshold_queue_depth: 3,
            stress_threshold_drop_rate: 0.05,
        }
    }
}

impl ClientFeedbackConfig {
    /// Check if the client is stressed based on metrics.
    pub fn is_stressed(&self, queue_depth: u8, drop_rate: f32) -> bool {
        queue_depth >= self.stress_threshold_queue_depth
            || drop_rate >= self.stress_threshold_drop_rate
    }
}

/// Immediate client alerts (sent as datagrams, no waiting).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ClientAlert {
    DecoderOverloaded { queue_depth: u8, drop_rate: f32 },
    OutOfMemory,
    DisplayChanged { new_resolution: (u32, u32), new_scale: f32 },
}
```

- [ ] **Step 5: Implement OverlayPacket**

Add to `overlay.rs`:

```rust
/// Overlay packet size in bytes.
pub const OVERLAY_PACKET_SIZE: usize = 128;

/// Zero-copy 128-byte binary packet for client overlay.
/// Sent every 100ms when overlay is enabled. No serialization — manual byte packing.
#[derive(Debug, Clone, PartialEq)]
pub struct OverlayPacket {
    pub fps: u8,
    pub degradation_level: u8,
    pub active_clients: u8,
    pub transport_type: u8,
    pub codec: [u8; 4],
    pub resolution_w: u16,
    pub resolution_h: u16,
    pub bitrate_kbps: u32,
    pub rtt_us: u32,
    pub loss_rate_permille: u16,
    pub capture_us: u32,
    pub encode_us: u32,
    pub network_us: u32,
    pub decode_us: u32,
    pub render_us: u32,
    pub total_us: u32,
    pub display_kbps: u32,
    pub input_kbps: u16,
    pub audio_kbps: u16,
    pub fileshare_kbps: u32,
    pub total_kbps: u32,
    pub available_kbps: u32,
}

impl OverlayPacket {
    pub fn to_bytes(&self) -> [u8; OVERLAY_PACKET_SIZE] {
        let mut buf = [0u8; OVERLAY_PACKET_SIZE];
        buf[0] = self.fps;
        buf[1] = self.degradation_level;
        buf[2] = self.active_clients;
        buf[3] = self.transport_type;
        buf[4..8].copy_from_slice(&self.codec);
        buf[8..10].copy_from_slice(&self.resolution_w.to_le_bytes());
        buf[10..12].copy_from_slice(&self.resolution_h.to_le_bytes());
        buf[12..16].copy_from_slice(&self.bitrate_kbps.to_le_bytes());
        buf[16..20].copy_from_slice(&self.rtt_us.to_le_bytes());
        buf[20..22].copy_from_slice(&self.loss_rate_permille.to_le_bytes());
        buf[22..26].copy_from_slice(&self.capture_us.to_le_bytes());
        buf[26..30].copy_from_slice(&self.encode_us.to_le_bytes());
        buf[30..34].copy_from_slice(&self.network_us.to_le_bytes());
        buf[34..38].copy_from_slice(&self.decode_us.to_le_bytes());
        buf[38..42].copy_from_slice(&self.render_us.to_le_bytes());
        buf[42..46].copy_from_slice(&self.total_us.to_le_bytes());
        buf[46..50].copy_from_slice(&self.display_kbps.to_le_bytes());
        buf[50..52].copy_from_slice(&self.input_kbps.to_le_bytes());
        buf[52..54].copy_from_slice(&self.audio_kbps.to_le_bytes());
        buf[54..58].copy_from_slice(&self.fileshare_kbps.to_le_bytes());
        buf[58..62].copy_from_slice(&self.total_kbps.to_le_bytes());
        buf[62..66].copy_from_slice(&self.available_kbps.to_le_bytes());
        // bytes 66..128 are reserved (zeroed)
        buf
    }

    pub fn from_bytes(buf: &[u8]) -> Option<Self> {
        if buf.len() < OVERLAY_PACKET_SIZE { return None; }
        Some(Self {
            fps: buf[0],
            degradation_level: buf[1],
            active_clients: buf[2],
            transport_type: buf[3],
            codec: [buf[4], buf[5], buf[6], buf[7]],
            resolution_w: u16::from_le_bytes([buf[8], buf[9]]),
            resolution_h: u16::from_le_bytes([buf[10], buf[11]]),
            bitrate_kbps: u32::from_le_bytes([buf[12], buf[13], buf[14], buf[15]]),
            rtt_us: u32::from_le_bytes([buf[16], buf[17], buf[18], buf[19]]),
            loss_rate_permille: u16::from_le_bytes([buf[20], buf[21]]),
            capture_us: u32::from_le_bytes([buf[22], buf[23], buf[24], buf[25]]),
            encode_us: u32::from_le_bytes([buf[26], buf[27], buf[28], buf[29]]),
            network_us: u32::from_le_bytes([buf[30], buf[31], buf[32], buf[33]]),
            decode_us: u32::from_le_bytes([buf[34], buf[35], buf[36], buf[37]]),
            render_us: u32::from_le_bytes([buf[38], buf[39], buf[40], buf[41]]),
            total_us: u32::from_le_bytes([buf[42], buf[43], buf[44], buf[45]]),
            display_kbps: u32::from_le_bytes([buf[46], buf[47], buf[48], buf[49]]),
            input_kbps: u16::from_le_bytes([buf[50], buf[51]]),
            audio_kbps: u16::from_le_bytes([buf[52], buf[53]]),
            fileshare_kbps: u32::from_le_bytes([buf[54], buf[55], buf[56], buf[57]]),
            total_kbps: u32::from_le_bytes([buf[58], buf[59], buf[60], buf[61]]),
            available_kbps: u32::from_le_bytes([buf[62], buf[63], buf[64], buf[65]]),
        })
    }
}
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test -p prism-observability`
Expected: 10 tests pass

- [ ] **Step 7: Commit**

```bash
git add crates/prism-observability/src/feedback.rs crates/prism-observability/src/overlay.rs
git commit -m "feat(observability): ClientFeedback, ClientAlert, OverlayPacket (128-byte binary)"
```

---

## Task 4: MetricsTimeSeries

**Files:**
- Modify: `crates/prism-observability/src/time_series.rs`

- [ ] **Step 1: Write failing tests**

```rust
use std::collections::VecDeque;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_ring_returns_empty() {
        let ring = TimeSeriesRing::new(300);
        assert!(ring.samples().is_empty());
    }

    #[test]
    fn ring_records_samples() {
        let mut ring = TimeSeriesRing::new(300);
        ring.push(TimeSample { timestamp_secs: 1, value: 42.0 });
        ring.push(TimeSample { timestamp_secs: 2, value: 43.0 });
        assert_eq!(ring.samples().len(), 2);
        assert_eq!(ring.latest().unwrap().value, 43.0);
    }

    #[test]
    fn ring_evicts_oldest_at_capacity() {
        let mut ring = TimeSeriesRing::new(3);
        ring.push(TimeSample { timestamp_secs: 1, value: 1.0 });
        ring.push(TimeSample { timestamp_secs: 2, value: 2.0 });
        ring.push(TimeSample { timestamp_secs: 3, value: 3.0 });
        ring.push(TimeSample { timestamp_secs: 4, value: 4.0 });
        assert_eq!(ring.samples().len(), 3);
        assert_eq!(ring.samples()[0].value, 2.0); // oldest evicted
        assert_eq!(ring.latest().unwrap().value, 4.0);
    }

    #[test]
    fn time_series_tracks_multiple_metrics() {
        let mut ts = MetricsTimeSeries::new(300);
        ts.record("rtt_us", 1, 5000.0);
        ts.record("fps", 1, 60.0);
        ts.record("rtt_us", 2, 6000.0);
        assert_eq!(ts.get("rtt_us").unwrap().samples().len(), 2);
        assert_eq!(ts.get("fps").unwrap().samples().len(), 1);
        assert!(ts.get("nonexistent").is_none());
    }

    #[test]
    fn time_series_sparkline_data() {
        let mut ts = MetricsTimeSeries::new(5);
        for i in 0..5 {
            ts.record("rtt", i, (i as f64) * 1000.0);
        }
        let values: Vec<f64> = ts.get("rtt").unwrap().samples().iter().map(|s| s.value).collect();
        assert_eq!(values, vec![0.0, 1000.0, 2000.0, 3000.0, 4000.0]);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p prism-observability -- time_series`
Expected: FAIL — types not defined

- [ ] **Step 3: Implement MetricsTimeSeries**

```rust
/// A single time-series sample.
#[derive(Debug, Clone, Copy)]
pub struct TimeSample {
    pub timestamp_secs: u64,
    pub value: f64,
}

/// Ring buffer of time-series samples with fixed capacity.
pub struct TimeSeriesRing {
    samples: VecDeque<TimeSample>,
    max_len: usize,
}

impl TimeSeriesRing {
    pub fn new(max_len: usize) -> Self {
        Self { samples: VecDeque::with_capacity(max_len), max_len }
    }

    pub fn push(&mut self, sample: TimeSample) {
        if self.samples.len() >= self.max_len {
            self.samples.pop_front();
        }
        self.samples.push_back(sample);
    }

    pub fn samples(&self) -> &VecDeque<TimeSample> {
        &self.samples
    }

    pub fn latest(&self) -> Option<&TimeSample> {
        self.samples.back()
    }
}

/// Per-metric time-series history. 300 samples at 1/sec = 5 minutes.
pub struct MetricsTimeSeries {
    series: std::collections::HashMap<String, TimeSeriesRing>,
    max_samples: usize,
}

impl MetricsTimeSeries {
    pub fn new(max_samples: usize) -> Self {
        Self { series: std::collections::HashMap::new(), max_samples }
    }

    pub fn record(&mut self, metric_name: &str, timestamp_secs: u64, value: f64) {
        let ring = self.series.entry(metric_name.to_string())
            .or_insert_with(|| TimeSeriesRing::new(self.max_samples));
        ring.push(TimeSample { timestamp_secs, value });
    }

    pub fn get(&self, metric_name: &str) -> Option<&TimeSeriesRing> {
        self.series.get(metric_name)
    }
}

impl Default for MetricsTimeSeries {
    fn default() -> Self { Self::new(300) }
}
```

- [ ] **Step 4: Update lib.rs re-exports**

```rust
pub use frame_trace::{FrameTrace, FrameLatencyBreakdown, FrameTracer};
pub use feedback::{ClientFeedback, ClientFeedbackConfig, ClientAlert};
pub use overlay::{OverlayPacket, OVERLAY_PACKET_SIZE};
pub use time_series::{MetricsTimeSeries, TimeSeriesRing, TimeSample};
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p prism-observability`
Expected: 15 tests pass

- [ ] **Step 6: Commit**

```bash
git add crates/prism-observability/
git commit -m "feat(observability): MetricsTimeSeries ring-buffer history"
```

---

## Task 5: prism-session Crate Setup + Core Types

**Files:**
- Modify: `Cargo.toml` (workspace root)
- Create: `crates/prism-session/Cargo.toml`
- Create: `crates/prism-session/src/lib.rs`
- Create: `crates/prism-session/src/error.rs`
- Create: `crates/prism-session/src/types.rs`
- Create: all placeholder source files

- [ ] **Step 1: Update workspace Cargo.toml**

Add `"crates/prism-session"` to members, `prism-session = { path = "crates/prism-session" }` to workspace.dependencies.

- [ ] **Step 2: Create crates/prism-session/Cargo.toml**

```toml
[package]
name = "prism-session"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
prism-protocol = { workspace = true }
prism-transport = { workspace = true }
prism-security = { workspace = true }
prism-metrics = { workspace = true }
bytes = { workspace = true }
thiserror = { workspace = true }
uuid = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
arc-swap = { workspace = true }
tokio = { workspace = true }
async-trait = { workspace = true }
```

- [ ] **Step 3: Create lib.rs + all placeholder files**

`lib.rs`:
```rust
pub mod error;
pub mod types;
pub mod control_msg;
pub mod channel;
pub mod routing;
pub mod tombstone;
pub mod heartbeat;
pub mod profiles;
pub mod negotiation;
pub mod dispatch;
pub mod arbiter;
pub mod tracker;
```

Create all source files as placeholders with a comment.

- [ ] **Step 4: Write failing tests for core types**

`crates/prism-session/src/error.rs`:
```rust
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SessionError {
    #[error("unknown channel: 0x{0:03X}")]
    UnknownChannel(u16),
    #[error("channel already owned by {0}")]
    ChannelConflict(uuid::Uuid),
    #[error("client not found: {0}")]
    ClientNotFound(uuid::Uuid),
    #[error("negotiation failed: {0}")]
    NegotiationFailed(String),
    #[error("tombstone expired")]
    TombstoneExpired,
    #[error("transport error: {0}")]
    Transport(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_error_display() {
        let err = SessionError::UnknownChannel(0x0E1);
        assert_eq!(format!("{err}"), "unknown channel: 0x0E1");
    }
}
```

`crates/prism-session/src/types.rs`:
```rust
use uuid::Uuid;
use serde::{Deserialize, Serialize};

pub type ClientId = Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionState {
    Authenticating,
    Active,
    Suspended,
    Tombstoned,
}

#[derive(Debug, Clone)]
pub enum SessionEvent {
    ClientConnected { client_id: ClientId, device_name: String },
    ClientDisconnected { client_id: ClientId, reason: String },
    ClientReconnected { client_id: ClientId, was_tombstoned: bool },
    ChannelOwnershipChanged { channel_id: u16, new_owner: Option<ClientId> },
    ProfileChanged { client_id: ClientId, profile: String },
}

#[derive(Debug, Clone)]
pub enum ArbiterEvent {
    AllocationChanged { client_id: ClientId },
    StarvationWarning { client_id: ClientId, channel_id: u16 },
    ReduceSendRate { client_id: ClientId, suggested_reduction: f32 },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_state_serialize_roundtrip() {
        let state = SessionState::Active;
        let json = serde_json::to_string(&state).unwrap();
        let decoded: SessionState = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, SessionState::Active);
    }

    #[test]
    fn client_id_is_uuid() {
        let id: ClientId = Uuid::new_v4();
        assert_eq!(id.get_version(), Some(uuid::Version::Random));
    }
}
```

- [ ] **Step 5: Run tests, verify pass, update lib.rs re-exports, commit**

```bash
git add crates/prism-session/ Cargo.toml
git commit -m "feat(session): scaffold crate, SessionError, SessionState, SessionEvent"
```

---

## Task 6: Control Message Types + ShutdownNotice

**Files:**
- Modify: `crates/prism-session/src/control_msg.rs`

- [ ] **Step 1: Write failing tests**

```rust
use serde::{Deserialize, Serialize};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_msg_types_distinct() {
        let types = [
            HEARTBEAT, HEARTBEAT_ACK, CAPABILITY_UPDATE, PROFILE_SWITCH,
            SESSION_INFO, SHUTDOWN_NOTICE, PROBE_REQUEST, PROBE_RESPONSE,
            CLIENT_FEEDBACK, CLIENT_ALERT, QUALITY_UPDATE, REDUCE_SEND_RATE,
            OVERLAY_TOGGLE, OVERLAY_DATA, KEY_ROTATION, CERT_RENEWAL,
            CHANNEL_TRANSFER, MONITOR_LAYOUT, THROUGHPUT_TOKEN,
        ];
        let mut set = std::collections::HashSet::new();
        for t in &types {
            assert!(set.insert(t), "duplicate msg type: 0x{:02X}", t);
        }
    }

    #[test]
    fn shutdown_notice_json_roundtrip() {
        let notice = ShutdownNotice {
            reason: "Server restarting".into(),
            seconds_remaining: 30,
            will_restart: true,
        };
        let json = serde_json::to_string(&notice).unwrap();
        let decoded: ShutdownNotice = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.seconds_remaining, 30);
        assert!(decoded.will_restart);
    }
}
```

- [ ] **Step 2: Implement control message types**

```rust
// Session management
pub const HEARTBEAT: u8 = 0x01;
pub const HEARTBEAT_ACK: u8 = 0x02;
pub const CAPABILITY_UPDATE: u8 = 0x03;
pub const PROFILE_SWITCH: u8 = 0x04;
pub const SESSION_INFO: u8 = 0x10;
pub const SHUTDOWN_NOTICE: u8 = 0x20;

// Quality & probing
pub const PROBE_REQUEST: u8 = 0x05;
pub const PROBE_RESPONSE: u8 = 0x06;
pub const CLIENT_FEEDBACK: u8 = 0x07;
pub const CLIENT_ALERT: u8 = 0x08;
pub const QUALITY_UPDATE: u8 = 0x0D;
pub const REDUCE_SEND_RATE: u8 = 0x0E;

// Overlay
pub const OVERLAY_TOGGLE: u8 = 0x09;
pub const OVERLAY_DATA: u8 = 0x0A;

// Security (delegated to SecurityGate)
pub const KEY_ROTATION: u8 = 0x0B;
pub const CERT_RENEWAL: u8 = 0x0C;

// Multi-client
pub const CHANNEL_TRANSFER: u8 = 0x0F;
pub const MONITOR_LAYOUT: u8 = 0x11;

// Transport
pub const THROUGHPUT_TOKEN: u8 = 0x12;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShutdownNotice {
    pub reason: String,
    pub seconds_remaining: u32,
    pub will_restart: bool,
}
```

- [ ] **Step 3: Run tests, commit**

```bash
git add crates/prism-session/src/control_msg.rs
git commit -m "feat(session): control message type registry + ShutdownNotice"
```

---

## Task 7: ChannelRegistry + ChannelOwnership

**Files:**
- Modify: `crates/prism-session/src/channel.rs`

- [ ] **Step 1: Write failing tests**

```rust
use std::collections::{HashMap, HashSet};
use crate::types::ClientId;
use crate::error::SessionError;
use prism_protocol::channel::EXTENSION_CHANNEL_START;
use uuid::Uuid;

#[cfg(test)]
mod tests {
    use super::*;

    fn test_registry() -> ChannelRegistry {
        ChannelRegistry::with_defaults()
    }

    fn client_a() -> ClientId { Uuid::from_bytes([1; 16]) }
    fn client_b() -> ClientId { Uuid::from_bytes([2; 16]) }

    #[test]
    fn request_exclusive_unowned_grants() {
        let mut reg = test_registry();
        let result = reg.request_channel(0x001, client_a()).unwrap(); // Display
        assert!(matches!(result, ChannelGrantResult::Granted));
    }

    #[test]
    fn request_exclusive_already_owned_by_self() {
        let mut reg = test_registry();
        reg.request_channel(0x001, client_a()).unwrap();
        let result = reg.request_channel(0x001, client_a()).unwrap();
        assert!(matches!(result, ChannelGrantResult::AlreadyOwned));
    }

    #[test]
    fn request_exclusive_owned_by_other_denied() {
        let mut reg = test_registry();
        reg.request_channel(0x001, client_a()).unwrap();
        let result = reg.request_channel(0x001, client_b()).unwrap();
        assert!(matches!(result, ChannelGrantResult::Denied { .. }));
    }

    #[test]
    fn request_shared_multiple_clients() {
        let mut reg = test_registry();
        let r1 = reg.request_channel(0x004, client_a()).unwrap(); // Clipboard
        let r2 = reg.request_channel(0x004, client_b()).unwrap();
        assert!(matches!(r1, ChannelGrantResult::Granted));
        assert!(matches!(r2, ChannelGrantResult::Granted));
    }

    #[test]
    fn request_transferable_on_request() {
        let mut reg = test_registry();
        reg.request_channel(0x0E4, client_a()).unwrap(); // Touch
        let result = reg.request_channel(0x0E4, client_b()).unwrap();
        assert!(matches!(result, ChannelGrantResult::Transferred { .. }));
    }

    #[test]
    fn release_all_clears_ownership() {
        let mut reg = test_registry();
        reg.request_channel(0x001, client_a()).unwrap();
        reg.request_channel(0x004, client_a()).unwrap();
        reg.release_all(client_a());
        // Now client_b can take Display
        let result = reg.request_channel(0x001, client_b()).unwrap();
        assert!(matches!(result, ChannelGrantResult::Granted));
    }

    #[test]
    fn unknown_channel_errors() {
        let mut reg = test_registry();
        let result = reg.request_channel(0x099, client_a());
        assert!(matches!(result, Err(SessionError::UnknownChannel(0x099))));
    }

    #[test]
    fn extension_channel_auto_created_shared() {
        let mut reg = test_registry();
        let result = reg.request_channel(0x100, client_a()).unwrap();
        assert!(matches!(result, ChannelGrantResult::Granted));
        let result = reg.request_channel(0x100, client_b()).unwrap();
        assert!(matches!(result, ChannelGrantResult::Granted));
    }
}
```

- [ ] **Step 2: Implement ChannelRegistry**

```rust
#[derive(Debug, Clone)]
pub enum TransferPolicy {
    OnRequest,
    OwnerApproves,
    ServerDecides,
}

#[derive(Debug)]
pub enum ChannelOwnership {
    Exclusive { owner: Option<ClientId> },
    Shared { subscribers: HashSet<ClientId> },
    Transferable { owner: Option<ClientId>, transfer_policy: TransferPolicy },
}

#[derive(Debug)]
pub enum ChannelGrantResult {
    Granted,
    AlreadyOwned,
    Denied { reason: String, current_owner: Option<ClientId> },
    Transferred { from: Option<ClientId> },
    PendingApproval { current_owner: Option<ClientId> },
}

pub struct ChannelRegistry {
    ownership: HashMap<u16, ChannelOwnership>,
}

impl ChannelRegistry {
    pub fn new() -> Self {
        Self { ownership: HashMap::new() }
    }

    /// Create a registry with default channel ownership assignments.
    pub fn with_defaults() -> Self {
        use prism_protocol::channel::*;
        let mut ownership = HashMap::new();
        // Exclusive
        for ch in [CHANNEL_DISPLAY, CHANNEL_INPUT, CHANNEL_CAMERA] {
            ownership.insert(ch, ChannelOwnership::Exclusive { owner: None });
        }
        // Shared
        for ch in [CHANNEL_CLIPBOARD, CHANNEL_CONTROL, CHANNEL_FILESHARE,
                    CHANNEL_AUDIO, CHANNEL_DEVICE, CHANNEL_NOTIFY, CHANNEL_SENSOR] {
            ownership.insert(ch, ChannelOwnership::Shared { subscribers: HashSet::new() });
        }
        // Transferable
        ownership.insert(CHANNEL_TOUCH, ChannelOwnership::Transferable {
            owner: None,
            transfer_policy: TransferPolicy::OnRequest,
        });
        Self { ownership }
    }

    pub fn request_channel(
        &mut self, channel_id: u16, client_id: ClientId,
    ) -> Result<ChannelGrantResult, SessionError> {
        match self.ownership.get_mut(&channel_id) {
            Some(ChannelOwnership::Exclusive { owner }) => {
                if owner.is_none() {
                    *owner = Some(client_id);
                    Ok(ChannelGrantResult::Granted)
                } else if *owner == Some(client_id) {
                    Ok(ChannelGrantResult::AlreadyOwned)
                } else {
                    Ok(ChannelGrantResult::Denied {
                        reason: "exclusively owned by another client".into(),
                        current_owner: *owner,
                    })
                }
            }
            Some(ChannelOwnership::Shared { subscribers }) => {
                subscribers.insert(client_id);
                Ok(ChannelGrantResult::Granted)
            }
            Some(ChannelOwnership::Transferable { owner, transfer_policy }) => {
                if owner.is_none() {
                    *owner = Some(client_id);
                    Ok(ChannelGrantResult::Granted)
                } else {
                    match transfer_policy {
                        TransferPolicy::OnRequest | TransferPolicy::ServerDecides => {
                            let old = *owner;
                            *owner = Some(client_id);
                            Ok(ChannelGrantResult::Transferred { from: old })
                        }
                        TransferPolicy::OwnerApproves => {
                            Ok(ChannelGrantResult::PendingApproval { current_owner: *owner })
                        }
                    }
                }
            }
            None if channel_id >= EXTENSION_CHANNEL_START => {
                self.ownership.insert(channel_id, ChannelOwnership::Shared {
                    subscribers: HashSet::from([client_id]),
                });
                Ok(ChannelGrantResult::Granted)
            }
            None => Err(SessionError::UnknownChannel(channel_id)),
        }
    }

    pub fn release_all(&mut self, client_id: ClientId) {
        for ownership in self.ownership.values_mut() {
            match ownership {
                ChannelOwnership::Exclusive { owner } if *owner == Some(client_id) => {
                    *owner = None;
                }
                ChannelOwnership::Shared { subscribers } => {
                    subscribers.remove(&client_id);
                }
                ChannelOwnership::Transferable { owner, .. } if *owner == Some(client_id) => {
                    *owner = None;
                }
                _ => {}
            }
        }
    }
}

impl Default for ChannelRegistry {
    fn default() -> Self { Self::with_defaults() }
}
```

- [ ] **Step 3: Run tests, commit**

```bash
git add crates/prism-session/src/channel.rs
git commit -m "feat(session): ChannelRegistry with Exclusive/Shared/Transferable ownership"
```

---

## Task 8: RoutingTable (ArcSwap)

**Files:**
- Modify: `crates/prism-session/src/routing.rs`

- [ ] **Step 1: Write failing tests**

```rust
use std::collections::HashMap;
use std::sync::Arc;
use arc_swap::ArcSwap;
use crate::types::ClientId;
use uuid::Uuid;

#[cfg(test)]
mod tests {
    use super::*;

    fn client_a() -> ClientId { Uuid::from_bytes([1; 16]) }
    fn client_b() -> ClientId { Uuid::from_bytes([2; 16]) }

    #[test]
    fn empty_routing_table() {
        let table = RoutingTable::new();
        let snap = table.snapshot();
        assert!(snap.channel_routes.is_empty());
        assert_eq!(snap.generation, 0);
    }

    #[test]
    fn batch_add_routes() {
        let table = RoutingTable::new();
        let entry = RouteEntry { client_id: client_a() };
        table.batch_update(vec![
            RoutingMutation::AddRoute { channel_id: 0x001, entry: entry.clone() },
            RoutingMutation::AddRoute { channel_id: 0x002, entry: entry.clone() },
        ]);
        let snap = table.snapshot();
        assert_eq!(snap.channel_routes.len(), 2);
        assert_eq!(snap.generation, 1); // single swap
    }

    #[test]
    fn remove_client_clears_all_routes() {
        let table = RoutingTable::new();
        let entry_a = RouteEntry { client_id: client_a() };
        let entry_b = RouteEntry { client_id: client_b() };
        table.batch_update(vec![
            RoutingMutation::AddRoute { channel_id: 0x001, entry: entry_a.clone() },
            RoutingMutation::AddRoute { channel_id: 0x001, entry: entry_b.clone() },
            RoutingMutation::AddRoute { channel_id: 0x002, entry: entry_a.clone() },
        ]);
        table.batch_update(vec![RoutingMutation::RemoveClient(client_a())]);
        let snap = table.snapshot();
        // Channel 0x001 should only have client_b
        assert_eq!(snap.channel_routes[&0x001].len(), 1);
        assert_eq!(snap.channel_routes[&0x001][0].client_id, client_b());
        // Channel 0x002 should be empty
        assert!(snap.channel_routes.get(&0x002).map_or(true, |r| r.is_empty()));
    }

    #[test]
    fn transfer_channel() {
        let table = RoutingTable::new();
        let entry_a = RouteEntry { client_id: client_a() };
        table.batch_update(vec![
            RoutingMutation::AddRoute { channel_id: 0x001, entry: entry_a },
        ]);
        let entry_b = RouteEntry { client_id: client_b() };
        table.batch_update(vec![
            RoutingMutation::TransferChannel {
                channel_id: 0x001,
                from: client_a(),
                to_entry: entry_b,
            },
        ]);
        let snap = table.snapshot();
        assert_eq!(snap.channel_routes[&0x001].len(), 1);
        assert_eq!(snap.channel_routes[&0x001][0].client_id, client_b());
    }

    #[test]
    fn generation_increments_per_batch() {
        let table = RoutingTable::new();
        assert_eq!(table.snapshot().generation, 0);
        table.batch_update(vec![]);
        assert_eq!(table.snapshot().generation, 1);
        table.batch_update(vec![]);
        assert_eq!(table.snapshot().generation, 2);
    }

    #[test]
    fn snapshot_is_consistent() {
        let table = RoutingTable::new();
        let entry = RouteEntry { client_id: client_a() };
        table.batch_update(vec![
            RoutingMutation::AddRoute { channel_id: 0x001, entry: entry.clone() },
        ]);
        let snap1 = table.snapshot();
        // Mutate table — snap1 should be unaffected
        table.batch_update(vec![RoutingMutation::RemoveClient(client_a())]);
        let snap2 = table.snapshot();
        assert_eq!(snap1.channel_routes[&0x001].len(), 1); // old snapshot preserved
        assert!(snap2.channel_routes.get(&0x001).map_or(true, |r| r.is_empty()));
    }
}
```

- [ ] **Step 2: Implement RoutingTable**

```rust
/// Immutable routing snapshot. Swapped atomically via ArcSwap.
#[derive(Debug, Clone)]
pub struct RoutingSnapshot {
    pub channel_routes: HashMap<u16, Vec<RouteEntry>>,
    pub generation: u64,
}

impl RoutingSnapshot {
    pub fn new() -> Self {
        Self { channel_routes: HashMap::new(), generation: 0 }
    }
}

impl Default for RoutingSnapshot {
    fn default() -> Self { Self::new() }
}

#[derive(Debug, Clone)]
pub struct RouteEntry {
    pub client_id: ClientId,
}

pub enum RoutingMutation {
    AddRoute { channel_id: u16, entry: RouteEntry },
    RemoveClient(ClientId),
    TransferChannel { channel_id: u16, from: ClientId, to_entry: RouteEntry },
}

/// Lock-free routing table. One atomic load per-frame read (~5ns).
pub struct RoutingTable {
    inner: ArcSwap<RoutingSnapshot>,
}

impl RoutingTable {
    pub fn new() -> Self {
        Self { inner: ArcSwap::from_pointee(RoutingSnapshot::new()) }
    }

    pub fn snapshot(&self) -> Arc<RoutingSnapshot> {
        self.inner.load_full()
    }

    pub fn batch_update(&self, mutations: Vec<RoutingMutation>) {
        let current = self.inner.load_full();
        let mut new_snapshot = (*current).clone();
        for mutation in mutations {
            match mutation {
                RoutingMutation::AddRoute { channel_id, entry } => {
                    new_snapshot.channel_routes.entry(channel_id).or_default().push(entry);
                }
                RoutingMutation::RemoveClient(client_id) => {
                    for routes in new_snapshot.channel_routes.values_mut() {
                        routes.retain(|r| r.client_id != client_id);
                    }
                }
                RoutingMutation::TransferChannel { channel_id, from, to_entry } => {
                    if let Some(routes) = new_snapshot.channel_routes.get_mut(&channel_id) {
                        routes.retain(|r| r.client_id != from);
                        routes.push(to_entry);
                    }
                }
            }
        }
        new_snapshot.generation += 1;
        self.inner.store(Arc::new(new_snapshot));
    }
}

impl Default for RoutingTable {
    fn default() -> Self { Self::new() }
}
```

- [ ] **Step 3: Run tests, commit**

```bash
git add crates/prism-session/src/routing.rs
git commit -m "feat(session): RoutingTable with ArcSwap lock-free reads + batch mutations"
```

---

## Task 9: TombstoneStore + ChannelRecoveryState

**Files:**
- Modify: `crates/prism-session/src/tombstone.rs`

- [ ] **Step 1: Write failing tests**

```rust
use std::collections::{HashMap, HashSet};
use crate::types::ClientId;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[cfg(test)]
mod tests {
    use super::*;

    fn client_a() -> ClientId { Uuid::from_bytes([1; 16]) }
    fn device_a() -> Uuid { Uuid::from_bytes([10; 16]) }

    #[test]
    fn create_and_claim_tombstone() {
        let mut store = TombstoneStore::new(300); // 5 min
        let tombstone = Tombstone::new(client_a(), device_a(), HashSet::from([0x001, 0x006]));
        store.insert(tombstone);
        assert_eq!(store.len(), 1);
        let claimed = store.claim_by_device(&device_a());
        assert!(claimed.is_some());
        assert_eq!(claimed.unwrap().client_id, client_a());
        assert_eq!(store.len(), 0); // consumed
    }

    #[test]
    fn claim_nonexistent_returns_none() {
        let store = TombstoneStore::new(300);
        assert!(store.claim_by_device(&device_a()).is_none());
    }

    #[test]
    fn expired_tombstones_garbage_collected() {
        let mut store = TombstoneStore::new(0); // immediate expiry
        let tombstone = Tombstone::new(client_a(), device_a(), HashSet::new());
        store.insert(tombstone);
        std::thread::sleep(std::time::Duration::from_millis(10));
        store.gc();
        assert_eq!(store.len(), 0);
    }

    #[test]
    fn channel_recovery_state_variants() {
        let states = vec![
            ChannelRecoveryState::SendIdr,
            ChannelRecoveryState::AudioReset,
            ChannelRecoveryState::NoRecovery,
        ];
        assert_eq!(states.len(), 3);
    }

    #[test]
    fn tombstone_json_roundtrip() {
        let tombstone = Tombstone::new(client_a(), device_a(), HashSet::from([0x001]));
        let json = serde_json::to_string(&tombstone).unwrap();
        let decoded: Tombstone = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.device_id, device_a());
    }
}
```

- [ ] **Step 2: Implement TombstoneStore**

```rust
/// Per-channel recovery strategy for reconnection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChannelRecoveryState {
    SendIdr,
    AudioReset,
    NoRecovery,
    ClipboardReplay,
    FileShareResume,
    CameraRenegotiate,
    NotificationReplay,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tombstone {
    pub client_id: ClientId,
    pub device_id: Uuid,
    pub created_at_secs: u64,
    pub subscribed_channels: HashSet<u16>,
    pub last_rtt_us: u64,
    pub last_bandwidth_bps: u64,
}

impl Tombstone {
    pub fn new(client_id: ClientId, device_id: Uuid, subscribed_channels: HashSet<u16>) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
        Self {
            client_id, device_id, created_at_secs: now,
            subscribed_channels, last_rtt_us: 0, last_bandwidth_bps: 0,
        }
    }

    pub fn is_expired(&self, max_age_secs: u64) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
        now.saturating_sub(self.created_at_secs) > max_age_secs
    }
}

pub struct TombstoneStore {
    tombstones: HashMap<Uuid, Tombstone>, // keyed by device_id
    max_age_secs: u64,
}

impl TombstoneStore {
    pub fn new(max_age_secs: u64) -> Self {
        Self { tombstones: HashMap::new(), max_age_secs }
    }

    pub fn insert(&mut self, tombstone: Tombstone) {
        self.tombstones.insert(tombstone.device_id, tombstone);
    }

    pub fn claim_by_device(&mut self, device_id: &Uuid) -> Option<Tombstone> {
        let ts = self.tombstones.remove(device_id)?;
        if ts.is_expired(self.max_age_secs) { return None; }
        Some(ts)
    }

    pub fn len(&self) -> usize { self.tombstones.len() }
    pub fn is_empty(&self) -> bool { self.tombstones.is_empty() }

    pub fn gc(&mut self) {
        self.tombstones.retain(|_, ts| !ts.is_expired(self.max_age_secs));
    }

    pub fn persist(&self, path: &std::path::Path) -> Result<(), std::io::Error> {
        let data = serde_json::to_vec(&self.tombstones)?;
        std::fs::write(path, data)
    }

    pub fn restore(path: &std::path::Path, max_age_secs: u64) -> Result<Self, std::io::Error> {
        if !path.exists() { return Ok(Self::new(max_age_secs)); }
        let data = std::fs::read(path)?;
        let tombstones: HashMap<Uuid, Tombstone> = serde_json::from_slice(&data)?;
        let mut store = Self { tombstones, max_age_secs };
        store.gc();
        Ok(store)
    }
}
```

- [ ] **Step 3: Run tests, commit**

```bash
git add crates/prism-session/src/tombstone.rs
git commit -m "feat(session): TombstoneStore with device-keyed lookup, GC, and persistence"
```

---

## Task 10: HeartbeatMonitor

**Files:**
- Modify: `crates/prism-session/src/heartbeat.rs`

- [ ] **Step 1: Write failing tests**

```rust
use std::collections::HashMap;
use std::time::{Duration, Instant};
use crate::types::ClientId;
use uuid::Uuid;

#[cfg(test)]
mod tests {
    use super::*;

    fn client_a() -> ClientId { Uuid::from_bytes([1; 16]) }

    #[test]
    fn new_client_is_alive() {
        let mut hb = HeartbeatMonitor::new(Duration::from_secs(10), Duration::from_secs(60));
        hb.register(client_a());
        assert!(!hb.needs_suspend(client_a()));
        assert!(!hb.needs_tombstone(client_a()));
    }

    #[test]
    fn activity_resets_timer() {
        let mut hb = HeartbeatMonitor::new(Duration::from_millis(10), Duration::from_millis(50));
        hb.register(client_a());
        std::thread::sleep(Duration::from_millis(8));
        hb.activity(client_a());
        std::thread::sleep(Duration::from_millis(8));
        assert!(!hb.needs_suspend(client_a())); // reset saved us
    }

    #[test]
    fn needs_suspend_after_threshold() {
        let mut hb = HeartbeatMonitor::new(Duration::from_millis(10), Duration::from_millis(100));
        hb.register(client_a());
        std::thread::sleep(Duration::from_millis(15));
        assert!(hb.needs_suspend(client_a()));
    }

    #[test]
    fn needs_tombstone_after_threshold() {
        let mut hb = HeartbeatMonitor::new(Duration::from_millis(5), Duration::from_millis(15));
        hb.register(client_a());
        std::thread::sleep(Duration::from_millis(20));
        assert!(hb.needs_tombstone(client_a()));
    }

    #[test]
    fn unregister_removes_client() {
        let mut hb = HeartbeatMonitor::new(Duration::from_secs(10), Duration::from_secs(60));
        hb.register(client_a());
        hb.unregister(client_a());
        assert!(!hb.needs_suspend(client_a())); // unknown client = false
    }
}
```

- [ ] **Step 2: Implement HeartbeatMonitor**

```rust
struct HeartbeatState {
    last_activity: Instant,
}

pub struct HeartbeatMonitor {
    clients: HashMap<ClientId, HeartbeatState>,
    suspend_threshold: Duration,
    tombstone_threshold: Duration,
}

impl HeartbeatMonitor {
    pub fn new(suspend_threshold: Duration, tombstone_threshold: Duration) -> Self {
        Self { clients: HashMap::new(), suspend_threshold, tombstone_threshold }
    }

    pub fn register(&mut self, client_id: ClientId) {
        self.clients.insert(client_id, HeartbeatState { last_activity: Instant::now() });
    }

    pub fn unregister(&mut self, client_id: ClientId) {
        self.clients.remove(&client_id);
    }

    pub fn activity(&mut self, client_id: ClientId) {
        if let Some(state) = self.clients.get_mut(&client_id) {
            state.last_activity = Instant::now();
        }
    }

    pub fn needs_suspend(&self, client_id: ClientId) -> bool {
        self.clients.get(&client_id)
            .is_some_and(|s| s.last_activity.elapsed() > self.suspend_threshold)
    }

    pub fn needs_tombstone(&self, client_id: ClientId) -> bool {
        self.clients.get(&client_id)
            .is_some_and(|s| s.last_activity.elapsed() > self.tombstone_threshold)
    }

    pub fn stale_clients(&self) -> Vec<(ClientId, bool, bool)> {
        self.clients.iter().map(|(&id, state)| {
            let elapsed = state.last_activity.elapsed();
            (id, elapsed > self.suspend_threshold, elapsed > self.tombstone_threshold)
        }).filter(|(_, suspend, _)| *suspend).collect()
    }
}
```

- [ ] **Step 3: Run tests, commit**

```bash
git add crates/prism-session/src/heartbeat.rs
git commit -m "feat(session): HeartbeatMonitor with suspend/tombstone thresholds"
```

---

## Task 11: ConnectionProfile + CapabilityNegotiator

**Files:**
- Modify: `crates/prism-session/src/profiles.rs`
- Modify: `crates/prism-session/src/negotiation.rs`

- [ ] **Step 1: Write failing tests for profiles**

`profiles.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gaming_profile_defaults() {
        let p = ConnectionProfile::gaming();
        assert_eq!(p.display.max_fps, 120);
        assert!(!p.display.prefer_lossless_text);
        assert!(!p.display.region_detection);
    }

    #[test]
    fn coding_profile_defaults() {
        let p = ConnectionProfile::coding();
        assert_eq!(p.display.max_fps, 60);
        assert!(p.display.prefer_lossless_text);
        assert!(p.display.region_detection);
    }
}
```

- [ ] **Step 2: Implement ConnectionProfile**

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EncoderPreset {
    UltraLowLatency,
    Quality,
    Balanced,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisplayProfile {
    pub prefer_lossless_text: bool,
    pub max_fps: u8,
    pub region_detection: bool,
    pub encoder_preset: EncoderPreset,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionProfile {
    pub name: String,
    pub display: DisplayProfile,
}

impl ConnectionProfile {
    pub fn gaming() -> Self {
        Self {
            name: "Gaming".into(),
            display: DisplayProfile {
                prefer_lossless_text: false,
                max_fps: 120,
                region_detection: false,
                encoder_preset: EncoderPreset::UltraLowLatency,
            },
        }
    }

    pub fn coding() -> Self {
        Self {
            name: "Coding".into(),
            display: DisplayProfile {
                prefer_lossless_text: true,
                max_fps: 60,
                region_detection: true,
                encoder_preset: EncoderPreset::Quality,
            },
        }
    }
}
```

- [ ] **Step 3: Write failing tests for CapabilityNegotiator**

`negotiation.rs`:
```rust
use std::collections::{HashMap, HashSet};
use prism_protocol::capability::*;
use prism_protocol::channel::*;

#[cfg(test)]
mod tests {
    use super::*;

    fn server_negotiator() -> CapabilityNegotiator {
        let mut server_channels = HashMap::new();
        server_channels.insert(CHANNEL_DISPLAY, ChannelCap {
            channel_id: CHANNEL_DISPLAY, channel_version: 1,
            config: ChannelConfig::Display(DisplayChannelConfig {
                max_resolution: (3840, 2160), max_fps: 60,
                supported_codecs: vec!["h264".into(), "h265".into()],
            }),
        });
        server_channels.insert(CHANNEL_INPUT, ChannelCap {
            channel_id: CHANNEL_INPUT, channel_version: 1,
            config: ChannelConfig::Input(InputChannelConfig { devices: vec!["keyboard".into(), "mouse".into()] }),
        });
        server_channels.insert(CHANNEL_CONTROL, ChannelCap {
            channel_id: CHANNEL_CONTROL, channel_version: 1,
            config: ChannelConfig::Control,
        });
        CapabilityNegotiator { server_channels }
    }

    #[test]
    fn matching_channels_negotiated() {
        let neg = server_negotiator();
        let client = ClientCapabilities {
            protocol_version: 1,
            channels: vec![
                ChannelCap { channel_id: CHANNEL_DISPLAY, channel_version: 1, config: ChannelConfig::Generic },
                ChannelCap { channel_id: CHANNEL_INPUT, channel_version: 1, config: ChannelConfig::Generic },
            ],
            performance: PerformanceProfile::default(),
        };
        let result = neg.negotiate(&client);
        assert_eq!(result.channels.len(), 2);
        assert!(result.rejected_channels.is_empty());
    }

    #[test]
    fn unmatched_channels_rejected() {
        let neg = server_negotiator();
        let client = ClientCapabilities {
            protocol_version: 1,
            channels: vec![
                ChannelCap { channel_id: CHANNEL_DISPLAY, channel_version: 1, config: ChannelConfig::Generic },
                ChannelCap { channel_id: 0x0E1, channel_version: 1, config: ChannelConfig::Generic }, // Notify - not on server
            ],
            performance: PerformanceProfile::default(),
        };
        let result = neg.negotiate(&client);
        assert_eq!(result.channels.len(), 1);
        assert_eq!(result.rejected_channels, vec![0x0E1]);
    }

    #[test]
    fn version_min_selected() {
        let mut neg = server_negotiator();
        // Server has version 2 for Display
        neg.server_channels.get_mut(&CHANNEL_DISPLAY).unwrap().channel_version = 2;
        let client = ClientCapabilities {
            protocol_version: 1,
            channels: vec![
                ChannelCap { channel_id: CHANNEL_DISPLAY, channel_version: 1, config: ChannelConfig::Generic },
            ],
            performance: PerformanceProfile::default(),
        };
        let result = neg.negotiate(&client);
        assert_eq!(result.channels[0].version, 1); // min(1, 2)
    }

    #[test]
    fn codec_priority_h265_preferred() {
        let neg = server_negotiator();
        let client = ClientCapabilities {
            protocol_version: 1,
            channels: vec![],
            performance: PerformanceProfile {
                supported_codecs: vec!["h264".into(), "h265".into()],
                ..PerformanceProfile::default()
            },
        };
        let result = neg.negotiate(&client);
        assert_eq!(result.display_codec, "h265");
    }

    #[test]
    fn codec_fallback_to_h264() {
        let neg = server_negotiator();
        let client = ClientCapabilities {
            protocol_version: 1,
            channels: vec![],
            performance: PerformanceProfile {
                supported_codecs: vec!["h264".into()], // no h265
                ..PerformanceProfile::default()
            },
        };
        let result = neg.negotiate(&client);
        assert_eq!(result.display_codec, "h264");
    }
}
```

- [ ] **Step 4: Implement CapabilityNegotiator**

```rust
pub struct NegotiatedChannel {
    pub channel_id: u16,
    pub version: u16,
}

pub struct NegotiationResult {
    pub protocol_version: u16,
    pub channels: Vec<NegotiatedChannel>,
    pub rejected_channels: Vec<u16>,
    pub display_codec: String,
}

pub struct CapabilityNegotiator {
    pub server_channels: HashMap<u16, ChannelCap>,
}

impl CapabilityNegotiator {
    pub fn negotiate(&self, client_caps: &ClientCapabilities) -> NegotiationResult {
        let mut granted = Vec::new();
        let mut rejected = Vec::new();

        for client_ch in &client_caps.channels {
            match self.server_channels.get(&client_ch.channel_id) {
                Some(server_ch) => {
                    granted.push(NegotiatedChannel {
                        channel_id: client_ch.channel_id,
                        version: client_ch.channel_version.min(server_ch.channel_version),
                    });
                }
                None => rejected.push(client_ch.channel_id),
            }
        }

        let display_codec = self.negotiate_codec(client_caps);

        NegotiationResult {
            protocol_version: client_caps.protocol_version.min(1),
            channels: granted,
            rejected_channels: rejected,
            display_codec,
        }
    }

    fn negotiate_codec(&self, client: &ClientCapabilities) -> String {
        let client_codecs: HashSet<_> = client.performance.supported_codecs.iter().cloned().collect();
        let server_codecs = self.server_channels.get(&CHANNEL_DISPLAY)
            .and_then(|c| match &c.config {
                ChannelConfig::Display(d) => Some(&d.supported_codecs),
                _ => None,
            });

        for codec in ["h265", "h264", "av1"] {
            if client_codecs.contains(codec) {
                if let Some(server) = server_codecs {
                    if server.iter().any(|c| c == codec) {
                        return codec.to_string();
                    }
                }
            }
        }
        "h264".to_string()
    }
}
```

- [ ] **Step 5: Run all tests, commit**

```bash
git add crates/prism-session/src/profiles.rs crates/prism-session/src/negotiation.rs
git commit -m "feat(session): ConnectionProfile (Gaming/Coding) + CapabilityNegotiator"
```

---

## Task 12: AllocationHandle + BandwidthNeeds + ChannelBandwidthTracker

**Files:**
- Modify: `crates/prism-session/src/tracker.rs`
- Modify: `crates/prism-session/src/arbiter.rs` (partial — just types)

- [ ] **Step 1: Write failing tests for ChannelBandwidthTracker**

`tracker.rs`:
```rust
use std::sync::atomic::{AtomicU64, Ordering};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_send_per_channel() {
        let tracker = ChannelBandwidthTracker::new();
        tracker.record_send(0x001, 1000);
        tracker.record_send(0x001, 2000);
        tracker.record_send(0x002, 500);
        assert_eq!(tracker.send_bytes(0x001), 3000);
        assert_eq!(tracker.send_bytes(0x002), 500);
    }

    #[test]
    fn record_recv_per_channel() {
        let tracker = ChannelBandwidthTracker::new();
        tracker.record_recv(0x001, 5000);
        assert_eq!(tracker.recv_bytes(0x001), 5000);
    }

    #[test]
    fn reset_clears_counters() {
        let tracker = ChannelBandwidthTracker::new();
        tracker.record_send(0x001, 1000);
        tracker.reset();
        assert_eq!(tracker.send_bytes(0x001), 0);
    }

    #[test]
    fn channel_index_wraps_correctly() {
        let tracker = ChannelBandwidthTracker::new();
        // Core channel 0x001 and extension channel 0x101 share index (0x01)
        // but this is expected — no collision between 0x01-0x07 and 0xE1-0xE4
        tracker.record_send(0x001, 100);
        assert_eq!(tracker.send_bytes(0x001), 100);
    }
}
```

- [ ] **Step 2: Implement ChannelBandwidthTracker**

```rust
/// Per-channel atomic bandwidth counters. 256 buckets indexed by channel_id & 0xFF.
/// One fetch_add per packet (~1ns). No collisions between core (0x01-0x07) and mobile (0xE1-0xE4) channels.
pub struct ChannelBandwidthTracker {
    send_counters: [AtomicU64; 256],
    recv_counters: [AtomicU64; 256],
}

impl ChannelBandwidthTracker {
    pub fn new() -> Self {
        Self {
            send_counters: std::array::from_fn(|_| AtomicU64::new(0)),
            recv_counters: std::array::from_fn(|_| AtomicU64::new(0)),
        }
    }

    #[inline(always)]
    pub fn record_send(&self, channel_id: u16, bytes: u32) {
        self.send_counters[(channel_id & 0xFF) as usize]
            .fetch_add(bytes as u64, Ordering::Relaxed);
    }

    #[inline(always)]
    pub fn record_recv(&self, channel_id: u16, bytes: u32) {
        self.recv_counters[(channel_id & 0xFF) as usize]
            .fetch_add(bytes as u64, Ordering::Relaxed);
    }

    pub fn send_bytes(&self, channel_id: u16) -> u64 {
        self.send_counters[(channel_id & 0xFF) as usize].load(Ordering::Relaxed)
    }

    pub fn recv_bytes(&self, channel_id: u16) -> u64 {
        self.recv_counters[(channel_id & 0xFF) as usize].load(Ordering::Relaxed)
    }

    pub fn reset(&self) {
        for c in &self.send_counters { c.store(0, Ordering::Relaxed); }
        for c in &self.recv_counters { c.store(0, Ordering::Relaxed); }
    }
}

impl Default for ChannelBandwidthTracker {
    fn default() -> Self { Self::new() }
}
```

- [ ] **Step 3: Write failing tests for AllocationHandle + BandwidthNeeds**

Add to `arbiter.rs`:

```rust
use std::sync::atomic::{AtomicU64, Ordering};
use crate::types::ClientId;
use prism_protocol::channel::ChannelPriority;

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn allocation_handle_atomic_read() {
        let handle = AllocationHandle::new(1_000_000, 500_000, 5_000_000);
        assert_eq!(handle.allocated_bps(), 1_000_000);
        assert_eq!(handle.min_bps(), 500_000);
        assert_eq!(handle.max_bps(), 5_000_000);
    }

    #[test]
    fn allocation_handle_update() {
        let handle = AllocationHandle::new(1_000_000, 500_000, 5_000_000);
        handle.set_allocated(2_000_000);
        assert_eq!(handle.allocated_bps(), 2_000_000);
    }

    #[test]
    fn bandwidth_needs_defaults_to_zero() {
        let needs = BandwidthNeeds::default();
        assert_eq!(needs.min_bps, 0);
        assert_eq!(needs.ideal_bps, 0);
        assert_eq!(needs.max_bps, 0);
    }
}
```

- [ ] **Step 4: Implement AllocationHandle + BandwidthNeeds**

```rust
/// Zero-cost allocation handle. Producers read via atomic load (~1ns).
pub struct AllocationHandle {
    allocated_bps: AtomicU64,
    min_bps: AtomicU64,
    max_bps: AtomicU64,
}

impl AllocationHandle {
    pub fn new(allocated: u64, min: u64, max: u64) -> Self {
        Self {
            allocated_bps: AtomicU64::new(allocated),
            min_bps: AtomicU64::new(min),
            max_bps: AtomicU64::new(max),
        }
    }

    #[inline(always)]
    pub fn allocated_bps(&self) -> u64 {
        self.allocated_bps.load(Ordering::Relaxed)
    }

    pub fn min_bps(&self) -> u64 {
        self.min_bps.load(Ordering::Relaxed)
    }

    pub fn max_bps(&self) -> u64 {
        self.max_bps.load(Ordering::Relaxed)
    }

    pub fn set_allocated(&self, bps: u64) {
        self.allocated_bps.store(bps, Ordering::Relaxed);
    }
}

/// Dynamic bandwidth needs reported by channel handlers.
#[derive(Debug, Clone, Copy, Default)]
pub struct BandwidthNeeds {
    pub min_bps: u64,
    pub ideal_bps: u64,
    pub max_bps: u64,
    pub urgency: f32,
}
```

- [ ] **Step 5: Run tests, commit**

```bash
git add crates/prism-session/src/tracker.rs crates/prism-session/src/arbiter.rs
git commit -m "feat(session): ChannelBandwidthTracker (atomic), AllocationHandle, BandwidthNeeds"
```

---

## Task 13: BandwidthArbiter + StarvationDetector

**Files:**
- Modify: `crates/prism-session/src/arbiter.rs`

- [ ] **Step 1: Write failing tests**

Append to arbiter.rs tests:

```rust
    fn client_a() -> ClientId { Uuid::from_bytes([1; 16]) }

    fn test_arbiter(total_bps: u64) -> BandwidthArbiter {
        BandwidthArbiter::new(total_bps)
    }

    #[test]
    fn single_channel_gets_all_bandwidth() {
        let mut arbiter = test_arbiter(10_000_000);
        arbiter.add_channel(client_a(), 0x001, ChannelPriority::High,
            BandwidthNeeds { min_bps: 500_000, ideal_bps: 5_000_000, max_bps: 10_000_000, urgency: 0.0 });
        arbiter.rebalance();
        let alloc = arbiter.allocation(client_a(), 0x001).unwrap();
        assert!(alloc >= 5_000_000, "single channel should get at least ideal: {}", alloc);
    }

    #[test]
    fn min_guarantees_satisfied() {
        let mut arbiter = test_arbiter(1_000_000); // tight budget
        arbiter.add_channel(client_a(), 0x001, ChannelPriority::High,
            BandwidthNeeds { min_bps: 400_000, ideal_bps: 2_000_000, max_bps: 5_000_000, urgency: 0.0 });
        arbiter.add_channel(client_a(), 0x002, ChannelPriority::Critical,
            BandwidthNeeds { min_bps: 100_000, ideal_bps: 500_000, max_bps: 1_000_000, urgency: 0.0 });
        arbiter.rebalance();
        assert!(arbiter.allocation(client_a(), 0x001).unwrap() >= 400_000);
        assert!(arbiter.allocation(client_a(), 0x002).unwrap() >= 100_000);
    }

    #[test]
    fn priority_weighting_higher_gets_more() {
        let mut arbiter = test_arbiter(1_000_000);
        arbiter.add_channel(client_a(), 0x001, ChannelPriority::High,   // weight 8
            BandwidthNeeds { min_bps: 0, ideal_bps: 1_000_000, max_bps: 1_000_000, urgency: 0.0 });
        arbiter.add_channel(client_a(), 0x007, ChannelPriority::Low,     // weight 2
            BandwidthNeeds { min_bps: 0, ideal_bps: 1_000_000, max_bps: 1_000_000, urgency: 0.0 });
        arbiter.rebalance();
        let high = arbiter.allocation(client_a(), 0x001).unwrap();
        let low = arbiter.allocation(client_a(), 0x007).unwrap();
        assert!(high > low, "High priority ({}) should get more than Low ({})", high, low);
    }

    #[test]
    fn remove_channel_frees_bandwidth() {
        let mut arbiter = test_arbiter(1_000_000);
        arbiter.add_channel(client_a(), 0x001, ChannelPriority::High,
            BandwidthNeeds { min_bps: 0, ideal_bps: 500_000, max_bps: 1_000_000, urgency: 0.0 });
        arbiter.add_channel(client_a(), 0x002, ChannelPriority::Critical,
            BandwidthNeeds { min_bps: 0, ideal_bps: 500_000, max_bps: 1_000_000, urgency: 0.0 });
        arbiter.rebalance();
        arbiter.remove_client(client_a());
        assert!(arbiter.allocation(client_a(), 0x001).is_none());
    }

    #[test]
    fn starvation_detected() {
        let mut detector = StarvationDetector::new(5);
        // Channel allocated 1Mbps but using 0 for 6 ticks
        for _ in 0..6 {
            detector.update(0x001, 1_000_000, 0);
        }
        let warnings = detector.check();
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].channel_id, 0x001);
    }

    #[test]
    fn starvation_clears_when_usage_recovers() {
        let mut detector = StarvationDetector::new(3);
        for _ in 0..4 { detector.update(0x001, 1_000_000, 0); }
        assert!(!detector.check().is_empty());
        // Usage recovers
        detector.update(0x001, 1_000_000, 500_000);
        assert!(detector.check().is_empty());
    }
```

- [ ] **Step 2: Implement BandwidthArbiter + StarvationDetector**

Add to `arbiter.rs`:

```rust
use std::collections::HashMap;

struct ChannelEntry {
    priority: ChannelPriority,
    needs: BandwidthNeeds,
    allocated_bps: u64,
}

/// Priority-weighted proportional bandwidth allocator.
pub struct BandwidthArbiter {
    channels: HashMap<(ClientId, u16), ChannelEntry>,
    total_bps: u64,
}

impl BandwidthArbiter {
    pub fn new(total_bps: u64) -> Self {
        Self { channels: HashMap::new(), total_bps }
    }

    pub fn add_channel(
        &mut self, client_id: ClientId, channel_id: u16,
        priority: ChannelPriority, needs: BandwidthNeeds,
    ) {
        self.channels.insert((client_id, channel_id), ChannelEntry {
            priority, needs, allocated_bps: 0,
        });
    }

    pub fn remove_client(&mut self, client_id: ClientId) {
        self.channels.retain(|(cid, _), _| *cid != client_id);
    }

    pub fn allocation(&self, client_id: ClientId, channel_id: u16) -> Option<u64> {
        self.channels.get(&(client_id, channel_id)).map(|e| e.allocated_bps)
    }

    /// Rebalance allocations using priority-weighted proportional allocation.
    pub fn rebalance(&mut self) {
        if self.channels.is_empty() { return; }

        // Step 1: satisfy minimums
        let total_min: u64 = self.channels.values().map(|e| e.needs.min_bps).sum();
        let mut remaining = self.total_bps.saturating_sub(total_min);

        for entry in self.channels.values_mut() {
            entry.allocated_bps = entry.needs.min_bps;
        }

        // Step 2: distribute remainder by priority weight, capped at max
        let total_weight: u64 = self.channels.values()
            .map(|e| prism_protocol::channel::priority_weight(e.priority) as u64)
            .sum();

        if total_weight > 0 && remaining > 0 {
            let entries: Vec<(ClientId, u16, u64, u64)> = self.channels.iter()
                .map(|(&(cid, ch), e)| {
                    let weight = prism_protocol::channel::priority_weight(e.priority) as u64;
                    let headroom = e.needs.max_bps.saturating_sub(e.needs.min_bps);
                    (cid, ch, weight, headroom)
                }).collect();

            for (cid, ch, weight, headroom) in entries {
                let share = (remaining * weight / total_weight).min(headroom);
                if let Some(entry) = self.channels.get_mut(&(cid, ch)) {
                    entry.allocated_bps += share;
                }
            }
        }
    }
}

/// Starvation warning when a channel uses less than its minimum for too long.
#[derive(Debug, Clone)]
pub struct StarvationWarning {
    pub channel_id: u16,
    pub allocated_bps: u64,
    pub actual_bps: u64,
    pub starved_ticks: u32,
}

/// Detects per-channel starvation (actual < min while allocated >= min).
pub struct StarvationDetector {
    channels: HashMap<u16, u32>, // channel_id → consecutive starved ticks
    threshold_ticks: u32,
}

impl StarvationDetector {
    pub fn new(threshold_ticks: u32) -> Self {
        Self { channels: HashMap::new(), threshold_ticks }
    }

    pub fn update(&mut self, channel_id: u16, allocated_bps: u64, actual_bps: u64) {
        if actual_bps < allocated_bps / 2 && allocated_bps > 0 {
            *self.channels.entry(channel_id).or_insert(0) += 1;
        } else {
            self.channels.remove(&channel_id);
        }
    }

    pub fn check(&self) -> Vec<StarvationWarning> {
        self.channels.iter()
            .filter(|(_, &ticks)| ticks > self.threshold_ticks)
            .map(|(&channel_id, &starved_ticks)| StarvationWarning {
                channel_id, allocated_bps: 0, actual_bps: 0, starved_ticks,
            })
            .collect()
    }
}
```

- [ ] **Step 3: Run tests, commit**

```bash
git add crates/prism-session/src/arbiter.rs
git commit -m "feat(session): BandwidthArbiter priority-weighted allocation + StarvationDetector"
```

---

## Task 14: ChannelHandler Trait + ChannelDispatcher

**Files:**
- Modify: `crates/prism-session/src/dispatch.rs`

- [ ] **Step 1: Write failing tests**

```rust
use std::collections::HashMap;
use std::sync::Arc;
use async_trait::async_trait;
use bytes::Bytes;
use crate::types::ClientId;
use crate::routing::RoutingTable;
use prism_protocol::channel::PrismPacket;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum ChannelError {
    #[error("handler error: {0}")]
    HandlerError(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    struct MockHandler {
        id: u16,
        call_count: AtomicU32,
    }

    impl MockHandler {
        fn new(id: u16) -> Self { Self { id, call_count: AtomicU32::new(0) } }
        fn calls(&self) -> u32 { self.call_count.load(Ordering::Relaxed) }
    }

    #[async_trait]
    impl ChannelHandler for MockHandler {
        fn channel_id(&self) -> u16 { self.id }
        async fn handle_datagram(&self, _from: ClientId, _data: Bytes) -> Result<(), ChannelError> {
            self.call_count.fetch_add(1, Ordering::Relaxed);
            Ok(())
        }
    }

    #[test]
    fn register_and_lookup() {
        let mut dispatcher = ChannelDispatcher::new();
        let handler = Arc::new(MockHandler::new(0x001));
        dispatcher.register(handler.clone());
        assert!(dispatcher.handler(0x001).is_some());
        assert!(dispatcher.handler(0x002).is_none());
    }

    #[tokio::test]
    async fn dispatch_routes_to_handler() {
        let mut dispatcher = ChannelDispatcher::new();
        let handler = Arc::new(MockHandler::new(0x001));
        dispatcher.register(handler.clone());
        let client = Uuid::from_bytes([1; 16]);
        dispatcher.dispatch(client, 0x001, Bytes::from_static(b"data")).await;
        assert_eq!(handler.calls(), 1);
    }

    #[tokio::test]
    async fn dispatch_unknown_channel_ignored() {
        let dispatcher = ChannelDispatcher::new();
        let client = Uuid::from_bytes([1; 16]);
        // Should not panic
        dispatcher.dispatch(client, 0x099, Bytes::from_static(b"data")).await;
    }

    #[test]
    fn register_multiple_handlers() {
        let mut dispatcher = ChannelDispatcher::new();
        dispatcher.register(Arc::new(MockHandler::new(0x001)));
        dispatcher.register(Arc::new(MockHandler::new(0x002)));
        dispatcher.register(Arc::new(MockHandler::new(0x003)));
        assert!(dispatcher.handler(0x001).is_some());
        assert!(dispatcher.handler(0x002).is_some());
        assert!(dispatcher.handler(0x003).is_some());
    }
}
```

- [ ] **Step 2: Implement ChannelHandler trait + ChannelDispatcher**

```rust
/// Channel handler trait. Subsystems (Display, Input, etc.) implement this.
#[async_trait]
pub trait ChannelHandler: Send + Sync {
    fn channel_id(&self) -> u16;
    async fn handle_datagram(&self, from: ClientId, data: Bytes) -> Result<(), ChannelError>;
}

/// Routes incoming messages to registered channel handlers.
pub struct ChannelDispatcher {
    handlers: HashMap<u16, Arc<dyn ChannelHandler>>,
}

impl ChannelDispatcher {
    pub fn new() -> Self {
        Self { handlers: HashMap::new() }
    }

    pub fn register(&mut self, handler: Arc<dyn ChannelHandler>) {
        self.handlers.insert(handler.channel_id(), handler);
    }

    pub fn handler(&self, channel_id: u16) -> Option<&Arc<dyn ChannelHandler>> {
        self.handlers.get(&channel_id)
    }

    pub async fn dispatch(&self, from: ClientId, channel_id: u16, data: Bytes) {
        if let Some(handler) = self.handlers.get(&channel_id) {
            if let Err(e) = handler.handle_datagram(from, data).await {
                // Log error but don't propagate — one bad handler shouldn't kill the recv loop
                eprintln!("Channel 0x{:03X} handler error: {}", channel_id, e);
            }
        }
    }
}

impl Default for ChannelDispatcher {
    fn default() -> Self { Self::new() }
}
```

- [ ] **Step 3: Update lib.rs with all re-exports**

```rust
pub mod error;
pub mod types;
pub mod control_msg;
pub mod channel;
pub mod routing;
pub mod tombstone;
pub mod heartbeat;
pub mod profiles;
pub mod negotiation;
pub mod dispatch;
pub mod arbiter;
pub mod tracker;

pub use error::SessionError;
pub use types::{ClientId, SessionState, SessionEvent, ArbiterEvent};
pub use channel::{ChannelRegistry, ChannelOwnership, ChannelGrantResult, TransferPolicy};
pub use routing::{RoutingTable, RoutingSnapshot, RouteEntry, RoutingMutation};
pub use tombstone::{TombstoneStore, Tombstone, ChannelRecoveryState};
pub use heartbeat::HeartbeatMonitor;
pub use profiles::{ConnectionProfile, DisplayProfile, EncoderPreset};
pub use negotiation::{CapabilityNegotiator, NegotiationResult, NegotiatedChannel};
pub use dispatch::{ChannelHandler, ChannelDispatcher, ChannelError};
pub use arbiter::{BandwidthArbiter, AllocationHandle, BandwidthNeeds, StarvationDetector, StarvationWarning};
pub use tracker::ChannelBandwidthTracker;
```

- [ ] **Step 4: Run all tests**

Run: `cargo test -p prism-session`
Expected: all tests pass

Run: `cargo test --workspace`
Expected: all crates pass

- [ ] **Step 5: Commit**

```bash
git add crates/prism-session/src/dispatch.rs crates/prism-session/src/lib.rs
git commit -m "feat(session): ChannelHandler trait + ChannelDispatcher"
```

---

## Summary

| Task | Component | Crate | Tests |
|------|-----------|-------|-------|
| 1 | FrameTrace + FrameLatencyBreakdown | prism-observability | 2 |
| 2 | FrameTracer adaptive sampling | prism-observability | 4 |
| 3 | ClientFeedback + ClientAlert + OverlayPacket | prism-observability | 4 |
| 4 | MetricsTimeSeries + TimeSeriesRing | prism-observability | 5 |
| 5 | SessionError + SessionState + SessionEvent | prism-session | 3 |
| 6 | Control message types + ShutdownNotice | prism-session | 2 |
| 7 | ChannelRegistry + ChannelOwnership | prism-session | 8 |
| 8 | RoutingTable (ArcSwap) + RoutingMutation | prism-session | 6 |
| 9 | TombstoneStore + ChannelRecoveryState | prism-session | 5 |
| 10 | HeartbeatMonitor | prism-session | 5 |
| 11 | ConnectionProfile + CapabilityNegotiator | prism-session | 7 |
| 12 | AllocationHandle + BandwidthNeeds + ChannelBandwidthTracker | prism-session | 7 |
| 13 | BandwidthArbiter + StarvationDetector | prism-session | 7 |
| 14 | ChannelHandler trait + ChannelDispatcher | prism-session | 4 |
| **Total** | | | **~69** |

**Deferred to Plan 5 (Display Engine):** Display Engine capture/classify/encode pipeline, region classification, degradation ladder.

**Deferred to Plan 6 (Integration):** SessionManager orchestrator struct, recv loop, connection lifecycle state machine, QuicTransportServer/Client, security+transport+session wiring, end-to-end integration tests.

**Not in Phase 1:** Multi-client fairness (Phase 3), Transferable active transfers (Phase 3), Media/Mobile/Companion profiles (Phase 2-3), Prometheus export (optional).
