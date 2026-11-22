use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use rsmotion_core::HardwareAccel;
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

use crate::error::{EncodeError, Result};

const DEFAULT_VAAPI_DEVICE: &str = "/dev/dri/renderD128";
static AUTO_ENCODER_CACHE: OnceLock<ResolvedEncoder> = OnceLock::new();

/// Resolved FFmpeg video encoder configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedEncoder {
    pub kind: HardwareAccel,
    pub ffmpeg_codec: String,
    pub vaapi_device: Option<PathBuf>,
}

fn test_vaapi(device: &Path) -> std::result::Result<(), String> {
    if !device.exists() {
        return Err(format!(
            "DRM render node {} does not exist",
            device.display()
        ));
    }
    let out = Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-vaapi_device"])
        .arg(device)
        .args([
            "-f",
            "lavfi",
            "-i",
            "color=c=black:s=64x64:d=0.1",
            "-vf",
            "format=nv12,hwupload",
            "-c:v",
            "h264_vaapi",
            "-frames:v",
            "2",
            "-f",
            "null",
            "-",
        ])
        .output()
        .map_err(|e| format!("failed to run ffmpeg: {e}"))?;

    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

fn test_qsv() -> std::result::Result<(), String> {
    let out = Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "lavfi",
            "-i",
            "color=c=black:s=64x64:d=0.1",
            "-vf",
            "format=nv12",
            "-c:v",
            "h264_qsv",
            "-frames:v",
            "2",
            "-f",
            "null",
            "-",
        ])
        .output()
        .map_err(|e| format!("failed to run ffmpeg: {e}"))?;

    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

fn test_x264() -> std::result::Result<(), String> {
    let out = Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "lavfi",
            "-i",
            "color=c=black:s=64x64:d=0.1",
            "-c:v",
            "libx264",
            "-frames:v",
            "2",
            "-f",
            "null",
            "-",
        ])
        .output()
        .map_err(|e| format!("failed to run ffmpeg: {e}"))?;

    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

/// Probes FFmpeg with a real 2-frame 64x64 test encode to pick the fastest working H.264 encoder.
pub fn probe_best_h264_encoder(requested: HardwareAccel) -> Result<ResolvedEncoder> {
    let vaapi_dev = PathBuf::from(DEFAULT_VAAPI_DEVICE);

    match requested {
        HardwareAccel::Vaapi => {
            test_vaapi(&vaapi_dev)
                .map_err(|e| EncodeError::EncoderUnavailable("h264_vaapi".to_string(), e))?;
            Ok(ResolvedEncoder {
                kind: HardwareAccel::Vaapi,
                ffmpeg_codec: "h264_vaapi".to_string(),
                vaapi_device: Some(vaapi_dev),
            })
        }
        HardwareAccel::Qsv => {
            test_qsv().map_err(|e| EncodeError::EncoderUnavailable("h264_qsv".to_string(), e))?;
            Ok(ResolvedEncoder {
                kind: HardwareAccel::Qsv,
                ffmpeg_codec: "h264_qsv".to_string(),
                vaapi_device: None,
            })
        }
        HardwareAccel::Software => {
            test_x264().map_err(|e| EncodeError::EncoderUnavailable("libx264".to_string(), e))?;
            Ok(ResolvedEncoder {
                kind: HardwareAccel::Software,
                ffmpeg_codec: "libx264".to_string(),
                vaapi_device: None,
            })
        }
        HardwareAccel::Auto => {
            if let Some(cached) = AUTO_ENCODER_CACHE.get() {
                return Ok(cached.clone());
            }

            let resolved = match test_vaapi(&vaapi_dev) {
                Ok(()) => {
                    info!(
                        "Selected H.264 encoder: h264_vaapi ({})",
                        vaapi_dev.display()
                    );
                    ResolvedEncoder {
                        kind: HardwareAccel::Vaapi,
                        ffmpeg_codec: "h264_vaapi".to_string(),
                        vaapi_device: Some(vaapi_dev),
                    }
                }
                Err(e) => {
                    debug!("VAAPI probe failed: {e}");
                    match test_qsv() {
                        Ok(()) => {
                            info!("Selected H.264 encoder: h264_qsv");
                            ResolvedEncoder {
                                kind: HardwareAccel::Qsv,
                                ffmpeg_codec: "h264_qsv".to_string(),
                                vaapi_device: None,
                            }
                        }
                        Err(e2) => {
                            debug!("QSV probe failed: {e2}");
                            test_x264().map_err(|e3| {
                                EncodeError::EncoderUnavailable("libx264".to_string(), e3)
                            })?;
                            info!("Selected H.264 encoder: libx264 (software fallback)");
                            ResolvedEncoder {
                                kind: HardwareAccel::Software,
                                ffmpeg_codec: "libx264".to_string(),
                                vaapi_device: None,
                            }
                        }
                    }
                }
            };

            let _ = AUTO_ENCODER_CACHE.set(resolved.clone());
            Ok(resolved)
        }
    }
}
