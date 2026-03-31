// RecvLoop: per-client receive loop that reads datagrams/streams and dispatches them.
//
// The live QUIC receive loop requires a running connection and is deferred to a
// later task. This module contains the pure classification and bandwidth-tracking
// logic, which is fully testable without a real connection.

use bytes::Bytes;
use prism_protocol::header::{PrismHeader, HEADER_SIZE};
use prism_protocol::channel::CHANNEL_CONTROL;
use prism_session::ChannelBandwidthTracker;
use prism_session::control_msg;

/// The action the recv loop should take for a received datagram.
#[derive(Debug, PartialEq)]
pub enum DatagramAction {
    /// A probe-response datagram — used for RTT measurement.
    ProbeResponse,
    /// A channel data datagram — forward to the named channel handler.
    ChannelDispatch { channel_id: u16 },
    /// The datagram is malformed or too short — discard silently.
    Drop,
}

/// Classify an incoming raw datagram into the appropriate action.
///
/// Pure function; no I/O. Decodes the first `HEADER_SIZE` bytes and returns
/// the routing decision. Datagrams shorter than `HEADER_SIZE` or with an
/// invalid header are classified as `Drop`.
pub fn classify_datagram(data: &Bytes) -> DatagramAction {
    if data.len() < HEADER_SIZE {
        return DatagramAction::Drop;
    }
    match PrismHeader::decode_from_slice(data) {
        Ok(header) => {
            if header.channel_id == CHANNEL_CONTROL
                && header.msg_type == control_msg::PROBE_RESPONSE
            {
                DatagramAction::ProbeResponse
            } else {
                DatagramAction::ChannelDispatch {
                    channel_id: header.channel_id,
                }
            }
        }
        Err(_) => DatagramAction::Drop,
    }
}

/// Record the received bytes for this datagram in the bandwidth tracker.
///
/// Uses `header.payload_length` (the declared payload size) as the byte count.
/// Call this after a successful `classify_datagram` that did not return `Drop`.
#[inline(always)]
pub fn record_datagram_bandwidth(tracker: &ChannelBandwidthTracker, header: &PrismHeader) {
    tracker.record_recv(header.channel_id, header.payload_length);
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::BytesMut;

    /// Build a minimal datagram containing only the encoded header.
    fn make_datagram(channel_id: u16, msg_type: u8, payload_len: u32) -> Bytes {
        let header = PrismHeader {
            version: 0,
            channel_id,
            msg_type,
            flags: 0,
            sequence: 1,
            timestamp_us: 0,
            payload_length: payload_len,
        };
        let mut buf = BytesMut::with_capacity(HEADER_SIZE);
        header.encode(&mut buf);
        buf.freeze()
    }

    #[test]
    fn classify_probe_response() {
        let data = make_datagram(CHANNEL_CONTROL, control_msg::PROBE_RESPONSE, 0);
        assert_eq!(classify_datagram(&data), DatagramAction::ProbeResponse);
    }

    #[test]
    fn classify_display_datagram() {
        // Channel 0x001, arbitrary msg_type 0x02 → ChannelDispatch
        let data = make_datagram(0x001, 0x02, 1000);
        assert_eq!(
            classify_datagram(&data),
            DatagramAction::ChannelDispatch { channel_id: 0x001 }
        );
    }

    #[test]
    fn classify_control_non_probe() {
        // CHANNEL_CONTROL + HEARTBEAT → ChannelDispatch (not a probe response)
        let data = make_datagram(CHANNEL_CONTROL, control_msg::HEARTBEAT, 0);
        assert_eq!(
            classify_datagram(&data),
            DatagramAction::ChannelDispatch {
                channel_id: CHANNEL_CONTROL
            }
        );
    }

    #[test]
    fn classify_too_short_drops() {
        // Only 4 bytes — far below the 16-byte header minimum.
        let data = Bytes::from_static(&[0x00, 0x10, 0x01, 0x00]);
        assert_eq!(classify_datagram(&data), DatagramAction::Drop);
    }

    #[test]
    fn bandwidth_tracking() {
        let tracker = ChannelBandwidthTracker::new();
        let header = PrismHeader {
            version: 0,
            channel_id: 0x001,
            msg_type: 0x02,
            flags: 0,
            sequence: 1,
            timestamp_us: 0,
            payload_length: 5000,
        };
        record_datagram_bandwidth(&tracker, &header);
        assert_eq!(tracker.recv_bytes(0x001), 5000);
    }
}
