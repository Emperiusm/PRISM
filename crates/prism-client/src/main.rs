//! PRISM Client binary.
//!
//! Connects to a PRISM server and receives display datagrams, printing
//! periodic frame statistics until the connection is closed.

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

    let mut stats = prism_client::FrameStats::new();
    let mut last_report = std::time::Instant::now();

    println!("Receiving frames... (Ctrl+C to stop)\n");

    loop {
        tokio::select! {
            result = connection.read_datagram() => {
                match result {
                    Ok(data) => {
                        if let Some((seq, _ch)) = prism_client::parse_display_datagram(&data) {
                            stats.record(seq, data.len());
                        }
                    }
                    Err(e) => {
                        println!("\nConnection closed: {}", e);
                        break;
                    }
                }
            }
            _ = tokio::time::sleep(std::time::Duration::from_secs(1)) => {}
        }

        if last_report.elapsed() >= std::time::Duration::from_secs(1) {
            println!(
                "Frames: {} | Gaps: {} | Avg FPS: {:.1} | Avg size: {} B | Total: {} KB",
                stats.frames_received,
                stats.gaps,
                stats.avg_fps(),
                stats.avg_frame_size(),
                stats.bytes_received / 1024,
            );
            last_report = std::time::Instant::now();
        }
    }

    println!("\n=== Final Stats ===");
    println!("  Frames: {}", stats.frames_received);
    println!("  Gaps: {}", stats.gaps);
    println!("  Avg FPS: {:.1}", stats.avg_fps());
    println!("  Total: {} KB", stats.bytes_received / 1024);

    connector.close();
    Ok(())
}
