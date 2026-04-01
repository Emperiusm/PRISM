#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let args: Vec<String> = std::env::args().collect();
    let use_dda = args.iter().any(|a| a == "--dda");
    let noise_mode = args.iter().any(|a| a == "--noise");

    // Parse --monitor <n> (default: 0)
    let monitor_index: u32 = args
        .windows(2)
        .find(|w| w[0] == "--monitor")
        .and_then(|w| w[1].parse::<u32>().ok())
        .unwrap_or(0);

    let mut app = prism_server::ServerApp::new(use_dda, noise_mode, monitor_index)?;

    tokio::select! {
        result = app.run() => result,
        _ = tokio::signal::ctrl_c() => {
            tracing::info!("Ctrl+C received, shutting down...");
            app.shutdown().await;
            Ok(())
        }
    }
}
