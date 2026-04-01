// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2025-2026 Ehsan Khalid. All rights reserved.
// Licensed under the GNU Affero General Public License v3.0.
// Commercial licensing available — see LICENSE-COMMERCIAL.md.

use crate::types::CodecId;

/// Hardware or software encoder backend available on this host.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EncoderBackend {
    /// NVIDIA NVENC (GPU encoder).
    Nvenc,
    /// AMD Advanced Media Framework (GPU encoder).
    Amf,
    /// Intel Quick Sync Video (GPU / iGPU encoder).
    Qsv,
    /// Apple VideoToolbox (macOS / iOS).
    VideoToolbox,
    /// VA-API (Linux GPU encode via Mesa / driver).
    Vaapi,
    /// CPU-based software encoder (last resort).
    Software,
}

impl EncoderBackend {
    /// Selection priority — lower value wins.  `Nvenc` is the preferred
    /// backend (0); `Software` is the last resort (255).
    pub fn priority(&self) -> u8 {
        match self {
            EncoderBackend::Nvenc => 0,
            EncoderBackend::Amf => 1,
            EncoderBackend::Qsv => 2,
            EncoderBackend::VideoToolbox => 3,
            EncoderBackend::Vaapi => 4,
            EncoderBackend::Software => 255,
        }
    }

    /// True when this backend can produce a lossless bitstream using GPU
    /// acceleration.
    pub fn supports_hw_lossless(&self) -> bool {
        matches!(self, EncoderBackend::Nvenc | EncoderBackend::Qsv)
    }

    /// Codecs that this backend is capable of encoding.
    pub fn supported_codecs(&self) -> Vec<CodecId> {
        match self {
            EncoderBackend::Nvenc => vec![CodecId::H264, CodecId::H265, CodecId::Av1],
            EncoderBackend::Amf => vec![CodecId::H264, CodecId::H265, CodecId::Av1],
            EncoderBackend::Qsv => vec![CodecId::H264, CodecId::H265, CodecId::Av1],
            EncoderBackend::VideoToolbox => vec![CodecId::H264, CodecId::H265],
            EncoderBackend::Vaapi => vec![CodecId::H264, CodecId::H265],
            EncoderBackend::Software => vec![CodecId::H264],
        }
    }
}

/// Choose the highest-priority backend from `available` that supports
/// `required_codec`, or `None` if no match exists.
pub fn select_best_encoder(
    available: &[EncoderBackend],
    required_codec: CodecId,
) -> Option<EncoderBackend> {
    available
        .iter()
        .filter(|b| b.supported_codecs().contains(&required_codec))
        .min_by_key(|b| b.priority())
        .copied()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nvenc_highest_priority() {
        let available = [
            EncoderBackend::Software,
            EncoderBackend::Amf,
            EncoderBackend::Nvenc,
        ];
        let best = select_best_encoder(&available, CodecId::H264);
        assert_eq!(best, Some(EncoderBackend::Nvenc));
    }

    #[test]
    fn falls_back_to_software() {
        let available = [EncoderBackend::Software];
        let best = select_best_encoder(&available, CodecId::H264);
        assert_eq!(best, Some(EncoderBackend::Software));
    }

    #[test]
    fn no_av1_software() {
        let available = [EncoderBackend::Software];
        let best = select_best_encoder(&available, CodecId::Av1);
        assert!(best.is_none(), "Software does not support AV1");
    }

    #[test]
    fn nvenc_supports_hw_lossless() {
        assert!(EncoderBackend::Nvenc.supports_hw_lossless());
        assert!(EncoderBackend::Qsv.supports_hw_lossless());
        assert!(!EncoderBackend::Software.supports_hw_lossless());
        assert!(!EncoderBackend::Amf.supports_hw_lossless());
    }
}
