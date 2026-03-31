use serde::{Deserialize, Serialize};

// === Control message type constants ===
pub const HEARTBEAT: u8 = 0x01;
pub const HEARTBEAT_ACK: u8 = 0x02;
pub const CAPABILITY_UPDATE: u8 = 0x03;
pub const PROFILE_SWITCH: u8 = 0x04;
pub const PROBE_REQUEST: u8 = 0x05;
pub const PROBE_RESPONSE: u8 = 0x06;
pub const CLIENT_FEEDBACK: u8 = 0x07;
pub const CLIENT_ALERT: u8 = 0x08;
pub const OVERLAY_TOGGLE: u8 = 0x09;
pub const OVERLAY_DATA: u8 = 0x0A;
pub const KEY_ROTATION: u8 = 0x0B;
pub const CERT_RENEWAL: u8 = 0x0C;
pub const QUALITY_UPDATE: u8 = 0x0D;
pub const REDUCE_SEND_RATE: u8 = 0x0E;
pub const CHANNEL_TRANSFER: u8 = 0x0F;
pub const SESSION_INFO: u8 = 0x10;
pub const MONITOR_LAYOUT: u8 = 0x11;
pub const THROUGHPUT_TOKEN: u8 = 0x12;
pub const SHUTDOWN_NOTICE: u8 = 0x20;

/// Sent by the server when it plans to shut down or restart.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ShutdownNotice {
    pub reason: String,
    pub seconds_remaining: u32,
    pub will_restart: bool,
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use super::*;

    #[test]
    fn all_msg_types_distinct() {
        let types = [
            HEARTBEAT,
            HEARTBEAT_ACK,
            CAPABILITY_UPDATE,
            PROFILE_SWITCH,
            PROBE_REQUEST,
            PROBE_RESPONSE,
            CLIENT_FEEDBACK,
            CLIENT_ALERT,
            OVERLAY_TOGGLE,
            OVERLAY_DATA,
            KEY_ROTATION,
            CERT_RENEWAL,
            QUALITY_UPDATE,
            REDUCE_SEND_RATE,
            CHANNEL_TRANSFER,
            SESSION_INFO,
            MONITOR_LAYOUT,
            THROUGHPUT_TOKEN,
            SHUTDOWN_NOTICE,
        ];
        let set: HashSet<u8> = types.iter().copied().collect();
        assert_eq!(set.len(), types.len(), "duplicate message type constants");
    }

    #[test]
    fn shutdown_notice_json_roundtrip() {
        let notice = ShutdownNotice {
            reason: "maintenance".to_string(),
            seconds_remaining: 30,
            will_restart: true,
        };
        let json = serde_json::to_string(&notice).expect("serialize");
        let back: ShutdownNotice = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(notice, back);
    }

    #[test]
    fn shutdown_notice_no_restart() {
        let notice = ShutdownNotice {
            reason: "crash".to_string(),
            seconds_remaining: 0,
            will_restart: false,
        };
        let json = serde_json::to_string(&notice).expect("serialize");
        let back: ShutdownNotice = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.will_restart, false);
        assert_eq!(back.seconds_remaining, 0);
    }

    #[test]
    fn count_is_nineteen() {
        // Sanity: exactly 19 distinct type constants
        let types = [
            HEARTBEAT, HEARTBEAT_ACK, CAPABILITY_UPDATE, PROFILE_SWITCH, PROBE_REQUEST,
            PROBE_RESPONSE, CLIENT_FEEDBACK, CLIENT_ALERT, OVERLAY_TOGGLE, OVERLAY_DATA,
            KEY_ROTATION, CERT_RENEWAL, QUALITY_UPDATE, REDUCE_SEND_RATE, CHANNEL_TRANSFER,
            SESSION_INFO, MONITOR_LAYOUT, THROUGHPUT_TOKEN, SHUTDOWN_NOTICE,
        ];
        assert_eq!(types.len(), 19);
    }
}
