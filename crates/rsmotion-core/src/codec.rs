use clap::ValueEnum;
use serde::{Deserialize, Serialize};

use crate::error::{Result, RsmotionError};

/// Supported output codecs matching `@remotion/renderer`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum Codec {
    H264,
    H265,
    Vp8,
    Vp9,
    #[value(name = "prores")]
    ProRes,
    Gif,
    Mp3,
    Aac,
    Wav,
}

impl Codec {
    /// Default Constant Rate Factor matching Remotion 4.0.545 (`crf.ts`).
    pub fn default_crf(self) -> Option<u8> {
        match self {
            Codec::H264 => Some(18),
            Codec::H265 => Some(23),
            Codec::Vp8 => Some(9),
            Codec::Vp9 => Some(28),
            Codec::ProRes | Codec::Gif | Codec::Mp3 | Codec::Aac | Codec::Wav => None,
        }
    }

    /// Valid CRF range `(min, max)` when the codec accepts CRF.
    pub fn crf_range(self) -> Option<(u8, u8)> {
        match self {
            Codec::H264 => Some((1, 51)),
            Codec::H265 => Some((0, 51)),
            Codec::Vp8 => Some((4, 63)),
            Codec::Vp9 => Some((0, 63)),
            Codec::ProRes | Codec::Gif | Codec::Mp3 | Codec::Aac | Codec::Wav => None,
        }
    }

    /// Validates an explicit CRF value for this codec.
    pub fn validate_crf(self, crf: u8) -> Result<()> {
        if let Some((min, max)) = self.crf_range() {
            if crf < min || crf > max {
                return Err(RsmotionError::InvalidCrf {
                    codec: self.as_str().to_string(),
                    crf,
                    min,
                    max,
                });
            }
        }
        Ok(())
    }

    /// Default file extension without leading dot.
    pub fn extension(self) -> &'static str {
        match self {
            Codec::H264 | Codec::H265 => "mp4",
            Codec::Vp8 | Codec::Vp9 => "webm",
            Codec::ProRes => "mov",
            Codec::Gif => "gif",
            Codec::Mp3 => "mp3",
            Codec::Aac => "aac",
            Codec::Wav => "wav",
        }
    }

    /// Whether the codec requires even width and height (H.264 / H.265).
    pub fn requires_even_dimensions(self) -> bool {
        matches!(self, Codec::H264 | Codec::H265)
    }

    /// Whether the codec outputs audio only without video frames.
    pub fn is_audio_only(self) -> bool {
        matches!(self, Codec::Mp3 | Codec::Aac | Codec::Wav)
    }

    /// Canonical CLI string name.
    pub fn as_str(self) -> &'static str {
        match self {
            Codec::H264 => "h264",
            Codec::H265 => "h265",
            Codec::Vp8 => "vp8",
            Codec::Vp9 => "vp9",
            Codec::ProRes => "prores",
            Codec::Gif => "gif",
            Codec::Mp3 => "mp3",
            Codec::Aac => "aac",
            Codec::Wav => "wav",
        }
    }
}

/// Output pixel format options.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum PixelFormat {
    Yuv420p,
    Yuv444p,
    Yuva420p,
    Rgb24,
    Rgba,
}

impl PixelFormat {
    pub fn as_str(self) -> &'static str {
        match self {
            PixelFormat::Yuv420p => "yuv420p",
            PixelFormat::Yuv444p => "yuv444p",
            PixelFormat::Yuva420p => "yuva420p",
            PixelFormat::Rgb24 => "rgb24",
            PixelFormat::Rgba => "rgba",
        }
    }
}

/// Still / frame-sequence image format options.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum ImageFormat {
    Png,
    Jpeg,
    Webp,
}

/// Hardware acceleration mode for FFmpeg encoding/decoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum HardwareAccel {
    Auto,
    Vaapi,
    Qsv,
    Software,
}

/// Raster backend selection for Tier 1 Skia rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum RasterBackend {
    Auto,
    Gpu,
    Cpu,
}

/// Execution tier selected by the analyzer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Tier {
    Tier1Native,
    Tier2Cdp,
    Tier3Fallback,
}

/// Validates and optionally rounds width/height down to even numbers for H.264/H.265.
pub fn validate_even_dimensions(width: u32, height: u32, codec: Codec) -> Result<(u32, u32)> {
    if width == 0 || height == 0 {
        return Err(RsmotionError::InvalidDimensions {
            width,
            height,
            reason: "width and height must be > 0".to_string(),
        });
    }

    if !codec.requires_even_dimensions() {
        return Ok((width, height));
    }

    let even_w = width - (width % 2);
    let even_h = height - (height % 2);
    if even_w == 0 || even_h == 0 {
        return Err(RsmotionError::InvalidDimensions {
            width,
            height,
            reason: format!("codec `{}` requires even dimensions >= 2", codec.as_str()),
        });
    }

    Ok((even_w, even_h))
}

/// Supported muxed audio output codecs matching Remotion `--audio-codec`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum AudioCodec {
    #[default]
    Aac,
    Mp3,
    Opus,
    #[value(name = "pcm-16")]
    Pcm16,
}

impl AudioCodec {
    pub fn as_str(self) -> &'static str {
        match self {
            AudioCodec::Aac => "aac",
            AudioCodec::Mp3 => "mp3",
            AudioCodec::Opus => "opus",
            AudioCodec::Pcm16 => "pcm-16",
        }
    }

    pub fn ffmpeg_codec(self) -> &'static str {
        match self {
            AudioCodec::Aac => "aac",
            AudioCodec::Mp3 => "libmp3lame",
            AudioCodec::Opus => "libopus",
            AudioCodec::Pcm16 => "pcm_s16le",
        }
    }
}

/// Output video color space tags matching Remotion `--color-space`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum ColorSpace {
    #[default]
    Default,
    Bt709,
    #[value(name = "bt2020-ncl")]
    Bt2020Ncl,
}

impl ColorSpace {
    pub fn as_str(self) -> &'static str {
        match self {
            ColorSpace::Default => "default",
            ColorSpace::Bt709 => "bt709",
            ColorSpace::Bt2020Ncl => "bt2020-ncl",
        }
    }
}

/// Apple ProRes encoding profiles matching Remotion `--prores-profile`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum ProResProfile {
    Proxy,
    Light,
    Standard,
    #[default]
    Hq,
    #[value(name = "4444")]
    FourFourFourFour,
    #[value(name = "4444-xq")]
    FourFourFourFourXq,
}

impl ProResProfile {
    pub fn as_str(self) -> &'static str {
        match self {
            ProResProfile::Proxy => "proxy",
            ProResProfile::Light => "light",
            ProResProfile::Standard => "standard",
            ProResProfile::Hq => "hq",
            ProResProfile::FourFourFourFour => "4444",
            ProResProfile::FourFourFourFourXq => "4444-xq",
        }
    }

    pub fn profile_id(self) -> &'static str {
        match self {
            ProResProfile::Proxy => "0",
            ProResProfile::Light => "1",
            ProResProfile::Standard => "2",
            ProResProfile::Hq => "3",
            ProResProfile::FourFourFourFour => "4",
            ProResProfile::FourFourFourFourXq => "5",
        }
    }
}
