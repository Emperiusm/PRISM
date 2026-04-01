// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2025-2026 Ehsan Khalid. All rights reserved.
// Licensed under the GNU Affero General Public License v3.0.
// Commercial licensing available — see LICENSE-COMMERCIAL.md.

pub mod feedback;
pub mod frame_trace;
pub mod overlay;
pub mod time_series;

// Re-exports for convenient top-level access.
pub use feedback::{ClientAlert, ClientFeedback, ClientFeedbackConfig};
pub use frame_trace::{FrameLatencyBreakdown, FrameTrace, FrameTracer};
pub use overlay::{OVERLAY_PACKET_SIZE, OverlayPacket};
pub use time_series::{MetricsTimeSeries, TimeSample, TimeSeriesRing};
