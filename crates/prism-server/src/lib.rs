// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2025-2026 Ehsan Khalid. All rights reserved.
// Licensed under the GNU Affero General Public License v3.0.
// Commercial licensing available — see LICENSE-COMMERCIAL.md.

pub mod acceptor;
pub mod allow_all_gate;
pub mod arbiter_task;
pub mod audio_sender;
pub mod audio_task;
pub mod client_connections;
pub mod client_session;
pub mod clipboard_stream;
pub mod config;
pub mod control_handler;
pub mod cursor_sender;
pub mod dda_capture;
pub mod frame_sender;
pub mod heartbeat_task;
pub mod hw_encoder;
pub mod input_handler;
pub mod negotiation_handler;
pub mod overlay_sender;
pub mod pairing_gate;
pub mod quality_monitor;
pub mod quality_task;
pub mod rate_limiter;
pub mod recv_loop;
pub mod server_app;
pub mod session_manager;
pub mod shutdown;
pub mod test_pattern;
pub mod throughput_endpoint;

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
pub use recv_loop::{
    DatagramAction, RecvLoopHandle, classify_datagram, record_datagram_bandwidth, spawn_recv_loop,
};

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

// negotiation_handler
pub use negotiation_handler::{build_server_negotiator, negotiate_on_stream};

// cursor_sender
pub use cursor_sender::{CursorSender, deserialize_cursor_shape, serialize_cursor_shape};

// arbiter_task
pub use arbiter_task::{display_allocation_bps, update_arbiter_from_tracker};

// overlay_sender
pub use overlay_sender::{build_overlay_datagram, build_overlay_packet};

// throughput_endpoint
pub use throughput_endpoint::{
    ThroughputEndpointConfig, build_throughput_config, is_throughput_channel,
};

pub mod auto_update;
pub mod client_metrics;
pub mod encode_pool;
pub mod frame_tracer_task;
pub mod metrics_collector;
pub mod service;
pub mod speculative_idr;
pub mod static_cache;

// speculative_idr
pub use speculative_idr::SpeculativeIdrController;

// encode_pool
pub use encode_pool::{EncodePoolConfig, EncodePoolStats, should_accept_job};

// static_cache
pub use static_cache::{CacheInstruction, CacheSavingsTracker};

// frame_tracer_task
pub use frame_tracer_task::PipelineTracer;

// client_metrics
pub use client_metrics::ClientMetrics;

// metrics_collector
pub use metrics_collector::MetricsCollector;

// service
pub use service::{ServiceCommand, is_service_mode, sc_create_command, sc_delete_command};

// auto_update
pub use auto_update::{CURRENT_VERSION, SemVer, UpdateStatus, check_version};
