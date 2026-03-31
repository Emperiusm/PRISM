pub mod error;
pub mod types;
pub mod control_msg;
pub mod channel;
pub mod routing;
pub mod tombstone;
pub mod heartbeat;
pub mod profiles;
pub mod negotiation;
pub mod dispatch;
pub mod arbiter;
pub mod tracker;

// ── Flat re-exports ──────────────────────────────────────────────────────────

// error
pub use error::SessionError;

// types
pub use types::{ArbiterEvent, ClientId, SessionEvent, SessionState};

// channel
pub use channel::{
    ChannelGrantResult, ChannelOwnership, ChannelRegistry, TransferPolicy,
};

// routing
pub use routing::{RouteEntry, RoutingMutation, RoutingSnapshot, RoutingTable};

// tombstone
pub use tombstone::{ChannelRecoveryState, Tombstone, TombstoneStore};

// heartbeat
pub use heartbeat::HeartbeatMonitor;

// profiles
pub use profiles::{ConnectionProfile, DisplayProfile, EncoderPreset};

// negotiation
pub use negotiation::{
    CapabilityNegotiator, ChannelCap, ChannelConfig, ClientCapabilities,
    ClientChannelCap, ClientPerformance, DisplayChannelConfig, NegotiatedChannel,
    NegotiationResult,
};

// dispatch
pub use dispatch::{ChannelDispatcher, ChannelError, ChannelHandler};

// arbiter
pub use arbiter::{
    AllocationHandle, BandwidthArbiter, BandwidthNeeds, StarvationDetector,
    StarvationWarning,
};

// tracker
pub use tracker::ChannelBandwidthTracker;
