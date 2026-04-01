// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2025-2026 Ehsan Khalid. All rights reserved.
// Licensed under the GNU Affero General Public License v3.0.
// Commercial licensing available — see LICENSE-COMMERCIAL.md.

//! Hardware-accelerated H.264 encoding via FFmpeg.
//! Supports NVENC (NVIDIA), QSV (Intel), AMF (AMD) with automatic fallback.
//!
//! Enable with: `cargo run -p prism-server --features hwenc`
//!
//! Without the feature, the module still compiles and always returns the
//! software (openh264) backend.

#[cfg(feature = "hwenc")]
pub mod ffmpeg_encoder {
    //! Internal FFmpeg-based encoder state. Used only when `hwenc` is enabled.

    pub struct FfmpegEncoder {
        pub encoder: ffmpeg_next::codec::encoder::Video,
    }
}

// ── Backend enum ──────────────────────────────────────────────────────────────

/// Which hardware encoder backend is in use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HwEncoderBackend {
    /// NVIDIA GPU via NVENC.
    Nvenc,
    /// Intel iGPU/dGPU via Quick Sync Video.
    Qsv,
    /// AMD GPU via AMF.
    Amf,
    /// CPU software encoder (openh264).
    Software,
}

impl HwEncoderBackend {
    /// Human-readable name including the vendor label.
    pub fn name(&self) -> &'static str {
        match self {
            HwEncoderBackend::Nvenc => "h264_nvenc (NVIDIA)",
            HwEncoderBackend::Qsv => "h264_qsv (Intel)",
            HwEncoderBackend::Amf => "h264_amf (AMD)",
            HwEncoderBackend::Software => "openh264 (software)",
        }
    }

    /// FFmpeg codec name used to look up / open the encoder.
    pub fn ffmpeg_codec_name(&self) -> &'static str {
        match self {
            HwEncoderBackend::Nvenc => "h264_nvenc",
            HwEncoderBackend::Qsv => "h264_qsv",
            HwEncoderBackend::Amf => "h264_amf",
            HwEncoderBackend::Software => "libx264",
        }
    }
}

// ── Encoder detection ─────────────────────────────────────────────────────────

/// Probe for available H.264 encoders and return them in priority order.
///
/// Hardware backends are listed first (when the `hwenc` feature is enabled and
/// the corresponding FFmpeg encoder is found). `Software` is always appended
/// last as the guaranteed fallback.
pub fn detect_available_encoders() -> Vec<HwEncoderBackend> {
    #[cfg(not(feature = "hwenc"))]
    return vec![HwEncoderBackend::Software];

    #[cfg(feature = "hwenc")]
    {
        // Initialise FFmpeg once; ignore errors (may already be initialised).
        let _ = ffmpeg_next::init();

        let mut available: Vec<HwEncoderBackend> = [
            HwEncoderBackend::Nvenc,
            HwEncoderBackend::Qsv,
            HwEncoderBackend::Amf,
        ]
        .into_iter()
        .filter(|b| ffmpeg_next::encoder::find_by_name(b.ffmpeg_codec_name()).is_some())
        .collect();

        available.push(HwEncoderBackend::Software);
        available
    }
}

// ── HwEncoder ─────────────────────────────────────────────────────────────────

/// Unified H.264 encoder that selects the best available backend at construction
/// time and falls back gracefully to software (openh264) if hardware fails.
pub struct HwEncoder {
    backend: HwEncoderBackend,
    width: u32,
    height: u32,
    /// Current target bitrate in bits per second.
    bitrate_bps: u64,
    #[cfg(feature = "hwenc")]
    ffmpeg_encoder: Option<ffmpeg_encoder::FfmpegEncoder>,
    openh264_encoder: Option<openh264::encoder::Encoder>,
}

impl HwEncoder {
    /// Create a new encoder for frames of `width × height` pixels at `bitrate_bps`.
    ///
    /// The constructor probes for hardware encoders in priority order
    /// (NVENC → QSV → AMF → software) and uses the first one that opens
    /// successfully.
    pub fn new(
        width: u32,
        height: u32,
        bitrate_bps: u64,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let available = detect_available_encoders();
        // `available` always has at least one entry (Software).
        let preferred = available[0];

        match preferred {
            #[cfg(feature = "hwenc")]
            HwEncoderBackend::Nvenc | HwEncoderBackend::Qsv | HwEncoderBackend::Amf => {
                match Self::create_ffmpeg_encoder(preferred, width, height, bitrate_bps) {
                    Ok(enc) => Ok(Self {
                        backend: preferred,
                        width,
                        height,
                        bitrate_bps,
                        ffmpeg_encoder: Some(enc),
                        openh264_encoder: None,
                    }),
                    Err(e) => {
                        eprintln!(
                            "[HwEncoder] {} failed to open: {} — falling back to openh264",
                            preferred.name(),
                            e
                        );
                        Self::create_software(width, height, bitrate_bps)
                    }
                }
            }
            _ => Self::create_software(width, height, bitrate_bps),
        }
    }

    // ── private constructors ──────────────────────────────────────────────────

    fn create_software(
        width: u32,
        height: u32,
        bitrate_bps: u64,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let api = openh264::OpenH264API::from_source();
        let config = openh264::encoder::EncoderConfig::new()
            .max_frame_rate(openh264::encoder::FrameRate::from_hz(30.0))
            .bitrate(openh264::encoder::BitRate::from_bps(bitrate_bps as u32));
        let encoder = openh264::encoder::Encoder::with_api_config(api, config)?;
        Ok(Self {
            backend: HwEncoderBackend::Software,
            width,
            height,
            bitrate_bps,
            #[cfg(feature = "hwenc")]
            ffmpeg_encoder: None,
            openh264_encoder: Some(encoder),
        })
    }

    #[cfg(feature = "hwenc")]
    fn create_ffmpeg_encoder(
        backend: HwEncoderBackend,
        width: u32,
        height: u32,
        bitrate_bps: u64,
    ) -> Result<ffmpeg_encoder::FfmpegEncoder, Box<dyn std::error::Error>> {
        use ffmpeg_next::codec;
        use ffmpeg_next::format::Pixel;
        use ffmpeg_next::util::rational::Rational;

        let codec = codec::encoder::find_by_name(backend.ffmpeg_codec_name()).ok_or_else(|| {
            format!(
                "encoder '{}' not found in FFmpeg",
                backend.ffmpeg_codec_name()
            )
        })?;

        let context = codec::context::Context::new_with_codec(codec);
        let mut video = context.encoder().video()?;

        video.set_width(width);
        video.set_height(height);
        video.set_format(Pixel::YUV420P);
        video.set_time_base(Rational::new(1, 30));
        video.set_bit_rate(bitrate_bps as usize);
        video.set_gop(u32::MAX); // no periodic I-frames (infinite GOP for streaming)
        video.set_max_b_frames(0); // no B-frames — minimise latency

        let mut opts = ffmpeg_next::Dictionary::new();
        match backend {
            HwEncoderBackend::Nvenc => {
                opts.set("preset", "p1"); // fastest NVENC preset
                opts.set("tune", "ull"); // ultra-low-latency
                opts.set("zerolatency", "1");
                opts.set("rc", "cbr"); // constant bitrate
            }
            HwEncoderBackend::Qsv => {
                opts.set("preset", "veryfast");
                opts.set("low_power", "1");
            }
            HwEncoderBackend::Amf => {
                opts.set("usage", "ultralowlatency");
                opts.set("rc", "cbr");
            }
            _ => {}
        }

        let opened = video.open_with(opts)?;
        Ok(ffmpeg_encoder::FfmpegEncoder { encoder: opened })
    }

    // ── public API ────────────────────────────────────────────────────────────

    /// The backend currently in use.
    pub fn backend(&self) -> HwEncoderBackend {
        self.backend
    }

    /// The frame width this encoder was configured for.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// The frame height this encoder was configured for.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// The current target bitrate in bits per second.
    pub fn bitrate_bps(&self) -> u64 {
        self.bitrate_bps
    }

    /// Reconfigure the encoder bitrate at runtime.
    ///
    /// openh264 does not expose a runtime set_bitrate API, so this method
    /// recreates the encoder when the new bitrate differs by more than 20%.
    /// Small adjustments are stored and applied on the next recreation.
    pub fn set_bitrate(&mut self, bitrate_bps: u64) -> Result<(), Box<dyn std::error::Error>> {
        let old = self.bitrate_bps;
        self.bitrate_bps = bitrate_bps;
        let ratio = if old > 0 {
            bitrate_bps as f64 / old as f64
        } else {
            2.0
        };
        if !(0.8..=1.2).contains(&ratio) {
            tracing::info!(
                old_bps = old,
                new_bps = bitrate_bps,
                "encoder bitrate reconfigure"
            );
            *self = Self::create_software(self.width, self.height, bitrate_bps)?;
        }
        Ok(())
    }

    /// Encode a BGRA8 frame and return a raw H.264 bitstream.
    ///
    /// `bgra` must be exactly `width * height * 4` bytes.
    pub fn encode_bgra(&mut self, bgra: &[u8]) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        match self.backend {
            #[cfg(feature = "hwenc")]
            HwEncoderBackend::Nvenc | HwEncoderBackend::Qsv | HwEncoderBackend::Amf => {
                self.encode_ffmpeg(bgra)
            }
            _ => self.encode_openh264(bgra),
        }
    }

    // ── encoding back-ends ────────────────────────────────────────────────────

    fn encode_openh264(&mut self, bgra: &[u8]) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let encoder = self.openh264_encoder.as_mut().unwrap();
        let yuv = bgra_to_yuv420_raw(bgra, self.width as usize, self.height as usize);
        let yuv_buf =
            openh264::formats::YUVBuffer::from_vec(yuv, self.width as usize, self.height as usize);
        let bitstream = encoder.encode(&yuv_buf)?;
        Ok(bitstream.to_vec())
    }

    #[cfg(feature = "hwenc")]
    fn encode_ffmpeg(&mut self, bgra: &[u8]) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        use ffmpeg_next::codec;
        use ffmpeg_next::format::Pixel;

        let enc = self.ffmpeg_encoder.as_mut().unwrap();
        let w = self.width as usize;
        let h = self.height as usize;

        // Convert BGRA to raw I420 bytes.
        let yuv = bgra_to_yuv420_raw(bgra, w, h);

        let mut frame = ffmpeg_next::frame::Video::new(Pixel::YUV420P, self.width, self.height);
        let half_h = h / 2;
        let half_w = w / 2;

        // Copy Y plane (row-by-row to handle stride differences).
        let y_stride = frame.stride(0);
        for row in 0..h {
            let src = row * w;
            let dst = row * y_stride;
            frame.data_mut(0)[dst..dst + w].copy_from_slice(&yuv[src..src + w]);
        }

        // Copy U plane.
        let u_offset = w * h;
        let u_stride = frame.stride(1);
        for row in 0..half_h {
            let src = u_offset + row * half_w;
            let dst = row * u_stride;
            frame.data_mut(1)[dst..dst + half_w].copy_from_slice(&yuv[src..src + half_w]);
        }

        // Copy V plane.
        let v_offset = u_offset + half_w * half_h;
        let v_stride = frame.stride(2);
        for row in 0..half_h {
            let src = v_offset + row * half_w;
            let dst = row * v_stride;
            frame.data_mut(2)[dst..dst + half_w].copy_from_slice(&yuv[src..src + half_w]);
        }

        enc.encoder.send_frame(&frame)?;

        let mut output = Vec::new();
        let mut packet = codec::packet::Packet::empty();
        while enc.encoder.receive_packet(&mut packet).is_ok() {
            output.extend_from_slice(packet.data().unwrap_or(&[]));
        }

        Ok(output)
    }
}

// ── YUV conversion ────────────────────────────────────────────────────────────

/// Convert a BGRA8 frame to a packed I420 byte buffer (Y plane, then U, then V).
///
/// This is the raw-bytes variant used by both the FFmpeg path and for
/// constructing the `YUVBuffer` passed to openh264.
///
/// U and V chroma planes are subsampled 2×2 by sampling the top-left pixel of
/// each 2×2 block.
pub fn bgra_to_yuv420_raw(bgra: &[u8], width: usize, height: usize) -> Vec<u8> {
    let y_size = width * height;
    let uv_w = width.div_ceil(2);
    let uv_h = height.div_ceil(2);
    let uv_size = uv_w * uv_h;
    let mut yuv = vec![0u8; y_size + 2 * uv_size];

    // Y plane — full resolution.
    for row in 0..height {
        for col in 0..width {
            let src = (row * width + col) * 4;
            let b = bgra[src] as f32;
            let g = bgra[src + 1] as f32;
            let r = bgra[src + 2] as f32;
            yuv[row * width + col] = (0.299 * r + 0.587 * g + 0.114 * b).round() as u8;
        }
    }

    // U and V planes — half resolution (top-left pixel of each 2×2 block).
    for uv_row in 0..uv_h {
        for uv_col in 0..uv_w {
            let src_row = uv_row * 2;
            let src_col = uv_col * 2;
            let src = (src_row * width + src_col) * 4;
            let b = bgra[src] as f32;
            let g = bgra[src + 1] as f32;
            let r = bgra[src + 2] as f32;
            let u = (-0.169 * r - 0.331 * g + 0.500 * b + 128.0)
                .round()
                .clamp(0.0, 255.0) as u8;
            let v = (0.500 * r - 0.419 * g - 0.081 * b + 128.0)
                .round()
                .clamp(0.0, 255.0) as u8;
            yuv[y_size + uv_row * uv_w + uv_col] = u;
            yuv[y_size + uv_size + uv_row * uv_w + uv_col] = v;
        }
    }

    yuv
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── detection ─────────────────────────────────────────────────────────────

    #[test]
    fn detect_encoders_always_includes_software() {
        let available = detect_available_encoders();
        assert!(!available.is_empty());
        assert_eq!(*available.last().unwrap(), HwEncoderBackend::Software);
    }

    #[test]
    fn detect_encoders_software_is_always_last() {
        let available = detect_available_encoders();
        // Software is always appended last; there must be at least one entry.
        assert_eq!(available.last(), Some(&HwEncoderBackend::Software));
    }

    // ── names ─────────────────────────────────────────────────────────────────

    #[test]
    fn backend_names() {
        assert_eq!(HwEncoderBackend::Nvenc.name(), "h264_nvenc (NVIDIA)");
        assert_eq!(HwEncoderBackend::Qsv.name(), "h264_qsv (Intel)");
        assert_eq!(HwEncoderBackend::Amf.name(), "h264_amf (AMD)");
        assert_eq!(HwEncoderBackend::Software.name(), "openh264 (software)");
    }

    #[test]
    fn ffmpeg_codec_names() {
        assert_eq!(HwEncoderBackend::Nvenc.ffmpeg_codec_name(), "h264_nvenc");
        assert_eq!(HwEncoderBackend::Qsv.ffmpeg_codec_name(), "h264_qsv");
        assert_eq!(HwEncoderBackend::Amf.ffmpeg_codec_name(), "h264_amf");
        assert_eq!(HwEncoderBackend::Software.ffmpeg_codec_name(), "libx264");
    }

    // ── YUV conversion ────────────────────────────────────────────────────────

    #[test]
    fn bgra_to_yuv420_raw_correct_size() {
        let bgra = vec![0u8; 640 * 480 * 4];
        let yuv = bgra_to_yuv420_raw(&bgra, 640, 480);
        // Y: 640*480=307200, U: 320*240=76800, V: 320*240=76800
        let uv_w = (640 + 1) / 2;
        let uv_h = (480 + 1) / 2;
        assert_eq!(yuv.len(), 640 * 480 + 2 * uv_w * uv_h);
    }

    #[test]
    fn bgra_to_yuv420_raw_black_frame() {
        // All-black BGRA → Y=0, U=128, V=128.
        let bgra = vec![0u8; 4 * 4 * 4]; // 4×4 black frame
        let yuv = bgra_to_yuv420_raw(&bgra, 4, 4);
        let y_size = 4 * 4;
        // All Y values should be 0.
        assert!(yuv[..y_size].iter().all(|&v| v == 0));
        // U/V should be 128 (neutral chroma).
        assert!(yuv[y_size..].iter().all(|&v| v == 128));
    }

    #[test]
    fn bgra_to_yuv420_raw_white_frame() {
        // All-white BGRA (255,255,255,255) → Y≈255, U≈128, V≈128.
        let bgra = vec![255u8; 4 * 4 * 4];
        let yuv = bgra_to_yuv420_raw(&bgra, 4, 4);
        let y_size = 4 * 4;
        assert!(yuv[..y_size].iter().all(|&v| v == 255));
        // U and V: -0.169*255 - 0.331*255 + 0.500*255 + 128 ≈ 128
        assert!(yuv[y_size..].iter().all(|&v| v == 128));
    }

    // ── software encoder ──────────────────────────────────────────────────────

    #[test]
    fn software_encoder_creates() {
        let encoder = HwEncoder::new(320, 240, 1_000_000);
        assert!(encoder.is_ok());
        assert_eq!(encoder.unwrap().backend(), HwEncoderBackend::Software);
    }

    #[test]
    fn software_encoder_reports_dimensions() {
        let encoder = HwEncoder::new(640, 480, 2_000_000).unwrap();
        assert_eq!(encoder.width(), 640);
        assert_eq!(encoder.height(), 480);
    }

    #[test]
    fn software_encoder_encodes_frame() {
        let mut encoder = HwEncoder::new(320, 240, 1_000_000).unwrap();
        let bgra = vec![128u8; 320 * 240 * 4]; // mid-grey frame
        let result = encoder.encode_bgra(&bgra);
        assert!(
            result.is_ok(),
            "encode_bgra returned error: {:?}",
            result.err()
        );
        assert!(
            !result.unwrap().is_empty(),
            "encoded bitstream must not be empty"
        );
    }

    #[test]
    fn software_encoder_encodes_multiple_frames() {
        let mut encoder = HwEncoder::new(160, 120, 500_000).unwrap();
        for i in 0..5u8 {
            let bgra = vec![i.wrapping_mul(50); 160 * 120 * 4];
            let result = encoder.encode_bgra(&bgra);
            assert!(result.is_ok(), "frame {} failed: {:?}", i, result.err());
        }
    }

    // ── bitrate reconfigure ───────────────────────────────────────────────────

    #[test]
    fn set_bitrate_small_change_no_recreate() {
        let mut encoder = HwEncoder::new(160, 120, 1_000_000).unwrap();
        // 1.1x change — within 20% threshold, no recreation.
        assert!(encoder.set_bitrate(1_100_000).is_ok());
        assert_eq!(encoder.bitrate_bps(), 1_100_000);
        assert_eq!(encoder.width(), 160);
        assert_eq!(encoder.height(), 120);
    }

    #[test]
    fn set_bitrate_large_change_recreates() {
        let mut encoder = HwEncoder::new(160, 120, 1_000_000).unwrap();
        // 2x change — exceeds 20% threshold, encoder is recreated.
        assert!(encoder.set_bitrate(2_000_000).is_ok());
        assert_eq!(encoder.bitrate_bps(), 2_000_000);
        assert_eq!(encoder.width(), 160);
        assert_eq!(encoder.height(), 120);
        // Encoder should still be functional after recreation.
        let bgra = vec![0u8; 160 * 120 * 4];
        assert!(encoder.encode_bgra(&bgra).is_ok());
    }
}
