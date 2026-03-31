/// Fixed byte size of a serialised `SlicePayloadHeader`.
pub const SLICE_HEADER_SIZE: usize = 24;

/// Wire-format header prepended to every encoded slice payload.
///
/// Layout (little-endian, 24 bytes total):
///
/// ```text
///  Offset  Size  Field
///  ------  ----  -----
///   0       1    decoder_slot
///   1       1    slice_index
///   2       1    total_slices
///   3       1    encoding_type
///   4       2    rect_x      (i16)
///   6       2    rect_y      (i16)
///   8       2    rect_w      (u16)
///  10       2    rect_h      (u16)
///  12       1    region_count
///  13       1    is_preview
///  14       4    replaces_seq (u32)
///  18       2    cursor_x    (u16)
///  20       2    cursor_y    (u16)
///  22       1    cursor_flags
///  23       1    _reserved
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlicePayloadHeader {
    /// Which decoder state slot the remote should use for this stream.
    pub decoder_slot: u8,
    /// Zero-based slice index within this region.
    pub slice_index: u8,
    /// Total number of slices for this region.
    pub total_slices: u8,
    /// Encoding type identifier (codec / strategy discriminant).
    pub encoding_type: u8,
    /// Region X origin in display coordinates.
    pub rect_x: i16,
    /// Region Y origin in display coordinates.
    pub rect_y: i16,
    /// Region width in pixels.
    pub rect_w: u16,
    /// Region height in pixels.
    pub rect_h: u16,
    /// Number of distinct regions in this frame.
    pub region_count: u8,
    /// Non-zero when this is a preview-quality encode.
    pub is_preview: u8,
    /// Sequence number of the frame this supersedes, or 0 when not applicable.
    pub replaces_seq: u32,
    /// Cursor X position piggybacked on the slice.
    pub cursor_x: u16,
    /// Cursor Y position piggybacked on the slice.
    pub cursor_y: u16,
    /// Cursor state flags (visibility, shape change, etc.).
    pub cursor_flags: u8,
    /// Padding / reserved for future use.
    pub _reserved: u8,
}

impl SlicePayloadHeader {
    /// Serialise to a fixed-size 24-byte array (little-endian).
    pub fn to_bytes(&self) -> [u8; SLICE_HEADER_SIZE] {
        let mut buf = [0u8; SLICE_HEADER_SIZE];
        self.encode_to_slice(&mut buf);
        buf
    }

    /// Zero-copy write into an existing buffer slice.
    ///
    /// Panics if `buf.len() < SLICE_HEADER_SIZE`.
    ///
    /// Returns the number of bytes written (always `SLICE_HEADER_SIZE`).
    pub fn encode_to_slice(&self, buf: &mut [u8]) -> usize {
        assert!(
            buf.len() >= SLICE_HEADER_SIZE,
            "buffer too small: {} < {}",
            buf.len(),
            SLICE_HEADER_SIZE
        );

        buf[0] = self.decoder_slot;
        buf[1] = self.slice_index;
        buf[2] = self.total_slices;
        buf[3] = self.encoding_type;

        let rect_x = self.rect_x.to_le_bytes();
        buf[4] = rect_x[0];
        buf[5] = rect_x[1];

        let rect_y = self.rect_y.to_le_bytes();
        buf[6] = rect_y[0];
        buf[7] = rect_y[1];

        let rect_w = self.rect_w.to_le_bytes();
        buf[8] = rect_w[0];
        buf[9] = rect_w[1];

        let rect_h = self.rect_h.to_le_bytes();
        buf[10] = rect_h[0];
        buf[11] = rect_h[1];

        buf[12] = self.region_count;
        buf[13] = self.is_preview;

        let replaces = self.replaces_seq.to_le_bytes();
        buf[14] = replaces[0];
        buf[15] = replaces[1];
        buf[16] = replaces[2];
        buf[17] = replaces[3];

        let cx = self.cursor_x.to_le_bytes();
        buf[18] = cx[0];
        buf[19] = cx[1];

        let cy = self.cursor_y.to_le_bytes();
        buf[20] = cy[0];
        buf[21] = cy[1];

        buf[22] = self.cursor_flags;
        buf[23] = self._reserved;

        SLICE_HEADER_SIZE
    }

    /// Deserialise from a byte slice.
    ///
    /// Returns `None` if the slice is shorter than `SLICE_HEADER_SIZE`.
    pub fn from_bytes(src: &[u8]) -> Option<Self> {
        if src.len() < SLICE_HEADER_SIZE {
            return None;
        }

        Some(Self {
            decoder_slot: src[0],
            slice_index: src[1],
            total_slices: src[2],
            encoding_type: src[3],
            rect_x: i16::from_le_bytes([src[4], src[5]]),
            rect_y: i16::from_le_bytes([src[6], src[7]]),
            rect_w: u16::from_le_bytes([src[8], src[9]]),
            rect_h: u16::from_le_bytes([src[10], src[11]]),
            region_count: src[12],
            is_preview: src[13],
            replaces_seq: u32::from_le_bytes([src[14], src[15], src[16], src[17]]),
            cursor_x: u16::from_le_bytes([src[18], src[19]]),
            cursor_y: u16::from_le_bytes([src[20], src[21]]),
            cursor_flags: src[22],
            _reserved: src[23],
        })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_header() -> SlicePayloadHeader {
        SlicePayloadHeader {
            decoder_slot: 1,
            slice_index: 0,
            total_slices: 2,
            encoding_type: 3,
            rect_x: -100,
            rect_y: 200,
            rect_w: 1280,
            rect_h: 720,
            region_count: 4,
            is_preview: 0,
            replaces_seq: 70_000,
            cursor_x: 32_768,
            cursor_y: 512,
            cursor_flags: 0x03,
            _reserved: 0,
        }
    }

    #[test]
    fn slice_header_size_is_24() {
        assert_eq!(SLICE_HEADER_SIZE, 24);
        let hdr = sample_header();
        assert_eq!(hdr.to_bytes().len(), 24);
    }

    #[test]
    fn slice_header_roundtrip() {
        let original = sample_header();
        let bytes = original.to_bytes();
        let recovered = SlicePayloadHeader::from_bytes(&bytes).expect("roundtrip failed");

        assert_eq!(recovered, original);

        // Spot-check the non-trivial values called out in the spec.
        assert_eq!(recovered.replaces_seq, 70_000, "replaces_seq must survive LE packing");
        assert_eq!(recovered.cursor_x, 32_768, "cursor_x=32768 must survive");
        assert_eq!(recovered.cursor_flags, 0x03, "cursor_flags must survive");
    }

    #[test]
    fn encode_to_slice_matches_to_bytes() {
        let hdr = sample_header();
        let reference = hdr.to_bytes();

        let mut buf = [0u8; 48]; // oversized on purpose
        let written = hdr.encode_to_slice(&mut buf);

        assert_eq!(written, SLICE_HEADER_SIZE);
        assert_eq!(&buf[..SLICE_HEADER_SIZE], &reference[..]);
        // Bytes beyond the header should be untouched.
        assert!(buf[SLICE_HEADER_SIZE..].iter().all(|&b| b == 0));
    }

    #[test]
    fn from_bytes_returns_none_on_short_slice() {
        let short = [0u8; 10];
        assert!(SlicePayloadHeader::from_bytes(&short).is_none());
    }
}
