#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let use_dda = std::env::args().any(|a| a == "--dda");
    let noise_mode = std::env::args().any(|a| a == "--noise");

    let mut app = prism_server::ServerApp::new(use_dda, noise_mode)?;

    tokio::select! {
        result = app.run() => result,
        _ = tokio::signal::ctrl_c() => {
            tracing::info!("Ctrl+C received, shutting down...");
            app.shutdown().await;
            Ok(())
        }
    }
}
