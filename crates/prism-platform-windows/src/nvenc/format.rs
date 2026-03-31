//! Pixel format conversions for NVENC.

use prism_display::TextureFormat;

// ── NvencBufferFormat ─────────────────────────────────────────────────────────

/// NVENC input buffer format, mapping from PRISM texture formats to the
/// constants expected by the NVENC API (`NV_ENC_BUFFER_FORMAT_*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NvencBufferFormat {
    /// YUV 4:2:0, NV12 layout (8-bit).
    Nv12,
    /// 32-bit ARGB (alpha, red, green, blue — little-endian).
    Argb,
    /// 32-bit ABGR (alpha, blue, green, red — little-endian).
    Abgr,
    /// YUV 4:2:0, P010 layout (10-bit packed in 16-bit words).
    P010,
}

impl NvencBufferFormat {
    /// Convert a PRISM [`TextureFormat`] to the matching NVENC buffer format.
    ///
    /// Returns `None` when no NVENC format corresponds to the given texture
    /// format (e.g. future formats not yet supported).
    pub fn from_texture_format(fmt: TextureFormat) -> Option<Self> {
        match fmt {
            TextureFormat::Nv12  => Some(Self::Nv12),
            TextureFormat::Bgra8 => Some(Self::Argb),
            TextureFormat::P010  => Some(Self::P010),
        }
    }

    /// Raw `NV_ENC_BUFFER_FORMAT` integer value expected by the NVENC API.
    pub fn to_nvenc_value(self) -> u32 {
        match self {
            Self::Nv12 => 1,
            Self::Argb => 10,
            Self::Abgr => 16,
            Self::P010 => 24,
        }
    }

    /// Bytes per pixel for this format.
    ///
    /// NV12 and P010 are planar YUV 4:2:0 formats, so the effective number of
    /// bytes per luma pixel is 1.5 and 3.0 respectively (including chroma
    /// overhead).  ARGB / ABGR are 4 bytes per pixel.
    pub fn bytes_per_pixel(self) -> f64 {
        match self {
            Self::Nv12        => 1.5,
            Self::Argb | Self::Abgr => 4.0,
            Self::P010        => 3.0,
        }
    }

    /// Minimum input buffer size in bytes required to hold a `width × height`
    /// frame in this format.
    pub fn buffer_size(self, width: u32, height: u32) -> usize {
        (width as f64 * height as f64 * self.bytes_per_pixel()) as usize
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nv12_from_texture() {
        let fmt = NvencBufferFormat::from_texture_format(TextureFormat::Nv12);
        assert_eq!(fmt, Some(NvencBufferFormat::Nv12));
    }

    #[test]
    fn bgra_maps_to_argb() {
        let fmt = NvencBufferFormat::from_texture_format(TextureFormat::Bgra8);
        assert_eq!(fmt, Some(NvencBufferFormat::Argb));
    }

    #[test]
    fn buffer_size_nv12_1080p() {
        // 1920 * 1080 * 1.5 = 3_110_400
        let size = NvencBufferFormat::Nv12.buffer_size(1920, 1080);
        assert_eq!(size, 3_110_400);
    }

    #[test]
    fn buffer_size_argb_4k() {
        // 3840 * 2160 * 4.0 = 33_177_600
        let size = NvencBufferFormat::Argb.buffer_size(3840, 2160);
        assert_eq!(size, 33_177_600);
    }

    #[test]
    fn nvenc_format_values() {
        assert_eq!(NvencBufferFormat::Nv12.to_nvenc_value(), 1);
        assert_eq!(NvencBufferFormat::Argb.to_nvenc_value(), 10);
        assert_eq!(NvencBufferFormat::Abgr.to_nvenc_value(), 16);
        assert_eq!(NvencBufferFormat::P010.to_nvenc_value(), 24);
    }
}
