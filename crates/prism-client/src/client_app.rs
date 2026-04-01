// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2025-2026 Ehsan Khalid. All rights reserved.
// Licensed under the GNU Affero General Public License v3.0.
// Commercial licensing available — see LICENSE-COMMERCIAL.md.

//! ClientApp: structured entry-point for the PRISM client.
//!
//! Encapsulates configuration, identity persistence, connection setup,
//! async task spawning, and the main-thread render loop.

use std::sync::mpsc as std_mpsc;
use std::time::{Duration, Instant};

use bytes::{Bytes, BytesMut};
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

    /// Run the client with automatic reconnection on failure.
    ///
    /// Calls [`connect_and_stream`] in a loop, backing off 3 seconds between
    /// attempts. Returns `Ok(())` on clean exit (window closed / Escape pressed)
    /// or propagates the last error after 100 failed attempts.
    pub async fn run(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let mut attempt = 0u32;
        loop {
            match self.connect_and_stream().await {
                Ok(()) => break Ok(()),
                Err(e) => {
                    attempt += 1;
                    if attempt > 100 {
                        tracing::error!("max reconnect attempts exceeded");
                        break Err(e);
                    }
                    tracing::warn!(attempt, error = %e, "connection lost, reconnecting in 3s...");
                    tokio::time::sleep(Duration::from_secs(3)).await;
                }
            }
        }
    }

    /// Connect to the server, perform optional Noise IK handshake, spawn all
    /// async tasks, and run the minifb render loop until the window is closed.
    ///
    /// The minifb render loop runs on the calling (main) thread as required by
    /// minifb's platform constraints.
    async fn connect_and_stream(&mut self) -> Result<(), Box<dyn std::error::Error>> {
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

        // ── Capability negotiation ────────────────────────────────────────────
        // Open a bi stream, send ClientCapabilities (prism_session format),
        // read the NegotiationResult back. Must happen before frame tasks start.
        {
            use prism_session::{ClientCapabilities, ClientChannelCap, ClientPerformance};
            use prism_protocol::channel::{CHANNEL_DISPLAY, CHANNEL_INPUT, CHANNEL_CONTROL};

            let (mut cap_send, mut cap_recv) = connection
                .open_bi()
                .await
                .map_err(|e| format!("cap negotiation: open_bi failed: {}", e))?;

            let client_caps = ClientCapabilities {
                channels: vec![
                    ClientChannelCap { channel_id: CHANNEL_DISPLAY, max_version: 1 },
                    ClientChannelCap { channel_id: CHANNEL_INPUT,   max_version: 1 },
                    ClientChannelCap { channel_id: CHANNEL_CONTROL, max_version: 1 },
                ],
                performance: ClientPerformance {
                    supported_codecs: vec!["h264".into(), "h265".into()],
                },
            };

            let json = serde_json::to_vec(&client_caps)
                .map_err(|e| format!("cap negotiation: serialize failed: {}", e))?;
            cap_send.write_all(&(json.len() as u32).to_le_bytes()).await
                .map_err(|e| format!("cap negotiation: write len failed: {}", e))?;
            cap_send.write_all(&json).await
                .map_err(|e| format!("cap negotiation: write body failed: {}", e))?;
            let _ = cap_send.finish();

            // Read server response.
            let mut len_buf = [0u8; 4];
            cap_recv.read_exact(&mut len_buf).await
                .map_err(|e| format!("cap negotiation: read len failed: {}", e))?;
            let resp_len = u32::from_le_bytes(len_buf) as usize;
            let mut resp_data = vec![0u8; resp_len];
            cap_recv.read_exact(&mut resp_data).await
                .map_err(|e| format!("cap negotiation: read body failed: {}", e))?;
            tracing::info!(bytes = resp_len, "capability negotiation complete");
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
            let ver_chan: u16 = 0x006u16;
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
        // The server opens ONE persistent uni stream per client and writes
        // length-prefixed frames on it.  We accept that single stream here and
        // read frames in a loop, avoiding per-frame stream-accept overhead.
        //
        // Wire format per frame:
        //   [4 bytes: inner_len u32 LE]   — byte count of the fields below
        //   [4 bytes: width     u32 LE]
        //   [4 bytes: height    u32 LE]
        //   [4 bytes: seq       u32 LE]
        //   [4 bytes: h264_len  u32 LE]
        //   [h264_len bytes: H.264 NAL bitstream]
        let conn_recv = connection.clone();
        let conn_idr = connection.clone();
        tokio::spawn(async move {
            let mut decoder = match Decoder::new() {
                Ok(d) => d,
                Err(e) => {
                    tracing::error!(error = %e, "failed to create H.264 decoder");
                    return;
                }
            };

            // IDR request support: detect frame gaps and ask server for a keyframe.
            let mut gap_detector = prism_display::protocol::FrameGapDetector::with_cooldown(
                std::time::Duration::from_secs(2),
            );

            let mut frames_received: u64 = 0;
            let mut last_log = Instant::now();

            tracing::info!("waiting for persistent frame stream from server");

            // Accept the one persistent uni stream the server opens for us.
            let mut recv = match conn_recv.accept_uni().await {
                Ok(r) => r,
                Err(e) => {
                    tracing::info!(error = %e, "failed to accept frame stream — connection closed?");
                    return;
                }
            };

            tracing::info!("frame stream accepted — receiving frames (close window or Ctrl+C to stop)");

            loop {
                // Read 4-byte length prefix.
                let mut len_buf = [0u8; 4];
                if let Err(e) = recv.read_exact(&mut len_buf).await {
                    tracing::info!(error = %e, "frame stream closed");
                    break;
                }
                let inner_len = u32::from_le_bytes(len_buf) as usize;

                // Sanity check: max ~4 MiB header + payload.
                if !(16..=4 * 1024 * 1024 + 16).contains(&inner_len) {
                    tracing::error!(inner_len, "invalid frame length prefix — stream corrupt");
                    break;
                }

                // Read the fixed 16-byte header: width, height, seq, h264_len.
                let mut header = [0u8; 16];
                if let Err(e) = recv.read_exact(&mut header).await {
                    tracing::error!(error = %e, "header read error");
                    break;
                }

                let width = u32::from_le_bytes([header[0], header[1], header[2], header[3]]) as usize;
                let height = u32::from_le_bytes([header[4], header[5], header[6], header[7]]) as usize;
                let seq = u32::from_le_bytes([header[8], header[9], header[10], header[11]]);
                let h264_len = u32::from_le_bytes([header[12], header[13], header[14], header[15]]) as usize;

                // Validate dimensions and payload size against the length prefix.
                if width == 0 || height == 0 || width > 4096 || height > 4096 {
                    tracing::error!(width, height, "invalid frame dimensions");
                    break;
                }
                if h264_len == 0 || h264_len > 4 * 1024 * 1024 {
                    tracing::error!(h264_len, "invalid h264_len");
                    break;
                }
                if inner_len != 16 + h264_len {
                    tracing::error!(inner_len, h264_len, "length prefix mismatch");
                    break;
                }

                // Read exactly h264_len bytes of H.264 NAL data.
                let mut h264_data = vec![0u8; h264_len];
                if let Err(e) = recv.read_exact(&mut h264_data).await {
                    tracing::error!(error = %e, "h264 read error");
                    break;
                }

                // Frame gap detection: if a gap is detected, send IDR_REQUEST.
                gap_detector.receive_seq(seq);
                if gap_detector.should_request_idr() {
                    tracing::warn!(seq, "frame gap detected — sending IDR_REQUEST");
                    // Build a 16-byte IDR_REQUEST datagram using the PRISM wire format.
                    // ver_chan packs version (0) into top 4 bits and CHANNEL_DISPLAY into low 12.
                    use prism_display::protocol::MSG_IDR_REQUEST;
                    use prism_protocol::channel::CHANNEL_DISPLAY;
                    let ver_chan: u16 = CHANNEL_DISPLAY;
                    let mut idr_bytes = [0u8; 16];
                    idr_bytes[0..2].copy_from_slice(&ver_chan.to_le_bytes());
                    idr_bytes[2] = MSG_IDR_REQUEST;
                    // flags, sequence, timestamp_us, payload_length all zero
                    let idr_dgram = bytes::Bytes::copy_from_slice(&idr_bytes);
                    if conn_idr.send_datagram(idr_dgram).is_err() {
                        tracing::debug!("IDR_REQUEST send failed (connection closing?)");
                    }
                }

                let yuv_frame = match decoder.decode(&h264_data) {
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

        // ── Spawn datagram reader (probe echo + overlay) ──────────────────────
        // The frame receive task above only reads from the uni stream. This task
        // reads incoming datagrams (heartbeats, probes, overlay packets) and
        // responds to PROBE_REQUEST with PROBE_RESPONSE so the server can
        // measure round-trip time.
        let dgram_conn = connection.clone();
        tokio::spawn(async move {
            use prism_protocol::header::{PrismHeader, HEADER_SIZE};
            use prism_protocol::channel::CHANNEL_CONTROL;
            use prism_session::control_msg::{PROBE_REQUEST, PROBE_RESPONSE};

            while let Ok(data) = dgram_conn.read_datagram().await {
                if data.len() >= HEADER_SIZE
                    && let Ok(header) = PrismHeader::decode_from_slice(&data)
                    && header.channel_id == CHANNEL_CONTROL
                    && header.msg_type == PROBE_REQUEST
                {
                    // Echo back with msg_type changed to PROBE_RESPONSE.
                    // msg_type is at byte offset 2 in the wire format.
                    let mut response = BytesMut::from(&data[..]);
                    response[2] = PROBE_RESPONSE;
                    let _ = dgram_conn.send_datagram(response.freeze());
                    tracing::trace!("probe echo sent");
                }
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

        // Fix 7: CursorPredictor — feeds local mouse positions for zero-latency
        // cursor feel; server corrections applied when positions diverge > 5 px.
        let mut cursor_predictor = crate::CursorPredictor::new(5.0);

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

            // Draw a simple 8x8 white cursor crosshair at predicted position.
            let (cx, cy) = cursor_predictor.display_position();
            let cx = cx as usize;
            let cy = cy as usize;
            let width = current_w;
            let height = current_h;
            if width > 0 && height > 0 {
                for dy in 0..8usize {
                    for dx in 0..8usize {
                        let px = cx + dx;
                        let py = cy + dy;
                        if px < width && py < height
                            && (dx == 3 || dx == 4 || dy == 3 || dy == 4)
                        {
                            current_buffer[py * width + px] = 0x00FFFFFF; // white
                        }
                    }
                }
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
                // Update predictor with raw pixel position for zero-latency feel.
                cursor_predictor.update_local(mx, my);
                tracing::trace!(
                    x = cursor_predictor.display_position().0,
                    y = cursor_predictor.display_position().1,
                    "cursor prediction"
                );

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

                if let Some(ref mut cb) = clipboard
                    && let Ok(text) = cb.get_text()
                {
                    let msg =
                        prism_protocol::clipboard::ClipboardMessage::text(&text);
                    if msg.content_hash != last_clipboard_hash
                        && clipboard_echo_guard.should_send(msg.content_hash)
                    {
                        last_clipboard_hash = msg.content_hash;
                        clipboard_echo_guard.remember(msg.content_hash);
                        tracing::debug!(
                            bytes = msg.data.len(),
                            hash = format_args!("{:#x}", msg.content_hash),
                            "clipboard: sending to server"
                        );
                        // Build a PRISM clipboard datagram and enqueue it on
                        // the input channel (reuses the existing sender task).
                        let json = msg.to_json();
                        let header = prism_protocol::header::PrismHeader {
                            version: prism_protocol::header::PROTOCOL_VERSION,
                            channel_id: prism_protocol::channel::CHANNEL_CLIPBOARD,
                            msg_type: 0x01,
                            flags: 0,
                            sequence: 0,
                            timestamp_us: 0,
                            payload_length: json.len() as u32,
                        };
                        let mut buf = BytesMut::with_capacity(
                            prism_protocol::header::HEADER_SIZE + json.len(),
                        );
                        header.encode(&mut buf);
                        buf.extend_from_slice(&json);
                        input_tx.send(buf.freeze()).ok();
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
