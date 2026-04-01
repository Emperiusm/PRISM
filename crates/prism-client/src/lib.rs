// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2025-2026 Ehsan Khalid. All rights reserved.
// Licensed under the GNU Affero General Public License v3.0.
// Commercial licensing available — see LICENSE-COMMERCIAL.md.

pub mod app;
pub mod session_bridge;
pub mod audio_player;
pub mod connector;
pub mod cursor_renderer;
pub mod frame_receiver;
pub mod input_sender;

pub mod renderer;
pub mod ui;
pub mod input;
pub mod config;

// Re-exports
pub use audio_player::AdaptiveJitterBuffer;
pub use connector::{ClientConnector, TlsMode};
pub use cursor_renderer::CursorPredictor;
pub use frame_receiver::{FrameStats, parse_display_datagram};
pub use input_sender::{InputSender, normalize_mouse};
