//! Overlay packet generation and datagram builder.
//!
//! The server calls [`build_overlay_packet`] every ~100 ms with current
//! performance metrics, then wraps the result with [`build_overlay_datagram`]
//! to produce a 144-byte PRISM control datagram ready for `send_datagram`.

use bytes::{Bytes, BytesMut};
use prism_observability::overlay::OverlayPacket;
use prism_protocol::{
    channel::CHANNEL_CONTROL,
    header::{PrismHeader, HEADER_SIZE, PROTOCOL_VERSION},
};
use prism_session::control_msg::OVERLAY_DATA;

/// Build an [`OverlayPacket`] from current server state.
///
/// # Parameters
/// - `fps` — current rendered frames per second
/// - `bitrate_kbps` — current video stream bitrate in kbps
/// - `rtt_us` — round-trip time to the client in microseconds
/// - `active_clients` — number of clients currently connected
/// - `width` / `height` — display resolution in pixels
pub fn build_overlay_packet(
    fps: u8,
    bitrate_kbps: u32,
    rtt_us: u32,
    active_clients: u8,
    width: u16,
    height: u16,
) -> OverlayPacket {
    OverlayPacket {
        fps,
        degradation_level: 0,
        active_clients,
        transport_type: 0, // QUIC
        codec: *b"h264",
        resolution_w: width,
        resolution_h: height,
        bitrate_kbps,
        rtt_us,
        loss_rate_permille: 0,
        capture_us: 0,
        encode_us: 0,
        network_us: 0,
        decode_us: 0,
        render_us: 0,
        total_us: 0,
        display_kbps: bitrate_kbps,
        input_kbps: 0,
        audio_kbps: 0,
        fileshare_kbps: 0,
        total_kbps: bitrate_kbps,
        available_kbps: 100_000, // 100 Mbps default
    }
}

/// Wrap an [`OverlayPacket`] in a PRISM control datagram.
///
/// Wire layout: `[PrismHeader (16 B)][OverlayPacket (128 B)]` = 144 bytes total.
pub fn build_overlay_datagram(packet: &OverlayPacket) -> Bytes {
    let overlay_bytes = packet.to_bytes();
    let header = PrismHeader {
        version: PROTOCOL_VERSION,
        channel_id: CHANNEL_CONTROL,
        msg_type: OVERLAY_DATA,
        flags: 0,
        sequence: 0,
        timestamp_us: 0,
        payload_length: overlay_bytes.len() as u32,
    };

    let mut buf = BytesMut::with_capacity(HEADER_SIZE + overlay_bytes.len());
    header.encode(&mut buf);
    buf.extend_from_slice(&overlay_bytes);
    buf.freeze()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use prism_protocol::header::PrismHeader;
    use prism_session::control_msg::OVERLAY_DATA;

    // 1. build_overlay_packet sets fields correctly.
    #[test]
    fn build_overlay_packet_fields() {
        let pkt = build_overlay_packet(30, 8_000, 12_000, 2, 1920, 1080);

        assert_eq!(pkt.fps, 30);
        assert_eq!(pkt.bitrate_kbps, 8_000);
        assert_eq!(pkt.rtt_us, 12_000);
        assert_eq!(pkt.active_clients, 2);
        assert_eq!(pkt.resolution_w, 1920);
        assert_eq!(pkt.resolution_h, 1080);
        assert_eq!(&pkt.codec, b"h264");
        assert_eq!(pkt.transport_type, 0);
        assert_eq!(pkt.display_kbps, 8_000);
        assert_eq!(pkt.total_kbps, 8_000);
        assert_eq!(pkt.available_kbps, 100_000);
        assert_eq!(pkt.degradation_level, 0);
    }

    // 2. build_overlay_datagram produces exactly HEADER_SIZE + OVERLAY_PACKET_SIZE bytes.
    #[test]
    fn overlay_datagram_size() {
        let pkt = build_overlay_packet(60, 5_000, 1_000, 1, 1920, 1080);
        let dgram = build_overlay_datagram(&pkt);

        assert_eq!(
            dgram.len(),
            HEADER_SIZE + OVERLAY_PACKET_SIZE,
            "expected {} + {} = {} bytes, got {}",
            HEADER_SIZE,
            OVERLAY_PACKET_SIZE,
            HEADER_SIZE + OVERLAY_PACKET_SIZE,
            dgram.len()
        );
        // Confirm the constant values are what the spec says.
        assert_eq!(HEADER_SIZE, 16);
        assert_eq!(OVERLAY_PACKET_SIZE, 128);
        assert_eq!(dgram.len(), 144);
    }

    // 3. Parsed header has CHANNEL_CONTROL and OVERLAY_DATA msg_type.
    #[test]
    fn overlay_datagram_valid_header() {
        let pkt = build_overlay_packet(15, 2_000, 5_000, 3, 2560, 1440);
        let dgram = build_overlay_datagram(&pkt);

        let hdr = PrismHeader::decode_from_slice(&dgram).expect("header decode");
        assert_eq!(hdr.channel_id, CHANNEL_CONTROL);
        assert_eq!(hdr.msg_type, OVERLAY_DATA);
        assert_eq!(hdr.payload_length, OVERLAY_PACKET_SIZE as u32);
        assert_eq!(hdr.version, PROTOCOL_VERSION);
    }
}
