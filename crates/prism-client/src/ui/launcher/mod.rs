// SPDX-License-Identifier: AGPL-3.0-or-later
//! Launcher mode - quick connect, saved connections, profiles, and settings.

pub mod card_grid;
pub mod nav;
pub mod profiles;
pub mod quick_connect;
pub mod server_card;
pub mod server_form;
pub mod settings;

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
