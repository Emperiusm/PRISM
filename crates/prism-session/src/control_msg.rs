// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2025-2026 Ehsan Khalid. All rights reserved.
// Licensed under the GNU Affero General Public License v3.0.
// Commercial licensing available — see LICENSE-COMMERCIAL.md.

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

/// Sent by client when switching active profile during a session.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProfileSwitchPayload {
    pub profile_name: String,
    pub max_fps: u8,
    pub encoder_preset: String,
    pub prefer_lossless_text: bool,
    pub region_detection: bool,
}

/// Sent by client to request specific quality parameter changes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct QualityUpdatePayload {
    pub encoder_preset: Option<String>,
    pub max_fps: Option<u8>,
    pub bitrate_bps: Option<u64>,
    pub lossless_text: Option<bool>,
    pub region_detection: Option<bool>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

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
    fn profile_switch_json_roundtrip() {
        let payload = ProfileSwitchPayload {
            profile_name: "Gaming".to_string(),
            max_fps: 120,
            encoder_preset: "UltraLowLatency".to_string(),
            prefer_lossless_text: false,
            region_detection: false,
        };
        let json = serde_json::to_string(&payload).expect("serialize");
        let back: ProfileSwitchPayload = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(payload, back);
    }

    #[test]
    fn quality_update_json_roundtrip() {
        let payload = QualityUpdatePayload {
            encoder_preset: Some("Balanced".to_string()),
            max_fps: Some(90),
            bitrate_bps: Some(20_000_000),
            lossless_text: Some(true),
            region_detection: Some(false),
        };
        let json = serde_json::to_string(&payload).expect("serialize");
        let back: QualityUpdatePayload = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(payload, back);
    }

    #[test]
    fn count_is_nineteen() {
        // Sanity: exactly 19 distinct type constants
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
        assert_eq!(types.len(), 19);
    }
}
