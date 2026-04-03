// SPDX-License-Identifier: AGPL-3.0-or-later
//! Launcher mode - quick connect, saved connections, profiles, and settings.

pub mod card_grid;
pub mod nav;
pub mod profiles;
pub mod quick_connect;
pub mod recent_list;
pub mod server_card;
pub mod server_form;
pub mod settings;
pub mod shell;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LauncherTab {
    Home,
    SavedConnections,
    Profiles,
    Settings,
}

impl LauncherTab {
    pub const PRIMARY: [LauncherTab; 3] = [
        LauncherTab::Home,
        LauncherTab::SavedConnections,
        LauncherTab::Profiles,
    ];

    pub fn label(self) -> &'static str {
        match self {
            LauncherTab::Home => "Home",
            LauncherTab::SavedConnections => "Saved Connections",
            LauncherTab::Profiles => "Profiles",
            LauncherTab::Settings => "Settings",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            LauncherTab::Home => "Home",
            LauncherTab::SavedConnections => "Saved Connections",
            LauncherTab::Profiles => "Profiles",
            LauncherTab::Settings => "Settings",
        }
    }

    pub fn subtitle(self) -> &'static str {
        match self {
            LauncherTab::Home => "Connect instantly or reopen the desktops you use most.",
            LauncherTab::SavedConnections => {
                "Browse saved desktops, reconnect quickly, and keep your machines organized."
            }
            LauncherTab::Profiles => {
                "Tune presets for gaming, coding, balanced work, and low-bandwidth sessions."
            }
            LauncherTab::Settings => {
                "Review identity, defaults, input behavior, and local client preferences."
            }
        }
    }
}

/// Settings sub-sections shown when the Settings tab is active.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsSection {
    General,
    IdentitySecurity,
    Streaming,
    Input,
    Audio,
}

impl SettingsSection {
    pub const ALL: [SettingsSection; 5] = [
        SettingsSection::General,
        SettingsSection::IdentitySecurity,
        SettingsSection::Streaming,
        SettingsSection::Input,
        SettingsSection::Audio,
    ];

    pub fn label(self) -> &'static str {
        match self {
            SettingsSection::General => "General",
            SettingsSection::IdentitySecurity => "Identity & Security",
            SettingsSection::Streaming => "Streaming",
            SettingsSection::Input => "Input",
            SettingsSection::Audio => "Audio",
        }
    }
}

/// Which modal is active on top of the launcher.
#[derive(Debug, Clone)]
pub enum ActiveModal {
    ServerForm { mode: FormMode },
    ConfirmDelete { server_id: uuid::Uuid, name: String },
}

/// Add vs edit mode for the server form.
#[derive(Debug, Clone)]
pub enum FormMode {
    Add,
    Edit { server_id: uuid::Uuid },
}
