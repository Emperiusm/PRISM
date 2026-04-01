// cursor_sender.rs — Cursor shape change detection and wire serialisation.
//
// Wire format for a cursor shape payload:
//   [4B width u32 LE][4B height u32 LE][4B hotspot_x u32 LE][4B hotspot_y u32 LE]
//   [8B hash u64 LE][N bytes RGBA pixel data]

use bytes::Bytes;
use prism_display::cursor::{CursorManager, CursorShape};

/// Byte offset at which pixel data begins in the serialised shape.
const HEADER_LEN: usize = 24; // 4+4+4+4+8

/// Tracks cursor shape state and emits serialised payloads only when the shape
/// actually changes.  Thread-affine — not `Send + Sync` by itself; callers
/// should place it inside a task or behind a mutex if sharing is needed.
pub struct CursorSender {
    manager: CursorManager,
    shapes_sent: u32,
}

impl CursorSender {
    /// Create a new sender with no initial shape.
    pub fn new() -> Self {
        Self {
            manager: CursorManager::new(),
            shapes_sent: 0,
        }
    }

    /// Offer a new cursor shape.
    ///
    /// Returns `Some(bytes)` if the shape differs from the last transmitted
    /// shape (i.e. the caller should send the payload on the wire).  Returns
    /// `None` when the shape hash is unchanged and no transmission is needed.
    pub fn update_shape(&mut self, shape: CursorShape) -> Option<Bytes> {
        if self.manager.update_shape(shape.clone()) {
            self.shapes_sent += 1;
            Some(serialize_cursor_shape(&shape))
        } else {
            None
        }
    }

    /// Number of distinct shapes transmitted so far (i.e. how many times
    /// `update_shape` returned `Some`).
    pub fn shapes_sent(&self) -> u32 {
        self.shapes_sent
    }
}

impl Default for CursorSender {
    fn default() -> Self {
        Self::new()
    }
}

/// Serialise a `CursorShape` to a contiguous byte buffer.
///
/// Format (all integers little-endian):
/// ```text
/// bytes  0– 3  width     (u32)
/// bytes  4– 7  height    (u32)
/// bytes  8–11  hotspot_x (u32)
/// bytes 12–15  hotspot_y (u32)
/// bytes 16–23  hash      (u64)
/// bytes 24..   RGBA pixel data
/// ```
pub fn serialize_cursor_shape(shape: &CursorShape) -> Bytes {
    let mut buf = Vec::with_capacity(HEADER_LEN + shape.data.len());
    buf.extend_from_slice(&shape.width.to_le_bytes());
    buf.extend_from_slice(&shape.height.to_le_bytes());
    buf.extend_from_slice(&shape.hotspot_x.to_le_bytes());
    buf.extend_from_slice(&shape.hotspot_y.to_le_bytes());
    buf.extend_from_slice(&shape.hash.to_le_bytes());
    buf.extend_from_slice(&shape.data);
    Bytes::from(buf)
}

/// Deserialise a `CursorShape` from a byte slice produced by
/// [`serialize_cursor_shape`].
///
/// Returns `None` if `data` is shorter than the 24-byte header.
pub fn deserialize_cursor_shape(data: &[u8]) -> Option<CursorShape> {
    if data.len() < HEADER_LEN {
        return None;
    }
    let width = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
    let height = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);
    let hotspot_x = u32::from_le_bytes([data[8], data[9], data[10], data[11]]);
    let hotspot_y = u32::from_le_bytes([data[12], data[13], data[14], data[15]]);
    let hash = u64::from_le_bytes([
        data[16], data[17], data[18], data[19],
        data[20], data[21], data[22], data[23],
    ]);
    Some(CursorShape {
        width,
        height,
        hotspot_x,
        hotspot_y,
        hash,
        data: Bytes::copy_from_slice(&data[HEADER_LEN..]),
    })
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal `CursorShape` with a specific hash and distinct pixel data.
    fn make_shape(hash: u64) -> CursorShape {
        CursorShape {
            width: 32,
            height: 32,
            hotspot_x: 1,
            hotspot_y: 2,
            hash,
            // Fill data with the low byte of hash so different hashes produce
            // different pixel payloads (important for roundtrip test).
            data: Bytes::from(vec![hash as u8; 32 * 32 * 4]),
        }
    }

    #[test]
    fn new_shape_triggers_send() {
        let mut sender = CursorSender::new();
        // The very first shape must always be transmitted.
        let result = sender.update_shape(make_shape(0xDEAD_BEEF));
        assert!(result.is_some(), "first shape must produce Some");
        assert_eq!(sender.shapes_sent(), 1);
    }

    #[test]
    fn same_shape_suppressed() {
        let mut sender = CursorSender::new();
        sender.update_shape(make_shape(0xAAAA_BBBB));
        // Second update with identical hash → suppressed.
        let result = sender.update_shape(make_shape(0xAAAA_BBBB));
        assert!(result.is_none(), "duplicate shape must be suppressed");
        assert_eq!(sender.shapes_sent(), 1);
    }

    #[test]
    fn different_shape_sent() {
        let mut sender = CursorSender::new();
        sender.update_shape(make_shape(0x1111_1111));
        // New hash → a second payload must be produced.
        let result = sender.update_shape(make_shape(0x2222_2222));
        assert!(result.is_some(), "changed shape must produce Some");
        assert_eq!(sender.shapes_sent(), 2);
    }

    #[test]
    fn serialize_deserialize_roundtrip() {
        let original = CursorShape {
            width: 16,
            height: 16,
            hotspot_x: 3,
            hotspot_y: 7,
            hash: 0xCAFE_BABE_DEAD_BEEF,
            data: Bytes::from(vec![0xABu8; 16 * 16 * 4]),
        };

        let bytes = serialize_cursor_shape(&original);
        let recovered = deserialize_cursor_shape(&bytes)
            .expect("deserialize must succeed for valid data");

        assert_eq!(recovered.width, original.width);
        assert_eq!(recovered.height, original.height);
        assert_eq!(recovered.hotspot_x, original.hotspot_x);
        assert_eq!(recovered.hotspot_y, original.hotspot_y);
        assert_eq!(recovered.hash, original.hash);
        assert_eq!(recovered.data, original.data);
    }
}
