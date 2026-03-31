use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use prism_display::capture::PlatformCapture;
use openh264::encoder::{Encoder, EncoderConfig};
use openh264::formats::YUVBuffer;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== PRISM Server v0.1.0 ===");

    let use_dda = std::env::args().any(|a| a == "--dda");

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

        // Create H.264 encoder.
        let mut encoder = match Encoder::with_api_config(
            openh264::OpenH264API::from_source(),
            EncoderConfig::new()
                .max_frame_rate(openh264::encoder::FrameRate::from_hz(15.0))
                .bitrate(openh264::encoder::BitRate::from_bps(5_000_000)),
        ) {
            Ok(e) => e,
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

            // Convert BGRA → YUV I420.
            let yuv = bgra_to_yuv420(&pixels, width as usize, height as usize);

            // Encode YUV → H.264 bitstream.
            let h264_data = match encoder.encode(&yuv) {
                Ok(bitstream) => bitstream.to_vec(),
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

// ── BGRA → YUV I420 conversion ────────────────────────────────────────────────

/// Convert a BGRA8 frame to a packed YUV I420 [`YUVBuffer`].
///
/// BGRA memory layout: [B, G, R, A] per pixel, row-major.
/// YUV I420: full-resolution Y plane, half-resolution U and V planes.
fn bgra_to_yuv420(bgra: &[u8], width: usize, height: usize) -> YUVBuffer {
    let y_size = width * height;
    let uv_w = (width + 1) / 2;
    let uv_h = (height + 1) / 2;
    let uv_size = uv_w * uv_h;
    let mut yuv_data = vec![0u8; y_size + 2 * uv_size];

    let y_plane = &mut yuv_data[..y_size];
    // Fill Y plane (full resolution).
    for row in 0..height {
        for col in 0..width {
            let src = (row * width + col) * 4;
            let b = bgra[src] as f32;
            let g = bgra[src + 1] as f32;
            let r = bgra[src + 2] as f32;
            let y = (0.299 * r + 0.587 * g + 0.114 * b).round() as u8;
            y_plane[row * width + col] = y;
        }
    }

    // Fill U and V planes (half resolution — average 2×2 blocks).
    for uv_row in 0..uv_h {
        for uv_col in 0..uv_w {
            // Sample the top-left pixel of each 2×2 block.
            let src_row = uv_row * 2;
            let src_col = uv_col * 2;
            let src = (src_row * width + src_col) * 4;
            let b = bgra[src] as f32;
            let g = bgra[src + 1] as f32;
            let r = bgra[src + 2] as f32;
            let u = (-0.169 * r - 0.331 * g + 0.500 * b + 128.0).round().clamp(0.0, 255.0) as u8;
            let v = (0.500 * r - 0.419 * g - 0.081 * b + 128.0).round().clamp(0.0, 255.0) as u8;
            yuv_data[y_size + uv_row * uv_w + uv_col] = u;
            yuv_data[y_size + uv_size + uv_row * uv_w + uv_col] = v;
        }
    }

    YUVBuffer::from_vec(yuv_data, width, height)
}
