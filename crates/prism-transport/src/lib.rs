// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2025-2026 Ehsan Khalid. All rights reserved.
// Licensed under the GNU Affero General Public License v3.0.
// Commercial licensing available — see LICENSE-COMMERCIAL.md.

pub mod connection;
pub mod framing;
pub mod unified;
pub mod coalesce;
pub mod stream_pool;
pub mod quality;
pub mod quic;

pub use connection::{TransportError, TransportType, StreamPriority, TransportMetrics, DelayAsymmetry, TransportEvent, OwnedSendStream, OwnedRecvStream, PrismConnection};
pub use framing::{FramedWriter, FramedReader, MAX_MESSAGE_SIZE};
pub use unified::{UnifiedConnection, ChannelRouting, ConnectionSlot};
pub use quality::{ConnectionQuality, QualityRecommendation};
pub use quic::QuicConnection;
