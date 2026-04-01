// Server configuration.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use serde::Deserialize;

// ── Default-value functions for serde ────────────────────────────────────────

fn default_listen_addr() -> String { "0.0.0.0:7000".to_string() }
fn default_throughput_addr() -> String { "0.0.0.0:7001".to_string() }
fn default_identity_path() -> PathBuf { PathBuf::from("identity.key") }
fn default_pairing_path() -> PathBuf { PathBuf::from("pairing.json") }
fn default_tombstone_path() -> PathBuf { PathBuf::from("tombstones.json") }
fn default_display_name() -> String { "PRISM Server".to_string() }
fn default_max_clients() -> usize { 4 }
fn default_heartbeat_suspend_secs() -> u64 { 10 }
fn default_heartbeat_tombstone_secs() -> u64 { 60 }
fn default_tombstone_max_age_secs() -> u64 { 300 }
fn default_total_bandwidth_bps() -> u64 { 100 * 1_000_000 }

// ── Raw TOML-deserializable representation ───────────────────────────────────

/// TOML-deserializable configuration.  Duration fields are expressed as `u64`
/// seconds and converted to [`std::time::Duration`] by [`ServerConfig::build`].
#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    /// Address the latency/control QUIC endpoint binds to (string form for TOML).
    #[serde(default = "default_listen_addr")]
    pub listen_addr_str: String,

    /// Address the throughput QUIC endpoint binds to (string form for TOML).
    #[serde(default = "default_throughput_addr")]
    pub throughput_addr_str: String,

    /// Path to the Noise identity key file.
    #[serde(default = "default_identity_path")]
    pub identity_path: PathBuf,

    /// Path to the pairing snapshot (approved device registry).
    #[serde(default = "default_pairing_path")]
    pub pairing_path: PathBuf,

    /// Path to the tombstone store for session resurrection.
    #[serde(default = "default_tombstone_path")]
    pub tombstone_path: PathBuf,

    /// Human-readable display name advertised to clients.
    #[serde(default = "default_display_name")]
    pub display_name: String,

    /// Maximum number of simultaneously connected clients.
    #[serde(default = "default_max_clients")]
    pub max_clients: usize,

    /// How long (seconds) a client can go silent before the session is
    /// suspended.
    #[serde(default = "default_heartbeat_suspend_secs")]
    pub heartbeat_suspend_secs: u64,

    /// How long (seconds) a suspended session is retained before it is
    /// tombstoned.
    #[serde(default = "default_heartbeat_tombstone_secs")]
    pub heartbeat_tombstone_secs: u64,

    /// Maximum age (seconds) of a tombstone before it is permanently discarded.
    #[serde(default = "default_tombstone_max_age_secs")]
    pub tombstone_max_age_secs: u64,

    /// Total aggregate send bandwidth cap across all clients (bytes per second).
    #[serde(default = "default_total_bandwidth_bps")]
    pub total_bandwidth_bps: u64,
}

impl ServerConfig {
    // ── Parsed accessors ──────────────────────────────────────────────────────

    /// Parsed listen address.
    pub fn listen_addr(&self) -> SocketAddr {
        self.listen_addr_str
            .parse()
            .unwrap_or_else(|_| "0.0.0.0:7000".parse().unwrap())
    }

    /// Parsed throughput address.
    pub fn throughput_addr(&self) -> SocketAddr {
        self.throughput_addr_str
            .parse()
            .unwrap_or_else(|_| "0.0.0.0:7001".parse().unwrap())
    }

    /// Heartbeat suspend duration.
    pub fn heartbeat_suspend(&self) -> Duration {
        Duration::from_secs(self.heartbeat_suspend_secs)
    }

    /// Heartbeat tombstone duration.
    pub fn heartbeat_tombstone(&self) -> Duration {
        Duration::from_secs(self.heartbeat_tombstone_secs)
    }

    /// Tombstone max age duration.
    pub fn tombstone_max_age(&self) -> Duration {
        Duration::from_secs(self.tombstone_max_age_secs)
    }

    // ── I/O helpers ───────────────────────────────────────────────────────────

    /// Load configuration from a TOML file.  Missing keys fall back to their
    /// `#[serde(default)]` values, so a partial file is always valid.
    pub fn load_from_file(path: &std::path::Path) -> Result<Self, Box<dyn std::error::Error>> {
        let text = std::fs::read_to_string(path)?;
        let cfg: Self = toml::from_str(&text)?;
        Ok(cfg)
    }

    /// Load configuration from `path` if the file exists; silently fall back
    /// to [`Default::default`] on any error (missing file, parse error, etc.).
    pub fn load_or_default(path: &std::path::Path) -> Self {
        Self::load_from_file(path).unwrap_or_default()
    }
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            listen_addr_str: default_listen_addr(),
            throughput_addr_str: default_throughput_addr(),
            identity_path: default_identity_path(),
            pairing_path: default_pairing_path(),
            tombstone_path: default_tombstone_path(),
            display_name: default_display_name(),
            max_clients: default_max_clients(),
            heartbeat_suspend_secs: default_heartbeat_suspend_secs(),
            heartbeat_tombstone_secs: default_heartbeat_tombstone_secs(),
            tombstone_max_age_secs: default_tombstone_max_age_secs(),
            total_bandwidth_bps: default_total_bandwidth_bps(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_valid() {
        let cfg = ServerConfig::default();

        assert_eq!(cfg.listen_addr().port(), 7000);
        assert_eq!(cfg.throughput_addr().port(), 7001);
        assert_eq!(cfg.max_clients, 4);
        assert_eq!(cfg.heartbeat_suspend(), Duration::from_secs(10));
        assert_eq!(cfg.heartbeat_tombstone(), Duration::from_secs(60));
        assert_eq!(cfg.tombstone_max_age(), Duration::from_secs(300));
        assert_eq!(cfg.total_bandwidth_bps, 100_000_000);
        assert_eq!(cfg.display_name, "PRISM Server");
    }

    #[test]
    fn config_custom_values() {
        let cfg = ServerConfig {
            listen_addr_str: "127.0.0.1:9000".to_string(),
            throughput_addr_str: "127.0.0.1:9001".to_string(),
            identity_path: PathBuf::from("/etc/prism/identity.key"),
            pairing_path: PathBuf::from("/etc/prism/pairing.json"),
            tombstone_path: PathBuf::from("/var/lib/prism/tombstones.json"),
            display_name: "My PRISM Node".to_string(),
            max_clients: 8,
            heartbeat_suspend_secs: 30,
            heartbeat_tombstone_secs: 120,
            tombstone_max_age_secs: 600,
            total_bandwidth_bps: 500 * 1_000_000,
        };

        assert_eq!(cfg.listen_addr().port(), 9000);
        assert_eq!(cfg.throughput_addr().port(), 9001);
        assert_eq!(cfg.max_clients, 8);
        assert_eq!(cfg.heartbeat_suspend(), Duration::from_secs(30));
        assert_eq!(cfg.heartbeat_tombstone(), Duration::from_secs(120));
        assert_eq!(cfg.tombstone_max_age(), Duration::from_secs(600));
        assert_eq!(cfg.total_bandwidth_bps, 500_000_000);
        assert_eq!(cfg.display_name, "My PRISM Node");
        assert_eq!(cfg.identity_path, PathBuf::from("/etc/prism/identity.key"));
        assert_eq!(cfg.pairing_path, PathBuf::from("/etc/prism/pairing.json"));
        assert_eq!(cfg.tombstone_path, PathBuf::from("/var/lib/prism/tombstones.json"));
    }

    #[test]
    fn config_from_toml_string() {
        let toml_str = r#"
            listen_addr_str = "127.0.0.1:5555"
            max_clients = 2
        "#;
        let config: ServerConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.listen_addr().port(), 5555);
        assert_eq!(config.max_clients, 2);
    }

    #[test]
    fn config_empty_toml_uses_defaults() {
        let config: ServerConfig = toml::from_str("").unwrap();
        assert_eq!(config.max_clients, 4);
    }
}
