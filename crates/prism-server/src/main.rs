use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use prism_display::capture::PlatformCapture;
use prism_server::hw_encoder::HwEncoder;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== PRISM Server v0.1.0 ===");

    let use_dda = std::env::args().any(|a| a == "--dda");
    let noise_mode = std::env::args().any(|a| a == "--noise");

    // Generate Noise IK server identity (always, so the key is ready if needed).
    let server_identity = Arc::new(prism_security::identity::LocalIdentity::generate("PRISM Server"));
    if noise_mode {
        println!("Noise IK enabled. Server public key:");
        println!("  {}", hex::encode(server_identity.x25519_public_bytes()));
    }

    let config = prism_server::ServerConfig::default();
    println!("Listening on {}", config.listen_addr);

    // TLS
    let cert = prism_server::SelfSignedCert::generate()?;
    println!("Generated self-signed TLS certificate");

    // Security (dev mode)
    let _gate = Arc::new(prism_server::AllowAllGate::new());
    println!("Security: AllowAllGate (dev mode)");

    // Capture backend selection
    #[cfg(windows)]
    if use_dda {
        use prism_server::dda_capture::dda_capture::DdaDesktopCapture;
        match DdaDesktopCapture::new() {
            Ok(cap) => {
                println!("Capture: DDA Desktop {}x{}", cap.width(), cap.height());
            }
            Err(e) => {
                eprintln!("DDA capture initialisation failed: {} — falling back to TestPattern", e);
            }
        }
    }

    if !use_dda || !cfg!(windows) {
        // Test pattern capture (fallback / non-Windows)
        let capture = prism_server::TestPatternCapture::new();
        let monitors = capture.enumerate_monitors()?;
        println!(
            "Capture: TestPattern {}x{} @ {}fps",
            monitors[0].resolution.0, monitors[0].resolution.1, monitors[0].refresh_rate
        );
    }

    // Session manager
    let session_manager = Arc::new(Mutex::new(prism_server::SessionManager::new(config.clone())));

    // Channel dispatcher + bandwidth tracker
    let mut dispatcher = prism_session::ChannelDispatcher::new();
    dispatcher.register(Arc::new(prism_server::ControlChannelHandler::new()));
    dispatcher.register(Arc::new(prism_server::InputChannelHandler::new(1920, 1080)));
    let dispatcher = Arc::new(dispatcher);
    let tracker = Arc::new(prism_session::ChannelBandwidthTracker::new());

    // Shared connection store for broadcasting frames
    let conn_store = Arc::new(prism_server::ClientConnectionStore::new());

    // QUIC endpoint
    let acceptor = prism_server::ConnectionAcceptor::bind(config.listen_addr, cert)?;
    println!("QUIC endpoint bound to {}", acceptor.local_addr());
    println!("Waiting for connections...\n");

    // Activity channel
    let (activity_tx, mut activity_rx) = mpsc::channel::<prism_session::ClientId>(256);
    let sm_activity = session_manager.clone();
    tokio::spawn(async move {
        while let Some(client_id) = activity_rx.recv().await {
            sm_activity.lock().await.activity(client_id);
        }
    });

    // Spawn frame sender task (~15fps) — encodes BGRA→H.264 and sends over QUIC uni streams.
    let conn_store_send = conn_store.clone();
    tokio::spawn(async move {
        // Decide capture source: DDA on Windows when --dda, else test pattern.
        #[cfg(windows)]
        let dda = if use_dda {
            use prism_server::dda_capture::dda_capture::DdaDesktopCapture;
            match DdaDesktopCapture::new() {
                Ok(cap) => Some(cap),
                Err(e) => {
                    eprintln!("[FrameSender] DDA init failed: {} — using TestPattern", e);
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
        let pattern_capture =
            prism_server::TestPatternCapture::with_resolution(width, height);

        // Create H.264 encoder — probes for hardware acceleration automatically.
        let mut encoder = match HwEncoder::new(width, height, 5_000_000) {
            Ok(e) => {
                println!("[FrameSender] Encoder: {} ({}x{})", e.backend().name(), width, height);
                e
            }
            Err(e) => {
                eprintln!("[FrameSender] Failed to create H.264 encoder: {}", e);
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
                        eprintln!("[FrameSender] DDA capture error: {}", e);
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
            let h264_data = match encoder.encode_bgra(&pixels) {
                Ok(data) => data,
                Err(e) => {
                    eprintln!("[FrameSender] encode error: {}", e);
                    seq = seq.wrapping_add(1);
                    continue;
                }
            };

            if h264_data.is_empty() {
                // Encoder buffering — try again next tick.
                seq = seq.wrapping_add(1);
                continue;
            }

            // Wire format:
            //   [4 bytes: width  u32 LE]
            //   [4 bytes: height u32 LE]
            //   [4 bytes: seq    u32 LE]
            //   [4 bytes: h264_len u32 LE]
            //   [h264_len bytes: H.264 NAL bitstream]
            let h264_len = h264_data.len() as u32;
            let mut frame_data = Vec::with_capacity(16 + h264_data.len());
            frame_data.extend_from_slice(&width.to_le_bytes());
            frame_data.extend_from_slice(&height.to_le_bytes());
            frame_data.extend_from_slice(&seq.to_le_bytes());
            frame_data.extend_from_slice(&h264_len.to_le_bytes());
            frame_data.extend_from_slice(&h264_data);

            bytes_sent_total += frame_data.len() as u64;

            // Snapshot connections so we don't hold the mutex across await.
            let conns = conn_store_send.snapshot();
            let mut sent = 0u32;
            for conn in &conns {
                match conn.open_uni().await {
                    Ok(mut send_stream) => {
                        if send_stream.write_all(&frame_data).await.is_ok() {
                            let _ = send_stream.finish();
                            sent += 1;
                        }
                    }
                    Err(e) => {
                        eprintln!("[FrameSender] open_uni error: {}", e);
                    }
                }
            }

            if sent > 0 {
                frames_sent += 1;
            }
            seq = seq.wrapping_add(1);

            // Log every second.
            if last_log.elapsed() >= std::time::Duration::from_secs(1) {
                if frames_sent > 0 {
                    println!(
                        "[FrameSender] {} frames/s to {} client(s) — {}x{} H.264 @ ~{}KB/frame",
                        frames_sent,
                        conn_store_send.client_count(),
                        width,
                        height,
                        bytes_sent_total / (frames_sent as u64) / 1024,
                    );
                }
                frames_sent = 0;
                bytes_sent_total = 0;
                last_log = std::time::Instant::now();
            }
        }
    });

    // Accept loop
    loop {
        let incoming = match acceptor.accept().await {
            Some(i) => i,
            None => {
                println!("Endpoint closed");
                break;
            }
        };

        let sm = session_manager.clone();
        let disp = dispatcher.clone();
        let track = tracker.clone();
        let act_tx = activity_tx.clone();
        let conn_store_clone = conn_store.clone();
        let server_identity_task = server_identity.clone();

        tokio::spawn(async move {
            match incoming.await {
                Ok(quinn_conn) => {
                    let remote = quinn_conn.remote_address();
                    println!("[{}] Connected", remote);

                    // ── Noise IK handshake (when --noise is set) ──────────────
                    if noise_mode {
                        // Accept the first bidirectional stream opened by the client
                        // for the Noise IK handshake exchange.
                        let (mut send, mut recv) = match quinn_conn.accept_bi().await {
                            Ok(streams) => streams,
                            Err(e) => {
                                println!("[{}] Noise: failed to accept bi stream: {}", remote, e);
                                return;
                            }
                        };

                        // Read the client's Noise initiator message.
                        let client_msg = match recv.read_to_end(4096).await {
                            Ok(data) => data,
                            Err(e) => {
                                println!("[{}] Noise: failed to read initiator: {}", remote, e);
                                return;
                            }
                        };

                        // Build and run the server-side Noise IK handshake.
                        let mut hs = match prism_security::handshake::ServerHandshake::new(&*server_identity_task) {
                            Ok(hs) => hs,
                            Err(e) => {
                                println!("[{}] Noise: handshake init failed: {}", remote, e);
                                return;
                            }
                        };

                        let response = match hs.respond(&client_msg) {
                            Ok(r) => r,
                            Err(e) => {
                                println!("[{}] Noise: respond failed (bad client key?): {}", remote, e);
                                return;
                            }
                        };

                        // Send the server's Noise response.
                        if let Err(e) = send.write_all(&response).await {
                            println!("[{}] Noise: failed to write response: {}", remote, e);
                            return;
                        }
                        let _ = send.finish();

                        // Finalise the handshake and extract the client's static key.
                        let result = match hs.finalize() {
                            Ok(r) => r,
                            Err(e) => {
                                println!("[{}] Noise: finalize failed: {}", remote, e);
                                return;
                            }
                        };

                        let client_key = result.remote_static.unwrap();
                        println!(
                            "[{}] Noise IK handshake OK. Client key: {}…",
                            remote,
                            hex::encode(&client_key[..8]),
                        );
                    }

                    // Clone quinn_conn before it is consumed by QuicConnection::new.
                    // We need one handle for the recv loop and one to store for sending.
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
                            println!("[{}] Session: {} channels granted", remote, granted.len());

                            // Clone before consuming: heartbeat sender needs its own handle.
                            let hb_conn = quinn_conn_for_store.clone();

                            // Store connection for frame sending
                            conn_store_clone.add(client_id, quinn_conn_for_store);
                            println!(
                                "[{}] Registered for frame broadcast ({})",
                                remote,
                                &client_id.to_string()[..8]
                            );

                            let _handle = prism_server::spawn_recv_loop(
                                client_id,
                                qc_recv as Arc<dyn prism_transport::PrismConnection>,
                                disp,
                                track,
                                act_tx,
                            );
                            println!(
                                "[{}] Recv loop started for {}",
                                remote,
                                &client_id.to_string()[..8]
                            );

                            // Spawn heartbeat sender: 16-byte HEARTBEAT datagram every 5 seconds.
                            let hb_gen = prism_server::HeartbeatGenerator::new();
                            tokio::spawn(async move {
                                let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));
                                loop {
                                    interval.tick().await;
                                    if hb_conn.send_datagram(hb_gen.packet()).is_err() { break; }
                                }
                            });
                        }
                        Err(e) => {
                            println!("[{}] Session failed: {}", remote, e);
                        }
                    }
                }
                Err(e) => println!("Connection error: {}", e),
            }
        });
    }

    Ok(())
}

