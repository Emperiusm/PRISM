// SPDX-License-Identifier: AGPL-3.0-or-later
//! SavedServer persistence with append-only log and compaction.

use std::{
    fs::{self, File, OpenOptions},
    io::{self, BufRead, BufReader, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Accent color helper
// ---------------------------------------------------------------------------

/// Deterministic accent color derived from the server display name.
/// Uses djb2 hashing, maps to a hue in the purple-cyan range (220–320°),
/// saturation 0.7, lightness 0.6, then converts HSL→RGB.
pub fn accent_color_for_name(name: &str) -> [u8; 3] {
    // djb2 hash
    let mut hash: u64 = 5381;
    for b in name.bytes() {
        hash = hash.wrapping_mul(33).wrapping_add(b as u64);
    }

    // Map hash to hue in [220, 320]
    let hue = 220.0 + (hash % 100) as f64;
    let saturation = 0.7_f64;
    let lightness = 0.6_f64;

    hsl_to_rgb(hue, saturation, lightness)
}

fn hsl_to_rgb(h: f64, s: f64, l: f64) -> [u8; 3] {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = l - c / 2.0;

    let (r1, g1, b1) = if h < 60.0 {
        (c, x, 0.0)
    } else if h < 120.0 {
        (x, c, 0.0)
    } else if h < 180.0 {
        (0.0, c, x)
    } else if h < 240.0 {
        (0.0, x, c)
    } else if h < 300.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };

    [
        ((r1 + m) * 255.0).round() as u8,
        ((g1 + m) * 255.0).round() as u8,
        ((b1 + m) * 255.0).round() as u8,
    ]
}

// ---------------------------------------------------------------------------
// SavedServer
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedServer {
    pub id: Uuid,
    pub display_name: String,
    pub address: String,
    pub noise_public_key: Option<String>,
    pub default_profile: String,
    pub accent_color: [u8; 3],
    pub last_connected: Option<u64>,
    pub last_resolution: Option<(u32, u32)>,
    pub last_codec: Option<String>,
    pub created_at: u64,
}

impl SavedServer {
    pub fn new(display_name: impl Into<String>, address: impl Into<String>) -> Self {
        let display_name = display_name.into();
        let address = address.into();
        let accent_color = accent_color_for_name(&display_name);
        let created_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Self {
            id: Uuid::now_v7(),
            display_name,
            address,
            noise_public_key: None,
            default_profile: String::from("default"),
            accent_color,
            last_connected: None,
            last_resolution: None,
            last_codec: None,
            created_at,
        }
    }
}

// ---------------------------------------------------------------------------
// Log entry (private, for append-only log)
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize)]
#[serde(tag = "op")]
enum LogEntry {
    #[serde(rename = "add")]
    Add { server: SavedServer },
    #[serde(rename = "update")]
    Update { server: SavedServer },
    #[serde(rename = "delete")]
    Delete { id: Uuid },
}

// ---------------------------------------------------------------------------
// ServerStore
// ---------------------------------------------------------------------------

pub struct ServerStore {
    dir: PathBuf,
    servers: Vec<SavedServer>,
}

impl ServerStore {
    /// Open (or create) a store rooted at `dir`.
    ///
    /// Loading order:
    /// 1. Read `servers.json` as the base snapshot (skipped if missing).
    /// 2. Replay every valid line of `servers.log` on top.
    pub fn open(dir: &Path) -> Result<Self, io::Error> {
        fs::create_dir_all(dir)?;

        let mut servers: Vec<SavedServer> = Vec::new();

        // 1. Load snapshot
        let snapshot_path = dir.join("servers.json");
        if snapshot_path.exists() {
            let data = fs::read(&snapshot_path)?;
            if let Ok(loaded) = serde_json::from_slice::<Vec<SavedServer>>(&data) {
                servers = loaded;
            }
        }

        // 2. Replay log
        let log_path = dir.join("servers.log");
        if log_path.exists() {
            let file = File::open(&log_path)?;
            let reader = BufReader::new(file);
            for line in reader.lines() {
                let line = match line {
                    Ok(l) if !l.trim().is_empty() => l,
                    _ => continue,
                };
                if let Ok(entry) = serde_json::from_str::<LogEntry>(&line) {
                    Self::apply_entry(&mut servers, entry);
                }
                // malformed lines are silently skipped
            }
        }

        Ok(Self {
            dir: dir.to_owned(),
            servers,
        })
    }

    pub fn servers(&self) -> &[SavedServer] {
        &self.servers
    }

    pub fn get(&self, id: Uuid) -> Option<&SavedServer> {
        self.servers.iter().find(|s| s.id == id)
    }

    pub fn add(&mut self, server: SavedServer) -> Result<(), io::Error> {
        let entry = LogEntry::Add { server: server.clone() };
        self.append_log(&entry)?;
        self.servers.push(server);
        Ok(())
    }

    pub fn update(&mut self, id: Uuid, f: impl FnOnce(&mut SavedServer)) -> Result<(), io::Error> {
        if let Some(server) = self.servers.iter_mut().find(|s| s.id == id) {
            f(server);
            let entry = LogEntry::Update { server: server.clone() };
            self.append_log(&entry)?;
        }
        Ok(())
    }

    pub fn delete(&mut self, id: Uuid) -> Result<(), io::Error> {
        let entry = LogEntry::Delete { id };
        self.append_log(&entry)?;
        self.servers.retain(|s| s.id != id);
        Ok(())
    }

    /// Write current state to `servers.json` and truncate `servers.log`.
    pub fn compact(&mut self) -> Result<(), io::Error> {
        let snapshot_path = self.dir.join("servers.json");
        let data = serde_json::to_vec_pretty(&self.servers)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
        fs::write(&snapshot_path, data)?;

        // Truncate (empty) the log
        let log_path = self.dir.join("servers.log");
        OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&log_path)?;

        Ok(())
    }

    // -----------------------------------------------------------------------
    // Private helpers
    // -----------------------------------------------------------------------

    fn append_log(&self, entry: &LogEntry) -> Result<(), io::Error> {
        let log_path = self.dir.join("servers.log");
        let mut file = OpenOptions::new()
            .append(true)
            .create(true)
            .open(&log_path)?;

        let mut line = serde_json::to_string(entry)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
        line.push('\n');
        file.write_all(line.as_bytes())?;
        Ok(())
    }

    fn apply_entry(servers: &mut Vec<SavedServer>, entry: LogEntry) {
        match entry {
            LogEntry::Add { server } => {
                servers.push(server);
            }
            LogEntry::Update { server } => {
                if let Some(existing) = servers.iter_mut().find(|s| s.id == server.id) {
                    *existing = server;
                }
            }
            LogEntry::Delete { id } => {
                servers.retain(|s| s.id != id);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn temp_store() -> (TempDir, ServerStore) {
        let dir = TempDir::new().expect("tempdir");
        let store = ServerStore::open(dir.path()).expect("open");
        (dir, store)
    }

    #[test]
    fn new_store_has_no_servers() {
        let (_dir, store) = temp_store();
        assert!(store.servers().is_empty());
    }

    #[test]
    fn add_and_retrieve_server() {
        let (_dir, mut store) = temp_store();
        let server = SavedServer::new("My Server", "192.168.1.1:4000");
        let id = server.id;
        store.add(server).expect("add");

        assert_eq!(store.servers().len(), 1);
        let retrieved = store.get(id).expect("get");
        assert_eq!(retrieved.display_name, "My Server");
        assert_eq!(retrieved.address, "192.168.1.1:4000");
    }

    #[test]
    fn update_server() {
        let (_dir, mut store) = temp_store();
        let server = SavedServer::new("Old Name", "10.0.0.1:4000");
        let id = server.id;
        store.add(server).expect("add");

        store
            .update(id, |s| {
                s.display_name = "New Name".to_string();
            })
            .expect("update");

        assert_eq!(store.get(id).unwrap().display_name, "New Name");
    }

    #[test]
    fn delete_server() {
        let (_dir, mut store) = temp_store();
        let server = SavedServer::new("Temp", "10.0.0.2:4000");
        let id = server.id;
        store.add(server).expect("add");
        assert_eq!(store.servers().len(), 1);

        store.delete(id).expect("delete");
        assert!(store.servers().is_empty());
        assert!(store.get(id).is_none());
    }

    #[test]
    fn persistence_survives_reopen() {
        let dir = TempDir::new().expect("tempdir");

        {
            let mut store = ServerStore::open(dir.path()).expect("open");
            let server = SavedServer::new("Persistent", "10.0.0.3:4000");
            store.add(server).expect("add");
        } // drop store

        let store2 = ServerStore::open(dir.path()).expect("reopen");
        assert_eq!(store2.servers().len(), 1);
        assert_eq!(store2.servers()[0].display_name, "Persistent");
    }

    #[test]
    fn compaction_works() {
        let dir = TempDir::new().expect("tempdir");

        {
            let mut store = ServerStore::open(dir.path()).expect("open");
            let server = SavedServer::new("ToDelete", "10.0.0.4:4000");
            let id = server.id;
            store.add(server).expect("add");
            store.delete(id).expect("delete");
            store.compact().expect("compact");
        }

        let store2 = ServerStore::open(dir.path()).expect("reopen");
        assert!(store2.servers().is_empty());
    }

    #[test]
    fn accent_color_from_name() {
        // Deterministic: same name produces same color
        let c1 = accent_color_for_name("Alpha");
        let c2 = accent_color_for_name("Alpha");
        assert_eq!(c1, c2);

        // Different names produce different colors
        let c3 = accent_color_for_name("Beta");
        assert_ne!(c1, c3);
    }
}
