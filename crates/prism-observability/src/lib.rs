pub mod frame_trace;
pub mod feedback;
pub mod overlay;
pub mod time_series;

// Re-exports for convenient top-level access.
pub use frame_trace::{FrameLatencyBreakdown, FrameTrace, FrameTracer};
pub use feedback::{ClientAlert, ClientFeedback, ClientFeedbackConfig};
pub use overlay::{OverlayPacket, OVERLAY_PACKET_SIZE};
pub use time_series::{MetricsTimeSeries, TimeSample, TimeSeriesRing};
