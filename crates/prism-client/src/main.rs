//! PRISM Client binary.
//!
//! Connects to a PRISM server, receives H.264-encoded frames over QUIC uni streams,
//! decodes them with openh264, and renders them in a minifb window.
//!
//! Frame wire format (from server):
//!   [4 bytes: width     u32 LE]
//!   [4 bytes: height    u32 LE]
//!   [4 bytes: seq       u32 LE]
//!   [4 bytes: h264_len  u32 LE]
//!   [h264_len bytes: H.264 NAL bitstream]

use std::sync::mpsc as std_mpsc;
use std::time::Instant;

use bytes::Bytes;

use minifb::{Window, WindowOptions};
use openh264::decoder::Decoder;
use openh264::formats::YUVSource;

/// A received frame ready for display.
struct Frame {
    width: usize,
    height: usize,
    /// minifb pixel buffer: 0RGB packed as u32.
    buffer: Vec<u32>,
    /// Server-assigned sequence number, shown in the window title.
    seq: u32,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();

    let server_addr = args.get(1)
        .cloned()
        .unwrap_or_else(|| "127.0.0.1:9876".to_string());
    let server_addr: std::net::SocketAddr = server_addr.parse()?;

    // Parse --noise <server_pubkey_hex> flag.
    let noise_server_key: Option<[u8; 32]> = args.iter()
        .position(|a| a == "--noise")
        .and_then(|i| args.get(i + 1))
        .map(|hex_key| {
            let bytes = hex::decode(hex_key).expect("--noise: invalid hex key");
            let mut key = [0u8; 32];
            assert_eq!(bytes.len(), 32, "--noise: key must be 32 bytes (64 hex chars)");
            key.copy_from_slice(&bytes);
            key
        });

    println!("=== PRISM Client v0.1.0 ===");
    println!("Connecting to {}...", server_addr);

    let connector = prism_client::ClientConnector::new(prism_client::TlsMode::InsecureTrustAll)?;
    let connection = connector.connect(server_addr, "localhost").await?;
    println!("Connected to {}", connection.remote_address());

    // ── Noise IK handshake (when --noise <server_pubkey_hex> is provided) ────
    if let Some(ref server_pubkey) = noise_server_key {
        let client_identity = prism_security::identity::LocalIdentity::generate("PRISM Client");
        println!(
            "Noise IK: client key: {}…",
            hex::encode(&client_identity.x25519_public_bytes()[..8])
        );

        // Open a bidirectional stream for the handshake.
        let (mut send, mut recv) = connection.open_bi().await
            .map_err(|e| format!("Noise: open_bi failed: {}", e))?;

        // Build the client-side Noise IK handshake and send the initiator message.
        let mut hs = prism_security::handshake::ClientHandshake::new(&client_identity, server_pubkey)
            .map_err(|e| format!("Noise: handshake init failed: {}", e))?;
        let init_msg = hs.initiate()
            .map_err(|e| format!("Noise: initiate failed: {}", e))?;

        send.write_all(&init_msg).await
            .map_err(|e| format!("Noise: write init failed: {}", e))?;
        let _ = send.finish();

        // Read the server's response and complete the handshake.
        let server_response = recv.read_to_end(4096).await
            .map_err(|e| format!("Noise: read response failed: {}", e))?;

        hs.process_response(&server_response)
            .map_err(|e| format!("Noise: process_response failed: {}", e))?;

        let _result = hs.finalize()
            .map_err(|e| format!("Noise: finalize failed: {}", e))?;

        println!("Noise IK handshake complete! Proceeding with authenticated connection.");
    }

    // Channel: async receiver -> main-thread renderer.
    // Use std::sync::mpsc so the sender (tokio task) can send without async,
    // and the receiver (main thread) can poll without blocking the event loop.
    let (tx, rx) = std_mpsc::sync_channel::<Frame>(4);

    // Channel: main-thread input capture -> async sender.
    // Main thread sends input datagrams (built by InputSender); the async task
    // forwards them to the server via connection.send_datagram().
    let (input_tx, input_rx) = std_mpsc::sync_channel::<Bytes>(64);

    // Spawn the async frame-receive + input-forward task.
    let conn_recv = connection.clone();
    let conn_input = connection.clone();

    // Spawn heartbeat sender.
    let hb_conn = connection.clone();
    tokio::spawn(async move {
        // Build heartbeat packet inline using raw protocol constants.
        // CHANNEL_CONTROL = 0x006, HEARTBEAT msg_type = 0x01
        // PrismHeader wire: ver_chan LE u16, msg_type u8, flags u8,
        //                   sequence LE u32, timestamp_us LE u32, payload_length LE u32
        let ver_chan: u16 = (0u16 << 12) | 0x006u16; // version=0, channel=CONTROL
        let mut header_bytes = [0u8; 16];
        header_bytes[0..2].copy_from_slice(&ver_chan.to_le_bytes());
        header_bytes[2] = 0x01; // HEARTBEAT
        // remaining fields (flags, sequence, timestamp_us, payload_length) are all zero
        let packet = Bytes::copy_from_slice(&header_bytes);

        let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));
        loop {
            interval.tick().await;
            if hb_conn.send_datagram(packet.clone()).is_err() { break; }
        }
    });

    // Spawn async input-forward task: drains the std_mpsc channel and sends datagrams.
    tokio::spawn(async move {
        loop {
            // Non-blocking drain: pull all pending input datagrams.
            // We use a short sleep to yield between polls so we don't spin 100%.
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

    tokio::spawn(async move {
        // Create H.264 decoder.
        let mut decoder = match Decoder::new() {
            Ok(d) => d,
            Err(e) => {
                eprintln!("[Receiver] Failed to create H.264 decoder: {}", e);
                return;
            }
        };

        let mut frames_received: u64 = 0;
        let mut last_log = Instant::now();

        println!("Receiving frames... (close window or Ctrl+C to stop)\n");

        loop {
            // Accept the next uni stream from the server.
            let mut recv = match conn_recv.accept_uni().await {
                Ok(r) => r,
                Err(e) => {
                    println!("\nConnection closed: {}", e);
                    break;
                }
            };

            // Read 16-byte header: width, height, seq, h264_len.
            let mut header = [0u8; 16];
            if let Err(e) = recv.read_exact(&mut header).await {
                eprintln!("[Receiver] header read error: {}", e);
                continue;
            }

            let width    = u32::from_le_bytes([header[0],  header[1],  header[2],  header[3]])  as usize;
            let height   = u32::from_le_bytes([header[4],  header[5],  header[6],  header[7]])  as usize;
            let seq      = u32::from_le_bytes([header[8],  header[9],  header[10], header[11]]);
            let h264_len = u32::from_le_bytes([header[12], header[13], header[14], header[15]]) as usize;

            if width == 0 || height == 0 || width > 4096 || height > 4096 {
                eprintln!("[Receiver] invalid dimensions {}x{}", width, height);
                continue;
            }
            if h264_len == 0 || h264_len > 4 * 1024 * 1024 {
                eprintln!("[Receiver] invalid h264_len {}", h264_len);
                continue;
            }

            // Read H.264 bitstream.
            let h264_data = match recv.read_to_end(h264_len + 16).await {
                Ok(b) => b,
                Err(e) => {
                    eprintln!("[Receiver] h264 read error: {}", e);
                    continue;
                }
            };

            if h264_data.len() < h264_len {
                eprintln!(
                    "[Receiver] short h264 data: {} < {}",
                    h264_data.len(),
                    h264_len
                );
                continue;
            }

            // Decode H.264 → YUV.
            let yuv_frame = match decoder.decode(&h264_data[..h264_len]) {
                Ok(Some(yuv)) => yuv,
                Ok(None) => {
                    // Decoder buffering — no output frame yet.
                    continue;
                }
                Err(e) => {
                    eprintln!("[Receiver] decode error: {}", e);
                    continue;
                }
            };

            // Convert YUV → 0RGB (minifb format).
            let (dec_w, dec_h) = yuv_frame.dimensions();
            let buffer = yuv420_to_rgb(&yuv_frame, dec_w, dec_h);

            frames_received += 1;

            // Log FPS every second.
            if last_log.elapsed().as_secs_f64() >= 1.0 {
                let elapsed = last_log.elapsed().as_secs_f64();
                println!(
                    "[Receiver] seq={} frames/s={:.1} ({}x{} H.264)",
                    seq,
                    frames_received as f64 / elapsed,
                    dec_w,
                    dec_h,
                );
                frames_received = 0;
                last_log = Instant::now();
            }

            // Send frame to render thread; drop on backpressure (non-blocking).
            let _ = tx.try_send(Frame { width, height, buffer, seq });
        }
    });

    // ── minifb render loop on the main thread ─────────────────────────────────
    // Initial window size matches server resolution.
    let initial_w = 1920;
    let initial_h = 1080;

    let mut window = Window::new(
        "PRISM Client",
        initial_w,
        initial_h,
        WindowOptions {
            resize: true,
            ..WindowOptions::default()
        },
    )?;

    // Limit to ~60 fps max (minifb helper).
    window.set_target_fps(60);

    // Keep a blank buffer until the first frame arrives.
    let mut current_buffer: Vec<u32> = vec![0u32; initial_w * initial_h];
    let mut current_w = initial_w;
    let mut current_h = initial_h;

    // Input sender and state tracking for the main-thread render loop.
    let mut input_sender = prism_client::InputSender::new();
    let mut last_mx: u16 = 0;
    let mut last_my: u16 = 0;
    let mut last_left = false;
    let mut last_right = false;
    let mut last_middle = false;

    while window.is_open() && !window.is_key_down(minifb::Key::Escape) {
        // Drain all pending frames from the channel; use the latest one.
        let mut latest: Option<Frame> = None;
        loop {
            match rx.try_recv() {
                Ok(frame) => {
                    latest = Some(frame);
                }
                Err(std_mpsc::TryRecvError::Empty) => break,
                Err(std_mpsc::TryRecvError::Disconnected) => {
                    println!("[Renderer] receiver channel disconnected — exiting.");
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
            .unwrap_or_else(|e| eprintln!("[Renderer] update error: {}", e));

        // ── Input capture ────────────────────────────────────────────────────

        // Key presses
        for key in window.get_keys_pressed(minifb::KeyRepeat::Yes) {
            let vk = key as u16;
            let event = prism_protocol::input::InputEvent::KeyDown { scancode: vk, vk };
            let dgram = input_sender.build_datagram(event);
            input_tx.send(dgram).ok();
        }

        // Mouse position
        if let Some((mx, my)) = window.get_mouse_pos(minifb::MouseMode::Clamp) {
            let (nx, ny) = prism_client::normalize_mouse(mx, my, current_w as u32, current_h as u32);
            if nx != last_mx || ny != last_my {
                let event = prism_protocol::input::InputEvent::MouseMove { x: nx, y: ny };
                let dgram = input_sender.build_datagram(event);
                input_tx.send(dgram).ok();
                last_mx = nx;
                last_my = ny;
            }
        }

        // Mouse buttons
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
    }

    connector.close();
    Ok(())
}

// ── YUV I420 → RGB conversion ─────────────────────────────────────────────────

/// Convert a decoded YUV frame to a minifb `0RGB` pixel buffer.
///
/// `yuv` implements [`YUVSource`] which exposes `y()`, `u()`, `v()` slices and
/// `strides()` = `(y_stride, u_stride, v_stride)`.
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

            let r = (y_val + 1.402 * (v_val - 128.0)).round().clamp(0.0, 255.0) as u32;
            let g = (y_val - 0.344 * (u_val - 128.0) - 0.714 * (v_val - 128.0)).round().clamp(0.0, 255.0) as u32;
            let b = (y_val + 1.772 * (u_val - 128.0)).round().clamp(0.0, 255.0) as u32;

            buffer.push((r << 16) | (g << 8) | b);
        }
    }

    buffer
}
