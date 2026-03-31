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
    let server_addr = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "127.0.0.1:9876".to_string());
    let server_addr: std::net::SocketAddr = server_addr.parse()?;

    println!("=== PRISM Client v0.1.0 ===");
    println!("Connecting to {}...", server_addr);

    let connector = prism_client::ClientConnector::new(prism_client::TlsMode::InsecureTrustAll)?;
    let connection = connector.connect(server_addr, "localhost").await?;
    println!("Connected to {}", connection.remote_address());

    // Channel: async receiver -> main-thread renderer.
    // Use std::sync::mpsc so the sender (tokio task) can send without async,
    // and the receiver (main thread) can poll without blocking the event loop.
    let (tx, rx) = std_mpsc::sync_channel::<Frame>(4);

    // Spawn the async frame-receive task.
    let conn_recv = connection.clone();
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
    // Initial window size; will be resized on first frame if needed.
    let initial_w = 640;
    let initial_h = 480;

    let mut window = Window::new(
        "PRISM Client",
        initial_w,
        initial_h,
        WindowOptions {
            resize: false,
            ..WindowOptions::default()
        },
    )?;

    // Limit to ~60 fps max (minifb helper).
    window.set_target_fps(60);

    // Keep a blank buffer until the first frame arrives.
    let mut current_buffer: Vec<u32> = vec![0u32; initial_w * initial_h];
    let mut current_w = initial_w;
    let mut current_h = initial_h;

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
