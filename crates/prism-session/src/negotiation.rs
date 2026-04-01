// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2025-2026 Ehsan Khalid. All rights reserved.
// Licensed under the GNU Affero General Public License v3.0.
// Commercial licensing available — see LICENSE-COMMERCIAL.md.

// Negotiation: capability negotiation between client and server.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use prism_protocol::channel::CHANNEL_DISPLAY;

// ---------------------------------------------------------------------------
// Client-side capability types
// ---------------------------------------------------------------------------

/// Performance-related capabilities advertised by the client.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ClientPerformance {
    /// Video codecs the client can decode, e.g. "h264", "h265", "av1".
    pub supported_codecs: Vec<String>,
}

/// A single channel capability entry from the client.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientChannelCap {
    pub channel_id: u16,
    /// Maximum protocol version the client supports for this channel.
    pub max_version: u16,
}

/// Full capability advertisement from a connecting client.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ClientCapabilities {
    pub channels: Vec<ClientChannelCap>,
    pub performance: ClientPerformance,
}

// ---------------------------------------------------------------------------
// Server-side channel capability types
// ---------------------------------------------------------------------------

/// Display-channel specific server configuration.
#[derive(Debug, Clone)]
pub struct DisplayChannelConfig {
    /// Codecs the server can encode.
    pub supported_codecs: Vec<String>,
}

/// Per-channel configuration carried inside `ChannelCap`.
#[derive(Debug, Clone)]
pub enum ChannelConfig {
    Display(DisplayChannelConfig),
    Generic,
}

/// Server's capability descriptor for one channel.
#[derive(Debug, Clone)]
pub struct ChannelCap {
    pub channel_id: u16,
    /// Maximum protocol version the server supports for this channel.
    pub max_version: u16,
    pub config: ChannelConfig,
}

// ---------------------------------------------------------------------------
// Negotiation result types
// ---------------------------------------------------------------------------

/// A successfully negotiated channel and its agreed protocol version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NegotiatedChannel {
    pub channel_id: u16,
    pub version: u16,
}

/// The full result of a capability negotiation handshake.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NegotiationResult {
    pub protocol_version: u16,
    /// Channels accepted by both sides with their negotiated version.
    pub channels: Vec<NegotiatedChannel>,
    /// Channel IDs the client requested that the server does not support.
    pub rejected_channels: Vec<u16>,
    /// The agreed display codec (e.g. "h264", "h265", "av1").
    pub display_codec: String,
}

// ---------------------------------------------------------------------------
// Negotiator
// ---------------------------------------------------------------------------

/// Performs capability negotiation on behalf of the server.
pub struct CapabilityNegotiator {
    /// Map of channel_id → server ChannelCap.
    server_channels: HashMap<u16, ChannelCap>,
}

impl CapabilityNegotiator {
    pub fn new(server_channels: HashMap<u16, ChannelCap>) -> Self {
        Self { server_channels }
    }

    /// Negotiate channels and codec with an incoming client.
    ///
    /// - Channels present on both sides: accepted at `min(client_version, server_version)`.
    /// - Channels the client requests but the server doesn't have: rejected.
    /// - Protocol version = min of all negotiated channel versions (or 1 if none).
    pub fn negotiate(&self, client_caps: &ClientCapabilities) -> NegotiationResult {
        let mut channels = Vec::new();
        let mut rejected_channels = Vec::new();

        for client_ch in &client_caps.channels {
            match self.server_channels.get(&client_ch.channel_id) {
                Some(server_ch) => {
                    let version = client_ch.max_version.min(server_ch.max_version);
                    channels.push(NegotiatedChannel {
                        channel_id: client_ch.channel_id,
                        version,
                    });
                }
                None => {
                    rejected_channels.push(client_ch.channel_id);
                }
            }
        }

        let protocol_version = channels.iter().map(|c| c.version).min().unwrap_or(1);
        let display_codec = self.negotiate_codec(client_caps);

        NegotiationResult {
            protocol_version,
            channels,
            rejected_channels,
            display_codec,
        }
    }

    /// Select the best display codec supported by both sides.
    ///
    /// Priority: h265 > h264 > av1. Falls back to "h264" if no match is found.
    fn negotiate_codec(&self, client: &ClientCapabilities) -> String {
        let client_codecs: HashSet<_> = client
            .performance
            .supported_codecs
            .iter()
            .cloned()
            .collect();
        let server_codecs =
            self.server_channels
                .get(&CHANNEL_DISPLAY)
                .and_then(|c| match &c.config {
                    ChannelConfig::Display(d) => Some(&d.supported_codecs),
                    _ => None,
                });
        for codec in ["h265", "h264", "av1"] {
            if client_codecs.contains(codec)
                && let Some(server) = server_codecs
                && server.iter().any(|c| c == codec)
            {
                return codec.to_string();
            }
        }
        "h264".to_string()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn display_cap(codecs: Vec<&str>) -> ChannelCap {
        ChannelCap {
            channel_id: CHANNEL_DISPLAY,
            max_version: 2,
            config: ChannelConfig::Display(DisplayChannelConfig {
                supported_codecs: codecs.into_iter().map(|s| s.to_string()).collect(),
            }),
        }
    }

    fn generic_cap(channel_id: u16, version: u16) -> ChannelCap {
        ChannelCap {
            channel_id,
            max_version: version,
            config: ChannelConfig::Generic,
        }
    }

    fn negotiator_with_display(codecs: Vec<&str>) -> CapabilityNegotiator {
        let mut map = HashMap::new();
        map.insert(CHANNEL_DISPLAY, display_cap(codecs));
        map.insert(0x0002, generic_cap(0x0002, 2));
        CapabilityNegotiator::new(map)
    }

    #[test]
    fn matching_channels_negotiated() {
        let neg = negotiator_with_display(vec!["h264"]);
        let client = ClientCapabilities {
            channels: vec![
                ClientChannelCap {
                    channel_id: CHANNEL_DISPLAY,
                    max_version: 1,
                },
                ClientChannelCap {
                    channel_id: 0x0002,
                    max_version: 2,
                },
            ],
            performance: ClientPerformance {
                supported_codecs: vec!["h264".to_string()],
            },
        };
        let result = neg.negotiate(&client);
        assert_eq!(result.channels.len(), 2);
        assert!(result.rejected_channels.is_empty());
    }

    #[test]
    fn unmatched_channels_rejected() {
        let neg = negotiator_with_display(vec!["h264"]);
        let client = ClientCapabilities {
            channels: vec![
                ClientChannelCap {
                    channel_id: CHANNEL_DISPLAY,
                    max_version: 1,
                },
                // 0x0E1 is not on the server
                ClientChannelCap {
                    channel_id: 0x0E1,
                    max_version: 1,
                },
            ],
            performance: ClientPerformance {
                supported_codecs: vec!["h264".to_string()],
            },
        };
        let result = neg.negotiate(&client);
        assert_eq!(result.channels.len(), 1);
        assert_eq!(result.rejected_channels, vec![0x0E1u16]);
    }

    #[test]
    fn version_min_selected() {
        // Server supports v2, client only v1 → negotiated v1.
        let mut map = HashMap::new();
        map.insert(CHANNEL_DISPLAY, display_cap(vec!["h264"]));
        let neg = CapabilityNegotiator::new(map);

        let client = ClientCapabilities {
            channels: vec![ClientChannelCap {
                channel_id: CHANNEL_DISPLAY,
                max_version: 1,
            }],
            performance: ClientPerformance {
                supported_codecs: vec!["h264".to_string()],
            },
        };
        let result = neg.negotiate(&client);
        assert_eq!(result.channels[0].version, 1);
    }

    #[test]
    fn codec_priority_h265_preferred() {
        let neg = negotiator_with_display(vec!["h264", "h265"]);
        let client = ClientCapabilities {
            channels: vec![],
            performance: ClientPerformance {
                supported_codecs: vec!["h264".to_string(), "h265".to_string()],
            },
        };
        let result = neg.negotiate(&client);
        assert_eq!(result.display_codec, "h265");
    }

    #[test]
    fn codec_fallback_to_h264() {
        let neg = negotiator_with_display(vec!["h264"]);
        let client = ClientCapabilities {
            channels: vec![],
            performance: ClientPerformance {
                supported_codecs: vec!["h264".to_string()],
            },
        };
        let result = neg.negotiate(&client);
        assert_eq!(result.display_codec, "h264");
    }
}
