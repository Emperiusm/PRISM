// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2025-2026 Ehsan Khalid. All rights reserved.
// Licensed under the GNU Affero General Public License v3.0.
// Commercial licensing available — see LICENSE-COMMERCIAL.md.

//! PRISM Client binary entry-point.
//!
//! All logic lives in [`prism_client::ClientApp`]. This file only parses
//! CLI arguments, builds a [`prism_client::ClientConfig`], and drives the app.

const VERSION: &str = env!("CARGO_PKG_VERSION");

const HELP: &str = "\
PRISM Client — Connect to a PRISM remote desktop server

USAGE:
    prism-client [HOST:PORT] [OPTIONS]

ARGUMENTS:
    HOST:PORT           Server address (default: 127.0.0.1:7000)

OPTIONS:
    --noise <KEY>       Server's Noise IK public key (64-char hex)
                        Enables end-to-end encryption
    --version           Print version and exit
    --help              Print this help and exit

EXAMPLES:
    prism-client                                    # Connect to localhost:7000
    prism-client 192.168.1.100:7000                 # Connect to remote server
    prism-client 10.0.0.5:7000 --noise a3f1...beef  # With encryption

CONTROLS:
    Mouse and keyboard input is forwarded to the server automatically.
    Clipboard syncs bidirectionally (copy on either side, paste on either).
    Close the window or press Ctrl+C to disconnect.
";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();

    // ── Immediate exit flags ─────────────────────────────────────────────
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print!("{HELP}");
        return Ok(());
    }
    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("prism-client {VERSION}");
        return Ok(());
    }

    tracing_subscriber::fmt::init();

    // ── Parse arguments ──────────────────────────────────────────────────
    // Skip args that are flags (--*) or values of --noise.
    let noise_pos = args.iter().position(|a| a == "--noise");
    let server_addr = args
        .iter()
        .enumerate()
        .skip(1)
        .filter(|(i, a)| !a.starts_with("--") && noise_pos.is_none_or(|np| *i != np + 1))
        .map(|(_, a)| a.as_str())
        .next()
        .unwrap_or("127.0.0.1:7000")
        .parse()?;

    let noise_key = args
        .iter()
        .position(|a| a == "--noise")
        .and_then(|i| args.get(i + 1))
        .map(|hex_str| {
            let bytes = hex::decode(hex_str).expect("invalid hex key — expected 64 hex characters");
            assert!(
                bytes.len() == 32,
                "Noise key must be exactly 32 bytes (64 hex characters)"
            );
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
