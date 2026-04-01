// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2025-2026 Ehsan Khalid. All rights reserved.
// Licensed under the GNU Affero General Public License v3.0.
// Commercial licensing available — see LICENSE-COMMERCIAL.md.

//! Entry-point for PRISM Client.

const VERSION: &str = env!("CARGO_PKG_VERSION");

const HELP: &str = "\
PRISM Client — remote desktop viewer

USAGE:
    prism-client [HOST:PORT] [OPTIONS]

ARGUMENTS:
    HOST:PORT           Server address (default: opens launcher)
                        Provide address to connect directly

OPTIONS:
    --noise <KEY>       Server's Noise IK public key (64-char hex)
    --config <PATH>     Custom config directory
    --init              Generate default servers.json
    --version           Print version and exit
    --help              Print this help and exit

EXAMPLES:
    prism-client                                    # Open launcher
    prism-client 192.168.1.100:7000                 # Direct connect
    prism-client 10.0.0.5:7000 --noise a3f1...beef  # With encryption
";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Custom panic hook: print the error and wait for Enter so the user can read it
    // when launching from Explorer (double-click) where the terminal auto-closes.
    std::panic::set_hook(Box::new(|info| {
        eprintln!("\n========================================");
        eprintln!("PRISM Client crashed!");
        eprintln!("========================================\n");
        eprintln!("{info}\n");
        eprintln!("Press Enter to exit...");
        let _ = std::io::Read::read(&mut std::io::stdin(), &mut [0u8]);
    }));

    let args: Vec<String> = std::env::args().skip(1).collect();

    // ── Immediate exit flags ─────────────────────────────────────────────
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print!("{HELP}");
        return Ok(());
    }
    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("prism-client {VERSION}");
        return Ok(());
    }
    if args.iter().any(|a| a == "--init") {
        let dir = dirs::home_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join(".prism");
        std::fs::create_dir_all(&dir)?;
        let path = dir.join("servers.json");
        if !path.exists() {
            std::fs::write(&path, "[]")?;
        }
        println!("Config initialized at {}", path.display());
        return Ok(());
    }

    tracing_subscriber::fmt::init();

    let config = prism_client::config::ClientConfig::from_args(&args)
        .map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;

    tracing::info!(
        mode = ?config.launch_mode,
        addr = ?config.server_addr,
        "Starting PRISM client"
    );

    let app = prism_client::app::PrismApp::new(config);
    if let Err(e) = app.run() {
        eprintln!("\n========================================");
        eprintln!("PRISM Client error: {e}");
        eprintln!("========================================\n");
        eprintln!("Press Enter to exit...");
        let _ = std::io::Read::read(&mut std::io::stdin(), &mut [0u8]);
        return Err(e);
    }
    Ok(())
}
