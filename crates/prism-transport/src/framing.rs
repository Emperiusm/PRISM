// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2025-2026 Ehsan Khalid. All rights reserved.
// Licensed under the GNU Affero General Public License v3.0.
// Commercial licensing available — see LICENSE-COMMERCIAL.md.

// Framing layer: encode/decode PRISM packets over transport streams.

use crate::connection::{OwnedRecvStream, OwnedSendStream, TransportError};

pub const MAX_MESSAGE_SIZE: usize = 16 * 1024 * 1024;

// ── FramedWriter ──────────────────────────────────────────────────────────────

pub struct FramedWriter {
    stream: OwnedSendStream,
}

impl FramedWriter {
    pub fn new(stream: OwnedSendStream) -> Self {
        Self { stream }
    }

    pub async fn send(&mut self, data: &[u8]) -> Result<(), TransportError> {
        self.stream
            .write(&(data.len() as u32).to_le_bytes())
            .await?;
        self.stream.write(data).await?;
        Ok(())
    }

    pub fn into_inner(self) -> OwnedSendStream {
        self.stream
    }
}

// ── FramedReader ──────────────────────────────────────────────────────────────

pub struct FramedReader {
    stream: OwnedRecvStream,
}

impl FramedReader {
    pub fn new(stream: OwnedRecvStream) -> Self {
        Self { stream }
    }

    pub async fn recv(&mut self) -> Result<Vec<u8>, TransportError> {
        let mut len_buf = [0u8; 4];
        self.stream.read_exact(&mut len_buf).await?;
        let len = u32::from_le_bytes(len_buf) as usize;
        if len > MAX_MESSAGE_SIZE {
            return Err(TransportError::MessageTooLarge(len));
        }
        let mut data = vec![0u8; len];
        self.stream.read_exact(&mut data).await?;
        Ok(data)
    }

    pub fn into_inner(self) -> OwnedRecvStream {
        self.stream
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn framed_roundtrip_simple() {
        let (send, buffer) = OwnedSendStream::mock();
        let mut writer = FramedWriter::new(send);
        writer.send(b"hello world").await.unwrap();
        let bytes = buffer.lock().unwrap().clone();
        let recv = OwnedRecvStream::mock(bytes);
        let mut reader = FramedReader::new(recv);
        let msg = reader.recv().await.unwrap();
        assert_eq!(msg, b"hello world");
    }

    #[tokio::test]
    async fn framed_roundtrip_empty_message() {
        let (send, buffer) = OwnedSendStream::mock();
        let mut writer = FramedWriter::new(send);
        writer.send(b"").await.unwrap();
        let bytes = buffer.lock().unwrap().clone();
        let recv = OwnedRecvStream::mock(bytes);
        let mut reader = FramedReader::new(recv);
        let msg = reader.recv().await.unwrap();
        assert!(msg.is_empty());
    }

    #[tokio::test]
    async fn framed_roundtrip_multiple_messages() {
        let (send, buffer) = OwnedSendStream::mock();
        let mut writer = FramedWriter::new(send);
        writer.send(b"one").await.unwrap();
        writer.send(b"two").await.unwrap();
        writer.send(b"three").await.unwrap();
        let bytes = buffer.lock().unwrap().clone();
        let recv = OwnedRecvStream::mock(bytes);
        let mut reader = FramedReader::new(recv);
        assert_eq!(reader.recv().await.unwrap(), b"one");
        assert_eq!(reader.recv().await.unwrap(), b"two");
        assert_eq!(reader.recv().await.unwrap(), b"three");
    }

    #[tokio::test]
    async fn framed_reader_rejects_oversized_message() {
        let mut data = Vec::new();
        data.extend_from_slice(&(20_000_000u32).to_le_bytes());
        data.extend_from_slice(b"x");
        let recv = OwnedRecvStream::mock(data);
        let mut reader = FramedReader::new(recv);
        let result = reader.recv().await;
        assert!(matches!(
            result,
            Err(TransportError::MessageTooLarge(20_000_000))
        ));
    }

    #[tokio::test]
    async fn framed_roundtrip_binary_data() {
        let payload: Vec<u8> = (0..=255).collect();
        let (send, buffer) = OwnedSendStream::mock();
        let mut writer = FramedWriter::new(send);
        writer.send(&payload).await.unwrap();
        let bytes = buffer.lock().unwrap().clone();
        let recv = OwnedRecvStream::mock(bytes);
        let mut reader = FramedReader::new(recv);
        assert_eq!(reader.recv().await.unwrap(), payload);
    }
}
