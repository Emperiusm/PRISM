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

pub use error::PlatformError;
