//! PRISM Client binary.
//!
//! Connects to a PRISM server, receives BGRA pixel frames over QUIC uni streams,
//! and renders them in a minifb window.
//!
//! Frame wire format (from server):
//!   [4 bytes: width  u32 LE]
//!   [4 bytes: height u32 LE]
//!   [4 bytes: seq    u32 LE]
//!   [width * height * 4 bytes: BGRA pixel data]

use std::sync::mpsc as std_mpsc;
use std::time::Instant;

use minifb::{Window, WindowOptions};

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

            // Read 12-byte header: width, height, seq.
            let mut header = [0u8; 12];
            if let Err(e) = recv.read_exact(&mut header).await {
                eprintln!("[Receiver] header read error: {}", e);
                continue;
            }

            let width  = u32::from_le_bytes([header[0], header[1], header[2], header[3]]) as usize;
            let height = u32::from_le_bytes([header[4], header[5], header[6], header[7]]) as usize;
            let seq    = u32::from_le_bytes([header[8], header[9], header[10], header[11]]);

            if width == 0 || height == 0 || width > 4096 || height > 4096 {
                eprintln!("[Receiver] invalid dimensions {}x{}", width, height);
                continue;
            }

            // Read pixel data.
            let pixel_count = width * height * 4;
            let pixel_bytes = match recv.read_to_end(pixel_count + 16).await {
                Ok(b) => b,
                Err(e) => {
                    eprintln!("[Receiver] pixel read error: {}", e);
                    continue;
                }
            };

            if pixel_bytes.len() < pixel_count {
                eprintln!(
                    "[Receiver] short pixel data: {} < {}",
                    pixel_bytes.len(),
                    pixel_count
                );
                continue;
            }

            // Convert BGRA -> 0RGB (minifb format).
            // BGRA bytes: [B, G, R, A] -> u32: (R << 16) | (G << 8) | B
            let buffer: Vec<u32> = pixel_bytes[..pixel_count]
                .chunks_exact(4)
                .map(|bgra| {
                    ((bgra[2] as u32) << 16)
                        | ((bgra[1] as u32) << 8)
                        | (bgra[0] as u32)
                })
                .collect();

            frames_received += 1;

            // Log FPS every second.
            if last_log.elapsed().as_secs() >= 1 {
                println!(
                    "[Receiver] seq={} frames_received={} fps~{}",
                    seq,
                    frames_received,
                    frames_received
                        .saturating_sub(
                            frames_received.saturating_sub(
                                last_log.elapsed().as_secs() * 10
                            )
                        )
                        .min(10)
                );
                last_log = Instant::now();
            }

            // Send frame to render thread; drop on backpressure (non-blocking).
            let _ = tx.try_send(Frame { width, height, buffer, seq });
        }
    });

    // ── minifb render loop on the main thread ─────────────────────────────────
    // Initial window size; will be resized on first frame if needed.
    let initial_w = 320;
    let initial_h = 240;

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
