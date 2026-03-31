pub mod config;
pub mod acceptor;
pub mod allow_all_gate;
pub mod session_manager;
pub mod client_session;
pub mod recv_loop;
pub mod quality_monitor;
pub mod shutdown;
pub mod test_pattern;

// ── Flat re-exports ──────────────────────────────────────────────────────────

// config
pub use config::ServerConfig;

// client_session
pub use client_session::ClientSession;

// session_manager
pub use session_manager::SessionManager;

// quality_monitor
pub use quality_monitor::{QualityMonitor, QualityUpdate};

// shutdown
pub use shutdown::{ShutdownCoordinator, ShutdownState};

// recv_loop
pub use recv_loop::{classify_datagram, record_datagram_bandwidth, DatagramAction};

// test_pattern
pub use test_pattern::TestPatternCapture;

// acceptor
pub use acceptor::{ConnectionAcceptor, SelfSignedCert};

// allow_all_gate
pub use allow_all_gate::AllowAllGate;
