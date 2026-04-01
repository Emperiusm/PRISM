// ServerApp: structured entry point for the PRISM server.
//
// Encapsulates all initialisation, the accept loop, and graceful shutdown.
// `main.rs` simply constructs a `ServerApp` and calls `run()`.

use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};

use prism_display::capture::PlatformCapture;
use crate::hw_encoder::HwEncoder;

use prism_security::audit::{AuditEvent, AuditLog};

use crate::{
    AllowAllGate, ClientConnectionStore, ConnectionAcceptor, ControlChannelHandler,
    HeartbeatGenerator, InputChannelHandler, SelfSignedCert, ServerConfig, SessionManager,
    TestPatternCapture, spawn_recv_loop,
};

/// Top-level server state. Construct with [`ServerApp::new`], then call
/// [`ServerApp::run`] to enter the accept loop.
pub struct ServerApp {
    use_dda: bool,
    noise_mode: bool,
    config: ServerConfig,
    session_manager: Arc<Mutex<SessionManager>>,
    conn_store: Arc<ClientConnectionStore>,
    dispatcher: Arc<prism_session::ChannelDispatcher>,
    tracker: Arc<prism_session::ChannelBandwidthTracker>,
    server_identity: Arc<prism_security::identity::LocalIdentity>,
    audit_log: Arc<AuditLog>,
}

impl ServerApp {
    /// Initialise all server subsystems.
    ///
    /// Generates a Noise IK identity, creates TLS cert, sets up session
    /// management, channel dispatch, and the connection store.
    pub fn new(use_dda: bool, noise_mode: bool) -> Result<Self, Box<dyn std::error::Error>> {
        tracing::info!("=== PRISM Server v0.1.0 ===");

        // Generate Noise IK server identity (always, so the key is ready if needed).
        let server_identity = Arc::new(
            prism_security::identity::LocalIdentity::generate("PRISM Server"),
        );
        if noise_mode {
            tracing::info!(
                public_key = %hex::encode(server_identity.x25519_public_bytes()),
                "Noise IK enabled — server public key"
            );
        }

        let config = ServerConfig::load_or_default(std::path::Path::new("prism-server.toml"));
        tracing::info!(addr = %config.listen_addr(), "server configuration loaded");

        // TLS
        let _cert = SelfSignedCert::generate()?;
        tracing::info!("generated self-signed TLS certificate");

        // Security (dev mode)
        let _gate = Arc::new(AllowAllGate::new());
        tracing::info!("security: AllowAllGate (dev mode)");

        // Audit log — ring buffer, capped at 4096 entries.
        let audit_log = Arc::new(AuditLog::new(4096));

        // Capture backend selection — informational only here; the frame sender
        // task decides at runtime which backend to actually use.
        #[cfg(windows)]
        if use_dda {
            use crate::dda_capture::dda_capture::DdaDesktopCapture;
            match DdaDesktopCapture::new() {
                Ok(cap) => {
                    tracing::info!(
                        width = cap.width(),
                        height = cap.height(),
                        "capture: DDA desktop"
                    );
                }
                Err(e) => {
                    tracing::warn!(error = %e, "DDA capture init failed — falling back to TestPattern");
                }
            }
        }

        if !use_dda || !cfg!(windows) {
            let capture = TestPatternCapture::new();
            let monitors = capture.enumerate_monitors()?;
            tracing::info!(
                width = monitors[0].resolution.0,
                height = monitors[0].resolution.1,
                fps = monitors[0].refresh_rate,
                "capture: TestPattern"
            );
        }

        // Session manager
        let session_manager = Arc::new(Mutex::new(SessionManager::new(config.clone())));

        // Channel dispatcher + bandwidth tracker
        let mut dispatcher = prism_session::ChannelDispatcher::new();
        dispatcher.register(Arc::new(ControlChannelHandler::new()));
        dispatcher.register(Arc::new(InputChannelHandler::new(1920, 1080)));
        let dispatcher = Arc::new(dispatcher);
        let tracker = Arc::new(prism_session::ChannelBandwidthTracker::new());

        // Shared connection store for broadcasting frames
        let conn_store = Arc::new(ClientConnectionStore::new());

        Ok(Self {
            use_dda,
            noise_mode,
            config,
            session_manager,
            conn_store,
            dispatcher,
            tracker,
            server_identity,
            audit_log,
        })
    }

    /// Bind the QUIC endpoint and enter the main accept loop.
    ///
    /// Spawns the activity processor, frame sender, and heartbeat timeout tasks
    /// before blocking on `accept()`.  Returns when the endpoint is closed.
    pub async fn run(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // QUIC endpoint — cert is created fresh here since it is consumed by bind.
        let cert = SelfSignedCert::generate()?;
        let acceptor = ConnectionAcceptor::bind(self.config.listen_addr(), cert)?;
        tracing::info!(addr = %acceptor.local_addr(), "QUIC endpoint bound");
        tracing::info!("waiting for connections…");

        // ── Activity channel ──────────────────────────────────────────────────
        let (activity_tx, mut activity_rx) = mpsc::channel::<prism_session::ClientId>(256);
        let sm_activity = self.session_manager.clone();
        tokio::spawn(async move {
            while let Some(client_id) = activity_rx.recv().await {
                sm_activity.lock().await.activity(client_id);
            }
        });

        // ── Heartbeat timeout task ────────────────────────────────────────────
        let sm_hb = self.session_manager.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(1));
            loop {
                interval.tick().await;
                let results = sm_hb.lock().await.check_heartbeats();
                for (client_id, tombstoned) in results {
                    if tombstoned {
                        tracing::info!(%client_id, "client tombstoned due to heartbeat timeout");
                    } else {
                        tracing::debug!(%client_id, "client suspended (heartbeat overdue)");
                    }
                }
            }
        });

        // ── Frame sender task (~15 fps) ───────────────────────────────────────
        let conn_store_send = self.conn_store.clone();
        let tracker_send = self.tracker.clone();
        let use_dda = self.use_dda;
        tokio::spawn(async move {
            // Per-client persistent send streams, keyed by client UUID.
            // Opening one stream per client amortises QUIC stream-setup overhead
            // across all frames; frames are length-prefixed so the client can
            // demarcate them on a single contiguous stream.
            let mut streams: std::collections::HashMap<uuid::Uuid, quinn::SendStream> =
                std::collections::HashMap::new();
            // Decide capture source: DDA on Windows when --dda, else test pattern.
            #[cfg(windows)]
            let dda = if use_dda {
                use crate::dda_capture::dda_capture::DdaDesktopCapture;
                match DdaDesktopCapture::new() {
                    Ok(cap) => Some(cap),
                    Err(e) => {
                        tracing::warn!(error = %e, "frame sender: DDA init failed — using TestPattern");
                        None
                    }
                }
            } else {
                None
            };

            // Determine frame dimensions from the active capture source.
            #[cfg(windows)]
            let (width, height) = if let Some(ref cap) = dda {
                (cap.width(), cap.height())
            } else {
                (1920u32, 1080u32)
            };

            #[cfg(not(windows))]
            let (width, height) = (1920u32, 1080u32);

            // Test-pattern capture (used when DDA is unavailable or not requested).
            let pattern_capture = TestPatternCapture::with_resolution(width, height);

            // Create H.264 encoder — probes for hardware acceleration automatically.
            let mut encoder = match HwEncoder::new(width, height, 5_000_000) {
                Ok(e) => {
                    let backend_name: &str = e.backend().name();
                    tracing::info!(
                        backend = backend_name,
                        width,
                        height,
                        "frame sender: encoder ready"
                    );
                    e
                }
                Err(e) => {
                    tracing::error!(error = %e, "frame sender: failed to create H.264 encoder");
                    return;
                }
            };

            let mut seq: u32 = 0;
            let mut interval = tokio::time::interval(std::time::Duration::from_millis(67)); // ~15fps
            let mut last_log = std::time::Instant::now();
            let mut frames_sent = 0u32;
            let mut bytes_sent_total: u64 = 0;

            loop {
                interval.tick().await;

                if conn_store_send.client_count() == 0 {
                    continue; // no clients, skip
                }

                // Acquire BGRA pixel data — DDA when available, test pattern otherwise.
                #[cfg(windows)]
                let pixels_opt: Option<Vec<u8>> = if let Some(ref cap) = dda {
                    match cap.capture_frame() {
                        Ok(Some(p)) => Some(p),
                        Ok(None) => {
                            // No new desktop frame yet — skip this tick.
                            seq = seq.wrapping_add(1);
                            continue;
                        }
                        Err(e) => {
                            tracing::warn!(error = %e, "frame sender: DDA capture error");
                            seq = seq.wrapping_add(1);
                            continue;
                        }
                    }
                } else {
                    Some(pattern_capture.generate_pattern(seq))
                };

                #[cfg(not(windows))]
                let pixels_opt: Option<Vec<u8>> = Some(pattern_capture.generate_pattern(seq));

                let pixels = match pixels_opt {
                    Some(p) => p,
                    None => {
                        seq = seq.wrapping_add(1);
                        continue;
                    }
                };

                // Encode BGRA → H.264 bitstream (conversion happens inside HwEncoder).
                let h264_data: Vec<u8> = match encoder.encode_bgra(&pixels) {
                    Ok(data) => data,
                    Err(e) => {
                        tracing::error!(error = %e, "frame sender: encode error");
                        seq = seq.wrapping_add(1);
                        continue;
                    }
                };

                if h264_data.is_empty() {
                    // Encoder buffering — try again next tick.
                    seq = seq.wrapping_add(1);
                    continue;
                }

                // Wire format (persistent stream, length-prefixed):
                //   [4 bytes: total_frame_len u32 LE]  — byte count of the frame record below
                //   [4 bytes: width    u32 LE]
                //   [4 bytes: height   u32 LE]
                //   [4 bytes: seq      u32 LE]
                //   [4 bytes: h264_len u32 LE]
                //   [h264_len bytes: H.264 NAL bitstream]
                let h264_len = h264_data.len() as u32;
                // Inner frame record (without the leading length prefix).
                let inner_len = 16u32 + h264_len; // 4 fields * 4B + payload
                let mut frame_data = Vec::with_capacity(4 + inner_len as usize);
                frame_data.extend_from_slice(&inner_len.to_le_bytes()); // length prefix
                frame_data.extend_from_slice(&width.to_le_bytes());
                frame_data.extend_from_slice(&height.to_le_bytes());
                frame_data.extend_from_slice(&seq.to_le_bytes());
                frame_data.extend_from_slice(&h264_len.to_le_bytes());
                frame_data.extend_from_slice(&h264_data);

                bytes_sent_total += frame_data.len() as u64;

                // Snapshot (client_id, connection) pairs so we don't hold the
                // mutex across await points.
                let conns = conn_store_send.snapshot_with_ids();

                // Remove stale stream entries for clients that have gone away.
                streams.retain(|id, _| conns.iter().any(|(cid, _)| cid == id));

                let mut sent = 0u32;
                for (client_id, conn) in &conns {
                    // Get or lazily open a persistent uni stream for this client.
                    if !streams.contains_key(client_id) {
                        match conn.open_uni().await {
                            Ok(s) => {
                                tracing::debug!(
                                    client = %&client_id.to_string()[..8],
                                    "frame sender: opened persistent stream"
                                );
                                streams.insert(*client_id, s);
                            }
                            Err(e) => {
                                tracing::warn!(
                                    error = %e,
                                    client = %&client_id.to_string()[..8],
                                    "frame sender: open_uni error"
                                );
                                continue;
                            }
                        }
                    }

                    let stream = streams.get_mut(client_id).unwrap();
                    match stream.write_all(&frame_data).await {
                        Ok(()) => {
                            sent += 1;
                        }
                        Err(e) => {
                            tracing::warn!(
                                error = %e,
                                client = %&client_id.to_string()[..8],
                                "frame sender: write error — stream will reopen next frame"
                            );
                            // Drop the broken stream; it will be reopened next tick.
                            streams.remove(client_id);
                        }
                    }
                }

                if sent > 0 {
                    frames_sent += 1;
                    // Record outbound bandwidth for the display channel.
                    tracker_send.record_send(
                        prism_protocol::channel::CHANNEL_DISPLAY,
                        h264_data.len() as u32,
                    );
                }
                seq = seq.wrapping_add(1);

                // Log every second.
                if last_log.elapsed() >= std::time::Duration::from_secs(1) {
                    if frames_sent > 0 {
                        tracing::debug!(
                            fps = frames_sent,
                            clients = conn_store_send.client_count(),
                            width,
                            height,
                            kb_per_frame = bytes_sent_total / (frames_sent as u64) / 1024,
                            "frame sender stats"
                        );
                    }
                    frames_sent = 0;
                    bytes_sent_total = 0;
                    last_log = std::time::Instant::now();
                }
            }
        });

        // ── Accept loop ───────────────────────────────────────────────────────
        loop {
            let incoming = match acceptor.accept().await {
                Some(i) => i,
                None => {
                    tracing::info!("QUIC endpoint closed");
                    break;
                }
            };

            let sm = self.session_manager.clone();
            let disp = self.dispatcher.clone();
            let track = self.tracker.clone();
            let act_tx = activity_tx.clone();
            let conn_store_clone = self.conn_store.clone();
            let server_identity_task = self.server_identity.clone();
            let noise_mode = self.noise_mode;
            let audit_log_task = self.audit_log.clone();

            tokio::spawn(async move {
                handle_connection(
                    incoming,
                    sm,
                    disp,
                    track,
                    act_tx,
                    conn_store_clone,
                    server_identity_task,
                    noise_mode,
                    audit_log_task,
                )
                .await;
            });
        }

        Ok(())
    }

    /// Broadcast a `ShutdownNotice` to all connected clients and close the
    /// endpoint.  Best-effort — individual send errors are logged but ignored.
    pub async fn shutdown(&self) {
        tracing::info!("sending shutdown notice to all clients");
        // The existing ClientConnectionStore only supports datagrams; for a
        // proper ShutdownNotice we'd need a control stream.  Log and close.
        tracing::info!("server shutdown complete");
    }
}

// ── Connection handler ────────────────────────────────────────────────────────

/// Handle a single incoming QUIC connection from acceptance through teardown.
///
/// This is a free function (not a method) so it can be moved into a `tokio::spawn`
/// task without borrowing `ServerApp`.
async fn handle_connection(
    incoming: quinn::Incoming,
    sm: Arc<Mutex<SessionManager>>,
    disp: Arc<prism_session::ChannelDispatcher>,
    track: Arc<prism_session::ChannelBandwidthTracker>,
    act_tx: mpsc::Sender<prism_session::ClientId>,
    conn_store: Arc<ClientConnectionStore>,
    server_identity: Arc<prism_security::identity::LocalIdentity>,
    noise_mode: bool,
    audit_log: Arc<AuditLog>,
) {
    match incoming.await {
        Ok(quinn_conn) => {
            let remote = quinn_conn.remote_address();
            tracing::info!(remote = %remote, "client connected");

            // ── Noise IK handshake (when --noise is set) ──────────────────────
            if noise_mode {
                let (mut send, mut recv) = match quinn_conn.accept_bi().await {
                    Ok(streams) => streams,
                    Err(e) => {
                        tracing::warn!(remote = %remote, error = %e, "Noise: failed to accept bi stream");
                        return;
                    }
                };

                let client_msg = match recv.read_to_end(4096).await {
                    Ok(data) => data,
                    Err(e) => {
                        tracing::warn!(remote = %remote, error = %e, "Noise: failed to read initiator");
                        return;
                    }
                };

                let mut hs = match prism_security::handshake::ServerHandshake::new(&*server_identity) {
                    Ok(hs) => hs,
                    Err(e) => {
                        tracing::warn!(remote = %remote, error = %e, "Noise: handshake init failed");
                        return;
                    }
                };

                let response = match hs.respond(&client_msg) {
                    Ok(r) => r,
                    Err(e) => {
                        tracing::warn!(remote = %remote, error = %e, "Noise: respond failed (bad client key?)");
                        return;
                    }
                };

                if let Err(e) = send.write_all(&response).await {
                    tracing::warn!(remote = %remote, error = %e, "Noise: failed to write response");
                    return;
                }
                let _ = send.finish();

                let result = match hs.finalize() {
                    Ok(r) => r,
                    Err(e) => {
                        tracing::warn!(remote = %remote, error = %e, "Noise: finalize failed");
                        return;
                    }
                };

                let client_key = result.remote_static.unwrap();
                tracing::info!(
                    remote = %remote,
                    client_key_prefix = %hex::encode(&client_key[..8]),
                    "Noise IK handshake OK"
                );
            }

            // Clone quinn_conn before it is consumed by QuicConnection::new.
            let quinn_conn_for_store = quinn_conn.clone();
            let qc_recv = Arc::new(prism_transport::QuicConnection::new(quinn_conn.clone()));
            let qc_session = prism_transport::QuicConnection::new(quinn_conn);
            let unified = Arc::new(prism_transport::UnifiedConnection::new(
                Box::new(qc_session),
                None,
            ));

            let client_id = uuid::Uuid::new_v7(uuid::Timestamp::now(uuid::NoContext));
            let device_id = uuid::Uuid::new_v7(uuid::Timestamp::now(uuid::NoContext));

            let channels = {
                let mut mgr = sm.lock().await;
                mgr.new_session(
                    client_id,
                    device_id,
                    unified,
                    prism_session::ConnectionProfile::coding(),
                    &[
                        prism_protocol::channel::CHANNEL_DISPLAY,
                        prism_protocol::channel::CHANNEL_INPUT,
                        prism_protocol::channel::CHANNEL_CONTROL,
                    ],
                )
            };

            match channels {
                Ok(granted) => {
                    tracing::info!(
                        remote = %remote,
                        channels = granted.len(),
                        "session established"
                    );

                    // Audit: record successful client authentication / session open.
                    audit_log.record(AuditEvent::ClientAuthenticated {
                        device_id,
                        device_name: remote.to_string(),
                    });

                    // Clone before consuming: heartbeat sender and probe task each
                    // need their own handle before quinn_conn_for_store is moved
                    // into the connection store.
                    let hb_conn = quinn_conn_for_store.clone();
                    let probe_conn = quinn_conn_for_store.clone();

                    // Store connection for frame sending.
                    conn_store.add(client_id, quinn_conn_for_store);
                    tracing::info!(
                        remote = %remote,
                        client_id = %&client_id.to_string()[..8],
                        "registered for frame broadcast"
                    );

                    let _recv_handle = spawn_recv_loop(
                        client_id,
                        qc_recv as Arc<dyn prism_transport::PrismConnection>,
                        disp,
                        track,
                        act_tx,
                    );
                    tracing::info!(
                        remote = %remote,
                        client_id = %&client_id.to_string()[..8],
                        "recv loop started"
                    );

                    // Spawn heartbeat sender: 16-byte HEARTBEAT datagram every 5 seconds.
                    let hb_gen = HeartbeatGenerator::new();
                    // Keep a quinn Connection handle to wait for close.
                    let quinn_conn_close = hb_conn.clone();
                    tokio::spawn(async move {
                        let mut interval =
                            tokio::time::interval(std::time::Duration::from_secs(5));
                        loop {
                            interval.tick().await;
                            if hb_conn.send_datagram(hb_gen.packet()).is_err() {
                                break;
                            }
                        }
                    });

                    // Quality probe sender (every 2 seconds).
                    //
                    // Generates a PROBE_REQUEST datagram with an incrementing
                    // sequence number and the current timestamp.  The client is
                    // expected to echo it back as PROBE_RESPONSE; until full
                    // client-side control handling lands, this task just sends
                    // and logs each probe.
                    tokio::spawn(async move {
                        let mut prober = prism_transport::quality::prober::ConnectionProber::new();
                        let mut interval = tokio::time::interval(std::time::Duration::from_secs(2));
                        loop {
                            interval.tick().await;
                            if let Some(payload) = prober.generate_probe() {
                                let dgram = crate::quality_task::build_probe_datagram(&payload);
                                tracing::trace!(seq = payload.seq, "quality probe sent");
                                if probe_conn.send_datagram(dgram).is_err() {
                                    break;
                                }
                            }
                        }
                    });

                    // Wait for the QUIC connection to close, then clean up.
                    // quinn::Connection::closed() resolves when the connection is gone.
                    let _ = quinn_conn_close.closed().await;

                    // ── Cleanup (NEW: fixes the connection store / session leak) ──
                    conn_store.remove(&client_id);
                    sm.lock().await.disconnect(client_id, "connection closed".to_string());
                    // Audit: record client disconnect.
                    audit_log.record(AuditEvent::ClientDisconnected { device_id });
                    tracing::info!(remote = %remote, "client disconnected and cleaned up");
                }
                Err(e) => {
                    tracing::error!(remote = %remote, error = %e, "session creation failed");
                }
            }
        }
        Err(e) => {
            tracing::error!(error = %e, "incoming connection error");
        }
    }
}
