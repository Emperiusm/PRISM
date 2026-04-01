// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2025-2026 Ehsan Khalid. All rights reserved.
// Licensed under the GNU Affero General Public License v3.0.
// Commercial licensing available — see LICENSE-COMMERCIAL.md.

pub mod coalesce;
pub mod connection;
pub mod framing;
pub mod quality;
pub mod quic;
pub mod stream_pool;
pub mod unified;

pub use connection::{
    DelayAsymmetry, OwnedRecvStream, OwnedSendStream, PrismConnection, StreamPriority,
    TransportError, TransportEvent, TransportMetrics, TransportType,
};
pub use framing::{FramedReader, FramedWriter, MAX_MESSAGE_SIZE};
pub use quality::{ConnectionQuality, QualityRecommendation};
pub use quic::QuicConnection;
pub use unified::{ChannelRouting, ConnectionSlot, UnifiedConnection};
