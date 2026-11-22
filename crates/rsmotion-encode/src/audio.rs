use std::collections::{BTreeMap, HashMap};
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use rsmotion_core::AudioCodec;
use serde::{Deserialize, Serialize};

use crate::error::{EncodeError, Result};

/// Configuration for muxed audio encoding in [`mix_and_mux_audio_with_options`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioMuxOptions {
    pub audio_codec: AudioCodec,
    pub audio_bitrate: Option<String>,
}

impl Default for AudioMuxOptions {
    fn default() -> Self {
        Self {
            audio_codec: AudioCodec::Aac,
            audio_bitrate: Some("192k".to_string()),
        }
    }
}

/// Per-frame audio/video asset entry collected from Remotion's `window.remotion_collectAssets()`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioTrackFrame {
    pub id: String,
    pub src: String,
    pub frame: u32,
    pub volume: f64,
    pub media_frame: f64,
    pub playback_rate: f64,
    pub audio_start_frame: f64,
}

fn has_audio_stream(path: &Path) -> Result<bool> {
    let out = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "a:0",
            "-show_entries",
            "stream=codec_type",
            "-of",
            "csv=p=0",
        ])
        .arg(path)
        .output()?;

    if !out.status.success() {
        return Ok(false);
    }
    Ok(!String::from_utf8_lossy(&out.stdout).trim().is_empty())
}

fn decode_pcm_f32_stereo(path: &Path, sample_rate: u32) -> Result<Vec<f32>> {
    let sr = sample_rate.to_string();
    let out = Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-i"])
        .arg(path)
        .args(["-vn", "-f", "f32le", "-ar", &sr, "-ac", "2", "pipe:1"])
        .output()?;

    if !out.status.success() {
        return Err(EncodeError::FfmpegFailed {
            code: out.status.code().unwrap_or(-1),
            stderr: String::from_utf8_lossy(&out.stderr).trim().to_string(),
        });
    }

    let mut samples = Vec::with_capacity(out.stdout.len() / 4);
    for &chunk in out.stdout.as_chunks::<4>().0 {
        samples.push(f32::from_le_bytes(chunk));
    }
    Ok(samples)
}

fn write_wav_pcm16(path: &Path, sample_rate: u32, stereo_f32: &[f32]) -> Result<()> {
    let channels: u16 = 2;
    let bits_per_sample: u16 = 16;
    let byte_rate = sample_rate * u32::from(channels) * u32::from(bits_per_sample / 8);
    let block_align = channels * (bits_per_sample / 8);
    let data_bytes = (stereo_f32.len() * 2) as u32;
    let riff_size = 36u32.saturating_add(data_bytes);

    let file = File::create(path)?;
    let mut w = BufWriter::with_capacity(64 * 1024, file);
    w.write_all(b"RIFF")?;
    w.write_all(&riff_size.to_le_bytes())?;
    w.write_all(b"WAVE")?;
    w.write_all(b"fmt ")?;
    w.write_all(&16u32.to_le_bytes())?; // PCM fmt chunk size
    w.write_all(&1u16.to_le_bytes())?; // PCM linear
    w.write_all(&channels.to_le_bytes())?;
    w.write_all(&sample_rate.to_le_bytes())?;
    w.write_all(&byte_rate.to_le_bytes())?;
    w.write_all(&block_align.to_le_bytes())?;
    w.write_all(&bits_per_sample.to_le_bytes())?;
    w.write_all(b"data")?;
    w.write_all(&data_bytes.to_le_bytes())?;

    for &s in stereo_f32 {
        let clamped = s.clamp(-1.0, 1.0);
        let q = (clamped * 32767.0).round() as i16;
        w.write_all(&q.to_le_bytes())?;
    }
    w.flush()?;
    Ok(())
}

/// Mixes all active audio tracks frame-accurately and muxes them with `video_only_mp4` into `final_mp4`.
///
/// Returns `Ok(true)` if an audio track was muxed, or `Ok(false)` if no audio streams were active
/// (in which case `video_only_mp4` is moved/copied to `final_mp4`).
pub fn mix_and_mux_audio(
    video_only_mp4: &Path,
    final_mp4: &Path,
    tracks: &[AudioTrackFrame],
    fps: f64,
    duration_in_frames: u32,
    sample_rate: u32,
    resolve_src: impl Fn(&str) -> Option<PathBuf>,
) -> Result<bool> {
    mix_and_mux_audio_with_options(
        video_only_mp4,
        final_mp4,
        tracks,
        fps,
        duration_in_frames,
        sample_rate,
        &AudioMuxOptions::default(),
        resolve_src,
    )
}

/// Mixes all active audio tracks with configurable audio codec and bitrate, muxing into `final_mp4`.
#[allow(clippy::too_many_arguments)]
pub fn mix_and_mux_audio_with_options(
    video_only_mp4: &Path,
    final_mp4: &Path,
    tracks: &[AudioTrackFrame],
    fps: f64,
    duration_in_frames: u32,
    sample_rate: u32,
    mux_opts: &AudioMuxOptions,
    resolve_src: impl Fn(&str) -> Option<PathBuf>,
) -> Result<bool> {
    if let Some(parent) = final_mp4.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }

    // Group & deduplicate by (track id, frame).
    let mut grouped: BTreeMap<&str, BTreeMap<u32, &AudioTrackFrame>> = BTreeMap::new();
    for t in tracks {
        grouped
            .entry(t.id.as_str())
            .or_default()
            .entry(t.frame)
            .or_insert(t);
    }

    // Decode unique sources that actually contain an audio stream.
    let mut decoded_sources: HashMap<String, Vec<f32>> = HashMap::new();
    let mut checked_sources: HashMap<String, bool> = HashMap::new();

    for frames_map in grouped.values() {
        for tf in frames_map.values() {
            if checked_sources.contains_key(&tf.src) {
                continue;
            }
            let Some(local_path) = resolve_src(&tf.src) else {
                checked_sources.insert(tf.src.clone(), false);
                continue;
            };
            if !local_path.exists() || !has_audio_stream(&local_path)? {
                checked_sources.insert(tf.src.clone(), false);
                continue;
            }
            let pcm = decode_pcm_f32_stereo(&local_path, sample_rate)?;
            decoded_sources.insert(tf.src.clone(), pcm);
            checked_sources.insert(tf.src.clone(), true);
        }
    }

    if decoded_sources.is_empty() {
        if video_only_mp4 != final_mp4 && std::fs::rename(video_only_mp4, final_mp4).is_err() {
            std::fs::copy(video_only_mp4, final_mp4)?;
            let _ = std::fs::remove_file(video_only_mp4);
        }
        return Ok(false);
    }

    let total_audio_samples =
        ((f64::from(duration_in_frames) / fps) * f64::from(sample_rate)).ceil() as usize;
    let mut mix_buf = vec![0.0_f32; total_audio_samples * 2];

    for frames_map in grouped.values() {
        for (&frame_idx, tf) in frames_map {
            let Some(src_pcm) = decoded_sources.get(&tf.src) else {
                continue;
            };
            let src_frames = src_pcm.len() / 2;
            if src_frames == 0 || tf.volume <= 0.0 {
                continue;
            }

            let out_start =
                ((f64::from(frame_idx) / fps) * f64::from(sample_rate)).floor() as usize;
            let out_end = (((f64::from(frame_idx + 1) / fps) * f64::from(sample_rate)).floor()
                as usize)
                .min(total_audio_samples);
            if out_start >= out_end {
                continue;
            }

            let src_start_time_secs = tf.media_frame / fps;
            let vol = tf.volume as f32;
            let rate = if tf.playback_rate > 0.0 {
                tf.playback_rate
            } else {
                1.0
            };

            for s in out_start..out_end {
                let dt = (s - out_start) as f64 / f64::from(sample_rate);
                let src_pos = (src_start_time_secs + dt * rate) * f64::from(sample_rate);
                if src_pos < 0.0 {
                    continue;
                }
                let i0 = src_pos.floor() as usize;
                if i0 >= src_frames {
                    continue;
                }
                let frac = (src_pos - (i0 as f64)) as f32;
                let i1 = (i0 + 1).min(src_frames - 1);

                let l0 = src_pcm[i0 * 2];
                let r0 = src_pcm[i0 * 2 + 1];
                let l1 = src_pcm[i1 * 2];
                let r1 = src_pcm[i1 * 2 + 1];

                mix_buf[s * 2] += (l0 + (l1 - l0) * frac) * vol;
                mix_buf[s * 2 + 1] += (r0 + (r1 - r0) * frac) * vol;
            }
        }
    }

    let wav_path = final_mp4.with_extension("rsmotion-mix.wav");
    write_wav_pcm16(&wav_path, sample_rate, &mix_buf)?;

    let mut cmd = Command::new("ffmpeg");
    cmd.args(["-y", "-hide_banner", "-loglevel", "error", "-i"])
        .arg(video_only_mp4)
        .arg("-i")
        .arg(&wav_path)
        .args([
            "-map",
            "0:v:0",
            "-map",
            "1:a:0",
            "-c:v",
            "copy",
            "-c:a",
            mux_opts.audio_codec.ffmpeg_codec(),
        ]);
    if !matches!(mux_opts.audio_codec, AudioCodec::Pcm16) {
        let bitrate = mux_opts.audio_bitrate.as_deref().unwrap_or("192k");
        cmd.args(["-b:a", bitrate]);
    }
    cmd.arg("-shortest");
    let ext = final_mp4
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase());
    if matches!(ext.as_deref(), Some("mp4" | "mov" | "m4v")) {
        cmd.args(["-movflags", "+faststart"]);
    }
    let out = cmd.arg(final_mp4).output();

    let _ = std::fs::remove_file(&wav_path);
    let out = out?;
    if !out.status.success() {
        return Err(EncodeError::FfmpegFailed {
            code: out.status.code().unwrap_or(-1),
            stderr: String::from_utf8_lossy(&out.stderr).trim().to_string(),
        });
    }

    if video_only_mp4 != final_mp4 {
        let _ = std::fs::remove_file(video_only_mp4);
    }

    Ok(true)
}

/// Extracts the audio stream from `video_source` to `audio_output` without decoding video frames.
///
/// Mirrors `@remotion/renderer`'s `extractAudio()` (`-vn -c:a copy`, falling back to container-default
/// transcode when stream copy is incompatible with the target extension).
pub fn extract_audio_track(video_source: &Path, audio_output: &Path) -> Result<()> {
    if let Some(parent) = audio_output.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }

    let copy_out = Command::new("ffmpeg")
        .args(["-y", "-hide_banner", "-loglevel", "error", "-i"])
        .arg(video_source)
        .args(["-vn", "-c:a", "copy"])
        .arg(audio_output)
        .output()?;

    if copy_out.status.success() {
        return Ok(());
    }

    let transcode_out = Command::new("ffmpeg")
        .args(["-y", "-hide_banner", "-loglevel", "error", "-i"])
        .arg(video_source)
        .arg("-vn")
        .arg(audio_output)
        .output()?;

    if !transcode_out.status.success() {
        return Err(EncodeError::FfmpegFailed {
            code: transcode_out.status.code().unwrap_or(-1),
            stderr: String::from_utf8_lossy(&transcode_out.stderr)
                .trim()
                .to_string(),
        });
    }

    Ok(())
}
