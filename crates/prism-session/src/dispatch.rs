use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use thiserror::Error;

use crate::types::ClientId;

#[derive(Debug, Error)]
pub enum ChannelError {
    #[error("handler error: {0}")]
    HandlerError(String),
}

/// Implemented by any component that processes incoming datagrams for a
/// specific channel. Register instances with [`ChannelDispatcher`].
#[async_trait]
pub trait ChannelHandler: Send + Sync {
    fn channel_id(&self) -> u16;
    async fn handle_datagram(&self, from: ClientId, data: Bytes) -> Result<(), ChannelError>;

    /// Handle an incoming bidirectional stream for this channel.
    ///
    /// The default implementation immediately drops both stream halves and
    /// returns `Ok(())`, making this a non-breaking addition for existing
    /// implementors.
    async fn handle_stream(
        &self,
        _from: ClientId,
        _send: prism_transport::OwnedSendStream,
        _recv: prism_transport::OwnedRecvStream,
    ) -> Result<(), ChannelError> {
        Ok(())
    }
}

/// Routes incoming datagrams to the correct [`ChannelHandler`] by channel ID.
pub struct ChannelDispatcher {
    handlers: HashMap<u16, Arc<dyn ChannelHandler>>,
}

impl ChannelDispatcher {
    pub fn new() -> Self {
        Self {
            handlers: HashMap::new(),
        }
    }

    /// Register a handler, replacing any previous handler for the same channel.
    pub fn register(&mut self, handler: Arc<dyn ChannelHandler>) {
        self.handlers.insert(handler.channel_id(), handler);
    }

    /// Look up the handler for a channel without dispatching.
    pub fn handler(&self, channel_id: u16) -> Option<&Arc<dyn ChannelHandler>> {
        self.handlers.get(&channel_id)
    }

    /// Dispatch a datagram to the registered handler, silently dropping it if
    /// no handler is registered for that channel.
    pub async fn dispatch(&self, from: ClientId, channel_id: u16, data: Bytes) {
        if let Some(handler) = self.handlers.get(&channel_id) {
            let _ = handler.handle_datagram(from, data).await;
        }
    }
}

impl Default for ChannelDispatcher {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use uuid::Uuid;

    struct MockHandler {
        id: u16,
        call_count: Arc<AtomicU32>,
    }

    #[async_trait]
    impl ChannelHandler for MockHandler {
        fn channel_id(&self) -> u16 {
            self.id
        }

        async fn handle_datagram(&self, _from: ClientId, _data: Bytes) -> Result<(), ChannelError> {
            self.call_count.fetch_add(1, Ordering::Relaxed);
            Ok(())
        }
    }

    fn make_handler(id: u16) -> (Arc<MockHandler>, Arc<AtomicU32>) {
        let counter = Arc::new(AtomicU32::new(0));
        let h = Arc::new(MockHandler { id, call_count: counter.clone() });
        (h, counter)
    }

    fn client() -> ClientId {
        Uuid::nil()
    }

    #[test]
    fn register_and_lookup() {
        let mut disp = ChannelDispatcher::new();
        let (h, _) = make_handler(0x001);
        disp.register(h);

        assert!(disp.handler(0x001).is_some());
        assert!(disp.handler(0x002).is_none());
    }

    #[tokio::test]
    async fn dispatch_routes_to_handler() {
        let mut disp = ChannelDispatcher::new();
        let (h, count) = make_handler(0x001);
        disp.register(h);

        disp.dispatch(client(), 0x001, Bytes::from_static(b"hello")).await;
        assert_eq!(count.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn dispatch_unknown_channel_ignored() {
        let disp = ChannelDispatcher::new();
        // Should not panic
        disp.dispatch(client(), 0x999, Bytes::from_static(b"ignored")).await;
    }

    #[test]
    fn register_multiple_handlers() {
        let mut disp = ChannelDispatcher::new();
        let (h1, _) = make_handler(0x001);
        let (h2, _) = make_handler(0x002);
        let (h3, _) = make_handler(0x003);
        disp.register(h1);
        disp.register(h2);
        disp.register(h3);

        assert!(disp.handler(0x001).is_some());
        assert!(disp.handler(0x002).is_some());
        assert!(disp.handler(0x003).is_some());
        assert!(disp.handler(0x004).is_none());
    }
}
