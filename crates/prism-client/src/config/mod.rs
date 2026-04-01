// SPDX-License-Identifier: AGPL-3.0-or-later
//! Client configuration — CLI args, saved servers, shader cache paths.

pub mod servers;

use std::net::SocketAddr;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaunchMode {
    Launcher,      // No address args — show launcher
    DirectConnect, // Address provided — connect directly
}

#[derive(Debug, Clone)]
pub struct ClientConfig {
    pub launch_mode: LaunchMode,
    pub server_addr: Option<SocketAddr>,
    pub noise_key: Option<[u8; 32]>,
    pub identity_path: PathBuf,
    pub servers_dir: PathBuf,
    pub shader_cache_dir: PathBuf,
}

impl ClientConfig {
    pub fn from_args(args: &[String]) -> Result<Self, String> {
        let prism_dir = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".prism");

        let mut identity_path = prism_dir.join("client_identity.json");
        let mut servers_dir = prism_dir.clone();
        let mut shader_cache_dir = prism_dir.join("shader_cache");
        let mut server_addr: Option<SocketAddr> = None;
        let mut noise_key: Option<[u8; 32]> = None;

        let mut i = 0;
        while i < args.len() {
            match args[i].as_str() {
                "--noise" => {
                    i += 1;
                    if i >= args.len() {
                        return Err("--noise requires a value".to_string());
                    }
                    let hex_str = &args[i];
                    if hex_str.len() != 64 {
                        return Err(format!(
                            "--noise value must be 64 hex chars, got {}",
                            hex_str.len()
                        ));
                    }
                    let bytes = hex::decode(hex_str)
                        .map_err(|e| format!("--noise hex decode error: {e}"))?;
                    let mut key = [0u8; 32];
                    key.copy_from_slice(&bytes);
                    noise_key = Some(key);
                }
                "--config" => {
                    i += 1;
                    if i >= args.len() {
                        return Err("--config requires a value".to_string());
                    }
                    servers_dir = PathBuf::from(&args[i]);
                }
                flag if flag.starts_with('-') => {
                    // Skip unknown flags; if they look like --key value, skip value too
                    // We only skip the flag itself here; paired values for unknown flags
                    // are left to be processed in the next iteration naturally.
                }
                candidate => {
                    if server_addr.is_none() {
                        server_addr = Some(
                            candidate
                                .parse::<SocketAddr>()
                                .map_err(|_| format!("invalid address: '{candidate}'"))?,
                        );
                    }
                }
            }
            i += 1;
        }

        // Override identity_path if servers_dir was changed
        if servers_dir != prism_dir {
            identity_path = servers_dir.join("client_identity.json");
            shader_cache_dir = servers_dir.join("shader_cache");
        }

        let launch_mode = if server_addr.is_some() {
            LaunchMode::DirectConnect
        } else {
            LaunchMode::Launcher
        };

        Ok(ClientConfig {
            launch_mode,
            server_addr,
            noise_key,
            identity_path,
            servers_dir,
            shader_cache_dir,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(s: &str) -> Vec<String> {
        s.split_whitespace().map(String::from).collect()
    }

    #[test]
    fn no_args_is_launcher_mode() {
        let cfg = ClientConfig::from_args(&[]).unwrap();
        assert_eq!(cfg.launch_mode, LaunchMode::Launcher);
        assert!(cfg.server_addr.is_none());
    }

    #[test]
    fn address_arg_is_direct_connect() {
        let cfg = ClientConfig::from_args(&args("192.168.1.100:7000")).unwrap();
        assert_eq!(cfg.launch_mode, LaunchMode::DirectConnect);
        assert_eq!(cfg.server_addr, Some("192.168.1.100:7000".parse().unwrap()));
    }

    #[test]
    fn noise_key_parsed() {
        let hex_key = "a".repeat(64);
        let input = format!("127.0.0.1:7000 --noise {hex_key}");
        let cfg = ClientConfig::from_args(&args(&input)).unwrap();
        assert!(cfg.noise_key.is_some());
    }

    #[test]
    fn invalid_address_errors() {
        let result = ClientConfig::from_args(&args("not-an-address"));
        assert!(result.is_err());
    }
}
