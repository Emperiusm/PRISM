// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2025-2026 Ehsan Khalid. All rights reserved.
// Licensed under the GNU Affero General Public License v3.0.
// Commercial licensing available — see LICENSE-COMMERCIAL.md.

use thiserror::Error;

use crate::header::HEADER_SIZE;

#[derive(Debug, Error)]
pub enum ProtocolError {
    #[error("buffer too short: need {HEADER_SIZE} bytes, got {0}")]
    BufferTooShort(usize),
    #[error("invalid channel ID 0x000 (reserved)")]
    ReservedChannel,
    #[error("unsupported protocol version {0}")]
    UnsupportedVersion(u8),
    #[error("channel ID 0x{0:03X} exceeds 12-bit maximum (0xFFF)")]
    ChannelIdOverflow(u16),
    #[error("serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),
}
