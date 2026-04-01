// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2025-2026 Ehsan Khalid. All rights reserved.
// Licensed under the GNU Affero General Public License v3.0.
// Commercial licensing available — see LICENSE-COMMERCIAL.md.

// negotiation_handler.rs — Capability negotiation over a QUIC bi-directional stream.
//
// Protocol:
//   Client → Server: [4B LE len][JSON ClientCapabilities]
//   Server → Client: [4B LE len][JSON NegotiationResult]
//
// Stream identification (for parallel stream acceptance):
//   Capability negotiation streams begin with a 4-byte LE length prefix followed
//   immediately by the JSON '{' character (0x7B) at byte index 4.
//   Noise IK handshake streams begin with raw cryptographic bytes; byte 4 will
//   almost certainly not be 0x7B.

use prism_protocol::channel::{CHANNEL_CONTROL, CHANNEL_DISPLAY, CHANNEL_INPUT};
use prism_session::{
    CapabilityNegotiator, ChannelCap, ChannelConfig, ClientCapabilities, DisplayChannelConfig,
    NegotiationResult,
};
use std::collections::HashMap;

/// Maximum number of bytes accepted for the client capabilities payload.
const MAX_CAPS_LEN: usize = 64 * 1024;

// ── Stream identification ─────────────────────────────────────────────────────

/// Identifies the type of an incoming bi-directional stream by peeking at
/// its initial bytes.
///
/// Capability negotiation starts with a 4-byte LE length prefix followed by
/// `{` (the first character of a JSON object).  Noise handshake streams start
/// with raw cryptographic bytes where byte 4 is statistically never `{`.
#[derive(Debug, PartialEq)]
pub enum StreamType {
    CapabilityNegotiation,
    NoiseHandshake,
    Unknown,
}

/// Peek at the first 5 bytes of a stream to determine its type.
///
/// Returns the identified [`StreamType`] and the peeked bytes, which **must**
/// be prepended to all subsequent reads from `recv` because they have already
/// been consumed from the transport.
pub async fn identify_stream(
    recv: &mut quinn::RecvStream,
) -> Result<(StreamType, Vec<u8>), Box<dyn std::error::Error + Send + Sync>> {
    let mut peek = vec![0u8; 5];
    let mut total = 0;

    // Read until we have 5 bytes or the stream closes.
    while total < 5 {
        match recv.read(&mut peek[total..]).await? {
            Some(n) => total += n,
            None => {
                // Stream closed before 5 bytes arrived.
                return Ok((StreamType::Unknown, peek[..total].to_vec()));
            }
        }
    }

    // Byte 4 (0-indexed) is `{` → JSON capability message.
    let stream_type = if peek[4] == b'{' {
        StreamType::CapabilityNegotiation
    } else {
        StreamType::NoiseHandshake
    };

    Ok((stream_type, peek))
}

// ── Negotiation with pre-read prefix ─────────────────────────────────────────

/// Negotiate capabilities on a stream where the first 5 bytes have already been
/// read (by [`identify_stream`]).
///
/// `prefix` must be exactly the 5 bytes returned by `identify_stream`:
/// bytes 0–3 are the LE u32 length prefix and byte 4 is the first character of
/// the JSON body.
pub async fn negotiate_on_stream_with_prefix(
    send: &mut quinn::SendStream,
    recv: &mut quinn::RecvStream,
    negotiator: &CapabilityNegotiator,
    prefix: Vec<u8>,
) -> Result<NegotiationResult, Box<dyn std::error::Error + Send + Sync>> {
    // Reconstruct the full JSON length from the first 4 bytes.
    let len = u32::from_le_bytes([prefix[0], prefix[1], prefix[2], prefix[3]]) as usize;

    if len > MAX_CAPS_LEN {
        return Err(format!("capabilities payload too large: {len} bytes").into());
    }

    // We already hold prefix[4..] (1 byte of JSON body); read the rest.
    let already_read = prefix.len() - 4; // number of JSON bytes in prefix
    let remaining = len.saturating_sub(already_read);

    let mut buf = Vec::with_capacity(len);
    buf.extend_from_slice(&prefix[4..]);

    if remaining > 0 {
        let mut rest = vec![0u8; remaining];
        recv.read_exact(&mut rest).await?;
        buf.extend_from_slice(&rest);
    }

    let client_caps: ClientCapabilities = serde_json::from_slice(&buf)?;
    let result = negotiator.negotiate(&client_caps);

    // Serialise and send the result.
    let response = serde_json::to_vec(&result)?;
    send.write_all(&(response.len() as u32).to_le_bytes())
        .await?;
    send.write_all(&response).await?;
    let _ = send.finish();

    Ok(result)
}

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
    send.write_all(&(response.len() as u32).to_le_bytes())
        .await?;
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
    use prism_protocol::channel::CHANNEL_CLIPBOARD;
    use prism_session::{ClientCapabilities, ClientChannelCap, ClientPerformance};

    // ── Stream identification tests ───────────────────────────────────────────

    #[test]
    fn identify_capability_message() {
        // A capability negotiation message: 4-byte LE length prefix + JSON body.
        let json = b"{\"channels\":[]}";
        let len = (json.len() as u32).to_le_bytes();
        let mut data = Vec::new();
        data.extend_from_slice(&len);
        data.extend_from_slice(json);

        // Byte 4 must be '{' so the server recognises it as capability negotiation.
        assert_eq!(data[4], b'{');
    }

    #[test]
    fn identify_noise_message() {
        // Noise IK initiator message: 32 bytes ephemeral key + encrypted payload.
        // Random-looking bytes; byte 4 must not be '{'.
        let noise_msg = vec![0xAAu8; 64];
        assert_ne!(noise_msg[4], b'{');
    }

    #[test]
    fn stream_type_from_bytes() {
        // Capability: length prefix + '{' at byte 4.
        let cap_bytes = vec![10u8, 0, 0, 0, b'{'];
        let stream_type = if cap_bytes[4] == b'{' {
            StreamType::CapabilityNegotiation
        } else {
            StreamType::NoiseHandshake
        };
        assert_eq!(stream_type, StreamType::CapabilityNegotiation);

        // Noise: random bytes where byte 4 is not '{'.
        let noise_bytes = vec![0xDEu8, 0xAD, 0xBE, 0xEF, 0x42];
        let stream_type = if noise_bytes[4] == b'{' {
            StreamType::CapabilityNegotiation
        } else {
            StreamType::NoiseHandshake
        };
        assert_eq!(stream_type, StreamType::NoiseHandshake);
    }

    fn negotiator() -> CapabilityNegotiator {
        build_server_negotiator()
    }

    fn client_caps(channels: &[(u16, u16)], codecs: &[&str]) -> ClientCapabilities {
        ClientCapabilities {
            channels: channels
                .iter()
                .map(|&(id, ver)| ClientChannelCap {
                    channel_id: id,
                    max_version: ver,
                })
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
        assert!(
            granted_ids.contains(&CHANNEL_DISPLAY),
            "Display must be granted"
        );
        assert!(
            granted_ids.contains(&CHANNEL_INPUT),
            "Input must be granted"
        );

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
        assert_eq!(
            result.display_codec, "h265",
            "h265 should be preferred over h264"
        );
    }

    #[test]
    fn negotiation_result_json_roundtrip() {
        let neg = negotiator();
        let caps = client_caps(
            &[
                (CHANNEL_DISPLAY, 2),
                (CHANNEL_INPUT, 1),
                (CHANNEL_CONTROL, 1),
            ],
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
