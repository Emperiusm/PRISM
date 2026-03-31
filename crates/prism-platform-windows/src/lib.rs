//! Windows-specific platform implementations for PRISM.
//!
//! Provides:
//! - DDA (Desktop Duplication API) screen capture
//! - NVENC hardware video encoding
//! - DXGI monitor enumeration
//! - GPU texture pool management

pub mod error;
pub mod d3d;
pub mod texture;
pub mod dda;
pub mod monitor;
pub mod damage;
pub mod nvenc;

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
