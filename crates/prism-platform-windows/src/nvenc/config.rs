//! NVENC encoder configuration types.

use prism_display::{CodecId, TextureFormat};

// ── RateControlMode ───────────────────────────────────────────────────────────

/// NVENC rate-control algorithm selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RateControlMode {
    /// Constant bit rate — maintains target bitrate at all times.
    Cbr,
    /// Variable bit rate — allows fluctuation around the target.
    Vbr,
    /// Constant quantization parameter — quality-driven, ignores bitrate.
    ConstQp,
    /// Fully lossless encode; ignores all bitrate/QP settings.
    Lossless,
}

// ── TuningMode ────────────────────────────────────────────────────────────────

/// NVENC encoder tuning preset controlling the latency/quality trade-off.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TuningMode {
    /// Minimize encode latency above all else (best for interactive streaming).
    UltraLowLatency,
    /// Balance between low latency and acceptable quality.
    LowLatency,
    /// Maximize quality at the cost of higher latency (for recording).
    HighQuality,
    /// Lossless tuning mode — pair with `RateControlMode::Lossless`.
    Lossless,
}

// ── NvencConfig ───────────────────────────────────────────────────────────────

/// Full configuration for an NVENC encode session.
///
/// Use the preset constructors ([`ultra_low_latency`], [`lossless`]) as
/// starting points and then fine-tune with the builder methods.
///
/// [`ultra_low_latency`]: NvencConfig::ultra_low_latency
/// [`lossless`]: NvencConfig::lossless
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NvencConfig {
    /// Video codec to use.
    pub codec: CodecId,
    /// Encode width in pixels.
    pub width: u32,
    /// Encode height in pixels.
    pub height: u32,
    /// Maximum encode frame rate in frames per second.
    pub max_fps: u8,
    /// Target encode bitrate in bits per second.
    pub bitrate_bps: u64,
    /// Peak / maximum bitrate allowed by the encoder.
    pub max_bitrate_bps: u64,
    /// Tuning mode (latency/quality knob).
    pub tuning: TuningMode,
    /// Rate-control algorithm.
    pub rate_control: RateControlMode,
    /// Number of B-frames (0 = no B-frames, required for low-latency).
    pub b_frames: u8,
    /// Lookahead frame count (0 = disabled, required for low-latency).
    pub lookahead: u8,
    /// GOP length in frames (`u32::MAX` = infinite GOP, i.e. keyframe only on demand).
    pub gop_length: u32,
    /// Minimum quantizer parameter (lower = higher quality).
    pub min_qp: u8,
    /// Maximum quantizer parameter (higher = lower quality).
    pub max_qp: u8,
    /// Number of slices per encoded frame (1 = no slicing).
    pub slice_count: u8,
    /// Pixel format of the input texture.
    pub input_format: TextureFormat,
}

impl NvencConfig {
    // ── Preset constructors ───────────────────────────────────────────────────

    /// Preset optimised for interactive streaming: minimal latency, CBR, no
    /// B-frames, infinite GOP, NV12 input.
    ///
    /// `max_bitrate_bps` is set to `bitrate_bps * 2` to absorb scene bursts.
    pub fn ultra_low_latency(
        codec: CodecId,
        width: u32,
        height: u32,
        bitrate_bps: u64,
    ) -> Self {
        Self {
            codec,
            width,
            height,
            max_fps: 60,
            bitrate_bps,
            max_bitrate_bps: bitrate_bps * 2,
            tuning: TuningMode::UltraLowLatency,
            rate_control: RateControlMode::Cbr,
            b_frames: 0,
            lookahead: 0,
            gop_length: u32::MAX,
            min_qp: 18,
            max_qp: 51,
            slice_count: 1,
            input_format: TextureFormat::Nv12,
        }
    }

    /// Preset for lossless capture: zero bitrate, BGRA8 input, lossless
    /// rate-control and tuning.
    pub fn lossless(codec: CodecId, width: u32, height: u32) -> Self {
        Self {
            codec,
            width,
            height,
            max_fps: 60,
            bitrate_bps: 0,
            max_bitrate_bps: 0,
            tuning: TuningMode::Lossless,
            rate_control: RateControlMode::Lossless,
            b_frames: 0,
            lookahead: 0,
            gop_length: u32::MAX,
            min_qp: 0,
            max_qp: 51,
            slice_count: 1,
            input_format: TextureFormat::Bgra8,
        }
    }

    // ── Builder methods ───────────────────────────────────────────────────────

    /// Override the number of slices per frame.
    pub fn with_slices(mut self, count: u8) -> Self {
        self.slice_count = count;
        self
    }

    /// Override the maximum frame rate.
    pub fn with_max_fps(mut self, fps: u8) -> Self {
        self.max_fps = fps;
        self
    }

    // ── Runtime change detection ──────────────────────────────────────────────

    /// Returns `true` when changing from `self` to `other` requires tearing
    /// down and re-initialising the encoder session.
    ///
    /// Structural changes — codec, resolution, pixel format, or slice count —
    /// all require reinitialisation.
    pub fn needs_reinit(&self, other: &NvencConfig) -> bool {
        self.codec        != other.codec
            || self.width       != other.width
            || self.height      != other.height
            || self.input_format != other.input_format
            || self.slice_count != other.slice_count
    }

    /// Returns `true` when the only difference between `self` and `other` is
    /// the bitrate / QP settings (a live bitrate update, no reinit needed).
    pub fn is_bitrate_change_only(&self, other: &NvencConfig) -> bool {
        !self.needs_reinit(other)
            && (self.bitrate_bps     != other.bitrate_bps
                || self.max_bitrate_bps != other.max_bitrate_bps
                || self.min_qp          != other.min_qp
                || self.max_qp          != other.max_qp)
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn ull() -> NvencConfig {
        NvencConfig::ultra_low_latency(CodecId::H264, 1920, 1080, 10_000_000)
    }

    #[test]
    fn ultra_low_latency_preset() {
        let cfg = ull();
        assert_eq!(cfg.codec, CodecId::H264);
        assert_eq!(cfg.width, 1920);
        assert_eq!(cfg.height, 1080);
        assert_eq!(cfg.bitrate_bps, 10_000_000);
        assert_eq!(cfg.max_bitrate_bps, 20_000_000);
        assert_eq!(cfg.b_frames, 0);
        assert_eq!(cfg.lookahead, 0);
        assert_eq!(cfg.gop_length, u32::MAX);
        assert_eq!(cfg.rate_control, RateControlMode::Cbr);
        assert_eq!(cfg.tuning, TuningMode::UltraLowLatency);
        assert_eq!(cfg.input_format, TextureFormat::Nv12);
        assert_eq!(cfg.min_qp, 18);
        assert_eq!(cfg.max_qp, 51);
        assert_eq!(cfg.slice_count, 1);
        assert_eq!(cfg.max_fps, 60);
    }

    #[test]
    fn lossless_preset() {
        let cfg = NvencConfig::lossless(CodecId::H265, 2560, 1440);
        assert_eq!(cfg.codec, CodecId::H265);
        assert_eq!(cfg.rate_control, RateControlMode::Lossless);
        assert_eq!(cfg.tuning, TuningMode::Lossless);
        assert_eq!(cfg.bitrate_bps, 0);
        assert_eq!(cfg.input_format, TextureFormat::Bgra8);
    }

    #[test]
    fn with_slices() {
        let cfg = ull().with_slices(4);
        assert_eq!(cfg.slice_count, 4);
    }

    #[test]
    fn with_max_fps() {
        let cfg = ull().with_max_fps(120);
        assert_eq!(cfg.max_fps, 120);
    }

    #[test]
    fn needs_reinit_on_resolution() {
        let a = ull();
        let b = NvencConfig::ultra_low_latency(CodecId::H264, 2560, 1440, 10_000_000);
        assert!(a.needs_reinit(&b));
    }

    #[test]
    fn no_reinit_on_bitrate() {
        let a = ull();
        let b = NvencConfig { bitrate_bps: 5_000_000, max_bitrate_bps: 10_000_000, ..ull() };
        assert!(!a.needs_reinit(&b));
        assert!(a.is_bitrate_change_only(&b));
    }

    #[test]
    fn reinit_on_codec() {
        let a = ull();
        let b = NvencConfig::ultra_low_latency(CodecId::H265, 1920, 1080, 10_000_000);
        assert!(a.needs_reinit(&b));
        assert!(!a.is_bitrate_change_only(&b));
    }

    #[test]
    fn reinit_on_format() {
        let a = ull();
        let b = NvencConfig { input_format: TextureFormat::P010, ..ull() };
        assert!(a.needs_reinit(&b));
    }
}
