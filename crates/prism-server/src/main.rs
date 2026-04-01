// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2025-2026 Ehsan Khalid. All rights reserved.
// Licensed under the GNU Affero General Public License v3.0.
// Commercial licensing available — see LICENSE-COMMERCIAL.md.

const VERSION: &str = env!("CARGO_PKG_VERSION");

const HELP: &str = "\
PRISM Server — Protocol for Remote Interactive Streaming & Multiplexing

USAGE:
    prism-server [OPTIONS]

OPTIONS:
    --dda               Use DXGI Desktop Duplication (real desktop capture)
                        Without this flag, a test pattern is streamed
    --noise             Enable Noise IK end-to-end encryption
                        Prints the server public key on startup
    --monitor <N>       Select monitor to capture (0-indexed, default: 0)
    --config <PATH>     Path to TOML config file (default: prism-server.toml)
    --port <PORT>       Override listen port (default: 7000)
    --bind <ADDR>       Override bind address (default: 0.0.0.0:7000)
    --init              Generate a default prism-server.toml and exit
    --version           Print version and exit
    --help              Print this help and exit

EXAMPLES:
    prism-server                          # Test pattern on port 7000
    prism-server --dda                    # Capture real desktop
    prism-server --dda --noise            # Desktop + encryption
    prism-server --dda --monitor 1        # Capture second monitor
    prism-server --port 9000              # Custom port
    prism-server --config my-config.toml  # Custom config file
    prism-server --init                   # Generate default config

CONFIG:
    The server loads prism-server.toml from the working directory by default.
    All config keys are optional — missing keys use sensible defaults.
    CLI flags override config file values.
    Run --init to generate a fully commented example config.

DOCS:
    https://github.com/Emperiusm/PRISM
";

const DEFAULT_CONFIG: &str = r#"# PRISM Server Configuration
# All values shown are defaults — uncomment and modify as needed.

# ── Network ──────────────────────────────────────────────────────────────────

# Address for the main QUIC endpoint (latency-sensitive: video, audio, input).
# listen_addr_str = "0.0.0.0:7000"

# Address for the throughput QUIC endpoint (bulk transfers, metrics).
# throughput_addr_str = "0.0.0.0:7001"

# ── Limits ───────────────────────────────────────────────────────────────────

# Maximum number of simultaneously connected clients.
# max_clients = 4

# Total aggregate send bandwidth cap across all clients (bytes/sec).
# 100_000_000 = 100 Mbps
# total_bandwidth_bps = 100000000

# ── Display ──────────────────────────────────────────────────────────────────

# Human-readable name advertised to clients.
# display_name = "PRISM Server"

# ── Session Management ───────────────────────────────────────────────────────

# Seconds of silence before a client session is suspended.
# heartbeat_suspend_secs = 10

# Seconds a suspended session is kept before tombstoning.
# heartbeat_tombstone_secs = 60

# Maximum age (seconds) of a tombstone before permanent removal.
# tombstone_max_age_secs = 300

# ── Security & Identity ─────────────────────────────────────────────────────

# Path to the Noise IK identity key file (auto-generated on first run).
# identity_path = "identity.key"

# Path to the pairing store (approved devices registry).
# pairing_path = "pairing.json"

# Path to the tombstone store (session resurrection).
# tombstone_path = "tombstones.json"
"#;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();

    // ── Immediate exit flags ─────────────────────────────────────────────
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print!("{HELP}");
        return Ok(());
    }
    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("prism-server {VERSION}");
        return Ok(());
    }
    if args.iter().any(|a| a == "--init") {
        let path = "prism-server.toml";
        if std::path::Path::new(path).exists() {
            eprintln!(
                "Error: {path} already exists. Remove it first or use --config to load a different file."
            );
            std::process::exit(1);
        }
        std::fs::write(path, DEFAULT_CONFIG)?;
        println!("Created {path} with default configuration.");
        println!("Edit the file, then start the server with: prism-server");
        return Ok(());
    }

    tracing_subscriber::fmt::init();

    // ── Parse flags ──────────────────────────────────────────────────────
    let use_dda = args.iter().any(|a| a == "--dda");
    let noise_mode = args.iter().any(|a| a == "--noise");

    let monitor_index: u32 = args
        .windows(2)
        .find(|w| w[0] == "--monitor")
        .and_then(|w| w[1].parse::<u32>().ok())
        .unwrap_or(0);

    let config_path = args
        .windows(2)
        .find(|w| w[0] == "--config")
        .map(|w| w[1].clone())
        .unwrap_or_else(|| "prism-server.toml".to_string());

    // Load config, then apply CLI overrides
    let mut config =
        prism_server::ServerConfig::load_or_default(std::path::Path::new(&config_path));

    if let Some(port) = args
        .windows(2)
        .find(|w| w[0] == "--port")
        .and_then(|w| w[1].parse::<u16>().ok())
    {
        config.listen_addr_str = format!("0.0.0.0:{port}");
    }
    if let Some(bind) = args
        .windows(2)
        .find(|w| w[0] == "--bind")
        .map(|w| w[1].clone())
    {
        config.listen_addr_str = bind;
    }

    let mut app = prism_server::ServerApp::with_config(use_dda, noise_mode, monitor_index, config)?;

    tokio::select! {
        result = app.run() => result,
        _ = tokio::signal::ctrl_c() => {
            tracing::info!("Ctrl+C received, shutting down...");
            app.shutdown().await;
            Ok(())
        }
    }
}
