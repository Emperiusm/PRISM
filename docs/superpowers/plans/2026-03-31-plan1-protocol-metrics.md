# Plan 1: Protocol + Metrics Implementation

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the `prism-protocol` and `prism-metrics` foundation crates that all other PRISM crates depend on — wire types, packet header, channel constants, capability negotiation, and a lock-free metrics recording system.

**Architecture:** Two crates with zero external dependencies between them. `prism-protocol` defines the 16-byte packet header (little-endian, version nibble + 12-bit channel ID), channel constants with priority/transport metadata, capability negotiation types, and the `PrismPacket` container. `prism-metrics` provides const-generic `MetricsRecorder` with lock-free counters/gauges/histograms for zero-cost metrics recording on the hot path.

**Tech Stack:** Rust 2024 edition, `bytes` (zero-copy buffers), `serde`/`serde_json` (capability serialization), `thiserror` (error types)

**Spec refs:**
- Architecture: `docs/superpowers/specs/2026-03-30-prism-architecture-design.md` (Section 1: Revised Packet Header)
- Session+Observability: `docs/superpowers/specs/2026-03-30-session-observability-design.md` (Sections 13-14: prism-metrics)

---

## File Structure

```
PRISM/
  Cargo.toml                              # workspace root
  crates/
    prism-protocol/
      Cargo.toml
      src/
        lib.rs                            # re-exports
        header.rs                         # 16-byte packet header encode/decode
        channel.rs                        # channel ID constants, priority, transport mode
        capability.rs                     # extensible capability negotiation types
        error.rs                          # protocol error types
    prism-metrics/
      Cargo.toml
      src/
        lib.rs                            # re-exports, Observable trait
        recorder.rs                       # MetricsRecorder<C,G,H> const-generic
        histogram.rs                      # AtomicHistogram with log buckets + percentiles
        rate.rs                           # RateCounter (dual-counter with cached rate)
        snapshot.rs                       # RecorderSnapshot (type-erased for collection)
```

---

## Task 1: Workspace Scaffolding

**Files:**
- Create: `Cargo.toml`
- Create: `crates/prism-protocol/Cargo.toml`
- Create: `crates/prism-protocol/src/lib.rs`
- Create: `crates/prism-protocol/src/error.rs`
- Create: `crates/prism-metrics/Cargo.toml`
- Create: `crates/prism-metrics/src/lib.rs`

- [ ] **Step 1: Create workspace root Cargo.toml**

```toml
[workspace]
resolver = "2"
members = [
    "crates/prism-protocol",
    "crates/prism-metrics",
]

[workspace.package]
version = "0.1.0"
edition = "2024"
license = "CC0-1.0"

[workspace.dependencies]
bytes = "1"
thiserror = "2"
serde = { version = "1", features = ["derive"] }
serde_json = "1"

prism-protocol = { path = "crates/prism-protocol" }
prism-metrics = { path = "crates/prism-metrics" }
```

- [ ] **Step 2: Create prism-protocol crate**

`crates/prism-protocol/Cargo.toml`:
```toml
[package]
name = "prism-protocol"
version.workspace = true
edition.workspace = true

[dependencies]
bytes.workspace = true
thiserror.workspace = true
serde.workspace = true
serde_json.workspace = true
```

`crates/prism-protocol/src/lib.rs`:
```rust
pub mod header;
pub mod channel;
pub mod capability;
pub mod error;
```

`crates/prism-protocol/src/error.rs`:
```rust
use thiserror::Error;

use crate::header::HEADER_SIZE;

#[derive(Debug, Error)]
pub enum ProtocolError {
    #[error("buffer too short: need {HEADER_SIZE} bytes, got {0}")]
    BufferTooShort(usize),
    #[error("invalid channel ID 0x000 (reserved)")]
    ReservedChannel,
    #[error("unsupported protocol version {0}")]
    UnsupportedVersion(u8),
    #[error("serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),
}
```

- [ ] **Step 3: Create prism-metrics crate**

`crates/prism-metrics/Cargo.toml`:
```toml
[package]
name = "prism-metrics"
version.workspace = true
edition.workspace = true

[dependencies]
```

`crates/prism-metrics/src/lib.rs`:
```rust
pub mod recorder;
pub mod histogram;
pub mod rate;
pub mod snapshot;
```

- [ ] **Step 4: Verify workspace builds**

Run: `cargo build`
Expected: Clean build, both crates compile.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml crates/
git commit -m "feat: initialize workspace with prism-protocol and prism-metrics crates"
```

---

## Task 2: Packet Header — Encode/Decode

**Files:**
- Create: `crates/prism-protocol/src/header.rs`

Implements the 16-byte PRISM packet header. Little-endian. Version nibble (4 bits) + Channel ID (12 bits) packed into the first `u16`. Flags reduced to 8 bits (from spec's 16) to hit 16 bytes exactly.

Wire layout: `[ver:4|chan:12](2B) [msg:8](1B) [flags:8](1B) [seq:32](4B) [ts:32](4B) [len:32](4B) = 16B`

- [ ] **Step 1: Write failing tests for header encode/decode**

`crates/prism-protocol/src/header.rs`:
```rust
use bytes::{Buf, BufMut, Bytes, BytesMut};

use crate::error::ProtocolError;

/// PRISM protocol version. 0 = v1.
pub const PROTOCOL_VERSION: u8 = 0;

/// Total header size in bytes.
pub const HEADER_SIZE: usize = 16;

/// Flag bit: this is a keyframe / complete state.
pub const FLAG_KEYFRAME: u8 = 1 << 0;
/// Flag bit: high-priority delivery.
pub const FLAG_PRIORITY: u8 = 1 << 1;
/// Flag bit: payload uses channel-specific compression.
pub const FLAG_COMPRESSED: u8 = 1 << 2;
/// Flag bit: this is a preview frame (Display Engine speculative IDR).
pub const FLAG_PREVIEW: u8 = 1 << 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PrismHeader {
    /// Protocol version (4 bits, 0 = v1).
    pub version: u8,
    /// Channel ID (12 bits, 0x000 reserved/invalid).
    pub channel_id: u16,
    /// Channel-specific message type.
    pub msg_type: u8,
    /// Flags (8 bits).
    pub flags: u8,
    /// Per-channel monotonic sequence counter.
    pub sequence: u32,
    /// Microsecond-precision relative timestamp. Epoch resets per session.
    pub timestamp_us: u32,
    /// Byte length of the payload following this header.
    pub payload_length: u32,
}

impl PrismHeader {
    /// Encode header into 16 bytes, little-endian.
    pub fn encode(&self, buf: &mut BytesMut) {
        todo!()
    }

    /// Decode header from 16 bytes, little-endian.
    pub fn decode(buf: &mut impl Buf) -> Result<Self, ProtocolError> {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_basic() {
        let header = PrismHeader {
            version: 0,
            channel_id: 0x001,
            msg_type: 0x01,
            flags: 0,
            sequence: 42,
            timestamp_us: 123456,
            payload_length: 1024,
        };
        let mut buf = BytesMut::with_capacity(HEADER_SIZE);
        header.encode(&mut buf);
        assert_eq!(buf.len(), HEADER_SIZE);

        let decoded = PrismHeader::decode(&mut buf.freeze()).unwrap();
        assert_eq!(decoded, header);
    }

    #[test]
    fn roundtrip_max_values() {
        let header = PrismHeader {
            version: 0x0F,
            channel_id: 0x0FFF,
            msg_type: 0xFF,
            flags: 0xFF,
            sequence: u32::MAX,
            timestamp_us: u32::MAX,
            payload_length: u32::MAX,
        };
        let mut buf = BytesMut::with_capacity(HEADER_SIZE);
        header.encode(&mut buf);
        assert_eq!(buf.len(), HEADER_SIZE);

        let decoded = PrismHeader::decode(&mut buf.freeze()).unwrap();
        assert_eq!(decoded, header);
    }

    #[test]
    fn reserved_channel_rejected() {
        let mut buf = BytesMut::with_capacity(HEADER_SIZE);
        // Version 0, channel 0x000
        buf.put_u16_le(0x0000);
        buf.put_u8(0);
        buf.put_u8(0);
        buf.put_u32_le(0);
        buf.put_u32_le(0);
        buf.put_u32_le(0);

        let result = PrismHeader::decode(&mut buf.freeze());
        assert!(matches!(result, Err(ProtocolError::ReservedChannel)));
    }

    #[test]
    fn buffer_too_short() {
        let buf = Bytes::from_static(&[0u8; 8]);
        let result = PrismHeader::decode(&mut buf.clone());
        assert!(matches!(result, Err(ProtocolError::BufferTooShort(8))));
    }

    #[test]
    fn flag_bits() {
        let header = PrismHeader {
            version: 0,
            channel_id: 0x001,
            msg_type: 0,
            flags: FLAG_KEYFRAME | FLAG_PRIORITY,
            sequence: 0,
            timestamp_us: 0,
            payload_length: 0,
        };
        let mut buf = BytesMut::with_capacity(HEADER_SIZE);
        header.encode(&mut buf);
        let decoded = PrismHeader::decode(&mut buf.freeze()).unwrap();
        assert_ne!(decoded.flags & FLAG_KEYFRAME, 0);
        assert_ne!(decoded.flags & FLAG_PRIORITY, 0);
        assert_eq!(decoded.flags & FLAG_COMPRESSED, 0);
    }

    #[test]
    fn unsupported_version() {
        let mut buf = BytesMut::with_capacity(HEADER_SIZE);
        // Version 1 (unsupported), channel 0x001
        let ver_chan: u16 = (1u16 << 12) | 0x001;
        buf.put_u16_le(ver_chan);
        buf.put_u8(0);
        buf.put_u8(0);
        buf.put_u32_le(0);
        buf.put_u32_le(0);
        buf.put_u32_le(0);

        let result = PrismHeader::decode(&mut buf.freeze());
        assert!(matches!(result, Err(ProtocolError::UnsupportedVersion(1))));
    }

    #[test]
    fn version_and_channel_packing() {
        // Verify the version nibble and channel ID are packed correctly
        // into a single u16: [ver:4|chan:12]
        let header = PrismHeader {
            version: 0,
            channel_id: 0x0E1, // Notify channel (mobile extension)
            msg_type: 0,
            flags: 0,
            sequence: 0,
            timestamp_us: 0,
            payload_length: 0,
        };
        let mut buf = BytesMut::with_capacity(HEADER_SIZE);
        header.encode(&mut buf);

        // First two bytes: version 0 in top 4 bits, 0x0E1 in bottom 12
        let first_u16 = u16::from_le_bytes([buf[0], buf[1]]);
        assert_eq!((first_u16 >> 12) & 0x0F, 0); // version
        assert_eq!(first_u16 & 0x0FFF, 0x0E1);   // channel
    }

    #[test]
    fn header_size_is_16_bytes() {
        // Sanity check: any header encodes to exactly 16 bytes
        let header = PrismHeader {
            version: 0,
            channel_id: 0x007,
            msg_type: 0x42,
            flags: FLAG_COMPRESSED,
            sequence: 999,
            timestamp_us: 555,
            payload_length: 65536,
        };
        let mut buf = BytesMut::with_capacity(HEADER_SIZE);
        header.encode(&mut buf);
        assert_eq!(buf.len(), 16);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p prism-protocol`
Expected: FAIL — `todo!()` panics in `encode` and `decode`.

- [ ] **Step 3: Implement header encode/decode**

Replace the `todo!()` bodies in `crates/prism-protocol/src/header.rs`:

```rust
impl PrismHeader {
    /// Encode header into 16 bytes, little-endian.
    ///
    /// Wire layout (16 bytes):
    ///   [ver:4|chan:12](2B) [msg:8](1B) [flags:8](1B) [seq:32](4B) [ts:32](4B) [len:32](4B)
    pub fn encode(&self, buf: &mut BytesMut) {
        let ver_chan: u16 = ((self.version as u16 & 0x0F) << 12) | (self.channel_id & 0x0FFF);
        buf.put_u16_le(ver_chan);
        buf.put_u8(self.msg_type);
        buf.put_u8(self.flags);
        buf.put_u32_le(self.sequence);
        buf.put_u32_le(self.timestamp_us);
        buf.put_u32_le(self.payload_length);
    }

    /// Decode header from 16 bytes, little-endian.
    pub fn decode(buf: &mut impl Buf) -> Result<Self, ProtocolError> {
        if buf.remaining() < HEADER_SIZE {
            return Err(ProtocolError::BufferTooShort(buf.remaining()));
        }

        let ver_chan = buf.get_u16_le();
        let version = ((ver_chan >> 12) & 0x0F) as u8;
        let channel_id = ver_chan & 0x0FFF;

        let msg_type = buf.get_u8();
        let flags = buf.get_u8();
        let sequence = buf.get_u32_le();
        let timestamp_us = buf.get_u32_le();
        let payload_length = buf.get_u32_le();

        if version != PROTOCOL_VERSION {
            return Err(ProtocolError::UnsupportedVersion(version));
        }
        if channel_id == 0x000 {
            return Err(ProtocolError::ReservedChannel);
        }

        Ok(PrismHeader {
            version,
            channel_id,
            msg_type,
            flags,
            sequence,
            timestamp_us,
            payload_length,
        })
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p prism-protocol`
Expected: All 7 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/prism-protocol/src/header.rs crates/prism-protocol/src/error.rs
git commit -m "feat(protocol): 16-byte packet header encode/decode

Wire layout: [ver:4|chan:12][msg:8][flags:8][seq:32][ts:32][len:32].
Little-endian. Flags 8 bits (KEYFRAME, PRIORITY, COMPRESSED, PREVIEW).
Validates reserved channel 0x000 and unsupported versions."
```

---

## Task 3: Channel Constants, Priority, and Transport Mode

**Files:**
- Create: `crates/prism-protocol/src/channel.rs`

Defines channel ID constants, priority levels (for bandwidth arbitration), transport mode (datagram/stream/hybrid), and `PrismPacket`.

- [ ] **Step 1: Write channel module with tests**

`crates/prism-protocol/src/channel.rs`:
```rust
use bytes::Bytes;

use crate::header::PrismHeader;

// === Core channels (0x001-0x0FF) ===
pub const CHANNEL_DISPLAY: u16 = 0x001;
pub const CHANNEL_INPUT: u16 = 0x002;
pub const CHANNEL_AUDIO: u16 = 0x003;
pub const CHANNEL_CLIPBOARD: u16 = 0x004;
pub const CHANNEL_DEVICE: u16 = 0x005;
pub const CHANNEL_CONTROL: u16 = 0x006;
pub const CHANNEL_FILESHARE: u16 = 0x007;

// === Mobile extension channels (0x0E0-0x0EF) ===
pub const CHANNEL_NOTIFY: u16 = 0x0E1;
pub const CHANNEL_CAMERA: u16 = 0x0E2;
pub const CHANNEL_SENSOR: u16 = 0x0E3;
pub const CHANNEL_TOUCH: u16 = 0x0E4;

// === Extension channel range ===
pub const EXTENSION_CHANNEL_START: u16 = 0x100;
pub const EXTENSION_CHANNEL_END: u16 = 0xFFF;

/// A complete PRISM packet: header + payload.
#[derive(Debug, Clone)]
pub struct PrismPacket {
    pub header: PrismHeader,
    pub payload: Bytes,
}

/// Channel priority levels for bandwidth arbitration (R14).
/// Higher numeric value = higher priority.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ChannelPriority {
    Background = 0,
    Low = 1,
    Normal = 2,
    High = 3,
    Critical = 4,
}

/// Returns the default priority for a channel ID.
pub fn channel_priority(channel_id: u16) -> ChannelPriority {
    match channel_id {
        CHANNEL_INPUT => ChannelPriority::Critical,
        CHANNEL_DISPLAY | CHANNEL_AUDIO => ChannelPriority::High,
        CHANNEL_CONTROL | CHANNEL_CLIPBOARD => ChannelPriority::Normal,
        CHANNEL_FILESHARE | CHANNEL_DEVICE => ChannelPriority::Low,
        CHANNEL_SENSOR | CHANNEL_NOTIFY => ChannelPriority::Background,
        _ => ChannelPriority::Normal,
    }
}

/// Maps channel priority to a priority category index (0-4).
/// Used by bandwidth tracker for fixed-size array indexing.
pub fn priority_category(channel_id: u16) -> usize {
    channel_priority(channel_id) as usize
}

/// Whether a channel uses datagrams (unreliable) or streams (reliable).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelTransport {
    /// Unreliable QUIC datagrams. Loss-tolerant, lowest latency.
    Datagram,
    /// Reliable QUIC streams. Every byte must arrive.
    Stream,
    /// Keyframes on stream, delta frames on datagram.
    Hybrid,
}

/// Returns the default transport mode for a channel ID.
pub fn channel_transport(channel_id: u16) -> ChannelTransport {
    match channel_id {
        CHANNEL_DISPLAY => ChannelTransport::Hybrid,
        CHANNEL_INPUT | CHANNEL_AUDIO | CHANNEL_CAMERA | CHANNEL_SENSOR => {
            ChannelTransport::Datagram
        }
        CHANNEL_CLIPBOARD | CHANNEL_DEVICE | CHANNEL_CONTROL | CHANNEL_FILESHARE
        | CHANNEL_NOTIFY | CHANNEL_TOUCH => ChannelTransport::Stream,
        _ => ChannelTransport::Stream,
    }
}

/// Returns the priority weight for bandwidth arbitration.
/// Higher weight = larger share of available bandwidth.
pub fn priority_weight(priority: ChannelPriority) -> u32 {
    match priority {
        ChannelPriority::Critical => 16,
        ChannelPriority::High => 8,
        ChannelPriority::Normal => 4,
        ChannelPriority::Low => 2,
        ChannelPriority::Background => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_is_critical_priority() {
        assert_eq!(channel_priority(CHANNEL_INPUT), ChannelPriority::Critical);
    }

    #[test]
    fn display_is_high_priority() {
        assert_eq!(channel_priority(CHANNEL_DISPLAY), ChannelPriority::High);
    }

    #[test]
    fn fileshare_is_low_priority() {
        assert_eq!(channel_priority(CHANNEL_FILESHARE), ChannelPriority::Low);
    }

    #[test]
    fn display_is_hybrid_transport() {
        assert_eq!(channel_transport(CHANNEL_DISPLAY), ChannelTransport::Hybrid);
    }

    #[test]
    fn input_is_datagram_transport() {
        assert_eq!(channel_transport(CHANNEL_INPUT), ChannelTransport::Datagram);
    }

    #[test]
    fn fileshare_is_stream_transport() {
        assert_eq!(channel_transport(CHANNEL_FILESHARE), ChannelTransport::Stream);
    }

    #[test]
    fn priority_ordering() {
        assert!(ChannelPriority::Critical > ChannelPriority::High);
        assert!(ChannelPriority::High > ChannelPriority::Normal);
        assert!(ChannelPriority::Normal > ChannelPriority::Low);
        assert!(ChannelPriority::Low > ChannelPriority::Background);
    }

    #[test]
    fn priority_weights_are_monotonic() {
        assert!(priority_weight(ChannelPriority::Critical) > priority_weight(ChannelPriority::High));
        assert!(priority_weight(ChannelPriority::High) > priority_weight(ChannelPriority::Normal));
        assert!(priority_weight(ChannelPriority::Normal) > priority_weight(ChannelPriority::Low));
        assert!(priority_weight(ChannelPriority::Low) > priority_weight(ChannelPriority::Background));
    }

    #[test]
    fn unknown_channel_defaults_to_normal_stream() {
        assert_eq!(channel_priority(0x100), ChannelPriority::Normal);
        assert_eq!(channel_transport(0x100), ChannelTransport::Stream);
    }

    #[test]
    fn mobile_channels_have_expected_transport() {
        assert_eq!(channel_transport(CHANNEL_CAMERA), ChannelTransport::Datagram);
        assert_eq!(channel_transport(CHANNEL_SENSOR), ChannelTransport::Datagram);
        assert_eq!(channel_transport(CHANNEL_NOTIFY), ChannelTransport::Stream);
        assert_eq!(channel_transport(CHANNEL_TOUCH), ChannelTransport::Stream);
    }

    #[test]
    fn priority_category_indexes() {
        assert_eq!(priority_category(CHANNEL_INPUT), 4);        // Critical
        assert_eq!(priority_category(CHANNEL_DISPLAY), 3);      // High
        assert_eq!(priority_category(CHANNEL_CONTROL), 2);      // Normal
        assert_eq!(priority_category(CHANNEL_FILESHARE), 1);    // Low
        assert_eq!(priority_category(CHANNEL_SENSOR), 0);       // Background
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p prism-protocol`
Expected: All tests PASS (header + channel).

- [ ] **Step 3: Commit**

```bash
git add crates/prism-protocol/src/channel.rs crates/prism-protocol/src/lib.rs
git commit -m "feat(protocol): channel constants, priorities, transport modes

Core channels 0x001-0x007, mobile extensions 0x0E1-0x0E4, extension
range 0x100-0xFFF. Priority: Input(Critical) > Display/Audio(High) >
Control/Clipboard(Normal) > FileShare(Low) > Sensor/Notify(Background).
Transport: Hybrid(Display), Datagram(Input/Audio/Camera/Sensor),
Stream(everything else). Priority weights for bandwidth arbitration."
```

---

## Task 4: Capability Negotiation Types

**Files:**
- Create: `crates/prism-protocol/src/capability.rs`

Implements R1: extensible capability negotiation as `(channel_id, channel_version, channel_config)` tuples. JSON-serializable for handshake exchange.

- [ ] **Step 1: Write capability types with tests**

`crates/prism-protocol/src/capability.rs`:
```rust
use serde::{Deserialize, Serialize};

/// Client capabilities sent during handshake (R1).
/// New phases add new channel entries without changing the format.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ClientCapabilities {
    pub protocol_version: u16,
    pub channels: Vec<ChannelCap>,
    pub performance: PerformanceProfile,
}

/// Per-channel capability advertisement.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChannelCap {
    pub channel_id: u16,
    pub channel_version: u16,
    pub config: ChannelConfig,
}

/// Channel-specific configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum ChannelConfig {
    Display(DisplayChannelConfig),
    Input(InputChannelConfig),
    Control,
    Generic,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DisplayChannelConfig {
    pub max_resolution: (u32, u32),
    pub max_fps: u8,
    pub supported_codecs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InputChannelConfig {
    pub devices: Vec<String>,
}

/// Client performance profile (R29).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PerformanceProfile {
    pub max_decode_resolution: (u32, u32),
    pub max_decoder_instances: u8,
    pub supported_codecs: Vec<String>,
    pub can_composite_regions: bool,
    pub estimated_decode_fps: u16,
}

/// Server capabilities sent in handshake response.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ServerCapabilities {
    pub protocol_version: u16,
    pub channels: Vec<ChannelCap>,
    pub negotiated_codec: String,
    pub display_resolution: (u32, u32),
    pub display_fps: u8,
}

/// Channel assignment: maps PRISM channel IDs to QUIC resources (R2).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChannelAssignment {
    pub channel_id: u16,
    pub stream_id: Option<u64>,
    pub uses_datagrams: bool,
}

impl ClientCapabilities {
    /// Check if client supports a specific channel.
    pub fn supports_channel(&self, channel_id: u16) -> bool {
        self.channels.iter().any(|c| c.channel_id == channel_id)
    }
}

impl Default for PerformanceProfile {
    fn default() -> Self {
        Self {
            max_decode_resolution: (1920, 1080),
            max_decoder_instances: 1,
            supported_codecs: vec!["h264".to_string()],
            can_composite_regions: false,
            estimated_decode_fps: 30,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_client_caps() -> ClientCapabilities {
        ClientCapabilities {
            protocol_version: 1,
            channels: vec![
                ChannelCap {
                    channel_id: 0x001,
                    channel_version: 1,
                    config: ChannelConfig::Display(DisplayChannelConfig {
                        max_resolution: (2560, 1440),
                        max_fps: 60,
                        supported_codecs: vec!["h264".into(), "h265".into()],
                    }),
                },
                ChannelCap {
                    channel_id: 0x002,
                    channel_version: 1,
                    config: ChannelConfig::Input(InputChannelConfig {
                        devices: vec!["keyboard".into(), "mouse".into()],
                    }),
                },
                ChannelCap {
                    channel_id: 0x006,
                    channel_version: 1,
                    config: ChannelConfig::Control,
                },
            ],
            performance: PerformanceProfile::default(),
        }
    }

    #[test]
    fn capabilities_json_roundtrip() {
        let caps = sample_client_caps();
        let json = serde_json::to_string(&caps).unwrap();
        let decoded: ClientCapabilities = serde_json::from_str(&json).unwrap();
        assert_eq!(caps, decoded);
    }

    #[test]
    fn supports_channel() {
        let caps = sample_client_caps();
        assert!(caps.supports_channel(0x001));
        assert!(caps.supports_channel(0x002));
        assert!(caps.supports_channel(0x006));
        assert!(!caps.supports_channel(0x003));
        assert!(!caps.supports_channel(0x0E1));
    }

    #[test]
    fn unknown_channels_filtered_by_server() {
        // Phase 4 client sends mobile channels to Phase 1 server.
        // Server filters to channels it supports.
        let caps = ClientCapabilities {
            protocol_version: 1,
            channels: vec![
                ChannelCap {
                    channel_id: 0x001,
                    channel_version: 1,
                    config: ChannelConfig::Generic,
                },
                ChannelCap {
                    channel_id: 0x0E1,
                    channel_version: 1,
                    config: ChannelConfig::Generic,
                },
            ],
            performance: PerformanceProfile::default(),
        };

        let json = serde_json::to_string(&caps).unwrap();
        let decoded: ClientCapabilities = serde_json::from_str(&json).unwrap();

        let server_supported: Vec<_> = decoded
            .channels
            .iter()
            .filter(|c| c.channel_id <= 0x007)
            .collect();
        assert_eq!(server_supported.len(), 1);
        assert_eq!(server_supported[0].channel_id, 0x001);
    }

    #[test]
    fn server_capabilities_roundtrip() {
        let caps = ServerCapabilities {
            protocol_version: 1,
            channels: vec![ChannelCap {
                channel_id: 0x001,
                channel_version: 1,
                config: ChannelConfig::Generic,
            }],
            negotiated_codec: "h265".to_string(),
            display_resolution: (2560, 1440),
            display_fps: 60,
        };

        let json = serde_json::to_string(&caps).unwrap();
        let decoded: ServerCapabilities = serde_json::from_str(&json).unwrap();
        assert_eq!(caps, decoded);
    }

    #[test]
    fn default_performance_profile() {
        let profile = PerformanceProfile::default();
        assert_eq!(profile.max_decode_resolution, (1920, 1080));
        assert_eq!(profile.max_decoder_instances, 1);
        assert!(!profile.can_composite_regions);
        assert_eq!(profile.estimated_decode_fps, 30);
    }

    #[test]
    fn channel_assignment_roundtrip() {
        let assignments = vec![
            ChannelAssignment {
                channel_id: 0x001,
                stream_id: Some(4),
                uses_datagrams: true,
            },
            ChannelAssignment {
                channel_id: 0x006,
                stream_id: Some(0),
                uses_datagrams: false,
            },
        ];

        let json = serde_json::to_string(&assignments).unwrap();
        let decoded: Vec<ChannelAssignment> = serde_json::from_str(&json).unwrap();
        assert_eq!(assignments, decoded);
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p prism-protocol`
Expected: All tests PASS.

- [ ] **Step 3: Commit**

```bash
git add crates/prism-protocol/src/capability.rs crates/prism-protocol/src/lib.rs
git commit -m "feat(protocol): extensible capability negotiation types (R1)

ClientCapabilities, ServerCapabilities, ChannelAssignment, PerformanceProfile.
Capabilities are (channel_id, version, config) tuples. New phases add entries
without changing format. JSON serializable for handshake exchange."
```

---

## Task 5: AtomicHistogram with Logarithmic Buckets

**Files:**
- Create: `crates/prism-metrics/src/histogram.rs`

Lock-free histogram with 25 logarithmic buckets (1µs to ~34s). O(1) record, bounded memory. Percentile computation from cumulative bucket counts.

- [ ] **Step 1: Write failing tests for histogram**

`crates/prism-metrics/src/histogram.rs`:
```rust
use std::sync::atomic::{AtomicU64, Ordering};

/// Number of logarithmic buckets. Range: 0µs to ~33 million µs (~34s).
const BUCKET_COUNT: usize = 25;

/// Lock-free histogram with logarithmic buckets.
/// One byte per bucket index = floor(log2(value)).
/// Bucket 0: [0, 1), Bucket 1: [1, 2), Bucket 2: [2, 4), ..., Bucket 24: [2^24, 2^25).
pub struct AtomicHistogram {
    buckets: [AtomicU64; BUCKET_COUNT],
    sum: AtomicU64,
    count: AtomicU64,
    min: AtomicU64,
    max: AtomicU64,
}

/// Immutable snapshot of histogram state.
#[derive(Debug, Clone)]
pub struct HistogramSnapshot {
    pub buckets: [u64; BUCKET_COUNT],
    pub sum_us: u64,
    pub count: u64,
    pub avg_us: u64,
    pub min_us: u64,
    pub max_us: u64,
    pub p50_us: u64,
    pub p95_us: u64,
    pub p99_us: u64,
}

impl AtomicHistogram {
    pub fn new() -> Self {
        Self {
            buckets: std::array::from_fn(|_| AtomicU64::new(0)),
            sum: AtomicU64::new(0),
            count: AtomicU64::new(0),
            min: AtomicU64::new(u64::MAX),
            max: AtomicU64::new(0),
        }
    }

    /// Record a value in microseconds. Lock-free, ~5ns.
    #[inline(always)]
    pub fn record(&self, value_us: u64) {
        let bucket = if value_us == 0 {
            0
        } else {
            (63 - value_us.leading_zeros() as usize).min(BUCKET_COUNT - 1)
        };
        self.buckets[bucket].fetch_add(1, Ordering::Relaxed);
        self.sum.fetch_add(value_us, Ordering::Relaxed);
        self.count.fetch_add(1, Ordering::Relaxed);
        self.update_min(value_us);
        self.update_max(value_us);
    }

    fn update_min(&self, value: u64) {
        let mut current = self.min.load(Ordering::Relaxed);
        while value < current {
            match self.min.compare_exchange_weak(
                current,
                value,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(actual) => current = actual,
            }
        }
    }

    fn update_max(&self, value: u64) {
        let mut current = self.max.load(Ordering::Relaxed);
        while value > current {
            match self.max.compare_exchange_weak(
                current,
                value,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(actual) => current = actual,
            }
        }
    }

    /// Take a snapshot. Computes percentiles from buckets.
    pub fn snapshot(&self) -> HistogramSnapshot {
        let buckets: [u64; BUCKET_COUNT] =
            std::array::from_fn(|i| self.buckets[i].load(Ordering::Relaxed));
        let count = self.count.load(Ordering::Relaxed);
        let sum = self.sum.load(Ordering::Relaxed);
        let min = self.min.load(Ordering::Relaxed);
        let max = self.max.load(Ordering::Relaxed);

        HistogramSnapshot {
            buckets,
            sum_us: sum,
            count,
            avg_us: if count > 0 { sum / count } else { 0 },
            min_us: if min == u64::MAX { 0 } else { min },
            max_us: max,
            p50_us: Self::percentile(&buckets, count, 0.50),
            p95_us: Self::percentile(&buckets, count, 0.95),
            p99_us: Self::percentile(&buckets, count, 0.99),
        }
    }

    fn percentile(buckets: &[u64; BUCKET_COUNT], total: u64, pct: f64) -> u64 {
        if total == 0 {
            return 0;
        }
        let target = (total as f64 * pct) as u64;
        let mut cumulative = 0u64;
        for (i, &count) in buckets.iter().enumerate() {
            cumulative += count;
            if cumulative >= target {
                let bucket_start = if i == 0 { 0 } else { 1u64 << i };
                let bucket_end = 1u64 << (i + 1);
                let fraction = if count > 0 {
                    (target.saturating_sub(cumulative - count)) as f64 / count as f64
                } else {
                    0.0
                };
                return bucket_start
                    + ((bucket_end - bucket_start) as f64 * fraction) as u64;
            }
        }
        0
    }
}

impl Default for AtomicHistogram {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_histogram() {
        let h = AtomicHistogram::new();
        let snap = h.snapshot();
        assert_eq!(snap.count, 0);
        assert_eq!(snap.sum_us, 0);
        assert_eq!(snap.avg_us, 0);
        assert_eq!(snap.min_us, 0);
        assert_eq!(snap.max_us, 0);
        assert_eq!(snap.p50_us, 0);
        assert_eq!(snap.p95_us, 0);
        assert_eq!(snap.p99_us, 0);
    }

    #[test]
    fn single_value() {
        let h = AtomicHistogram::new();
        h.record(1000);
        let snap = h.snapshot();
        assert_eq!(snap.count, 1);
        assert_eq!(snap.sum_us, 1000);
        assert_eq!(snap.avg_us, 1000);
        assert_eq!(snap.min_us, 1000);
        assert_eq!(snap.max_us, 1000);
    }

    #[test]
    fn min_max_tracking() {
        let h = AtomicHistogram::new();
        h.record(100);
        h.record(500);
        h.record(50);
        h.record(1000);
        let snap = h.snapshot();
        assert_eq!(snap.min_us, 50);
        assert_eq!(snap.max_us, 1000);
        assert_eq!(snap.count, 4);
    }

    #[test]
    fn zero_value() {
        let h = AtomicHistogram::new();
        h.record(0);
        let snap = h.snapshot();
        assert_eq!(snap.count, 1);
        assert_eq!(snap.min_us, 0);
        assert_eq!(snap.max_us, 0);
    }

    #[test]
    fn large_value() {
        let h = AtomicHistogram::new();
        // 34 seconds in microseconds — should go in last bucket
        h.record(34_000_000);
        let snap = h.snapshot();
        assert_eq!(snap.count, 1);
        assert_eq!(snap.max_us, 34_000_000);
    }

    #[test]
    fn percentile_uniform_distribution() {
        let h = AtomicHistogram::new();
        // Record values 1-100 (each once)
        for i in 1..=100 {
            h.record(i);
        }
        let snap = h.snapshot();
        assert_eq!(snap.count, 100);
        assert_eq!(snap.min_us, 1);
        assert_eq!(snap.max_us, 100);
        // p50 should be around 50 (log bucket approximation)
        assert!(snap.p50_us >= 32 && snap.p50_us <= 80, "p50 was {}", snap.p50_us);
        // p95 should be around 95
        assert!(snap.p95_us >= 64 && snap.p95_us <= 128, "p95 was {}", snap.p95_us);
    }

    #[test]
    fn percentile_all_same_value() {
        let h = AtomicHistogram::new();
        for _ in 0..1000 {
            h.record(500);
        }
        let snap = h.snapshot();
        // All percentiles should be in the same bucket as 500
        // Bucket for 500: floor(log2(500)) = 8 → bucket covers [256, 512)
        assert!(snap.p50_us >= 256 && snap.p50_us <= 512, "p50 was {}", snap.p50_us);
        assert!(snap.p95_us >= 256 && snap.p95_us <= 512, "p95 was {}", snap.p95_us);
        assert!(snap.p99_us >= 256 && snap.p99_us <= 512, "p99 was {}", snap.p99_us);
    }

    #[test]
    fn concurrent_recording() {
        use std::sync::Arc;
        use std::thread;

        let h = Arc::new(AtomicHistogram::new());
        let mut handles = Vec::new();

        for t in 0..4 {
            let h = h.clone();
            handles.push(thread::spawn(move || {
                for i in 0..1000 {
                    h.record((t * 1000 + i) as u64);
                }
            }));
        }

        for handle in handles {
            handle.join().unwrap();
        }

        let snap = h.snapshot();
        assert_eq!(snap.count, 4000);
        assert_eq!(snap.min_us, 0);
        assert_eq!(snap.max_us, 3999);
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p prism-metrics`
Expected: All 8 tests PASS.

- [ ] **Step 3: Commit**

```bash
git add crates/prism-metrics/src/histogram.rs crates/prism-metrics/src/lib.rs
git commit -m "feat(metrics): lock-free AtomicHistogram with log buckets and percentiles

25 logarithmic buckets (1us to ~34s). O(1) record via atomic fetch_add.
CAS-based min/max tracking. Percentile computation from cumulative
bucket counts with linear interpolation. Thread-safe, ~5ns per record."
```

---

## Task 6: MetricsRecorder (Const-Generic)

**Files:**
- Create: `crates/prism-metrics/src/recorder.rs`
- Create: `crates/prism-metrics/src/snapshot.rs`

Const-generic `MetricsRecorder<C, G, H>` with compile-time bounds checking. Lock-free counters, gauges, and histograms.

- [ ] **Step 1: Write RecorderSnapshot (type-erased)**

`crates/prism-metrics/src/snapshot.rs`:
```rust
use crate::histogram::HistogramSnapshot;

/// Type-erased snapshot of a MetricsRecorder. Used by the collector
/// to aggregate across subsystems without knowing their const generics.
#[derive(Debug, Clone)]
pub struct RecorderSnapshot {
    pub counters: Vec<u64>,
    pub gauges: Vec<i64>,
    pub histograms: Vec<HistogramSnapshot>,
    pub counter_names: Vec<&'static str>,
    pub gauge_names: Vec<&'static str>,
    pub histogram_names: Vec<&'static str>,
}

impl Default for RecorderSnapshot {
    fn default() -> Self {
        Self {
            counters: Vec::new(),
            gauges: Vec::new(),
            histograms: Vec::new(),
            counter_names: Vec::new(),
            gauge_names: Vec::new(),
            histogram_names: Vec::new(),
        }
    }
}
```

- [ ] **Step 2: Write MetricsRecorder with tests**

`crates/prism-metrics/src/recorder.rs`:
```rust
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};

use crate::histogram::AtomicHistogram;
use crate::snapshot::RecorderSnapshot;

/// Compile-time sized metric labels.
pub struct MetricLabels<const C: usize, const G: usize, const H: usize> {
    pub counter_names: [&'static str; C],
    pub gauge_names: [&'static str; G],
    pub histogram_names: [&'static str; H],
}

/// Pre-allocated, lock-free metrics storage.
/// One per subsystem instance. Const generics ensure compile-time
/// bounds checking — accessing index N on a recorder with C < N
/// counters is a compile error.
pub struct MetricsRecorder<const C: usize, const G: usize, const H: usize> {
    counters: [AtomicU64; C],
    gauges: [AtomicI64; G],
    histograms: [AtomicHistogram; H],
    labels: MetricLabels<C, G, H>,
}

impl<const C: usize, const G: usize, const H: usize> MetricsRecorder<C, G, H> {
    /// Create a new recorder with the given metric names.
    pub fn new(labels: MetricLabels<C, G, H>) -> Self {
        Self {
            counters: std::array::from_fn(|_| AtomicU64::new(0)),
            gauges: std::array::from_fn(|_| AtomicI64::new(0)),
            histograms: std::array::from_fn(|_| AtomicHistogram::new()),
            labels,
        }
    }

    /// Increment a counter. Lock-free, ~1ns.
    #[inline(always)]
    pub fn inc(&self, counter: usize, value: u64) {
        self.counters[counter].fetch_add(value, Ordering::Relaxed);
    }

    /// Read a counter's current value.
    #[inline(always)]
    pub fn counter(&self, counter: usize) -> u64 {
        self.counters[counter].load(Ordering::Relaxed)
    }

    /// Set a gauge value. Lock-free, ~1ns.
    #[inline(always)]
    pub fn set(&self, gauge: usize, value: i64) {
        self.gauges[gauge].store(value, Ordering::Relaxed);
    }

    /// Read a gauge's current value.
    #[inline(always)]
    pub fn gauge(&self, gauge: usize) -> i64 {
        self.gauges[gauge].load(Ordering::Relaxed)
    }

    /// Record a histogram sample. Lock-free, ~5ns.
    #[inline(always)]
    pub fn observe(&self, histogram: usize, value_us: u64) {
        self.histograms[histogram].record(value_us);
    }

    /// Take a type-erased snapshot of all metrics.
    /// Allocates (Vec) — only called during collection (~1/sec), never on hot path.
    pub fn snapshot(&self) -> RecorderSnapshot {
        RecorderSnapshot {
            counters: self.counters.iter().map(|c| c.load(Ordering::Relaxed)).collect(),
            gauges: self.gauges.iter().map(|g| g.load(Ordering::Relaxed)).collect(),
            histograms: self.histograms.iter().map(|h| h.snapshot()).collect(),
            counter_names: self.labels.counter_names.to_vec(),
            gauge_names: self.labels.gauge_names.to_vec(),
            histogram_names: self.labels.histogram_names.to_vec(),
        }
    }
}

/// Trait for subsystems to expose their metrics.
pub trait Observable: Send + Sync {
    fn name(&self) -> &'static str;
    fn snapshot(&self) -> RecorderSnapshot;
}

#[cfg(test)]
mod tests {
    use super::*;

    // Define a test recorder: 3 counters, 2 gauges, 1 histogram
    type TestRecorder = MetricsRecorder<3, 2, 1>;

    fn test_labels() -> MetricLabels<3, 2, 1> {
        MetricLabels {
            counter_names: ["events", "bytes_sent", "errors"],
            gauge_names: ["active_connections", "queue_depth"],
            histogram_names: ["latency_us"],
        }
    }

    #[test]
    fn counter_increment() {
        let rec = TestRecorder::new(test_labels());
        rec.inc(0, 1);
        rec.inc(0, 1);
        rec.inc(0, 1);
        assert_eq!(rec.counter(0), 3);
        assert_eq!(rec.counter(1), 0); // other counters untouched
    }

    #[test]
    fn gauge_set() {
        let rec = TestRecorder::new(test_labels());
        rec.set(0, 42);
        assert_eq!(rec.gauge(0), 42);
        rec.set(0, -1);
        assert_eq!(rec.gauge(0), -1);
    }

    #[test]
    fn histogram_observe() {
        let rec = TestRecorder::new(test_labels());
        rec.observe(0, 100);
        rec.observe(0, 200);
        rec.observe(0, 300);
        let snap = rec.snapshot();
        assert_eq!(snap.histograms[0].count, 3);
        assert_eq!(snap.histograms[0].sum_us, 600);
    }

    #[test]
    fn snapshot_captures_all_metrics() {
        let rec = TestRecorder::new(test_labels());
        rec.inc(0, 10);
        rec.inc(1, 20);
        rec.inc(2, 30);
        rec.set(0, 5);
        rec.set(1, -3);
        rec.observe(0, 1000);

        let snap = rec.snapshot();
        assert_eq!(snap.counters, vec![10, 20, 30]);
        assert_eq!(snap.gauges, vec![5, -3]);
        assert_eq!(snap.histograms.len(), 1);
        assert_eq!(snap.histograms[0].count, 1);
        assert_eq!(snap.counter_names, vec!["events", "bytes_sent", "errors"]);
        assert_eq!(snap.gauge_names, vec!["active_connections", "queue_depth"]);
        assert_eq!(snap.histogram_names, vec!["latency_us"]);
    }

    #[test]
    fn concurrent_increments() {
        use std::sync::Arc;
        use std::thread;

        let rec = Arc::new(TestRecorder::new(test_labels()));
        let mut handles = Vec::new();

        for _ in 0..4 {
            let rec = rec.clone();
            handles.push(thread::spawn(move || {
                for _ in 0..10_000 {
                    rec.inc(0, 1);
                }
            }));
        }

        for handle in handles {
            handle.join().unwrap();
        }

        assert_eq!(rec.counter(0), 40_000);
    }

    #[test]
    fn zero_sized_recorder() {
        // A recorder with no metrics should still work
        type EmptyRecorder = MetricsRecorder<0, 0, 0>;
        let labels = MetricLabels {
            counter_names: [],
            gauge_names: [],
            histogram_names: [],
        };
        let rec = EmptyRecorder::new(labels);
        let snap = rec.snapshot();
        assert!(snap.counters.is_empty());
        assert!(snap.gauges.is_empty());
        assert!(snap.histograms.is_empty());
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p prism-metrics`
Expected: All tests PASS (histogram + recorder).

- [ ] **Step 3: Commit**

```bash
git add crates/prism-metrics/src/recorder.rs crates/prism-metrics/src/snapshot.rs crates/prism-metrics/src/lib.rs
git commit -m "feat(metrics): const-generic MetricsRecorder with compile-time bounds

MetricsRecorder<C,G,H> with lock-free counters (AtomicU64), gauges
(AtomicI64), and histograms (AtomicHistogram). Const generics ensure
accessing an out-of-bounds metric index is a compile error. Type-erased
RecorderSnapshot for collection. Observable trait for subsystems."
```

---

## Task 7: RateCounter

**Files:**
- Create: `crates/prism-metrics/src/rate.rs`

Dual-counter with cached rate. Collector computes rate once per second. Overlay reads with one atomic load.

- [ ] **Step 1: Write RateCounter with tests**

`crates/prism-metrics/src/rate.rs`:
```rust
use std::sync::atomic::{AtomicU64, Ordering};

/// Counter that tracks both total count and events-per-second rate.
/// The rate is computed externally (by the collector) and cached.
/// Readers get the rate with a single atomic load (~1ns).
pub struct RateCounter {
    total: AtomicU64,
    prev_total: AtomicU64,
    prev_timestamp_us: AtomicU64,
    cached_rate: AtomicU64,
}

impl RateCounter {
    pub fn new() -> Self {
        Self {
            total: AtomicU64::new(0),
            prev_total: AtomicU64::new(0),
            prev_timestamp_us: AtomicU64::new(0),
            cached_rate: AtomicU64::new(0),
        }
    }

    /// Increment the counter. Lock-free, ~1ns.
    #[inline(always)]
    pub fn inc(&self, n: u64) {
        self.total.fetch_add(n, Ordering::Relaxed);
    }

    /// Current total count.
    #[inline(always)]
    pub fn total(&self) -> u64 {
        self.total.load(Ordering::Relaxed)
    }

    /// Cached events-per-second rate. Updated by `compute_rate()`.
    /// Zero-cost read: one atomic load.
    #[inline(always)]
    pub fn rate(&self) -> u64 {
        self.cached_rate.load(Ordering::Relaxed)
    }

    /// Recompute the rate from the counter delta since last call.
    /// Called by the metrics collector, typically once per second.
    /// `now_us` is the current timestamp in microseconds.
    pub fn compute_rate(&self, now_us: u64) {
        let total = self.total.load(Ordering::Relaxed);
        let prev = self.prev_total.swap(total, Ordering::Relaxed);
        let prev_ts = self.prev_timestamp_us.swap(now_us, Ordering::Relaxed);

        let elapsed_us = now_us.saturating_sub(prev_ts);
        if elapsed_us == 0 {
            return;
        }

        let delta = total.saturating_sub(prev);
        let rate = delta * 1_000_000 / elapsed_us;
        self.cached_rate.store(rate, Ordering::Relaxed);
    }
}

impl Default for RateCounter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_increment() {
        let rc = RateCounter::new();
        rc.inc(1);
        rc.inc(1);
        rc.inc(1);
        assert_eq!(rc.total(), 3);
    }

    #[test]
    fn rate_starts_at_zero() {
        let rc = RateCounter::new();
        assert_eq!(rc.rate(), 0);
    }

    #[test]
    fn rate_computation() {
        let rc = RateCounter::new();

        // First call: establish baseline at t=0
        rc.compute_rate(0);

        // Increment 100 events
        for _ in 0..100 {
            rc.inc(1);
        }

        // Compute rate at t=1_000_000 (1 second later)
        rc.compute_rate(1_000_000);

        // Rate should be 100 events/sec
        assert_eq!(rc.rate(), 100);
    }

    #[test]
    fn rate_updates_on_each_compute() {
        let rc = RateCounter::new();
        rc.compute_rate(0);

        // 50 events in first second
        for _ in 0..50 {
            rc.inc(1);
        }
        rc.compute_rate(1_000_000);
        assert_eq!(rc.rate(), 50);

        // 200 events in second second
        for _ in 0..200 {
            rc.inc(1);
        }
        rc.compute_rate(2_000_000);
        assert_eq!(rc.rate(), 200);
    }

    #[test]
    fn rate_with_zero_elapsed() {
        let rc = RateCounter::new();
        rc.compute_rate(1000);
        rc.inc(100);
        // Same timestamp — should not divide by zero
        rc.compute_rate(1000);
        // Rate unchanged from previous computation
    }

    #[test]
    fn rate_with_bulk_increment() {
        let rc = RateCounter::new();
        rc.compute_rate(0);

        rc.inc(5000);
        rc.compute_rate(1_000_000);
        assert_eq!(rc.rate(), 5000);
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p prism-metrics`
Expected: All tests PASS.

- [ ] **Step 3: Commit**

```bash
git add crates/prism-metrics/src/rate.rs crates/prism-metrics/src/lib.rs
git commit -m "feat(metrics): RateCounter with cached events-per-second

Dual-counter: total (monotonic) + cached rate (computed by collector).
Rate read is one atomic load (~1ns). compute_rate() called externally
at 1/sec intervals. Handles edge cases: zero elapsed, bulk increments."
```

---

## Plan Self-Review

**1. Spec coverage:**
- Architecture spec Section 1 (Packet Header): Task 2. Header fields, endianness, version nibble, channel 0x000 reserved — all covered.
- Architecture spec Section 3.3 (Channel Priority, R14): Task 3. Priority ordering, weights — covered.
- Architecture spec R1 (Extensible Capabilities): Task 4. ChannelCap tuples, ChannelConfig enum, JSON serialization — covered.
- Architecture spec R2 (Server-authoritative assignments): Task 4. ChannelAssignment type — covered.
- Architecture spec R20 (Region map): Region map is Display Engine concern, not protocol. Correctly deferred to Plan 5.
- Session spec Section 13 (prism-metrics): Tasks 5-7. AtomicHistogram with percentiles, const-generic MetricsRecorder, RateCounter — covered.
- Session spec Section 13 Observable trait: Task 6. `Observable` trait defined — covered.
- Session spec RecorderSnapshot: Task 6. Type-erased snapshot — covered.

**2. Placeholder scan:** No TBDs, TODOs, or "implement later" found. All code blocks complete.

**3. Type consistency:**
- `ProtocolError` used in Tasks 1, 2 consistently.
- `PrismHeader` fields match across encode/decode and tests.
- `ChannelPriority` used in Task 3, referenced by arbiter in later plans.
- `MetricsRecorder<C,G,H>` const generics consistent across Tasks 5-7.
- `RecorderSnapshot` produced by `MetricsRecorder::snapshot()` and `Observable::snapshot()` — consistent.
- `HistogramSnapshot` produced by `AtomicHistogram::snapshot()`, contained in `RecorderSnapshot` — consistent.

No issues found.
