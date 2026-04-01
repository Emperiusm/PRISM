// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2025-2026 Ehsan Khalid. All rights reserved.
// Licensed under the GNU Affero General Public License v3.0.
// Commercial licensing available — see LICENSE-COMMERCIAL.md.

/// Fixed size of a serialised [`OverlayPacket`] in bytes.
pub const OVERLAY_PACKET_SIZE: usize = 128;

/// Compact binary packet broadcast to every connected client to drive the
/// in-game HUD / debug overlay.
///
/// The packet is intentionally kept small so it can be sent on every display
/// refresh without any meaningful bandwidth overhead. Fields are serialised in
/// little-endian byte order; bytes 66–127 are reserved and must be zeroed.
///
/// # Wire layout (little-endian, packed, no padding)
///
/// ```text
/// Offset  Size  Field
///  0       1    fps
///  1       1    degradation_level
///  2       1    active_clients
///  3       1    transport_type
///  4       4    codec
///  8       2    resolution_w
/// 10       2    resolution_h
/// 12       4    bitrate_kbps
/// 16       4    rtt_us
/// 20       2    loss_rate_permille
/// 22       4    capture_us
/// 26       4    encode_us
/// 30       4    network_us
/// 34       4    decode_us
/// 38       4    render_us
/// 42       4    total_us
/// 46       4    display_kbps
/// 50       2    input_kbps
/// 52       2    audio_kbps
/// 54       4    fileshare_kbps
/// 58       4    total_kbps
/// 62       4    available_kbps
/// 66      62    reserved (zeroed)
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct OverlayPacket {
    // --- display quality ---
    /// Current rendered frames per second.
    pub fps: u8,
    /// Degradation level: 0 = full quality, higher = more aggressive reduction.
    pub degradation_level: u8,
    /// Number of clients currently connected.
    pub active_clients: u8,
    /// Transport protocol identifier (application-defined).
    pub transport_type: u8,
    /// Four-byte codec tag (e.g. `b"H265"`).
    pub codec: [u8; 4],
    /// Horizontal display resolution in pixels.
    pub resolution_w: u16,
    /// Vertical display resolution in pixels.
    pub resolution_h: u16,

    // --- network ---
    /// Current video stream bitrate (kbps).
    pub bitrate_kbps: u32,
    /// Round-trip time to the client (µs).
    pub rtt_us: u32,
    /// Packet-loss rate in permille (0–1000).
    pub loss_rate_permille: u16,

    // --- pipeline latency (µs) ---
    pub capture_us: u32,
    pub encode_us: u32,
    pub network_us: u32,
    pub decode_us: u32,
    pub render_us: u32,
    pub total_us: u32,

    // --- bandwidth breakdown (kbps) ---
    /// Video display stream bandwidth.
    pub display_kbps: u32,
    /// Input (keyboard/mouse) channel bandwidth.
    pub input_kbps: u16,
    /// Audio stream bandwidth.
    pub audio_kbps: u16,
    /// File sharing channel bandwidth.
    pub fileshare_kbps: u32,
    /// Total outbound bandwidth.
    pub total_kbps: u32,
    /// Estimated available bandwidth reported by the congestion controller.
    pub available_kbps: u32,
}

impl OverlayPacket {
    /// Serialise the packet into a 128-byte buffer (little-endian, reserved bytes zeroed).
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
        // bytes 66..128 remain zeroed (reserved)

        buf
    }

    /// Deserialise a packet from a 128-byte buffer.
    ///
    /// Returns `None` if `bytes.len() < 128`.
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < OVERLAY_PACKET_SIZE {
            return None;
        }

        let u16_at = |off: usize| -> u16 {
            u16::from_le_bytes([bytes[off], bytes[off + 1]])
        };
        let u32_at = |off: usize| -> u32 {
            u32::from_le_bytes([bytes[off], bytes[off + 1], bytes[off + 2], bytes[off + 3]])
        };

        Some(Self {
            fps: bytes[0],
            degradation_level: bytes[1],
            active_clients: bytes[2],
            transport_type: bytes[3],
            codec: [bytes[4], bytes[5], bytes[6], bytes[7]],
            resolution_w: u16_at(8),
            resolution_h: u16_at(10),
            bitrate_kbps: u32_at(12),
            rtt_us: u32_at(16),
            loss_rate_permille: u16_at(20),
            capture_us: u32_at(22),
            encode_us: u32_at(26),
            network_us: u32_at(30),
            decode_us: u32_at(34),
            render_us: u32_at(38),
            total_us: u32_at(42),
            display_kbps: u32_at(46),
            input_kbps: u16_at(50),
            audio_kbps: u16_at(52),
            fileshare_kbps: u32_at(54),
            total_kbps: u32_at(58),
            available_kbps: u32_at(62),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn full_packet() -> OverlayPacket {
        OverlayPacket {
            fps: 60,
            degradation_level: 1,
            active_clients: 3,
            transport_type: 2,
            codec: *b"H265",
            resolution_w: 1920,
            resolution_h: 1080,
            bitrate_kbps: 8_000,
            rtt_us: 12_000,
            loss_rate_permille: 5,
            capture_us: 300,
            encode_us: 2_000,
            network_us: 3_000,
            decode_us: 1_500,
            render_us: 400,
            total_us: 7_200,
            display_kbps: 7_500,
            input_kbps: 50,
            audio_kbps: 128,
            fileshare_kbps: 200,
            total_kbps: 7_878,
            available_kbps: 10_000,
        }
    }

    #[test]
    fn serialised_size_is_128() {
        let pkt = full_packet();
        let bytes = pkt.to_bytes();
        assert_eq!(bytes.len(), OVERLAY_PACKET_SIZE);
        assert_eq!(OVERLAY_PACKET_SIZE, 128);
    }

    #[test]
    fn roundtrip_all_fields() {
        let original = full_packet();
        let bytes = original.to_bytes();
        let decoded = OverlayPacket::from_bytes(&bytes).expect("decode");

        assert_eq!(decoded.fps, original.fps);
        assert_eq!(decoded.degradation_level, original.degradation_level);
        assert_eq!(decoded.active_clients, original.active_clients);
        assert_eq!(decoded.transport_type, original.transport_type);
        assert_eq!(decoded.codec, original.codec);
        assert_eq!(decoded.resolution_w, original.resolution_w);
        assert_eq!(decoded.resolution_h, original.resolution_h);
        assert_eq!(decoded.bitrate_kbps, original.bitrate_kbps);
        assert_eq!(decoded.rtt_us, original.rtt_us);
        assert_eq!(decoded.loss_rate_permille, original.loss_rate_permille);
        assert_eq!(decoded.capture_us, original.capture_us);
        assert_eq!(decoded.encode_us, original.encode_us);
        assert_eq!(decoded.network_us, original.network_us);
        assert_eq!(decoded.decode_us, original.decode_us);
        assert_eq!(decoded.render_us, original.render_us);
        assert_eq!(decoded.total_us, original.total_us);
        assert_eq!(decoded.display_kbps, original.display_kbps);
        assert_eq!(decoded.input_kbps, original.input_kbps);
        assert_eq!(decoded.audio_kbps, original.audio_kbps);
        assert_eq!(decoded.fileshare_kbps, original.fileshare_kbps);
        assert_eq!(decoded.total_kbps, original.total_kbps);
        assert_eq!(decoded.available_kbps, original.available_kbps);
    }

    #[test]
    fn reserved_bytes_are_zeroed() {
        let bytes = full_packet().to_bytes();
        assert!(
            bytes[66..128].iter().all(|&b| b == 0),
            "reserved bytes must be zeroed"
        );
    }

    #[test]
    fn from_bytes_returns_none_on_short_slice() {
        let short = [0u8; 64];
        assert!(OverlayPacket::from_bytes(&short).is_none());
    }
}
