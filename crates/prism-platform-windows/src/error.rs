//! Platform-level error types for the Windows capture/encode pipeline.

use thiserror::Error;
use prism_display::{CaptureError, DisplayId, TextureFormat};

/// Errors originating from Windows platform subsystems (DXGI, D3D11, NVENC).
#[derive(Debug, Error)]
pub enum PlatformError {
    /// A Windows HRESULT failure code that did not map to a named variant.
    #[error("windows HRESULT error: 0x{0:08X}")]
    HResult(u32),

    /// The D3D/DXGI device was lost and must be recreated.
    #[error("D3D device lost")]
    DeviceLost,

    /// Desktop duplication access was lost (e.g. UAC dialog, secure desktop).
    #[error("desktop duplication access lost")]
    AccessLost,

    /// A timed-out wait on the duplication acquire call.
    #[error("duplication acquire timed out")]
    WaitTimeout,

    /// No suitable DXGI adapter was found.
    #[error("no suitable DXGI adapter found")]
    NoAdapter,

    /// The requested DXGI output (monitor) index does not exist.
    #[error("no DXGI output at index {0}")]
    NoOutput(u32),

    /// D3D11 device creation failed.
    #[error("D3D11 device creation failed")]
    DeviceCreationFailed,

    /// The current OS/driver does not support Desktop Duplication.
    #[error("desktop duplication is not supported on this system")]
    DuplicationNotSupported,

    /// The texture pool has no free slots.
    #[error("texture pool exhausted: all {0} slots in use")]
    PoolExhausted(usize),

    /// NVENC returned a driver-level error.
    #[error("NVENC error: {0}")]
    NvencError(String),

    /// NVENC is not available on this system (no NVIDIA GPU or old driver).
    #[error("NVENC is not available on this system")]
    NvencNotAvailable,

    /// A pixel format not supported by the requested operation.
    #[error("unsupported texture format: {0:?}")]
    UnsupportedFormat(TextureFormat),

    /// Catch-all for errors that don't fit a specific variant.
    #[error("platform error: {0}")]
    Other(String),
}

// ── HRESULT conversion ────────────────────────────────────────────────────────

/// Map a Windows HRESULT to the most descriptive `PlatformError` variant.
///
/// Known DXGI/D3D HRESULTs:
/// - `0x887A0005` (`DXGI_ERROR_DEVICE_REMOVED`) → [`PlatformError::DeviceLost`]
/// - `0x887A0026` (`DXGI_ERROR_DEVICE_RESET`)   → [`PlatformError::DeviceLost`]
/// - `0x887A0021` (`DXGI_ERROR_ACCESS_LOST`)    → [`PlatformError::AccessLost`]
/// - `0x887A0027` (`DXGI_ERROR_WAIT_TIMEOUT`)   → [`PlatformError::WaitTimeout`]
/// - `0x887A0022` (`DXGI_ERROR_NOT_CURRENTLY_AVAILABLE`) → [`PlatformError::DuplicationNotSupported`]
/// - All others                                 → [`PlatformError::HResult`]
pub fn from_hresult(hr: u32) -> PlatformError {
    match hr {
        0x887A_0005 => PlatformError::DeviceLost,
        0x887A_0026 => PlatformError::DeviceLost,
        0x887A_0021 => PlatformError::AccessLost,
        0x887A_0027 => PlatformError::WaitTimeout,
        0x887A_0022 => PlatformError::DuplicationNotSupported,
        other       => PlatformError::HResult(other),
    }
}

impl PlatformError {
    /// Returns `true` for errors that are transient and the operation may be
    /// retried without recreating the device.
    pub fn is_recoverable(&self) -> bool {
        matches!(self, PlatformError::AccessLost | PlatformError::WaitTimeout)
    }

    /// Returns `true` when the D3D device is lost and the entire device stack
    /// must be torn down and rebuilt.
    pub fn is_device_lost(&self) -> bool {
        matches!(self, PlatformError::DeviceLost)
    }
}

// ── Conversion to prism_display::CaptureError ─────────────────────────────────

impl From<PlatformError> for CaptureError {
    fn from(err: PlatformError) -> CaptureError {
        match err {
            PlatformError::DuplicationNotSupported => CaptureError::Unsupported,
            PlatformError::NoOutput(idx) => CaptureError::DisplayNotFound(DisplayId(idx)),
            other => CaptureError::Failed(other.to_string()),
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_hresult_device_removed() {
        assert!(matches!(from_hresult(0x887A_0005), PlatformError::DeviceLost));
    }

    #[test]
    fn from_hresult_device_reset() {
        assert!(matches!(from_hresult(0x887A_0026), PlatformError::DeviceLost));
    }

    #[test]
    fn from_hresult_access_lost_and_timeout() {
        assert!(matches!(from_hresult(0x887A_0021), PlatformError::AccessLost));
        assert!(matches!(from_hresult(0x887A_0027), PlatformError::WaitTimeout));
    }

    #[test]
    fn from_hresult_unknown_becomes_hresult_variant() {
        let err = from_hresult(0xDEAD_BEEF);
        assert!(matches!(err, PlatformError::HResult(0xDEAD_BEEF)));
    }

    #[test]
    fn recoverable_and_device_lost_flags() {
        assert!(PlatformError::AccessLost.is_recoverable());
        assert!(PlatformError::WaitTimeout.is_recoverable());
        assert!(!PlatformError::DeviceLost.is_recoverable());
        assert!(PlatformError::DeviceLost.is_device_lost());
        assert!(!PlatformError::AccessLost.is_device_lost());
    }

    #[test]
    fn conversion_to_capture_error() {
        // DuplicationNotSupported → CaptureError::Unsupported
        let e: CaptureError = PlatformError::DuplicationNotSupported.into();
        assert!(matches!(e, CaptureError::Unsupported));

        // NoOutput(3) → CaptureError::DisplayNotFound(DisplayId(3))
        let e: CaptureError = PlatformError::NoOutput(3).into();
        assert!(matches!(e, CaptureError::DisplayNotFound(DisplayId(3))));

        // Other errors → CaptureError::Failed
        let e: CaptureError = PlatformError::DeviceLost.into();
        assert!(matches!(e, CaptureError::Failed(_)));

        let e: CaptureError = PlatformError::NvencNotAvailable.into();
        assert!(matches!(e, CaptureError::Failed(_)));
    }
}
