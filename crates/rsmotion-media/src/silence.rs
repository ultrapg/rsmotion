use std::path::Path;
use std::process::Command;

use serde::{Deserialize, Serialize};

use crate::error::{MediaError, Result};
use crate::probe::probe_media;

/// One contiguous silent or audible time interval in seconds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SilentPart {
    pub start_in_seconds: f64,
    pub end_in_seconds: f64,
}

/// Silence & audible intervals matching `@remotion/renderer`'s `getSilentParts()` payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SilentPartsResult {
    pub silent_parts: Vec<SilentPart>,
    pub audible_parts: Vec<SilentPart>,
    pub duration_in_seconds: f64,
}

fn parse_silencedetect_number(line: &str, marker: &str) -> Option<f64> {
    let idx = line.find(marker)?;
    let rest = line[idx + marker.len()..].trim_start();
    let tok = rest.split_whitespace().next()?;
    tok.parse::<f64>().ok()
}

/// Detects silent and audible segments in a video or audio file via FFmpeg `silencedetect`.
pub fn get_silent_parts(
    src: &Path,
    noise_threshold_db: f64,
    min_duration_secs: f64,
) -> Result<SilentPartsResult> {
    if !src.is_file() {
        return Err(MediaError::NotFound(src.display().to_string()));
    }
    if min_duration_secs <= 0.0 || !min_duration_secs.is_finite() {
        return Err(MediaError::ProbeFailed(format!(
            "minDurationInSeconds must be greater than 0, got {min_duration_secs}"
        )));
    }
    if noise_threshold_db >= 30.0 || !noise_threshold_db.is_finite() {
        return Err(MediaError::ProbeFailed(format!(
            "noiseThresholdInDecibels must be less than 30, got {noise_threshold_db}"
        )));
    }

    let meta = probe_media(src)?;
    let duration_in_seconds = meta.duration_secs.max(0.0);

    let filter = format!("silencedetect=noise={noise_threshold_db}dB:d={min_duration_secs}");
    let out = Command::new("ffmpeg")
        .args(["-hide_banner", "-nostats", "-i"])
        .arg(src)
        .args(["-vn", "-af", &filter, "-f", "null", "-"])
        .output()?;

    if !out.status.success() {
        return Err(MediaError::ProbeFailed(format!(
            "ffmpeg silencedetect exited with {:?}: {}",
            out.status.code(),
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }

    let stderr = String::from_utf8_lossy(&out.stderr);
    let mut silent_parts = Vec::new();
    let mut pending_start: Option<f64> = None;

    for line in stderr.lines() {
        if !line.contains("silencedetect") {
            continue;
        }
        if let Some(start) = parse_silencedetect_number(line, "silence_start:") {
            pending_start = Some(start.max(0.0));
        } else if let Some(end) = parse_silencedetect_number(line, "silence_end:") {
            let start = pending_start.take().unwrap_or(0.0).max(0.0);
            let clamped_end = end.min(duration_in_seconds.max(end));
            if clamped_end > start {
                silent_parts.push(SilentPart {
                    start_in_seconds: start,
                    end_in_seconds: clamped_end,
                });
            }
        }
    }

    if let Some(start) = pending_start {
        if duration_in_seconds - start >= (min_duration_secs - 1e-3).max(0.0) {
            silent_parts.push(SilentPart {
                start_in_seconds: start,
                end_in_seconds: duration_in_seconds,
            });
        }
    }

    let mut audible_parts = Vec::new();
    let mut last_end = 0.0_f64;
    for sp in &silent_parts {
        if sp.start_in_seconds - last_end > 1e-6 {
            audible_parts.push(SilentPart {
                start_in_seconds: last_end,
                end_in_seconds: sp.start_in_seconds,
            });
        }
        last_end = sp.end_in_seconds;
    }
    if duration_in_seconds - last_end > 1e-6 {
        audible_parts.push(SilentPart {
            start_in_seconds: last_end,
            end_in_seconds: duration_in_seconds,
        });
    }

    Ok(SilentPartsResult {
        silent_parts,
        audible_parts,
        duration_in_seconds,
    })
}
