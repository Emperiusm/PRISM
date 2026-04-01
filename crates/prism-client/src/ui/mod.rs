// SPDX-License-Identifier: AGPL-3.0-or-later
//! UI state machine and widget system for launcher and in-session overlay.

pub mod launcher;
pub mod overlay;
pub mod widgets;

use crate::config::LaunchMode;

// ---------------------------------------------------------------------------
// UiState
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiState {
    Launcher,
    Connecting,
    Stream,
    Overlay,
}

impl UiState {
    /// Choose the appropriate initial state based on how the client was launched.
    pub fn initial(mode: LaunchMode) -> Self {
        match mode {
            LaunchMode::Launcher => UiState::Launcher,
            LaunchMode::DirectConnect => UiState::Connecting,
        }
    }

    /// True when the remote stream surface should be rendered.
    pub fn shows_stream(self) -> bool {
        matches!(self, UiState::Stream | UiState::Overlay)
    }

    /// True when the in-session overlay panels are visible.
    pub fn shows_overlay(self) -> bool {
        matches!(self, UiState::Overlay)
    }

    /// True when the launcher / connect-progress UI is visible.
    pub fn shows_launcher(self) -> bool {
        matches!(self, UiState::Launcher | UiState::Connecting)
    }

    /// True when keyboard/mouse events should be forwarded to the remote host.
    pub fn forwards_input(self) -> bool {
        matches!(self, UiState::Stream)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_state_from_launch_mode() {
        assert_eq!(UiState::initial(LaunchMode::Launcher), UiState::Launcher);
        assert_eq!(
            UiState::initial(LaunchMode::DirectConnect),
            UiState::Connecting
        );
    }

    #[test]
    fn stream_visible_in_correct_states() {
        assert!(!UiState::Launcher.shows_stream());
        assert!(!UiState::Connecting.shows_stream());
        assert!(UiState::Stream.shows_stream());
        assert!(UiState::Overlay.shows_stream());
    }

    #[test]
    fn input_forwarded_only_in_stream() {
        assert!(!UiState::Launcher.forwards_input());
        assert!(!UiState::Connecting.forwards_input());
        assert!(UiState::Stream.forwards_input());
        assert!(!UiState::Overlay.forwards_input());
    }
}
