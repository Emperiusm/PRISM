//! ClientApp: structured entry-point for the PRISM client.
//!
//! Encapsulates configuration, identity persistence, connection setup,
//! async task spawning, and the main-thread render loop.

use std::sync::mpsc as std_mpsc;
use std::time::{Duration, Instant};

use bytes::Bytes;
use minifb::{Window, WindowOptions};
use openh264::decoder::Decoder;
use openh264::formats::YUVSource;

/// Configuration for the PRISM client.
pub struct ClientConfig {
    pub server_addr: std::net::SocketAddr,
    /// Optional Noise IK server public key (32 bytes). When set, the client
    /// performs a Noise IK handshake immediately after the QUIC connection.
    pub noise_key: Option<[u8; 32]>,
    /// Path to the persisted client identity file.
    pub identity_path: std::path::PathBuf,
}

/// A received frame ready for display.
struct Frame {
    width: usize,
    height: usize,
    /// minifb pixel buffer: 0RGB packed as u32.
    buffer: Vec<u32>,
    /// Server-assigned sequence number, shown in the window title.
    seq: u32,
}

/// Top-level PRISM client application.
pub struct ClientApp {
    config: ClientConfig,
}

impl ClientApp {
    pub fn new(config: ClientConfig) -> Self {
        Self { config }
    }

    /// Run the client: connect, optionally handshake, spawn async tasks, render.
    ///
    /// The minifb render loop runs on the calling (main) thread as required by
    /// minifb's platform constraints.
    pub async fn run(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let server_addr = self.config.server_addr;

        tracing::info!("PRISM Client v0.1.0");
        tracing::info!(addr = %server_addr, "connecting to server");

        let connector =
            crate::ClientConnector::new(crate::TlsMode::InsecureTrustAll)?;
        let connection = connector.connect(server_addr, "localhost").await?;
        tracing::info!(addr = %connection.remote_address(), "connected to server");

        // ── Load or generate client identity ─────────────────────────────────
        // Ensure the parent directory exists so load_or_generate can write the file.
        if let Some(parent) = self.config.identity_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let identity = prism_security::identity::LocalIdentity::load_or_generate(
            &self.config.identity_path,
            "PRISM Client",
        )?;
        tracing::info!(
            key = %hex::encode(&identity.x25519_public_bytes()[..8]),
            path = %self.config.identity_path.display(),
            "client identity loaded"
        );

        // ── Noise IK handshake (optional) ─────────────────────────────────────
        if let Some(ref server_pubkey) = self.config.noise_key {
            tracing::info!(
                key = %hex::encode(&identity.x25519_public_bytes()[..8]),
                "Noise IK: initiating handshake"
            );

            let (mut send, mut recv) = connection
                .open_bi()
                .await
                .map_err(|e| format!("Noise: open_bi failed: {}", e))?;

            let mut hs =
                prism_security::handshake::ClientHandshake::new(&identity, server_pubkey)
                    .map_err(|e| format!("Noise: handshake init failed: {}", e))?;
            let init_msg = hs
                .initiate()
                .map_err(|e| format!("Noise: initiate failed: {}", e))?;

            send.write_all(&init_msg)
                .await
                .map_err(|e| format!("Noise: write init failed: {}", e))?;
            let _ = send.finish();

            let server_response = recv
                .read_to_end(4096)
                .await
                .map_err(|e| format!("Noise: read response failed: {}", e))?;

            hs.process_response(&server_response)
                .map_err(|e| format!("Noise: process_response failed: {}", e))?;
            let _result = hs
                .finalize()
                .map_err(|e| format!("Noise: finalize failed: {}", e))?;

            tracing::info!("Noise IK handshake complete");
        }

        // ── Channel: async receiver → main-thread renderer ────────────────────
        // std::sync::mpsc allows the tokio task to send without async and the
        // main thread to poll without blocking the event loop.
        let (tx, rx) = std_mpsc::sync_channel::<Frame>(4);

        // ── Channel: main-thread input capture → async sender ─────────────────
        let (input_tx, input_rx) = std_mpsc::sync_channel::<Bytes>(64);

        // ── Spawn heartbeat sender ────────────────────────────────────────────
        let hb_conn = connection.clone();
        tokio::spawn(async move {
            // CHANNEL_CONTROL = 0x006, HEARTBEAT msg_type = 0x01
            // PrismHeader: ver_chan LE u16, msg_type u8, flags u8,
            //              sequence LE u32, timestamp_us LE u32, payload_length LE u32
            let ver_chan: u16 = (0u16 << 12) | 0x006u16;
            let mut header_bytes = [0u8; 16];
            header_bytes[0..2].copy_from_slice(&ver_chan.to_le_bytes());
            header_bytes[2] = 0x01; // HEARTBEAT
            let packet = Bytes::copy_from_slice(&header_bytes);

            let mut interval =
                tokio::time::interval(std::time::Duration::from_secs(5));
            loop {
                interval.tick().await;
                if hb_conn.send_datagram(packet.clone()).is_err() {
                    break;
                }
            }
        });

        // ── Spawn async input-forward task ────────────────────────────────────
        let conn_input = connection.clone();
        tokio::spawn(async move {
            loop {
                let mut sent_any = false;
                while let Ok(dgram) = input_rx.try_recv() {
                    if conn_input.send_datagram(dgram).is_err() {
                        return;
                    }
                    sent_any = true;
                }
                if !sent_any {
                    tokio::time::sleep(std::time::Duration::from_millis(4)).await;
                }
            }
        });

        // ── Spawn async frame-receive task ────────────────────────────────────
        let conn_recv = connection.clone();
        tokio::spawn(async move {
            let mut decoder = match Decoder::new() {
                Ok(d) => d,
                Err(e) => {
                    tracing::error!(error = %e, "failed to create H.264 decoder");
                    return;
                }
            };

            let mut frames_received: u64 = 0;
            let mut last_log = Instant::now();

            tracing::info!("receiving frames (close window or Ctrl+C to stop)");

            loop {
                let mut recv = match conn_recv.accept_uni().await {
                    Ok(r) => r,
                    Err(e) => {
                        tracing::info!(error = %e, "connection closed");
                        break;
                    }
                };

                // Read 16-byte header: width, height, seq, h264_len.
                let mut header = [0u8; 16];
                if let Err(e) = recv.read_exact(&mut header).await {
                    tracing::error!(error = %e, "header read error");
                    continue;
                }

                let width = u32::from_le_bytes([
                    header[0], header[1], header[2], header[3],
                ]) as usize;
                let height = u32::from_le_bytes([
                    header[4], header[5], header[6], header[7],
                ]) as usize;
                let seq = u32::from_le_bytes([
                    header[8], header[9], header[10], header[11],
                ]);
                let h264_len = u32::from_le_bytes([
                    header[12], header[13], header[14], header[15],
                ]) as usize;

                if width == 0 || height == 0 || width > 4096 || height > 4096 {
                    tracing::error!(width, height, "invalid frame dimensions");
                    continue;
                }
                if h264_len == 0 || h264_len > 4 * 1024 * 1024 {
                    tracing::error!(h264_len, "invalid h264_len");
                    continue;
                }

                let h264_data = match recv.read_to_end(h264_len + 16).await {
                    Ok(b) => b,
                    Err(e) => {
                        tracing::error!(error = %e, "h264 read error");
                        continue;
                    }
                };

                if h264_data.len() < h264_len {
                    tracing::error!(
                        got = h264_data.len(),
                        expected = h264_len,
                        "short h264 data"
                    );
                    continue;
                }

                let yuv_frame = match decoder.decode(&h264_data[..h264_len]) {
                    Ok(Some(yuv)) => yuv,
                    Ok(None) => continue, // decoder buffering
                    Err(e) => {
                        tracing::error!(error = %e, "decode error");
                        continue;
                    }
                };

                let (dec_w, dec_h) = yuv_frame.dimensions();
                let buffer = yuv420_to_rgb(&yuv_frame, dec_w, dec_h);

                frames_received += 1;

                if last_log.elapsed().as_secs_f64() >= 1.0 {
                    let elapsed = last_log.elapsed().as_secs_f64();
                    tracing::info!(
                        seq,
                        fps = format_args!("{:.1}", frames_received as f64 / elapsed),
                        width = dec_w,
                        height = dec_h,
                        "receiver stats"
                    );
                    frames_received = 0;
                    last_log = Instant::now();
                }

                // Non-blocking send; drop on backpressure.
                let _ = tx.try_send(Frame { width, height, buffer, seq });
            }
        });

        // ── Clipboard setup ───────────────────────────────────────────────────
        let mut clipboard: Option<arboard::Clipboard> =
            arboard::Clipboard::new().ok();
        let clipboard_echo_guard =
            prism_protocol::clipboard::ClipboardEchoGuard::new();
        let mut last_clipboard_check = Instant::now();
        let mut last_clipboard_hash: u64 = 0;

        // ── minifb render loop (main thread) ──────────────────────────────────
        let initial_w = 1920usize;
        let initial_h = 1080usize;

        let mut window = Window::new(
            "PRISM Client",
            initial_w,
            initial_h,
            WindowOptions {
                resize: true,
                ..WindowOptions::default()
            },
        )?;

        window.set_target_fps(60);

        let mut current_buffer: Vec<u32> = vec![0u32; initial_w * initial_h];
        let mut current_w = initial_w;
        let mut current_h = initial_h;

        let mut input_sender = crate::InputSender::new();
        let mut last_mx: u16 = 0;
        let mut last_my: u16 = 0;
        let mut last_left = false;
        let mut last_right = false;
        let mut last_middle = false;

        while window.is_open() && !window.is_key_down(minifb::Key::Escape) {
            // Drain all pending frames; display the latest.
            let mut latest: Option<Frame> = None;
            loop {
                match rx.try_recv() {
                    Ok(frame) => {
                        latest = Some(frame);
                    }
                    Err(std_mpsc::TryRecvError::Empty) => break,
                    Err(std_mpsc::TryRecvError::Disconnected) => {
                        tracing::info!("receiver channel disconnected — exiting");
                        connector.close();
                        return Ok(());
                    }
                }
            }

            if let Some(frame) = latest {
                current_w = frame.width;
                current_h = frame.height;
                window.set_title(&format!("PRISM Client — frame #{}", frame.seq));
                current_buffer = frame.buffer;
            }

            window
                .update_with_buffer(&current_buffer, current_w, current_h)
                .unwrap_or_else(|e| tracing::error!(error = %e, "renderer update error"));

            // ── Input capture ────────────────────────────────────────────────

            for key in window.get_keys_pressed(minifb::KeyRepeat::Yes) {
                let vk = key as u16;
                let event =
                    prism_protocol::input::InputEvent::KeyDown { scancode: vk, vk };
                let dgram = input_sender.build_datagram(event);
                input_tx.send(dgram).ok();
            }

            if let Some((mx, my)) =
                window.get_mouse_pos(minifb::MouseMode::Clamp)
            {
                let (nx, ny) = crate::normalize_mouse(
                    mx,
                    my,
                    current_w as u32,
                    current_h as u32,
                );
                if nx != last_mx || ny != last_my {
                    let event =
                        prism_protocol::input::InputEvent::MouseMove { x: nx, y: ny };
                    let dgram = input_sender.build_datagram(event);
                    input_tx.send(dgram).ok();
                    last_mx = nx;
                    last_my = ny;
                }
            }

            let left_down = window.get_mouse_down(minifb::MouseButton::Left);
            let right_down = window.get_mouse_down(minifb::MouseButton::Right);
            let middle_down = window.get_mouse_down(minifb::MouseButton::Middle);

            if left_down && !last_left {
                let event = prism_protocol::input::InputEvent::MouseDown {
                    button: prism_protocol::input::MouseButton::Left,
                };
                input_tx.send(input_sender.build_datagram(event)).ok();
            } else if !left_down && last_left {
                let event = prism_protocol::input::InputEvent::MouseUp {
                    button: prism_protocol::input::MouseButton::Left,
                };
                input_tx.send(input_sender.build_datagram(event)).ok();
            }

            if right_down && !last_right {
                let event = prism_protocol::input::InputEvent::MouseDown {
                    button: prism_protocol::input::MouseButton::Right,
                };
                input_tx.send(input_sender.build_datagram(event)).ok();
            } else if !right_down && last_right {
                let event = prism_protocol::input::InputEvent::MouseUp {
                    button: prism_protocol::input::MouseButton::Right,
                };
                input_tx.send(input_sender.build_datagram(event)).ok();
            }

            if middle_down && !last_middle {
                let event = prism_protocol::input::InputEvent::MouseDown {
                    button: prism_protocol::input::MouseButton::Middle,
                };
                input_tx.send(input_sender.build_datagram(event)).ok();
            } else if !middle_down && last_middle {
                let event = prism_protocol::input::InputEvent::MouseUp {
                    button: prism_protocol::input::MouseButton::Middle,
                };
                input_tx.send(input_sender.build_datagram(event)).ok();
            }

            last_left = left_down;
            last_right = right_down;
            last_middle = middle_down;

            // ── Clipboard poll (every 500 ms) ────────────────────────────────
            if last_clipboard_check.elapsed() >= Duration::from_millis(500) {
                last_clipboard_check = Instant::now();

                if let Some(ref mut cb) = clipboard {
                    if let Ok(text) = cb.get_text() {
                        let msg =
                            prism_protocol::clipboard::ClipboardMessage::text(&text);
                        if msg.content_hash != last_clipboard_hash
                            && clipboard_echo_guard.should_send(msg.content_hash)
                        {
                            last_clipboard_hash = msg.content_hash;
                            tracing::debug!(
                                bytes = msg.data.len(),
                                hash = format_args!("{:#x}", msg.content_hash),
                                "clipboard: new content detected"
                            );
                        }
                    }
                }
            }
        }

        connector.close();
        Ok(())
    }
}

// ── YUV I420 → RGB conversion ─────────────────────────────────────────────────

/// Convert a decoded YUV frame to a minifb `0RGB` pixel buffer.
fn yuv420_to_rgb<S: YUVSource>(yuv: &S, width: usize, height: usize) -> Vec<u32> {
    let (y_stride, u_stride, v_stride) = yuv.strides();
    let y_plane = yuv.y();
    let u_plane = yuv.u();
    let v_plane = yuv.v();

    let mut buffer = Vec::with_capacity(width * height);

    for row in 0..height {
        for col in 0..width {
            let y_val = y_plane[row * y_stride + col] as f32;
            let uv_row = row / 2;
            let uv_col = col / 2;
            let u_val = u_plane[uv_row * u_stride + uv_col] as f32;
            let v_val = v_plane[uv_row * v_stride + uv_col] as f32;

            let r = (y_val + 1.402 * (v_val - 128.0))
                .round()
                .clamp(0.0, 255.0) as u32;
            let g = (y_val - 0.344 * (u_val - 128.0) - 0.714 * (v_val - 128.0))
                .round()
                .clamp(0.0, 255.0) as u32;
            let b = (y_val + 1.772 * (u_val - 128.0))
                .round()
                .clamp(0.0, 255.0) as u32;

            buffer.push((r << 16) | (g << 8) | b);
        }
    }

    buffer
}
