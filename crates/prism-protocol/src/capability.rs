use serde::{Deserialize, Serialize};

/// Client capabilities sent during handshake (R1).
/// New phases add new channel entries without changing the format.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ClientCapabilities {
    pub protocol_version: u16,
    pub channels: Vec<ChannelCap>,
    pub performance: PerformanceProfile,
}

/// Per-channel capability advertisement.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChannelCap {
    pub channel_id: u16,
    pub channel_version: u16,
    pub config: ChannelConfig,
}

/// Channel-specific configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum ChannelConfig {
    Display(DisplayChannelConfig),
    Input(InputChannelConfig),
    Control,
    Generic,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DisplayChannelConfig {
    pub max_resolution: (u32, u32),
    pub max_fps: u8,
    pub supported_codecs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InputChannelConfig {
    pub devices: Vec<String>,
}

/// Client performance profile (R29).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PerformanceProfile {
    pub max_decode_resolution: (u32, u32),
    pub max_decoder_instances: u8,
    pub supported_codecs: Vec<String>,
    pub can_composite_regions: bool,
    pub estimated_decode_fps: u16,
}

/// Server capabilities sent in handshake response.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ServerCapabilities {
    pub protocol_version: u16,
    pub channels: Vec<ChannelCap>,
    pub negotiated_codec: String,
    pub display_resolution: (u32, u32),
    pub display_fps: u8,
}

/// Channel assignment: maps PRISM channel IDs to QUIC resources (R2).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChannelAssignment {
    pub channel_id: u16,
    pub stream_id: Option<u64>,
    pub uses_datagrams: bool,
}

impl ClientCapabilities {
    /// Check if client supports a specific channel.
    pub fn supports_channel(&self, channel_id: u16) -> bool {
        self.channels.iter().any(|c| c.channel_id == channel_id)
    }
}

impl Default for PerformanceProfile {
    fn default() -> Self {
        Self {
            max_decode_resolution: (1920, 1080),
            max_decoder_instances: 1,
            supported_codecs: vec!["h264".to_string()],
            can_composite_regions: false,
            estimated_decode_fps: 30,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_client_caps() -> ClientCapabilities {
        ClientCapabilities {
            protocol_version: 1,
            channels: vec![
                ChannelCap {
                    channel_id: 0x001,
                    channel_version: 1,
                    config: ChannelConfig::Display(DisplayChannelConfig {
                        max_resolution: (2560, 1440),
                        max_fps: 60,
                        supported_codecs: vec!["h264".into(), "h265".into()],
                    }),
                },
                ChannelCap {
                    channel_id: 0x002,
                    channel_version: 1,
                    config: ChannelConfig::Input(InputChannelConfig {
                        devices: vec!["keyboard".into(), "mouse".into()],
                    }),
                },
                ChannelCap {
                    channel_id: 0x006,
                    channel_version: 1,
                    config: ChannelConfig::Control,
                },
            ],
            performance: PerformanceProfile::default(),
        }
    }

    #[test]
    fn capabilities_json_roundtrip() {
        let caps = sample_client_caps();
        let json = serde_json::to_string(&caps).unwrap();
        let decoded: ClientCapabilities = serde_json::from_str(&json).unwrap();
        assert_eq!(caps, decoded);
    }

    #[test]
    fn supports_channel() {
        let caps = sample_client_caps();
        assert!(caps.supports_channel(0x001));
        assert!(caps.supports_channel(0x002));
        assert!(caps.supports_channel(0x006));
        assert!(!caps.supports_channel(0x003));
        assert!(!caps.supports_channel(0x0E1));
    }

    #[test]
    fn unknown_channels_filtered_by_server() {
        let caps = ClientCapabilities {
            protocol_version: 1,
            channels: vec![
                ChannelCap {
                    channel_id: 0x001,
                    channel_version: 1,
                    config: ChannelConfig::Generic,
                },
                ChannelCap {
                    channel_id: 0x0E1,
                    channel_version: 1,
                    config: ChannelConfig::Generic,
                },
            ],
            performance: PerformanceProfile::default(),
        };

        let json = serde_json::to_string(&caps).unwrap();
        let decoded: ClientCapabilities = serde_json::from_str(&json).unwrap();

        let server_supported: Vec<_> = decoded
            .channels
            .iter()
            .filter(|c| c.channel_id <= 0x007)
            .collect();
        assert_eq!(server_supported.len(), 1);
        assert_eq!(server_supported[0].channel_id, 0x001);
    }

    #[test]
    fn server_capabilities_roundtrip() {
        let caps = ServerCapabilities {
            protocol_version: 1,
            channels: vec![ChannelCap {
                channel_id: 0x001,
                channel_version: 1,
                config: ChannelConfig::Generic,
            }],
            negotiated_codec: "h265".to_string(),
            display_resolution: (2560, 1440),
            display_fps: 60,
        };

        let json = serde_json::to_string(&caps).unwrap();
        let decoded: ServerCapabilities = serde_json::from_str(&json).unwrap();
        assert_eq!(caps, decoded);
    }

    #[test]
    fn default_performance_profile() {
        let profile = PerformanceProfile::default();
        assert_eq!(profile.max_decode_resolution, (1920, 1080));
        assert_eq!(profile.max_decoder_instances, 1);
        assert!(!profile.can_composite_regions);
        assert_eq!(profile.estimated_decode_fps, 30);
    }

    #[test]
    fn channel_assignment_roundtrip() {
        let assignments = vec![
            ChannelAssignment {
                channel_id: 0x001,
                stream_id: Some(4),
                uses_datagrams: true,
            },
            ChannelAssignment {
                channel_id: 0x006,
                stream_id: Some(0),
                uses_datagrams: false,
            },
        ];

        let json = serde_json::to_string(&assignments).unwrap();
        let decoded: Vec<ChannelAssignment> = serde_json::from_str(&json).unwrap();
        assert_eq!(assignments, decoded);
    }
}
