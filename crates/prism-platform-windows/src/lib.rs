// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2025-2026 Ehsan Khalid. All rights reserved.
// Licensed under the GNU Affero General Public License v3.0.
// Commercial licensing available — see LICENSE-COMMERCIAL.md.

//! Windows-specific platform implementations for PRISM.
//!
//! Provides:
//! - DDA (Desktop Duplication API) screen capture
//! - NVENC hardware video encoding
//! - DXGI monitor enumeration
//! - GPU texture pool management

pub mod d3d;
pub mod damage;
pub mod dda;
pub mod error;
pub mod monitor;
pub mod nvenc;
pub mod texture;

// ── error ─────────────────────────────────────────────────────────────────────
pub use error::PlatformError;
pub use error::from_hresult;

// ── d3d ───────────────────────────────────────────────────────────────────────
pub use d3d::{D3DDeviceConfig, D3DFeatureLevel};

// ── texture ───────────────────────────────────────────────────────────────────
pub use texture::{TexturePool, TexturePoolConfig, TextureSlot};

// ── monitor ───────────────────────────────────────────────────────────────────
pub use monitor::{DxgiOutputInfo, OutputRotation, parse_display_id};

// ── damage ────────────────────────────────────────────────────────────────────
pub use damage::{DxgiRect, extract_damage, is_full_damage};

// ── dda ───────────────────────────────────────────────────────────────────────
pub use dda::{DdaCapture, DdaCaptureConfig, DdaCaptureState};

// ── nvenc ─────────────────────────────────────────────────────────────────────
pub use nvenc::config::{NvencConfig, RateControlMode, TuningMode};
pub use nvenc::format::NvencBufferFormat;
pub use nvenc::session::{EncodeResult, NvencSessionState, NvencStats};
