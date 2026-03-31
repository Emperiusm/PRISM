use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum SessionError {
    #[error("unknown channel: 0x{0:03X}")]
    UnknownChannel(u16),
    #[error("channel already owned by {0}")]
    ChannelConflict(Uuid),
    #[error("client not found: {0}")]
    ClientNotFound(Uuid),
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
    fn unknown_channel_display_format() {
        let e = SessionError::UnknownChannel(0x0E1);
        assert_eq!(e.to_string(), "unknown channel: 0x0E1");
    }

    #[test]
    fn channel_conflict_display() {
        let id = Uuid::nil();
        let e = SessionError::ChannelConflict(id);
        assert!(e.to_string().starts_with("channel already owned by"));
    }

    #[test]
    fn client_not_found_display() {
        let id = Uuid::nil();
        let e = SessionError::ClientNotFound(id);
        assert!(e.to_string().starts_with("client not found:"));
    }

    #[test]
    fn negotiation_failed_display() {
        let e = SessionError::NegotiationFailed("codec mismatch".to_string());
        assert_eq!(e.to_string(), "negotiation failed: codec mismatch");
    }

    #[test]
    fn tombstone_expired_display() {
        let e = SessionError::TombstoneExpired;
        assert_eq!(e.to_string(), "tombstone expired");
    }

    #[test]
    fn transport_display() {
        let e = SessionError::Transport("connection reset".to_string());
        assert_eq!(e.to_string(), "transport error: connection reset");
    }
}
