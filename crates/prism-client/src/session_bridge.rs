// SPDX-License-Identifier: AGPL-3.0-or-later
//! SessionBridge — typed channel layer between UI thread and async network tasks.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;
use tokio::sync::{mpsc, watch};

use crate::ui::overlay::stats_bar::SessionStats;

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

/// Commands from UI → Network
#[derive(Debug, Clone)]
pub enum ControlCommand {
    SwitchProfile(String),
    UpdateQuality {
        encoder_preset: Option<String>,
        max_fps: Option<u8>,
        lossless_text: Option<bool>,
        region_detection: Option<bool>,
    },
    SetBandwidthLimit(u64),
    SelectMonitor(u8),
    Disconnect,
    RequestServerInfo,
}

// ---------------------------------------------------------------------------
// Connection lifecycle types
// ---------------------------------------------------------------------------

/// Connection request from launcher
#[derive(Debug)]
pub struct ConnectRequest {
    pub server_addr: SocketAddr,
    pub noise_key: Option<[u8; 32]>,
    pub identity_path: PathBuf,
    pub preferred_profile: String,
}

/// Server info received after connection
#[derive(Debug, Clone, Default)]
pub struct ServerInfo {
    pub display_name: String,
    pub resolution: (u32, u32),
    pub refresh_rate: u8,
    pub codec: String,
    pub encryption: bool,
    pub monitors: Vec<MonitorInfo>,
}

#[derive(Debug, Clone, Default)]
pub struct MonitorInfo {
    pub index: u8,
    pub width: u32,
    pub height: u32,
    pub x: i32,
    pub y: i32,
    pub is_primary: bool,
}

/// Connection result
#[derive(Debug)]
pub enum ConnectResult {
    Connected { server_info: ServerInfo },
    Failed { error: String, retryable: bool },
}

/// Encryption status for display
#[derive(Debug, Clone, Default)]
pub enum EncryptionStatus {
    #[default]
    None,
    NoiseIK,
}

/// Server status from pinger
#[derive(Debug, Clone)]
pub enum ServerStatus {
    Unknown,
    /// RTT
    Online(Duration),
    Offline,
}

impl Default for ServerStatus {
    fn default() -> Self {
        ServerStatus::Unknown
    }
}

// ---------------------------------------------------------------------------
// Frame
// ---------------------------------------------------------------------------

/// Frame data from decoder.
pub struct Frame {
    pub width: usize,
    pub height: usize,
    pub y_data: Vec<u8>,
    pub u_data: Vec<u8>,
    pub v_data: Vec<u8>,
    pub seq: u32,
}

// ---------------------------------------------------------------------------
// SessionBridge
// ---------------------------------------------------------------------------

/// Owned by the UI thread. Provides non-blocking access to network state.
pub struct SessionBridge {
    /// Frame reception (Network → UI)
    pub frame_rx: Option<std::sync::mpsc::Receiver<Frame>>,

    /// Stats (Network → UI, latest-only)
    pub stats_rx: Option<watch::Receiver<SessionStats>>,

    /// Server info (Network → UI)
    pub server_info_rx: Option<watch::Receiver<ServerInfo>>,

    /// Control commands (UI → Network)
    pub control_tx: Option<mpsc::Sender<ControlCommand>>,

    /// Input forwarding (UI → Network)
    pub input_tx: Option<std::sync::mpsc::SyncSender<bytes::Bytes>>,
}

impl SessionBridge {
    /// Create an unconnected bridge (all channels absent).
    pub fn new() -> Self {
        Self {
            frame_rx: None,
            stats_rx: None,
            server_info_rx: None,
            control_tx: None,
            input_tx: None,
        }
    }

    /// Create paired channels for a new connection.
    ///
    /// Returns `(bridge, NetworkSide)` — bridge goes to the UI thread,
    /// `NetworkSide` goes to the async network tasks.
    pub fn create_connected() -> (Self, NetworkSide) {
        let (frame_tx, frame_rx) = std::sync::mpsc::sync_channel(4);
        let (stats_tx, stats_rx) = watch::channel(SessionStats::default());
        let (server_info_tx, server_info_rx) = watch::channel(ServerInfo::default());
        let (control_tx, control_rx) = mpsc::channel(16);
        let (input_tx, input_rx) = std::sync::mpsc::sync_channel(64);

        let bridge = Self {
            frame_rx: Some(frame_rx),
            stats_rx: Some(stats_rx),
            server_info_rx: Some(server_info_rx),
            control_tx: Some(control_tx),
            input_tx: Some(input_tx),
        };

        let network = NetworkSide {
            frame_tx,
            stats_tx,
            server_info_tx,
            control_rx,
            input_rx,
        };

        (bridge, network)
    }

    /// Try to receive the latest frame (non-blocking).
    pub fn try_recv_frame(&self) -> Option<Frame> {
        self.frame_rx.as_ref()?.try_recv().ok()
    }

    /// Get the latest stats snapshot.
    pub fn current_stats(&self) -> Option<SessionStats> {
        self.stats_rx.as_ref().map(|rx| rx.borrow().clone())
    }

    /// Get the latest server info snapshot.
    pub fn current_server_info(&self) -> Option<ServerInfo> {
        self.server_info_rx.as_ref().map(|rx| rx.borrow().clone())
    }

    /// Send a control command (non-blocking, best-effort).
    pub fn send_control(&self, cmd: ControlCommand) {
        if let Some(tx) = &self.control_tx {
            let _ = tx.try_send(cmd);
        }
    }

    /// Send an input datagram (non-blocking, best-effort).
    pub fn send_input(&self, data: bytes::Bytes) {
        if let Some(tx) = &self.input_tx {
            let _ = tx.try_send(data);
        }
    }

    /// Returns `true` when the bridge has active channels (i.e. is connected).
    pub fn is_connected(&self) -> bool {
        self.frame_rx.is_some()
    }
}

impl Default for SessionBridge {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// NetworkSide
// ---------------------------------------------------------------------------

/// Held by the async network tasks. Sends data to the UI thread.
pub struct NetworkSide {
    pub frame_tx: std::sync::mpsc::SyncSender<Frame>,
    pub stats_tx: watch::Sender<SessionStats>,
    pub server_info_tx: watch::Sender<ServerInfo>,
    pub control_rx: mpsc::Receiver<ControlCommand>,
    pub input_rx: std::sync::mpsc::Receiver<bytes::Bytes>,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bridge_unconnected() {
        let bridge = SessionBridge::new();
        assert!(!bridge.is_connected());
        assert!(bridge.try_recv_frame().is_none());
        assert!(bridge.current_stats().is_none());
    }

    #[test]
    fn bridge_connected_channels() {
        let (bridge, network) = SessionBridge::create_connected();
        assert!(bridge.is_connected());

        let frame = Frame {
            width: 1920,
            height: 1080,
            y_data: vec![0; 10],
            u_data: vec![0; 5],
            v_data: vec![0; 5],
            seq: 1,
        };
        network.frame_tx.send(frame).unwrap();

        let received = bridge.try_recv_frame().unwrap();
        assert_eq!(received.width, 1920);
        assert_eq!(received.seq, 1);
    }

    #[test]
    fn bridge_stats_latest_only() {
        let (bridge, network) = SessionBridge::create_connected();

        let _ = network.stats_tx.send(SessionStats { fps: 30.0, ..Default::default() });
        let _ = network.stats_tx.send(SessionStats { fps: 60.0, ..Default::default() });

        let stats = bridge.current_stats().unwrap();
        assert_eq!(stats.fps, 60.0);
    }

    #[test]
    fn bridge_control_command() {
        let (bridge, mut network) = SessionBridge::create_connected();
        bridge.send_control(ControlCommand::SwitchProfile("Coding".into()));

        let cmd = network.control_rx.try_recv().unwrap();
        assert!(matches!(cmd, ControlCommand::SwitchProfile(ref s) if s == "Coding"));
    }

    #[test]
    fn bridge_server_info_default() {
        let (bridge, _network) = SessionBridge::create_connected();
        let info = bridge.current_server_info().unwrap();
        assert!(info.display_name.is_empty());
        assert_eq!(info.resolution, (0, 0));
    }

    #[test]
    fn bridge_input_send() {
        let (bridge, network) = SessionBridge::create_connected();
        let payload = bytes::Bytes::from_static(b"hello");
        bridge.send_input(payload.clone());

        let received = network.input_rx.try_recv().unwrap();
        assert_eq!(received, payload);
    }
}
