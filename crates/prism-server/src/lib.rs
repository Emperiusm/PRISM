pub mod config;
pub mod acceptor;
pub mod rate_limiter;
pub mod allow_all_gate;
pub mod server_app;
pub mod pairing_gate;
pub mod session_manager;
pub mod client_session;
pub mod recv_loop;
pub mod quality_monitor;
pub mod shutdown;
pub mod test_pattern;
pub mod frame_sender;
pub mod client_connections;
pub mod dda_capture;
pub mod hw_encoder;
pub mod heartbeat_task;
pub mod control_handler;
pub mod input_handler;
pub mod audio_sender;
pub mod quality_task;
pub mod clipboard_stream;

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
pub use recv_loop::{classify_datagram, record_datagram_bandwidth, DatagramAction, RecvLoopHandle, spawn_recv_loop};

// test_pattern
pub use test_pattern::TestPatternCapture;

// acceptor
pub use acceptor::{ConnectionAcceptor, SelfSignedCert};

// allow_all_gate
pub use allow_all_gate::AllowAllGate;

// pairing_gate
pub use pairing_gate::TofuGate;

// frame_sender
pub use frame_sender::{FrameSender, build_display_datagram};

// client_connections
pub use client_connections::ClientConnectionStore;

// heartbeat_task
pub use heartbeat_task::HeartbeatGenerator;

// control_handler
pub use control_handler::ControlChannelHandler;

// input_handler
pub use input_handler::InputChannelHandler;

// server_app
pub use server_app::ServerApp;

// quality_task
pub use quality_task::{QualityCache, build_probe_datagram, evaluate_quality};

// clipboard_stream
pub use clipboard_stream::ClipboardSyncState;

// rate_limiter
pub use rate_limiter::ConnectionRateLimiter;
