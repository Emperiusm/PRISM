use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub type ClientId = Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionState {
    Authenticating,
    Active,
    Suspended,
    Tombstoned,
}

#[derive(Debug, Clone)]
pub enum SessionEvent {
    ClientConnected {
        client_id: ClientId,
        device_name: String,
    },
    ClientDisconnected {
        client_id: ClientId,
        reason: String,
    },
    ClientReconnected {
        client_id: ClientId,
        was_tombstoned: bool,
    },
    ChannelOwnershipChanged {
        channel_id: u16,
        new_owner: Option<ClientId>,
    },
    ProfileChanged {
        client_id: ClientId,
        profile: String,
    },
}

#[derive(Debug, Clone)]
pub enum ArbiterEvent {
    AllocationChanged {
        client_id: ClientId,
    },
    StarvationWarning {
        client_id: ClientId,
        channel_id: u16,
    },
    ReduceSendRate {
        client_id: ClientId,
        suggested_reduction: f32,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_id_is_uuid() {
        let id: ClientId = Uuid::new_v7(uuid::Timestamp::now(uuid::NoContext));
        assert!(!id.is_nil());
    }

    #[test]
    fn session_state_serde_roundtrip() {
        for state in [
            SessionState::Authenticating,
            SessionState::Active,
            SessionState::Suspended,
            SessionState::Tombstoned,
        ] {
            let json = serde_json::to_string(&state).expect("serialize");
            let back: SessionState = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(state, back);
        }
    }

    #[test]
    fn session_event_client_connected_fields() {
        let id = Uuid::nil();
        let ev = SessionEvent::ClientConnected {
            client_id: id,
            device_name: "test-device".to_string(),
        };
        match ev {
            SessionEvent::ClientConnected { client_id, device_name } => {
                assert_eq!(client_id, id);
                assert_eq!(device_name, "test-device");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn arbiter_event_reduce_send_rate_fields() {
        let id = Uuid::nil();
        let ev = ArbiterEvent::ReduceSendRate {
            client_id: id,
            suggested_reduction: 0.25,
        };
        match ev {
            ArbiterEvent::ReduceSendRate { client_id, suggested_reduction } => {
                assert_eq!(client_id, id);
                assert!((suggested_reduction - 0.25).abs() < f32::EPSILON);
            }
            _ => panic!("wrong variant"),
        }
    }
}
