// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2025-2026 Ehsan Khalid. All rights reserved.
// Licensed under the GNU Affero General Public License v3.0.
// Commercial licensing available — see LICENSE-COMMERCIAL.md.

pub mod audio_player;
pub mod client_app;
pub mod connector;
pub mod cursor_renderer;
pub mod frame_receiver;
pub mod input_sender;

// Re-exports
pub use audio_player::AdaptiveJitterBuffer;
pub use client_app::{ClientApp, ClientConfig};
pub use connector::{ClientConnector, TlsMode};
pub use cursor_renderer::CursorPredictor;
pub use frame_receiver::{FrameStats, parse_display_datagram};
pub use input_sender::{InputSender, normalize_mouse};
