//! PRISM Client binary entry-point.
//!
//! All logic lives in [`prism_client::ClientApp`]. This file only parses
//! CLI arguments, builds a [`prism_client::ClientConfig`], and drives the app.

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let server_addr = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "127.0.0.1:7000".to_string())
        .parse()?;

    let noise_key = std::env::args()
        .position(|a| a == "--noise")
        .and_then(|i| std::env::args().nth(i + 1))
        .map(|hex_str| {
            let bytes = hex::decode(&hex_str).expect("invalid hex key");
            let mut key = [0u8; 32];
            key.copy_from_slice(&bytes);
            key
        });

    let identity_path = dirs::home_dir()
        .unwrap_or_default()
        .join(".prism")
        .join("client_identity.json");

    let config = prism_client::ClientConfig {
        server_addr,
        noise_key,
        identity_path,
    };

    let mut app = prism_client::ClientApp::new(config);
    app.run().await
}
