use bytes::Bytes;
use prism_protocol::{
    channel::CHANNEL_INPUT,
    header::{PrismHeader, HEADER_SIZE, PROTOCOL_VERSION},
    input::{InputEvent, INPUT_EVENT_SIZE},
};

/// Map window pixel coordinates to the 0–65535 normalised range used by the
/// PRISM input wire format.
///
/// Clamps to `[0, 65535]` so out-of-window pointer positions are safe.
pub fn normalize_mouse(x: f32, y: f32, window_w: u32, window_h: u32) -> (u16, u16) {
    let nx = if window_w == 0 {
        0u16
    } else {
        ((x / window_w as f32) * 65535.0).clamp(0.0, 65535.0).round() as u16
    };

    let ny = if window_h == 0 {
        0u16
    } else {
        ((y / window_h as f32) * 65535.0).clamp(0.0, 65535.0).round() as u16
    };

    (nx, ny)
}

/// Builds PRISM input datagrams with a pre-encoded header template.
///
/// The header is encoded once at construction time; `build_datagram` clones
/// it, patches the monotonically-increasing sequence number in-place (bytes
/// 4–7, little-endian), and appends the serialised [`InputEvent`].
pub struct InputSender {
    /// Pre-built 16-byte header with CHANNEL_INPUT, msg_type=0x01,
    /// payload_length=INPUT_EVENT_SIZE. Sequence field is overwritten per
    /// datagram.
    header_template: [u8; HEADER_SIZE],
    sequence: u32,
}

impl InputSender {
    /// Create a new sender. The template header is encoded here so that hot
    /// path datagram construction only needs a memcpy + sequence patch.
    pub fn new() -> Self {
        let header = PrismHeader {
            version: PROTOCOL_VERSION,
            channel_id: CHANNEL_INPUT,
            msg_type: 0x01,
            flags: 0,
            sequence: 0,
            timestamp_us: 0,
            payload_length: INPUT_EVENT_SIZE as u32,
        };
        let mut template = [0u8; HEADER_SIZE];
        header.encode_to_slice(&mut template);

        Self {
            header_template: template,
            sequence: 0,
        }
    }

    /// Build a complete datagram for the given input event.
    ///
    /// The sequence number is stamped into bytes 4–7 of the header copy, then
    /// incremented so each successive call has a unique sequence.
    pub fn build_datagram(&mut self, event: InputEvent) -> Bytes {
        let mut buf = [0u8; HEADER_SIZE + INPUT_EVENT_SIZE];

        // Copy the pre-built template.
        buf[..HEADER_SIZE].copy_from_slice(&self.header_template);

        // Patch the sequence field (bytes 4-7, little-endian).
        buf[4..8].copy_from_slice(&self.sequence.to_le_bytes());

        // Append the serialised event.
        buf[HEADER_SIZE..].copy_from_slice(&event.to_bytes());

        self.sequence = self.sequence.wrapping_add(1);

        Bytes::copy_from_slice(&buf)
    }

    /// Current sequence counter value (equals the number of datagrams built).
    pub fn sequence(&self) -> u32 {
        self.sequence
    }
}

impl Default for InputSender {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use prism_protocol::{
        channel::CHANNEL_INPUT,
        header::{PrismHeader, HEADER_SIZE},
        input::INPUT_EVENT_SIZE,
    };

    // ── 1. build_datagram_correct_size ──────────────────────────────────────

    #[test]
    fn build_datagram_correct_size() {
        let mut sender = InputSender::new();
        let event = InputEvent::KeyDown { scancode: 0x001C, vk: 0x000D };
        let dg = sender.build_datagram(event);
        assert_eq!(dg.len(), HEADER_SIZE + INPUT_EVENT_SIZE);
    }

    // ── 2. sequence_increments ──────────────────────────────────────────────

    #[test]
    fn sequence_increments() {
        let mut sender = InputSender::new();
        let event = InputEvent::MouseMove { x: 100, y: 200 };
        sender.build_datagram(event);
        sender.build_datagram(event);
        assert_eq!(sender.sequence(), 2);
    }

    // ── 3. datagram_has_valid_header ────────────────────────────────────────

    #[test]
    fn datagram_has_valid_header() {
        let mut sender = InputSender::new();
        let event = InputEvent::MouseMove { x: 0, y: 0 };
        let dg = sender.build_datagram(event);
        let header = PrismHeader::decode_from_slice(&dg).expect("header decode failed");
        assert_eq!(header.channel_id, CHANNEL_INPUT);
        assert_eq!(header.payload_length, INPUT_EVENT_SIZE as u32);
    }

    // ── 4. normalize_center ─────────────────────────────────────────────────

    #[test]
    fn normalize_center() {
        let (nx, ny) = normalize_mouse(960.0, 540.0, 1920, 1080);
        // Allow ±1 LSB rounding tolerance.
        assert!((nx as i32 - 32768).abs() <= 1, "nx={nx}");
        assert!((ny as i32 - 32768).abs() <= 1, "ny={ny}");
    }

    // ── 5. normalize_edges ──────────────────────────────────────────────────

    #[test]
    fn normalize_edges() {
        // Top-left corner.
        let (nx, ny) = normalize_mouse(0.0, 0.0, 1920, 1080);
        assert_eq!(nx, 0);
        assert_eq!(ny, 0);

        // Right edge: x == window_w maps to 65535.
        let (nx, ny) = normalize_mouse(1920.0, 0.0, 1920, 1080);
        assert_eq!(nx, 65535);
        assert_eq!(ny, 0);
    }
}
