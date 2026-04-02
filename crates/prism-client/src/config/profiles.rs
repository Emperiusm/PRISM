// SPDX-License-Identifier: AGPL-3.0-or-later
//! ProfileStore persistence with append-only log and snapshot compaction.

use std::{
    fs::{self, File, OpenOptions},
    io::{self, BufRead, BufReader, Write},
    path::{Path, PathBuf},
};

use prism_session::EncoderPreset;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AudioMode {
    Stereo,
    VoiceOptimized,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProfileConfig {
    pub id: Uuid,
    pub name: String,
    pub builtin: bool,
    pub bitrate_bps: u64,
    pub max_fps: u8,
    pub encoder_preset: EncoderPreset,
    pub prefer_native_scaling: bool,
    pub audio_mode: AudioMode,
    pub prefer_av1: bool,
    pub exclusive_input: bool,
    pub touch_mode: bool,
    pub auto_reconnect: bool,
}

fn builtin_profiles() -> Vec<ProfileConfig> {
    vec![
        ProfileConfig {
            id: Uuid::parse_str("11111111-1111-7111-8111-111111111111")
                .expect("valid builtin profile uuid"),
            name: "Gaming".to_string(),
            builtin: true,
            bitrate_bps: 45_000_000,
            max_fps: 120,
            encoder_preset: EncoderPreset::UltraLowLatency,
            prefer_native_scaling: true,
            audio_mode: AudioMode::Stereo,
            prefer_av1: true,
            exclusive_input: true,
            touch_mode: false,
            auto_reconnect: true,
        },
        ProfileConfig {
            id: Uuid::parse_str("22222222-2222-7222-8222-222222222222")
                .expect("valid builtin profile uuid"),
            name: "Coding".to_string(),
            builtin: true,
            bitrate_bps: 30_000_000,
            max_fps: 60,
            encoder_preset: EncoderPreset::Quality,
            prefer_native_scaling: true,
            audio_mode: AudioMode::Stereo,
            prefer_av1: true,
            exclusive_input: true,
            touch_mode: false,
            auto_reconnect: true,
        },
        ProfileConfig {
            id: Uuid::parse_str("33333333-3333-7333-8333-333333333333")
                .expect("valid builtin profile uuid"),
            name: "Balanced".to_string(),
            builtin: true,
            bitrate_bps: 25_000_000,
            max_fps: 90,
            encoder_preset: EncoderPreset::Balanced,
            prefer_native_scaling: true,
            audio_mode: AudioMode::Stereo,
            prefer_av1: true,
            exclusive_input: false,
            touch_mode: false,
            auto_reconnect: true,
        },
        ProfileConfig {
            id: Uuid::parse_str("44444444-4444-7444-8444-444444444444")
                .expect("valid builtin profile uuid"),
            name: "Low Bandwidth".to_string(),
            builtin: true,
            bitrate_bps: 8_000_000,
            max_fps: 45,
            encoder_preset: EncoderPreset::Balanced,
            prefer_native_scaling: false,
            audio_mode: AudioMode::VoiceOptimized,
            prefer_av1: false,
            exclusive_input: false,
            touch_mode: false,
            auto_reconnect: false,
        },
    ]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op")]
enum LogEntry {
    #[serde(rename = "add")]
    Add { profile: ProfileConfig },
    #[serde(rename = "update")]
    Update { profile: ProfileConfig },
    #[serde(rename = "delete")]
    Delete { id: Uuid },
}

pub struct ProfileStore {
    dir: PathBuf,
    profiles: Vec<ProfileConfig>,
}

impl ProfileStore {
    pub fn open(dir: &Path) -> Result<Self, io::Error> {
        fs::create_dir_all(dir)?;

        let mut profiles = builtin_profiles();
        let snapshot_path = dir.join("profiles.json");
        if snapshot_path.exists() {
            let data = fs::read(&snapshot_path)?;
            if let Ok(loaded) = serde_json::from_slice::<Vec<ProfileConfig>>(&data) {
                profiles = loaded;
            }
        }

        let log_path = dir.join("profiles.log");
        if log_path.exists() {
            let file = File::open(&log_path)?;
            let reader = BufReader::new(file);
            for line in reader.lines() {
                let line = match line {
                    Ok(v) if !v.trim().is_empty() => v,
                    _ => continue,
                };
                if let Ok(entry) = serde_json::from_str::<LogEntry>(&line) {
                    Self::apply_entry(&mut profiles, entry);
                }
            }
        }

        if profiles.is_empty() {
            profiles = builtin_profiles();
        }

        Ok(Self {
            dir: dir.to_owned(),
            profiles,
        })
    }

    pub fn list(&self) -> &[ProfileConfig] {
        &self.profiles
    }

    pub fn get(&self, id: Uuid) -> Option<&ProfileConfig> {
        self.profiles.iter().find(|p| p.id == id)
    }

    pub fn get_by_name(&self, name: &str) -> Option<&ProfileConfig> {
        self.profiles.iter().find(|p| p.name == name)
    }

    pub fn add(&mut self, mut profile: ProfileConfig) -> Result<Uuid, io::Error> {
        if profile.id.is_nil() {
            profile.id = Uuid::now_v7();
        }
        let entry = LogEntry::Add {
            profile: profile.clone(),
        };
        self.append_log(&entry)?;
        let id = profile.id;
        self.profiles.push(profile);
        Ok(id)
    }

    pub fn update(&mut self, id: Uuid, mut profile: ProfileConfig) -> Result<(), io::Error> {
        let Some(existing_idx) = self.profiles.iter().position(|p| p.id == id) else {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("profile {id} not found"),
            ));
        };

        profile.id = id;
        if self.profiles[existing_idx].builtin {
            profile.builtin = true;
        }

        let entry = LogEntry::Update {
            profile: profile.clone(),
        };
        self.append_log(&entry)?;
        self.profiles[existing_idx] = profile;
        Ok(())
    }

    pub fn delete(&mut self, id: Uuid) -> Result<(), io::Error> {
        let Some(existing) = self.profiles.iter().find(|p| p.id == id) else {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("profile {id} not found"),
            ));
        };
        if existing.builtin {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "cannot delete builtin profile",
            ));
        }

        self.append_log(&LogEntry::Delete { id })?;
        self.profiles.retain(|p| p.id != id);
        Ok(())
    }

    pub fn compact(&mut self) -> Result<(), io::Error> {
        let snapshot_path = self.dir.join("profiles.json");
        let data = serde_json::to_vec_pretty(&self.profiles).map_err(io::Error::other)?;
        fs::write(&snapshot_path, data)?;

        let log_path = self.dir.join("profiles.log");
        OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&log_path)?;

        Ok(())
    }

    fn append_log(&self, entry: &LogEntry) -> Result<(), io::Error> {
        let log_path = self.dir.join("profiles.log");
        let mut file = OpenOptions::new()
            .append(true)
            .create(true)
            .open(&log_path)?;

        let mut line = serde_json::to_string(entry).map_err(io::Error::other)?;
        line.push('\n');
        file.write_all(line.as_bytes())
    }

    fn apply_entry(profiles: &mut Vec<ProfileConfig>, entry: LogEntry) {
        match entry {
            LogEntry::Add { profile } => profiles.push(profile),
            LogEntry::Update { profile } => {
                if let Some(existing) = profiles.iter_mut().find(|p| p.id == profile.id) {
                    *existing = profile;
                }
            }
            LogEntry::Delete { id } => {
                profiles.retain(|p| p.id != id);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn temp_store() -> (TempDir, ProfileStore) {
        let dir = TempDir::new().expect("tempdir");
        let store = ProfileStore::open(dir.path()).expect("open profile store");
        (dir, store)
    }

    #[test]
    fn loads_default_profiles() {
        let (_dir, store) = temp_store();
        assert_eq!(store.list().len(), 4);
        assert_eq!(store.list()[0].name, "Gaming");
        assert_eq!(store.list()[1].name, "Coding");
    }

    #[test]
    fn get_by_name() {
        let (_dir, store) = temp_store();
        let gaming = store.get_by_name("Gaming").expect("gaming profile exists");
        assert_eq!(gaming.max_fps, 120);
        assert!(store.get_by_name("Nonexistent").is_none());
    }

    #[test]
    fn update_persists() {
        let (dir, mut store) = temp_store();
        let id = store.list()[0].id;
        let mut updated = store.get(id).expect("profile").clone();
        updated.bitrate_bps = 50_000_000;
        store.update(id, updated).expect("update");

        let store2 = ProfileStore::open(dir.path()).expect("reopen");
        assert_eq!(
            store2.get(id).expect("profile after reopen").bitrate_bps,
            50_000_000
        );
    }

    #[test]
    fn add_custom_profile() {
        let (_dir, mut store) = temp_store();
        let custom = ProfileConfig {
            id: Uuid::now_v7(),
            name: "Custom".to_string(),
            builtin: false,
            bitrate_bps: 20_000_000,
            max_fps: 90,
            encoder_preset: EncoderPreset::Balanced,
            prefer_native_scaling: true,
            audio_mode: AudioMode::Stereo,
            prefer_av1: true,
            exclusive_input: false,
            touch_mode: false,
            auto_reconnect: true,
        };
        let id = store.add(custom).expect("add custom");
        assert_eq!(store.list().len(), 5);
        assert_eq!(store.get(id).expect("custom by id").name, "Custom");
    }

    #[test]
    fn delete_custom_profile() {
        let (_dir, mut store) = temp_store();
        let custom = ProfileConfig {
            id: Uuid::now_v7(),
            name: "Temp".to_string(),
            builtin: false,
            bitrate_bps: 10_000_000,
            max_fps: 30,
            encoder_preset: EncoderPreset::Balanced,
            prefer_native_scaling: false,
            audio_mode: AudioMode::VoiceOptimized,
            prefer_av1: false,
            exclusive_input: false,
            touch_mode: false,
            auto_reconnect: false,
        };
        let id = store.add(custom).expect("add custom");
        assert_eq!(store.list().len(), 5);
        store.delete(id).expect("delete custom");
        assert_eq!(store.list().len(), 4);
    }

    #[test]
    fn cannot_delete_builtin() {
        let (_dir, mut store) = temp_store();
        let id = store.list()[0].id;
        let result = store.delete(id);
        assert!(result.is_err());
        assert_eq!(store.list().len(), 4);
    }

    #[test]
    fn compact_and_reload() {
        let (dir, mut store) = temp_store();
        let id = store.list()[0].id;
        let mut updated = store.get(id).expect("profile").clone();
        updated.bitrate_bps = 42_000_000;
        store.update(id, updated).expect("update");
        store.compact().expect("compact");

        let store2 = ProfileStore::open(dir.path()).expect("reopen");
        assert_eq!(
            store2.get(id).expect("profile after compact").bitrate_bps,
            42_000_000
        );
    }
}
