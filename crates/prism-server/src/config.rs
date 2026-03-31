// Server configuration.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

/// Top-level configuration for the PRISM server.
#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// Address the latency/control QUIC endpoint binds to.
    pub listen_addr: SocketAddr,

    /// Address the throughput QUIC endpoint binds to (may equal `listen_addr`
    /// when running with a single endpoint).
    pub throughput_addr: SocketAddr,

    /// Path to the Noise identity key file.
    pub identity_path: PathBuf,

    /// Path to the pairing snapshot (approved device registry).
    pub pairing_path: PathBuf,

    /// Path to the tombstone store for session resurrection.
    pub tombstone_path: PathBuf,

    /// Human-readable display name advertised to clients.
    pub display_name: String,

    /// Maximum number of simultaneously connected clients.
    pub max_clients: usize,

    /// How long a client can go silent before the session is suspended.
    pub heartbeat_suspend: Duration,

    /// How long a suspended session is retained before it is tombstoned.
    pub heartbeat_tombstone: Duration,

    /// Maximum age of a tombstone before it is permanently discarded.
    pub tombstone_max_age: Duration,

    /// Total aggregate send bandwidth cap across all clients (bytes per second).
    pub total_bandwidth_bps: u64,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            listen_addr: "0.0.0.0:7000".parse().expect("valid default listen addr"),
            throughput_addr: "0.0.0.0:7001".parse().expect("valid default throughput addr"),
            identity_path: PathBuf::from("identity.key"),
            pairing_path: PathBuf::from("pairing.json"),
            tombstone_path: PathBuf::from("tombstones.json"),
            display_name: "PRISM Server".to_string(),
            max_clients: 4,
            heartbeat_suspend: Duration::from_secs(10),
            heartbeat_tombstone: Duration::from_secs(60),
            tombstone_max_age: Duration::from_secs(300),
            total_bandwidth_bps: 100 * 1_000_000, // 100 Mbps
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_valid() {
        let cfg = ServerConfig::default();

        assert_eq!(cfg.listen_addr.port(), 7000);
        assert_eq!(cfg.throughput_addr.port(), 7001);
        assert_eq!(cfg.max_clients, 4);
        assert_eq!(cfg.heartbeat_suspend, Duration::from_secs(10));
        assert_eq!(cfg.heartbeat_tombstone, Duration::from_secs(60));
        assert_eq!(cfg.tombstone_max_age, Duration::from_secs(300));
        assert_eq!(cfg.total_bandwidth_bps, 100_000_000);
        assert_eq!(cfg.display_name, "PRISM Server");
    }

    #[test]
    fn config_custom_values() {
        let cfg = ServerConfig {
            listen_addr: "127.0.0.1:9000".parse().unwrap(),
            throughput_addr: "127.0.0.1:9001".parse().unwrap(),
            identity_path: PathBuf::from("/etc/prism/identity.key"),
            pairing_path: PathBuf::from("/etc/prism/pairing.json"),
            tombstone_path: PathBuf::from("/var/lib/prism/tombstones.json"),
            display_name: "My PRISM Node".to_string(),
            max_clients: 8,
            heartbeat_suspend: Duration::from_secs(30),
            heartbeat_tombstone: Duration::from_secs(120),
            tombstone_max_age: Duration::from_secs(600),
            total_bandwidth_bps: 500 * 1_000_000,
        };

        assert_eq!(cfg.listen_addr.port(), 9000);
        assert_eq!(cfg.throughput_addr.port(), 9001);
        assert_eq!(cfg.max_clients, 8);
        assert_eq!(cfg.heartbeat_suspend, Duration::from_secs(30));
        assert_eq!(cfg.heartbeat_tombstone, Duration::from_secs(120));
        assert_eq!(cfg.tombstone_max_age, Duration::from_secs(600));
        assert_eq!(cfg.total_bandwidth_bps, 500_000_000);
        assert_eq!(cfg.display_name, "My PRISM Node");
        assert_eq!(cfg.identity_path, PathBuf::from("/etc/prism/identity.key"));
        assert_eq!(cfg.pairing_path, PathBuf::from("/etc/prism/pairing.json"));
        assert_eq!(cfg.tombstone_path, PathBuf::from("/var/lib/prism/tombstones.json"));
    }
}
