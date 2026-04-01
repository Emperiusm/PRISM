// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2025-2026 Ehsan Khalid. All rights reserved.
// Licensed under the GNU Affero General Public License v3.0.
// Commercial licensing available — see LICENSE-COMMERCIAL.md.

use snow::{Builder, HandshakeState, TransportState};
use thiserror::Error;

use crate::identity::LocalIdentity;

const NOISE_PATTERN: &str = "Noise_IK_25519_ChaChaPoly_SHA256";

#[derive(Debug, Error)]
pub enum HandshakeError {
    #[error("noise protocol error: {0}")]
    Noise(#[from] snow::Error),
    #[error("handshake not complete")]
    NotComplete,
}

pub struct HandshakeResult {
    pub transport: TransportState,
    pub remote_static: Option<[u8; 32]>,
}

pub struct ServerHandshake {
    state: HandshakeState,
}

impl ServerHandshake {
    pub fn new(identity: &LocalIdentity) -> Result<Self, HandshakeError> {
        let state = Builder::new(NOISE_PATTERN.parse().unwrap())
            .local_private_key(&identity.x25519_secret_bytes())
            .build_responder()?;
        Ok(Self { state })
    }

    pub fn respond(&mut self, client_msg: &[u8]) -> Result<Vec<u8>, HandshakeError> {
        let mut read_buf = vec![0u8; 65535];
        self.state.read_message(client_msg, &mut read_buf)?;
        let mut response = vec![0u8; 65535];
        let len = self.state.write_message(&[], &mut response)?;
        response.truncate(len);
        Ok(response)
    }

    pub fn finalize(self) -> Result<HandshakeResult, HandshakeError> {
        if !self.state.is_handshake_finished() {
            return Err(HandshakeError::NotComplete);
        }
        let remote_static = self.state.get_remote_static().map(|s| {
            let mut key = [0u8; 32];
            key.copy_from_slice(s);
            key
        });
        let transport = self.state.into_transport_mode()?;
        Ok(HandshakeResult {
            transport,
            remote_static,
        })
    }
}

pub struct ClientHandshake {
    state: HandshakeState,
}

impl ClientHandshake {
    pub fn new(
        identity: &LocalIdentity,
        server_public_key: &[u8; 32],
    ) -> Result<Self, HandshakeError> {
        let state = Builder::new(NOISE_PATTERN.parse().unwrap())
            .local_private_key(&identity.x25519_secret_bytes())
            .remote_public_key(server_public_key)
            .build_initiator()?;
        Ok(Self { state })
    }

    pub fn initiate(&mut self) -> Result<Vec<u8>, HandshakeError> {
        let mut msg = vec![0u8; 65535];
        let len = self.state.write_message(&[], &mut msg)?;
        msg.truncate(len);
        Ok(msg)
    }

    pub fn process_response(&mut self, server_msg: &[u8]) -> Result<(), HandshakeError> {
        let mut read_buf = vec![0u8; 65535];
        self.state.read_message(server_msg, &mut read_buf)?;
        Ok(())
    }

    pub fn finalize(self) -> Result<HandshakeResult, HandshakeError> {
        if !self.state.is_handshake_finished() {
            return Err(HandshakeError::NotComplete);
        }
        let transport = self.state.into_transport_mode()?;
        Ok(HandshakeResult {
            transport,
            remote_static: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::LocalIdentity;

    #[test]
    fn handshake_completes_in_one_roundtrip() {
        let server_id = LocalIdentity::generate("Server");
        let client_id = LocalIdentity::generate("Client");

        let mut client_hs =
            ClientHandshake::new(&client_id, &server_id.x25519_public_bytes()).unwrap();
        let client_msg = client_hs.initiate().unwrap();

        let mut server_hs = ServerHandshake::new(&server_id).unwrap();
        let server_msg = server_hs.respond(&client_msg).unwrap();
        client_hs.process_response(&server_msg).unwrap();

        let mut server_result = server_hs.finalize().unwrap();
        let mut client_result = client_hs.finalize().unwrap();

        assert_eq!(
            server_result.remote_static.unwrap(),
            client_id.x25519_public_bytes()
        );

        let mut enc_buf = vec![0u8; 1024];
        let mut dec_buf = vec![0u8; 1024];
        let len = client_result
            .transport
            .write_message(b"hello", &mut enc_buf)
            .unwrap();
        let dec_len = server_result
            .transport
            .read_message(&enc_buf[..len], &mut dec_buf)
            .unwrap();
        assert_eq!(&dec_buf[..dec_len], b"hello");
    }

    #[test]
    fn wrong_server_key_fails() {
        let server_id = LocalIdentity::generate("Server");
        let client_id = LocalIdentity::generate("Client");
        let wrong_id = LocalIdentity::generate("Wrong");

        let mut client_hs =
            ClientHandshake::new(&client_id, &wrong_id.x25519_public_bytes()).unwrap();
        let client_msg = client_hs.initiate().unwrap();

        let mut server_hs = ServerHandshake::new(&server_id).unwrap();
        assert!(server_hs.respond(&client_msg).is_err());
    }

    #[test]
    fn finalize_before_complete_fails() {
        let id = LocalIdentity::generate("Server");
        let hs = ServerHandshake::new(&id).unwrap();
        assert!(matches!(hs.finalize(), Err(HandshakeError::NotComplete)));
    }
}
