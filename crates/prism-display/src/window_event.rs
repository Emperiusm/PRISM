// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2025-2026 Ehsan Khalid. All rights reserved.
// Licensed under the GNU Affero General Public License v3.0.
// Commercial licensing available — see LICENSE-COMMERCIAL.md.

/// Events emitted by the window management layer describing state changes that
/// the encode pipeline should react to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WindowEvent {
    /// A different window moved to the foreground.
    ForegroundChanged { hwnd: u64 },
    /// The window started minimising.
    MinimizeStart { hwnd: u64 },
    /// The window finished restoring from minimised state.
    MinimizeEnd { hwnd: u64 },
    /// The user finished resizing or moving the window.
    MoveSizeEnd { hwnd: u64 },
    /// The window was closed / destroyed.
    WindowDestroyed { hwnd: u64 },
}

impl WindowEvent {
    /// True when this event should cause the encoder to emit a speculative IDR
    /// on the next frame to minimise visual artefacts.
    pub fn triggers_speculative_idr(&self) -> bool {
        matches!(
            self,
            WindowEvent::ForegroundChanged { .. } | WindowEvent::MinimizeEnd { .. }
        )
    }

    /// Extract the window handle associated with this event.
    pub fn hwnd(&self) -> u64 {
        match self {
            WindowEvent::ForegroundChanged { hwnd }
            | WindowEvent::MinimizeStart { hwnd }
            | WindowEvent::MinimizeEnd { hwnd }
            | WindowEvent::MoveSizeEnd { hwnd }
            | WindowEvent::WindowDestroyed { hwnd } => *hwnd,
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn foreground_triggers_speculative() {
        let ev = WindowEvent::ForegroundChanged { hwnd: 0x1000 };
        assert!(ev.triggers_speculative_idr());

        let ev = WindowEvent::MinimizeEnd { hwnd: 0x2000 };
        assert!(ev.triggers_speculative_idr());
    }

    #[test]
    fn move_size_does_not() {
        let ev = WindowEvent::MoveSizeEnd { hwnd: 0x3000 };
        assert!(!ev.triggers_speculative_idr());

        let ev = WindowEvent::MinimizeStart { hwnd: 0x4000 };
        assert!(!ev.triggers_speculative_idr());

        let ev = WindowEvent::WindowDestroyed { hwnd: 0x5000 };
        assert!(!ev.triggers_speculative_idr());
    }

    #[test]
    fn hwnd_extraction() {
        let events = [
            WindowEvent::ForegroundChanged { hwnd: 0xAAAA },
            WindowEvent::MinimizeStart { hwnd: 0xBBBB },
            WindowEvent::MinimizeEnd { hwnd: 0xCCCC },
            WindowEvent::MoveSizeEnd { hwnd: 0xDDDD },
            WindowEvent::WindowDestroyed { hwnd: 0xEEEE },
        ];
        let expected = [0xAAAA_u64, 0xBBBB, 0xCCCC, 0xDDDD, 0xEEEE];
        for (ev, &exp) in events.iter().zip(expected.iter()) {
            assert_eq!(ev.hwnd(), exp, "hwnd mismatch for {:?}", ev);
        }
    }
}
