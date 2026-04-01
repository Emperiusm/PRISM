// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2025-2026 Ehsan Khalid. All rights reserved.
// Licensed under the GNU Affero General Public License v3.0.
// Commercial licensing available — see LICENSE-COMMERCIAL.md.

// negotiation_handler.rs — Capability negotiation over a QUIC bi-directional stream.
//
// Protocol:
//   Client → Server: [4B LE len][JSON ClientCapabilities]
//   Server → Client: [4B LE len][JSON NegotiationResult]

use prism_session::{
    CapabilityNegotiator, ChannelCap, ChannelConfig, ClientCapabilities,
    DisplayChannelConfig, NegotiationResult,
};
use prism_protocol::channel::{CHANNEL_CONTROL, CHANNEL_DISPLAY, CHANNEL_INPUT};
use std::collections::HashMap;

/// Maximum number of bytes accepted for the client capabilities payload.
const MAX_CAPS_LEN: usize = 64 * 1024;

/// Read client capabilities (length-prefixed JSON) from `recv`, negotiate with
/// `negotiator`, and write the `NegotiationResult` (length-prefixed JSON) to
/// `send`.
///
/// Returns the agreed `NegotiationResult` on success.
pub async fn negotiate_on_stream(
    send: &mut quinn::SendStream,
    recv: &mut quinn::RecvStream,
    negotiator: &CapabilityNegotiator,
) -> Result<NegotiationResult, Box<dyn std::error::Error + Send + Sync>> {
    // Read 4-byte little-endian length prefix.
    let mut len_buf = [0u8; 4];
    recv.read_exact(&mut len_buf).await?;
    let len = u32::from_le_bytes(len_buf) as usize;

    if len > MAX_CAPS_LEN {
        return Err(format!("capabilities payload too large: {len} bytes").into());
    }

    // Read the JSON body.
    let mut data = vec![0u8; len];
    recv.read_exact(&mut data).await?;

    let client_caps: ClientCapabilities = serde_json::from_slice(&data)?;
    let result = negotiator.negotiate(&client_caps);

    // Serialise and send the result.
    let response = serde_json::to_vec(&result)?;
    send.write_all(&(response.len() as u32).to_le_bytes()).await?;
    send.write_all(&response).await?;
    let _ = send.finish();

    Ok(result)
}

/// Build a `CapabilityNegotiator` pre-loaded with the server's standard
/// channels (Display, Input, Control).
///
/// Called during connection setup to obtain a negotiator instance for
/// [`negotiate_on_stream`].
pub fn build_server_negotiator() -> CapabilityNegotiator {
    let mut map: HashMap<u16, ChannelCap> = HashMap::new();

    map.insert(
        CHANNEL_DISPLAY,
        ChannelCap {
            channel_id: CHANNEL_DISPLAY,
            max_version: 2,
            config: ChannelConfig::Display(DisplayChannelConfig {
                supported_codecs: vec!["h264".to_string(), "h265".to_string()],
            }),
        },
    );

    map.insert(
        CHANNEL_INPUT,
        ChannelCap {
            channel_id: CHANNEL_INPUT,
            max_version: 1,
            config: ChannelConfig::Generic,
        },
    );

    map.insert(
        CHANNEL_CONTROL,
        ChannelCap {
            channel_id: CHANNEL_CONTROL,
            max_version: 1,
            config: ChannelConfig::Generic,
        },
    );

    CapabilityNegotiator::new(map)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use prism_session::{ClientCapabilities, ClientChannelCap, ClientPerformance};
    use prism_protocol::channel::CHANNEL_CLIPBOARD;

    fn negotiator() -> CapabilityNegotiator {
        build_server_negotiator()
    }

    fn client_caps(channels: &[(u16, u16)], codecs: &[&str]) -> ClientCapabilities {
        ClientCapabilities {
            channels: channels
                .iter()
                .map(|&(id, ver)| ClientChannelCap { channel_id: id, max_version: ver })
                .collect(),
            performance: ClientPerformance {
                supported_codecs: codecs.iter().map(|s| s.to_string()).collect(),
            },
        }
    }

    #[test]
    fn negotiate_returns_intersection() {
        // Client wants Display + Input + Clipboard; server only has Display + Input + Control.
        let neg = negotiator();
        let caps = client_caps(
            &[
                (CHANNEL_DISPLAY, 1),
                (CHANNEL_INPUT, 1),
                (CHANNEL_CLIPBOARD, 1),
            ],
            &["h264"],
        );
        let result = neg.negotiate(&caps);

        // Display and Input granted.
        let granted_ids: Vec<u16> = result.channels.iter().map(|c| c.channel_id).collect();
        assert!(granted_ids.contains(&CHANNEL_DISPLAY), "Display must be granted");
        assert!(granted_ids.contains(&CHANNEL_INPUT), "Input must be granted");

        // Clipboard rejected.
        assert!(
            result.rejected_channels.contains(&CHANNEL_CLIPBOARD),
            "Clipboard must be rejected"
        );
        assert_eq!(result.channels.len(), 2);
        assert_eq!(result.rejected_channels.len(), 1);
    }

    #[test]
    fn negotiate_codec_priority() {
        // Client supports h264 + h265; server supports both → h265 wins.
        let neg = negotiator();
        let caps = client_caps(&[], &["h264", "h265"]);
        let result = neg.negotiate(&caps);
        assert_eq!(result.display_codec, "h265", "h265 should be preferred over h264");
    }

    #[test]
    fn negotiation_result_json_roundtrip() {
        let neg = negotiator();
        let caps = client_caps(
            &[(CHANNEL_DISPLAY, 2), (CHANNEL_INPUT, 1), (CHANNEL_CONTROL, 1)],
            &["h265"],
        );
        let result = neg.negotiate(&caps);

        let json = serde_json::to_string(&result).expect("serialise NegotiationResult");
        let decoded: NegotiationResult =
            serde_json::from_str(&json).expect("deserialise NegotiationResult");

        assert_eq!(result.channels.len(), decoded.channels.len());
        assert_eq!(result.rejected_channels, decoded.rejected_channels);
        assert_eq!(result.display_codec, decoded.display_codec);
        assert_eq!(result.protocol_version, decoded.protocol_version);
    }
}
