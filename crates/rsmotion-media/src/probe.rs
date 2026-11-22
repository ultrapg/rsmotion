use std::path::Path;
use std::process::Command;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{MediaError, Result};

/// Probed media container and primary stream metadata.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MediaMetadata {
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub duration_secs: f64,
    pub nb_frames: Option<u64>,
    pub codec_name: Option<String>,
    pub has_video: bool,
    pub has_audio: bool,
}

fn parse_rate(raw: Option<&str>) -> Option<f64> {
    let s = raw?.trim();
    if s.is_empty() || s == "0/0" {
        return None;
    }
    if let Some((num, den)) = s.split_once('/') {
        let n = num.parse::<f64>().ok()?;
        let d = den.parse::<f64>().ok()?;
        if n > 0.0 && d > 0.0 {
            return Some(n / d);
        }
        return None;
    }
    let val = s.parse::<f64>().ok()?;
    (val > 0.0 && val.is_finite()).then_some(val)
}

fn parse_duration(val: Option<&Value>) -> Option<f64> {
    let s = val?.as_str()?;
    let d = s.trim().parse::<f64>().ok()?;
    (d > 0.0 && d.is_finite()).then_some(d)
}

/// Probes video/audio metadata for `path` using `ffprobe`.
pub fn probe_media(path: &Path) -> Result<MediaMetadata> {
    let out = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration:stream=codec_type,codec_name,width,height,r_frame_rate,avg_frame_rate,nb_frames,duration",
            "-of",
            "json",
        ])
        .arg(path)
        .output()?;

    if !out.status.success() {
        return Err(MediaError::ProbeFailed(format!(
            "ffprobe failed on {}: {}",
            path.display(),
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }

    let root: Value = serde_json::from_slice(&out.stdout)
        .map_err(|e| MediaError::ProbeFailed(format!("invalid ffprobe JSON: {e}")))?;

    let streams = root
        .get("streams")
        .and_then(|v| v.as_array())
        .ok_or_else(|| MediaError::ProbeFailed("missing streams array in ffprobe JSON".into()))?;

    let mut width = 0u32;
    let mut height = 0u32;
    let mut fps = 30.0_f64;
    let mut nb_frames = None;
    let mut codec_name = None;
    let mut has_video = false;
    let mut has_audio = false;
    let mut video_duration = None;
    let mut audio_duration = None;

    for stream in streams {
        let kind = stream
            .get("codec_type")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        match kind {
            "video" if !has_video => {
                let w = stream.get("width").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                let h = stream.get("height").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                if w > 0 && h > 0 {
                    has_video = true;
                    width = w;
                    height = h;
                    fps = parse_rate(stream.get("r_frame_rate").and_then(|v| v.as_str()))
                        .or_else(|| {
                            parse_rate(stream.get("avg_frame_rate").and_then(|v| v.as_str()))
                        })
                        .unwrap_or(30.0);
                    nb_frames = stream
                        .get("nb_frames")
                        .and_then(|v| v.as_str())
                        .and_then(|s| s.parse::<u64>().ok())
                        .filter(|&n| n > 0);
                    codec_name = stream
                        .get("codec_name")
                        .and_then(|v| v.as_str())
                        .map(str::to_string);
                    video_duration = parse_duration(stream.get("duration"));
                }
            }
            "audio" => {
                has_audio = true;
                if audio_duration.is_none() {
                    audio_duration = parse_duration(stream.get("duration"));
                }
            }
            _ => {}
        }
    }

    let format_duration = parse_duration(root.get("format").and_then(|f| f.get("duration")));
    let duration_secs = video_duration
        .or(audio_duration)
        .or(format_duration)
        .unwrap_or(0.0);

    Ok(MediaMetadata {
        width,
        height,
        fps,
        duration_secs,
        nb_frames,
        codec_name,
        has_video,
        has_audio,
    })
}
