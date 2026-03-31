use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use async_trait::async_trait;
use bytes::Bytes;
use prism_protocol::{
    channel::CHANNEL_INPUT,
    header::{PrismHeader, HEADER_SIZE},
    input::{InputEvent, INPUT_EVENT_SIZE},
};
use prism_session::{
    dispatch::{ChannelError, ChannelHandler},
    types::ClientId,
};

/// Map a 0–65535 normalised coordinate to a screen pixel position.
///
/// Uses 64-bit intermediate arithmetic to avoid overflow on large screen
/// dimensions, then truncates back to `i32`.
pub fn normalized_to_screen(normalized: u16, screen_size: u32) -> i32 {
    (normalized as u64 * screen_size as u64 / 65535) as i32
}

/// Counters for the input channel handler.
#[derive(Debug, Default)]
pub struct InputStats {
    /// Total input events received (regardless of injection outcome).
    pub events_received: AtomicU32,
    /// Total input events successfully injected (or mock-injected on non-Windows).
    pub events_injected: AtomicU32,
}

/// `ChannelHandler` implementation for [`CHANNEL_INPUT`].
///
/// Parses incoming datagrams as `PrismHeader` + [`InputEvent`] and injects
/// the event into the local input subsystem. On non-Windows platforms the
/// injection is mocked (counter incremented, no OS call made).
pub struct InputChannelHandler {
    screen_width: u32,
    screen_height: u32,
    stats: Arc<InputStats>,
}

impl InputChannelHandler {
    /// Create a new handler for a screen with the given pixel dimensions.
    pub fn new(screen_width: u32, screen_height: u32) -> Self {
        Self {
            screen_width,
            screen_height,
            stats: Arc::new(InputStats::default()),
        }
    }

    /// Borrow the shared stats handle.
    pub fn stats(&self) -> Arc<InputStats> {
        self.stats.clone()
    }

    /// Inject a parsed input event. Returns `true` when injection succeeded.
    fn inject(&self, event: InputEvent) -> bool {
        // Translate normalised mouse coordinates to screen pixels when needed
        // so the compiler doesn't complain about unused fields.
        let _ = match event {
            InputEvent::MouseMove { x, y } => (
                normalized_to_screen(x, self.screen_width),
                normalized_to_screen(y, self.screen_height),
            ),
            _ => (0, 0),
        };

        #[cfg(target_os = "windows")]
        {
            // Win32 SendInput FFI will be wired up in a later task.
            // For now, treat every event as successfully queued.
            true
        }

        #[cfg(not(target_os = "windows"))]
        {
            // Mock injection on non-Windows platforms.
            true
        }
    }
}

impl Default for InputChannelHandler {
    fn default() -> Self {
        Self::new(1920, 1080)
    }
}

#[async_trait]
impl ChannelHandler for InputChannelHandler {
    fn channel_id(&self) -> u16 {
        CHANNEL_INPUT
    }

    async fn handle_datagram(&self, _from: ClientId, data: Bytes) -> Result<(), ChannelError> {
        // Need at least header + one full input event.
        if data.len() < HEADER_SIZE + INPUT_EVENT_SIZE {
            return Ok(());
        }

        // Silently discard if the header itself is malformed.
        if PrismHeader::decode_from_slice(&data).is_err() {
            return Ok(());
        }

        // Parse the input event that immediately follows the header.
        let event = match InputEvent::from_bytes(&data[HEADER_SIZE..]) {
            Some(e) => e,
            None => return Ok(()),
        };

        self.stats.events_received.fetch_add(1, Ordering::Relaxed);

        if self.inject(event) {
            self.stats.events_injected.fetch_add(1, Ordering::Relaxed);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::BytesMut;
    use prism_protocol::{
        channel::CHANNEL_INPUT,
        header::{PrismHeader, PROTOCOL_VERSION},
        input::{InputEvent, INPUT_EVENT_SIZE},
    };
    use uuid::Uuid;

    fn client() -> ClientId {
        Uuid::nil()
    }

    /// Build a valid datagram: PrismHeader + InputEvent bytes.
    fn make_input_datagram(event: InputEvent) -> Bytes {
        let header = PrismHeader {
            version: PROTOCOL_VERSION,
            channel_id: CHANNEL_INPUT,
            msg_type: 0x01,
            flags: 0,
            sequence: 0,
            timestamp_us: 0,
            payload_length: INPUT_EVENT_SIZE as u32,
        };
        let mut buf = BytesMut::with_capacity(HEADER_SIZE + INPUT_EVENT_SIZE);
        header.encode(&mut buf);
        buf.extend_from_slice(&event.to_bytes());
        buf.freeze()
    }

    // ── 1. normalized_to_screen_center ──────────────────────────────────────

    #[test]
    fn normalized_to_screen_center() {
        // 32768 / 65535 * 1920 ≈ 960
        let result = normalized_to_screen(32768, 1920);
        assert!(
            (result - 960).abs() <= 1,
            "expected ~960, got {result}"
        );
    }

    // ── 2. normalized_to_screen_edges ───────────────────────────────────────

    #[test]
    fn normalized_to_screen_edges() {
        assert_eq!(normalized_to_screen(0, 1920), 0);
        assert_eq!(normalized_to_screen(65535, 1920), 1920);
    }

    // ── 3. handler_channel_id ───────────────────────────────────────────────

    #[test]
    fn handler_channel_id() {
        let handler = InputChannelHandler::new(1920, 1080);
        assert_eq!(handler.channel_id(), CHANNEL_INPUT);
    }

    // ── 4. processes_key_down ───────────────────────────────────────────────

    #[tokio::test]
    async fn processes_key_down() {
        let handler = InputChannelHandler::new(1920, 1080);
        let stats = handler.stats();
        let event = InputEvent::KeyDown { scancode: 0x001C, vk: 0x000D };
        let data = make_input_datagram(event);
        handler.handle_datagram(client(), data).await.unwrap();
        assert_eq!(stats.events_received.load(Ordering::Relaxed), 1);
    }

    // ── 5. processes_mouse_move ─────────────────────────────────────────────

    #[tokio::test]
    async fn processes_mouse_move() {
        let handler = InputChannelHandler::new(1920, 1080);
        let stats = handler.stats();
        let event = InputEvent::MouseMove { x: 32768, y: 32768 };
        let data = make_input_datagram(event);
        handler.handle_datagram(client(), data).await.unwrap();
        assert_eq!(stats.events_received.load(Ordering::Relaxed), 1);
    }

    // ── 6. short_datagram_ignored ───────────────────────────────────────────

    #[tokio::test]
    async fn short_datagram_ignored() {
        let handler = InputChannelHandler::new(1920, 1080);
        let stats = handler.stats();
        // 10 bytes — well below HEADER_SIZE (16) + INPUT_EVENT_SIZE (12) = 28.
        let data = Bytes::from(vec![0u8; 10]);
        handler.handle_datagram(client(), data).await.unwrap();
        assert_eq!(stats.events_received.load(Ordering::Relaxed), 0);
    }
}
