use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use prism_display::capture::PlatformCapture;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== PRISM Server v0.1.0 ===");

    let config = prism_server::ServerConfig::default();
    println!("Listening on {}", config.listen_addr);

    // TLS
    let cert = prism_server::SelfSignedCert::generate()?;
    println!("Generated self-signed TLS certificate");

    // Security (dev mode)
    let _gate = Arc::new(prism_server::AllowAllGate::new());
    println!("Security: AllowAllGate (dev mode)");

    // Test pattern capture
    let capture = prism_server::TestPatternCapture::new();
    let monitors = capture.enumerate_monitors()?;
    println!(
        "Capture: TestPattern {}x{} @ {}fps",
        monitors[0].resolution.0, monitors[0].resolution.1, monitors[0].refresh_rate
    );

    // Session manager
    let session_manager = Arc::new(Mutex::new(prism_server::SessionManager::new(config.clone())));

    // Channel dispatcher + bandwidth tracker
    let dispatcher = Arc::new(prism_session::ChannelDispatcher::new());
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

    // Spawn frame sender task (~10fps) — sends BGRA frames over QUIC uni streams.
    let conn_store_send = conn_store.clone();
    tokio::spawn(async move {
        // 320×240 test pattern capture.
        let pattern_capture = prism_server::TestPatternCapture::with_resolution(320, 240);
        const WIDTH: u32 = 320;
        const HEIGHT: u32 = 240;

        let mut seq: u32 = 0;
        let mut interval = tokio::time::interval(std::time::Duration::from_millis(100)); // ~10fps
        let mut last_log = std::time::Instant::now();
        let mut frames_sent = 0u32;

        loop {
            interval.tick().await;

            if conn_store_send.client_count() == 0 {
                continue; // no clients, skip
            }

            // Generate BGRA pixel data for this frame.
            let pixels = pattern_capture.generate_pattern(seq);

            // Build wire frame: [width u32 LE][height u32 LE][seq u32 LE][BGRA pixels]
            let mut frame_data = Vec::with_capacity(12 + pixels.len());
            frame_data.extend_from_slice(&WIDTH.to_le_bytes());
            frame_data.extend_from_slice(&HEIGHT.to_le_bytes());
            frame_data.extend_from_slice(&seq.to_le_bytes());
            frame_data.extend_from_slice(&pixels);

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
                        "[FrameSender] {} frames sent to {} client(s) (~10fps, {}x{} BGRA)",
                        frames_sent,
                        conn_store_send.client_count(),
                        WIDTH,
                        HEIGHT
                    );
                }
                frames_sent = 0;
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

        tokio::spawn(async move {
            match incoming.await {
                Ok(quinn_conn) => {
                    let remote = quinn_conn.remote_address();
                    println!("[{}] Connected", remote);

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
