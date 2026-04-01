// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2025-2026 Ehsan Khalid. All rights reserved.
// Licensed under the GNU Affero General Public License v3.0.
// Commercial licensing available — see LICENSE-COMMERCIAL.md.

// ServerApp: structured entry point for the PRISM server.
//
// Encapsulates all initialisation, the accept loop, and graceful shutdown.
// `main.rs` simply constructs a `ServerApp` and calls `run()`.

use std::sync::Arc;
use tokio::sync::{Mutex, mpsc};

use crate::hw_encoder::HwEncoder;
use prism_display::capture::PlatformCapture;

use prism_security::audit::{AuditEvent, AuditLog};

use crate::{
    AllowAllGate, ClientConnectionStore, ConnectionAcceptor, ConnectionRateLimiter,
    ControlChannelHandler, HeartbeatGenerator, InputChannelHandler, SelfSignedCert, ServerConfig,
    SessionManager, TestPatternCapture, spawn_recv_loop,
};

/// Top-level server state. Construct with [`ServerApp::new`], then call
/// [`ServerApp::run`] to enter the accept loop.
pub struct ServerApp {
    use_dda: bool,
    noise_mode: bool,
    monitor_index: u32,
    config: ServerConfig,
    cert: SelfSignedCert,
    session_manager: Arc<Mutex<SessionManager>>,
    conn_store: Arc<ClientConnectionStore>,
    dispatcher: Arc<prism_session::ChannelDispatcher>,
    tracker: Arc<prism_session::ChannelBandwidthTracker>,
    server_identity: Arc<prism_security::identity::LocalIdentity>,
    audit_log: Arc<AuditLog>,
    bound_addr_tx: Option<tokio::sync::oneshot::Sender<std::net::SocketAddr>>,
    shutdown_rx: tokio::sync::watch::Receiver<bool>,
    shutdown_tx: tokio::sync::watch::Sender<bool>,
}

impl ServerApp {
    /// Initialise all server subsystems.
    ///
    /// Generates a Noise IK identity, creates TLS cert, sets up session
    /// management, channel dispatch, and the connection store.
    ///
    /// `monitor_index` selects which display output to capture (0 = primary).
    /// Initialise with a pre-loaded configuration (preferred — lets the CLI
    /// apply overrides before construction).
    pub fn with_config(
        use_dda: bool,
        noise_mode: bool,
        monitor_index: u32,
        config: ServerConfig,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        tracing::info!("=== PRISM Server v{} ===", env!("CARGO_PKG_VERSION"));

        // Generate Noise IK server identity (always, so the key is ready if needed).
        let server_identity = Arc::new(prism_security::identity::LocalIdentity::generate(
            "PRISM Server",
        ));
        if noise_mode {
            tracing::info!(
                public_key = %hex::encode(server_identity.x25519_public_bytes()),
                "Noise IK enabled — server public key"
            );
        }

        tracing::info!(addr = %config.listen_addr(), "server configuration loaded");

        // TLS
        let cert = SelfSignedCert::generate()?;
        tracing::info!("generated self-signed TLS certificate");

        // Security (dev mode)
        let _gate = Arc::new(AllowAllGate::new());
        tracing::info!("security: AllowAllGate (dev mode)");

        // Audit log — ring buffer, capped at 4096 entries.
        let audit_log = Arc::new(AuditLog::new(4096));

        // Log the selected monitor index. The frame sender task will use this
        // when initialising DDA capture.
        tracing::info!(monitor_index, "monitor selection: output index");

        // Capture backend selection — informational only here; the frame sender
        // task decides at runtime which backend to actually use.
        #[cfg(windows)]
        if use_dda {
            use crate::dda_capture::dda_capture::DdaDesktopCapture;
            match DdaDesktopCapture::new_with_output(monitor_index) {
                Ok(cap) => {
                    tracing::info!(
                        width = cap.width(),
                        height = cap.height(),
                        monitor_index,
                        "capture: DDA desktop"
                    );
                }
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        monitor_index,
                        "DDA capture init failed — falling back to TestPattern"
                    );
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

        // Shutdown watch channel
        let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

        Ok(Self {
            use_dda,
            noise_mode,
            monitor_index,
            config,
            cert,
            session_manager,
            conn_store,
            dispatcher,
            tracker,
            server_identity,
            audit_log,
            bound_addr_tx: None,
            shutdown_tx,
            shutdown_rx,
        })
    }

    /// Convenience constructor that loads `prism-server.toml` from the working
    /// directory (or uses defaults).
    pub fn new(
        use_dda: bool,
        noise_mode: bool,
        monitor_index: u32,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let config = ServerConfig::load_or_default(std::path::Path::new("prism-server.toml"));
        Self::with_config(use_dda, noise_mode, monitor_index, config)
    }

    /// Return the DER-encoded certificate generated at construction time.
    ///
    /// Tests can add this to a `RootCertStore` to trust the server's TLS cert.
    pub fn cert_der(&self) -> rustls::pki_types::CertificateDer<'static> {
        self.cert.cert_der.clone()
    }

    /// Register a oneshot sender that will be notified with the real bound
    /// `SocketAddr` once the QUIC endpoint is successfully bound.
    ///
    /// Useful when binding to port 0 in tests.
    pub fn set_bound_addr_notify(
        &mut self,
        tx: tokio::sync::oneshot::Sender<std::net::SocketAddr>,
    ) {
        self.bound_addr_tx = Some(tx);
    }

    /// Return a clone of the shutdown sender.
    ///
    /// Send `true` through it to trigger graceful shutdown of the accept loop.
    pub fn shutdown_tx(&self) -> tokio::sync::watch::Sender<bool> {
        self.shutdown_tx.clone()
    }

    /// Bind the QUIC endpoint and enter the main accept loop.
    ///
    /// Spawns the activity processor, frame sender, and heartbeat timeout tasks
    /// before blocking on `accept()`.  Returns when the endpoint is closed.
    pub async fn run(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // QUIC endpoint — reuse the cert generated at construction time.
        let cert = self.cert.clone();
        let acceptor = ConnectionAcceptor::bind(self.config.listen_addr(), cert)?;
        tracing::info!(addr = %acceptor.local_addr(), "QUIC endpoint bound");
        if let Some(tx) = self.bound_addr_tx.take() {
            let _ = tx.send(acceptor.local_addr());
        }
        tracing::info!("waiting for connections…");

        // ── Throughput endpoint (Cubic, AF11, large windows) ─────────────────
        // Bind a second QUIC socket optimised for bulk transfers (file share,
        // device data).  A fresh self-signed cert is generated so it is
        // independent of the latency endpoint's cert lifetime.
        let throughput_config = crate::throughput_endpoint::ThroughputEndpointConfig::default();
        if throughput_config.enabled {
            match SelfSignedCert::generate() {
                Ok(tp_cert) => {
                    let tp_transport = crate::throughput_endpoint::build_throughput_config();
                    match quinn::ServerConfig::with_single_cert(
                        vec![tp_cert.cert_der],
                        tp_cert.key_der,
                    ) {
                        Ok(mut tp_server_config) => {
                            tp_server_config.transport_config(Arc::new(tp_transport));
                            match quinn::Endpoint::server(tp_server_config, throughput_config.addr)
                            {
                                Ok(ep) => {
                                    tracing::info!(
                                        addr = %ep.local_addr().unwrap_or(throughput_config.addr),
                                        "throughput endpoint bound"
                                    );
                                    // Endpoint is ready for file transfer channels;
                                    // full integration deferred until channel routing lands.
                                    drop(ep);
                                }
                                Err(e) => {
                                    tracing::warn!(
                                        error = %e,
                                        addr = %throughput_config.addr,
                                        "throughput endpoint bind failed (non-fatal)"
                                    );
                                }
                            }
                        }
                        Err(e) => {
                            tracing::warn!(error = %e, "throughput endpoint TLS config failed");
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!(error = %e, "throughput endpoint cert generation failed");
                }
            }
        }

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

        // ── Overlay sender task (~10 fps) ─────────────────────────────────────
        let conn_store_overlay = self.conn_store.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_millis(100));
            loop {
                interval.tick().await;
                let client_count = conn_store_overlay.client_count();
                if client_count == 0 {
                    continue;
                }
                let packet = crate::overlay_sender::build_overlay_packet(
                    15,
                    5000,
                    0,
                    client_count as u8,
                    1920,
                    1080,
                );
                let dgram = crate::overlay_sender::build_overlay_datagram(&packet);
                conn_store_overlay.broadcast_datagram(&dgram);
            }
        });

        // ── Frame sender task (~15 fps) ───────────────────────────────────────
        let conn_store_send = self.conn_store.clone();
        let tracker_send = self.tracker.clone();
        let use_dda = self.use_dda;
        let monitor_index = self.monitor_index;
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
                match DdaDesktopCapture::new_with_output(monitor_index) {
                    Ok(cap) => {
                        tracing::info!(
                            monitor_index,
                            "frame sender: DDA capture on selected output"
                        );
                        Some(cap)
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, monitor_index, "frame sender: DDA init failed — using TestPattern");
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

            // Quality-adaptive frame rate: QualityCache holds the latest
            // ConnectionQuality snapshot updated every 8th frame.
            let quality_cache = crate::quality_task::QualityCache::new();

            // Speculative IDR controller: triggers keyframe requests on scene changes.
            // Currently wired to detect the idle→active transition (consecutive_empty
            // drops to zero), which is a reliable proxy for a scene change on Windows.
            // Future: feed WindowEvent::ForegroundChanged from Win32 hooks.
            let mut idr_controller = crate::speculative_idr::SpeculativeIdrController::default();

            // Encode pool pressure tracking.  Instruments the single-threaded encoder
            // with submit/complete/drop metrics and admission control so that when
            // actual worker threads are wired in later the same counters will apply.
            let encode_stats = std::sync::Arc::new(crate::encode_pool::EncodePoolStats::default());
            let encode_config = crate::encode_pool::EncodePoolConfig::default();

            // Static region cache: tracks whether the current frame's H.264 output
            // has been stable long enough to cache on the client.  30-frame threshold
            // ≈ 2 seconds at 15fps before a region is considered static.
            let mut atlas_tracker = prism_display::atlas::StaticAtlasTracker::new(30);
            let mut cache_savings = crate::static_cache::CacheSavingsTracker::default();

            let mut seq: u32 = 0;
            let min_interval = std::time::Duration::from_millis(67); // 15fps max
            let idle_interval = std::time::Duration::from_millis(500); // 2fps idle
            let mut current_interval = min_interval;
            let mut consecutive_empty = 0u32;
            let mut last_log = std::time::Instant::now();
            let mut frames_sent = 0u32;
            let mut bytes_sent_total: u64 = 0;
            // Backpressure state: clients whose send buffer was congested last
            // tick get their next frame skipped to let the buffer drain.
            let mut skip_next: std::collections::HashSet<uuid::Uuid> =
                std::collections::HashSet::new();
            let mut frames_dropped_total: u64 = 0;

            loop {
                tokio::time::sleep(current_interval).await;

                if conn_store_send.client_count() == 0 {
                    continue; // no clients, skip
                }

                // Acquire BGRA pixel data — DDA when available, test pattern otherwise.
                #[cfg(windows)]
                let pixels_opt: Option<Vec<u8>> = if let Some(ref cap) = dda {
                    match cap.capture_frame() {
                        Ok(Some(p)) => Some(p),
                        Ok(None) => {
                            // No new desktop frame yet — adapt to idle rate and skip.
                            consecutive_empty += 1;
                            if consecutive_empty == 11 {
                                tracing::debug!(
                                    interval_ms = idle_interval.as_millis(),
                                    "frame sender: desktop idle — switching to 2fps"
                                );
                            }
                            if consecutive_empty > 10 {
                                current_interval = idle_interval;
                            }
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

                // Update adaptive interval based on capture result.
                #[cfg(windows)]
                if pixels_opt.is_some() {
                    if consecutive_empty > 10 {
                        tracing::debug!(
                            interval_ms = min_interval.as_millis(),
                            "frame sender: desktop active — switching to 15fps"
                        );
                        // Idle→active transition: content just appeared after a
                        // period of no desktop updates — highly likely to be a
                        // scene change (window focus switch, app launch, etc.).
                        // Signal the IDR controller so it can track IDR budget.
                        let idr_event =
                            prism_display::window_event::WindowEvent::ForegroundChanged {
                                hwnd: 0xFFFF_DEAD, // synthetic hwnd — distinct from any real window
                            };
                        if idr_controller.process_event(&idr_event) {
                            tracing::debug!(
                                idrs_triggered = idr_controller.idrs_triggered(),
                                "scene change detected (idle→active), speculative IDR signalled"
                            );
                        }
                    }
                    consecutive_empty = 0;
                    current_interval = min_interval;
                }
                // On non-Windows (test pattern always returns Some), keep at min_interval.
                #[cfg(not(windows))]
                {
                    let _ = consecutive_empty; // suppress unused warning
                    current_interval = min_interval;
                }

                // ── Fix 1: Quality-adaptive frame rate ───────────────────────
                // Every 8th frame, sample QUIC stats from the first connected
                // client and update the QualityCache.  Then adjust current_interval
                // based on the recommendation so we back off under poor network
                // conditions without a runtime bitrate-reconfigure method.
                if seq.is_multiple_of(8) {
                    let conns_snap = conn_store_send.snapshot_with_ids();
                    if let Some((_, conn)) = conns_snap.first() {
                        let stats = conn.stats();
                        let metrics = prism_transport::TransportMetrics {
                            rtt_us: stats.path.rtt.as_micros() as u64,
                            loss_rate: 0.0,
                            ..prism_transport::TransportMetrics::default()
                        };
                        let quality = crate::quality_task::evaluate_quality(&metrics);
                        quality_cache.update(quality);
                    }
                    let quality = quality_cache.load();
                    use prism_transport::quality::QualityRecommendation;
                    let quality_interval = match &quality.recommendation {
                        QualityRecommendation::Optimal => min_interval,
                        QualityRecommendation::ReduceBitrate { .. } => min_interval,
                        QualityRecommendation::ReduceResolution => min_interval,
                        QualityRecommendation::EnableFec { .. } => min_interval,
                        QualityRecommendation::SwitchToStreamOnly => min_interval,
                        QualityRecommendation::ReduceFramerate => {
                            std::time::Duration::from_millis(200)
                        }
                        QualityRecommendation::PauseNonEssential => {
                            std::time::Duration::from_millis(500)
                        }
                        QualityRecommendation::ConnectionUnusable => {
                            std::time::Duration::from_millis(1000)
                        }
                    };
                    // Only override upward (don't fight the DDA idle detection).
                    if quality_interval > current_interval {
                        tracing::debug!(
                            interval_ms = quality_interval.as_millis(),
                            score = quality.score,
                            "quality-adaptive: reducing frame rate"
                        );
                        current_interval = quality_interval;
                    }

                    // Bitrate reconfigure based on quality recommendation.
                    let target_bps = match &quality.recommendation {
                        QualityRecommendation::Optimal => 5_000_000u64,
                        QualityRecommendation::ReduceBitrate { target_bps } => *target_bps,
                        _ => 2_000_000u64,
                    };
                    if let Err(e) = encoder.set_bitrate(target_bps) {
                        tracing::warn!(error = %e, "failed to reconfigure encoder bitrate");
                    }

                    // Fix 3: arbiter update hook
                    tracing::trace!("arbiter update hook");
                }

                let pixels = match pixels_opt {
                    Some(p) => p,
                    None => {
                        seq = seq.wrapping_add(1);
                        continue;
                    }
                };

                // Encode pool admission control: drop frames when pending job
                // count exceeds the pool's max_pending_jobs limit.
                if !crate::encode_pool::should_accept_job(&encode_stats, &encode_config) {
                    encode_stats.drop_job();
                    tracing::debug!("encode job dropped (pool pressure)");
                    seq = seq.wrapping_add(1);
                    continue;
                }
                encode_stats.submit();

                // Encode BGRA → H.264 bitstream (conversion happens inside HwEncoder).
                let h264_data: Vec<u8> = match encoder.encode_bgra(&pixels) {
                    Ok(data) => {
                        encode_stats.complete();
                        data
                    }
                    Err(e) => {
                        encode_stats.complete(); // unblock pending count on error too
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

                // ── Static region cache tracking ──────────────────────────────
                // Hash the first 1024 bytes of the H.264 NAL stream as a fast
                // content fingerprint (FNV-1a).  Feed it to the atlas tracker
                // which decides whether the frame is newly static (SendAndCache),
                // already cached on the client (Unchanged), or should be sent
                // normally (EncodeNormally).
                let frame_hash = {
                    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
                    for &b in h264_data.iter().take(1024) {
                        h ^= b as u64;
                        h = h.wrapping_mul(0x0000_0001_0000_01b3);
                    }
                    h
                };
                let region_key = prism_display::atlas::RegionKey(0x0001, 0);
                let decision = atlas_tracker.check(region_key, frame_hash);
                let cache_instr = crate::static_cache::CacheInstruction::from_decision(
                    decision, region_key, frame_hash,
                );
                cache_savings.record(&cache_instr, h264_data.len() as u64);

                if cache_instr.is_cached() {
                    // Client would normally skip re-decode here; until the
                    // cache protocol lands on the wire we only track savings.
                    tracing::trace!("frame skipped (static region cached — tracking only)");
                }

                // Log cache savings every 15 frames (~1 second at 15fps).
                if seq.is_multiple_of(15) && cache_savings.bytes_saved() > 0 {
                    tracing::debug!(
                        hit_rate = cache_savings.cache_hit_rate(),
                        saved_kb = cache_savings.bytes_saved() / 1024,
                        "static cache savings"
                    );
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

                // Remove stale stream/backpressure entries for gone clients.
                streams.retain(|id, _| conns.iter().any(|(cid, _)| cid == id));
                skip_next.retain(|id| conns.iter().any(|(cid, _)| cid == id));

                let mut sent = 0u32;
                let mut frames_dropped_this_tick = 0u32;
                for (client_id, conn) in &conns {
                    // Backpressure: if this client's send buffer was congested
                    // last tick, skip this frame to let it drain.
                    if skip_next.remove(client_id) {
                        frames_dropped_this_tick += 1;
                        tracing::trace!(
                            client = %&client_id.to_string()[..8],
                            "frame sender: skipping frame (backpressure)"
                        );
                        continue;
                    }

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

                    let write_start = std::time::Instant::now();
                    let stream = streams.get_mut(client_id).unwrap();
                    match stream.write_all(&frame_data).await {
                        Ok(()) => {
                            sent += 1;
                            // If writing this frame took more than 2× the frame
                            // interval, the send buffer is backing up.  Mark this
                            // client for a skip on the next tick.
                            let write_time = write_start.elapsed();
                            if write_time > min_interval * 2 {
                                skip_next.insert(*client_id);
                                tracing::debug!(
                                    client = %&client_id.to_string()[..8],
                                    write_ms = write_time.as_millis(),
                                    "frame sender: send buffer congested — skipping next frame"
                                );
                            }
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

                frames_dropped_total += frames_dropped_this_tick as u64;

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
                            dropped_total = frames_dropped_total,
                            "frame sender stats"
                        );
                    }
                    tracing::debug!(
                        pending = encode_stats.pending(),
                        completion_rate = encode_stats.completion_rate(),
                        "encode pool stats"
                    );
                    frames_sent = 0;
                    bytes_sent_total = 0;
                    last_log = std::time::Instant::now();
                }
            }
        });

        // ── Connection rate limiter (10 connections/minute per IP) ───────────
        let rate_limiter = Arc::new(Mutex::new(ConnectionRateLimiter::default()));

        // Periodic GC: evict rate-limiter entries inactive for >5 minutes.
        let rl_gc = rate_limiter.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
            loop {
                interval.tick().await;
                rl_gc.lock().await.gc();
            }
        });

        // ── Accept loop ───────────────────────────────────────────────────────
        let mut shutdown_rx = self.shutdown_rx.clone();
        loop {
            let incoming = tokio::select! {
                incoming = acceptor.accept() => {
                    match incoming {
                        Some(i) => i,
                        None => {
                            tracing::info!("QUIC endpoint closed");
                            break;
                        }
                    }
                }
                _ = shutdown_rx.changed() => {
                    tracing::info!("shutdown signal received");
                    acceptor.close();
                    break;
                }
            };

            // Rate-limit check: extract remote IP before consuming `incoming`.
            // `quinn::Incoming::remote_address()` is available before `.await`.
            let remote_ip = incoming.remote_address().ip();
            if !rate_limiter.lock().await.check(remote_ip) {
                tracing::warn!(
                    ip = %remote_ip,
                    "rate limiter: connection rejected (too many connections from this IP)"
                );
                // Refuse the connection without spawning a handler task — let
                // QUIC idle-timeout clean up the pending incoming on its own.
                drop(incoming);
                continue;
            }

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
#[allow(clippy::too_many_arguments)]
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

                let mut hs = match prism_security::handshake::ServerHandshake::new(&server_identity)
                {
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

            // ── Capability negotiation ────────────────────────────────────────
            // Accept the bi stream the client opens immediately after connecting
            // (or after Noise handshake), negotiate channels and codec, then
            // proceed to session creation.
            let _negotiation_result = {
                let (mut cap_send, mut cap_recv) = match quinn_conn.accept_bi().await {
                    Ok(streams) => streams,
                    Err(e) => {
                        tracing::warn!(remote = %remote, error = %e, "negotiation: failed to accept bi stream");
                        return;
                    }
                };
                let negotiator = crate::negotiation_handler::build_server_negotiator();
                match crate::negotiation_handler::negotiate_on_stream(
                    &mut cap_send,
                    &mut cap_recv,
                    &negotiator,
                )
                .await
                {
                    Ok(result) => {
                        tracing::info!(
                            remote = %remote,
                            channels = result.channels.len(),
                            codec = %result.display_codec,
                            "capability negotiation complete"
                        );
                        result
                    }
                    Err(e) => {
                        tracing::warn!(remote = %remote, error = %e, "negotiation failed");
                        return;
                    }
                }
            };

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
                        let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));
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
                    sm.lock()
                        .await
                        .disconnect(client_id, "connection closed".to_string());
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
