// SPDX-License-Identifier: AGPL-3.0-or-later
//! User preferences persisted as JSON.

use std::{fs, io, path::Path};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserPrefs {
    pub default_profile: String,
    pub exclusive_keyboard: bool,
    pub relative_mouse: bool,
}

impl Default for UserPrefs {
    fn default() -> Self {
        Self {
            default_profile: "Balanced".to_string(),
            exclusive_keyboard: true,
            relative_mouse: false,
        }
    }
}

impl UserPrefs {
    pub fn load(dir: &Path) -> Self {
        let path = dir.join("user_prefs.json");
        if path.exists()
            && let Ok(data) = fs::read(&path)
            && let Ok(prefs) = serde_json::from_slice::<UserPrefs>(&data)
        {
            return prefs;
        }
        Self::default()
    }

    pub fn save(&self, dir: &Path) -> Result<(), io::Error> {
        fs::create_dir_all(dir)?;
        let path = dir.join("user_prefs.json");
        let data = serde_json::to_string_pretty(self).map_err(io::Error::other)?;
        fs::write(&path, data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn default_prefs() {
        let prefs = UserPrefs::default();
        assert_eq!(prefs.default_profile, "Balanced");
        assert!(prefs.exclusive_keyboard);
        assert!(!prefs.relative_mouse);
    }

    #[test]
    fn save_and_load() {
        let dir = TempDir::new().expect("tempdir");
        let prefs = UserPrefs {
            default_profile: "Gaming".to_string(),
            exclusive_keyboard: false,
            ..UserPrefs::default()
        };
        prefs.save(dir.path()).expect("save prefs");

        let loaded = UserPrefs::load(dir.path());
        assert_eq!(loaded.default_profile, "Gaming");
        assert!(!loaded.exclusive_keyboard);
    }

    #[test]
    fn missing_file_returns_default() {
        let dir = TempDir::new().expect("tempdir");
        let loaded = UserPrefs::load(dir.path());
        assert_eq!(loaded.default_profile, "Balanced");
    }
}
