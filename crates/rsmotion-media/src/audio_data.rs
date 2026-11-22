use std::path::{Path, PathBuf};
use std::process::{self, Command};
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::Value;

use crate::error::{MediaError, Result};

static TEMP_AUDIO_SEQ: AtomicU64 = AtomicU64::new(0);

/// Decoded interleaved 32-bit float PCM waveform from an audio or video container.
#[derive(Debug, Clone)]
pub struct DecodedAudioPcm {
    pub sample_rate: u32,
    pub channels: u32,
    pub duration_secs: f64,
    pub interleaved_f32_le: Vec<u8>,
}

struct TempFileGuard(PathBuf);

impl Drop for TempFileGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn probe_audio_stream_params(path: &Path) -> (u32, u32) {
    let out = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "a:0",
            "-show_entries",
            "stream=sample_rate,channels",
            "-of",
            "json",
        ])
        .arg(path)
        .output();

    let Ok(out) = out else {
        return (48000, 2);
    };
    if !out.status.success() {
        return (48000, 2);
    }
    let Ok(root) = serde_json::from_slice::<Value>(&out.stdout) else {
        return (48000, 2);
    };
    let Some(stream) = root
        .get("streams")
        .and_then(|s| s.as_array())
        .and_then(|arr| arr.first())
    else {
        return (48000, 2);
    };

    let sample_rate = stream
        .get("sample_rate")
        .and_then(|v| {
            v.as_str()
                .and_then(|s| s.parse::<u32>().ok())
                .or_else(|| v.as_u64().map(|n| n as u32))
        })
        .filter(|&sr| sr >= 8000)
        .unwrap_or(48000);
    let channels = stream
        .get("channels")
        .and_then(|v| v.as_u64())
        .map(|n| n as u32)
        .filter(|&c| (1..=8).contains(&c))
        .unwrap_or(2);
    (sample_rate, channels)
}

fn run_ffmpeg_pcm(
    path: &Path,
    format_hint: Option<&str>,
    sample_rate: u32,
    channels: u32,
) -> Result<DecodedAudioPcm> {
    let mut cmd = Command::new("ffmpeg");
    cmd.args(["-v", "error"]);
    if let Some(fmt) = format_hint {
        cmd.args(["-f", fmt]);
    }
    cmd.arg("-i").arg(path).args([
        "-vn",
        "-ar",
        &sample_rate.to_string(),
        "-ac",
        &channels.to_string(),
        "-f",
        "f32le",
        "-acodec",
        "pcm_f32le",
        "pipe:1",
    ]);

    let out = cmd.output()?;
    if !out.status.success() {
        return Err(MediaError::DecodeFailed(format!(
            "ffmpeg audio decode failed for {}: {}",
            path.display(),
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }

    let bytes_per_frame = 4usize * (channels as usize);
    let frame_count = out.stdout.len().checked_div(bytes_per_frame).unwrap_or(0);
    let duration_secs = if sample_rate > 0 {
        (frame_count as f64) / f64::from(sample_rate)
    } else {
        0.0
    };

    Ok(DecodedAudioPcm {
        sample_rate,
        channels,
        duration_secs,
        interleaved_f32_le: out.stdout,
    })
}

/// Decodes the primary audio stream of a local audio/video file to interleaved `f32le` PCM.
pub fn decode_audio_file(path: &Path, target_sample_rate: Option<u32>) -> Result<DecodedAudioPcm> {
    let (probed_sr, probed_ch) = probe_audio_stream_params(path);
    let sample_rate = target_sample_rate
        .filter(|&sr| sr >= 8000)
        .unwrap_or(probed_sr);
    let channels = probed_ch.clamp(1, 8);
    run_ffmpeg_pcm(path, None, sample_rate, channels)
}

/// Decodes in-memory audio container/bitstream bytes via a temporary file so seekable demuxers succeed.
pub fn decode_audio_bytes(
    bytes: &[u8],
    format_hint: Option<&str>,
    target_sample_rate: Option<u32>,
    channels_hint: Option<u32>,
) -> Result<DecodedAudioPcm> {
    let seq = TEMP_AUDIO_SEQ.fetch_add(1, Ordering::Relaxed);
    let ext = match format_hint {
        Some(fmt) if fmt.chars().all(|c| c.is_ascii_alphanumeric()) => fmt,
        _ => "bin",
    };
    let tmp_path = std::env::temp_dir().join(format!("rsmotion-aud-{}-{seq}.{ext}", process::id()));
    std::fs::write(&tmp_path, bytes)?;
    let _guard = TempFileGuard(tmp_path.clone());

    let (probed_sr, probed_ch) = if target_sample_rate.is_none() || channels_hint.is_none() {
        probe_audio_stream_params(&tmp_path)
    } else {
        (48000, 2)
    };
    let sample_rate = target_sample_rate
        .filter(|&sr| sr >= 8000)
        .unwrap_or(probed_sr);
    let channels = channels_hint
        .filter(|&c| (1..=8).contains(&c))
        .unwrap_or(probed_ch)
        .clamp(1, 8);

    run_ffmpeg_pcm(&tmp_path, format_hint, sample_rate, channels)
}

/// Prepends a 7-byte ADTS header to a raw AAC-LC access unit so FFmpeg can decode raw WebCodecs chunks.
pub fn wrap_aac_frame_adts(frame: &[u8], sample_rate: u32, channels: u32) -> Vec<u8> {
    let freq_idx: u8 = match sample_rate {
        96000 => 0,
        88200 => 1,
        64000 => 2,
        48000 => 3,
        44100 => 4,
        32000 => 5,
        24000 => 6,
        22050 => 7,
        16000 => 8,
        12000 => 9,
        11025 => 10,
        8000 => 11,
        7350 => 12,
        _ => 3,
    };
    let profile: u8 = 1; // AAC-LC (object type 2 - 1)
    let ch_cfg: u8 = (channels.clamp(1, 7)) as u8;
    let frame_len = frame.len() + 7;

    let mut out = Vec::with_capacity(frame_len);
    out.push(0xFF);
    out.push(0xF1); // MPEG-4, Layer 0, no CRC
    out.push(((profile & 0x03) << 6) | ((freq_idx & 0x0F) << 2) | ((ch_cfg >> 2) & 0x01));
    out.push(((ch_cfg & 0x03) << 6) | (((frame_len >> 11) & 0x03) as u8));
    out.push(((frame_len >> 3) & 0xFF) as u8);
    out.push((((frame_len & 0x07) << 5) as u8) | 0x1F);
    out.push(0xFC);
    out.extend_from_slice(frame);
    out
}

/// Stateful in-process audio chunk decoder backed by pure-Rust Symphonia codecs.
pub struct StreamingAudioDecoder {
    decoder: Box<dyn symphonia::core::codecs::Decoder>,
    sample_rate: u32,
    channels: u32,
}

impl StreamingAudioDecoder {
    /// Creates a new stateful codec decoder for WebCodecs `AudioDecoder` configurations, returning `None` if unsupported.
    pub fn new(codec: &str, sample_rate: u32, channels: u32, extra_data: &[u8]) -> Option<Self> {
        use symphonia::core::audio::Channels;
        use symphonia::core::codecs::{
            CodecParameters, DecoderOptions, CODEC_TYPE_AAC, CODEC_TYPE_ALAC, CODEC_TYPE_FLAC,
            CODEC_TYPE_MP3, CODEC_TYPE_VORBIS,
        };

        let lower = codec.trim().to_ascii_lowercase();
        let codec_type = if lower.contains("mp3") {
            CODEC_TYPE_MP3
        } else if lower.starts_with("mp4a") || lower.contains("aac") {
            CODEC_TYPE_AAC
        } else if lower.contains("flac") {
            CODEC_TYPE_FLAC
        } else if lower.contains("vorbis") {
            CODEC_TYPE_VORBIS
        } else if lower.contains("alac") {
            CODEC_TYPE_ALAC
        } else {
            return None;
        };

        let mut params = CodecParameters::new();
        params.for_codec(codec_type);
        if sample_rate > 0 {
            params.with_sample_rate(sample_rate);
        }
        let ch_mask = match channels {
            1 => Some(Channels::FRONT_LEFT),
            2 => Some(Channels::FRONT_LEFT | Channels::FRONT_RIGHT),
            3 => Some(Channels::FRONT_LEFT | Channels::FRONT_RIGHT | Channels::FRONT_CENTRE),
            6 => Some(
                Channels::FRONT_LEFT
                    | Channels::FRONT_RIGHT
                    | Channels::FRONT_CENTRE
                    | Channels::LFE1
                    | Channels::REAR_LEFT
                    | Channels::REAR_RIGHT,
            ),
            _ => None,
        };
        if let Some(mask) = ch_mask {
            params.with_channels(mask);
        }
        if !extra_data.is_empty() {
            params.with_extra_data(extra_data.to_vec().into_boxed_slice());
        }

        let decoder = symphonia::default::get_codecs()
            .make(&params, &DecoderOptions::default())
            .ok()?;

        Some(Self {
            decoder,
            sample_rate: sample_rate.max(8000),
            channels: channels.clamp(1, 8),
        })
    }

    /// Decodes a sequence of compressed audio packet slices into interleaved `f32le` PCM.
    pub fn decode_packets(
        &mut self,
        chunks: &[u8],
        frame_lengths: &[u32],
    ) -> Result<DecodedAudioPcm> {
        use symphonia::core::audio::SampleBuffer;
        use symphonia::core::errors::Error as SymError;
        use symphonia::core::formats::Packet;

        let mut packets: Vec<&[u8]> = Vec::new();
        if frame_lengths.is_empty() {
            if !chunks.is_empty() {
                packets.push(chunks);
            }
        } else {
            let mut off = 0usize;
            for &len in frame_lengths {
                let end = off.saturating_add(len as usize).min(chunks.len());
                if end > off {
                    packets.push(&chunks[off..end]);
                }
                off = end;
            }
        }

        let mut interleaved_f32_le = Vec::new();
        let mut total_frames = 0usize;

        for pkt_bytes in packets {
            let pkt = Packet::new_from_slice(0, 0, 0, pkt_bytes);
            match self.decoder.decode(&pkt) {
                Ok(buf_ref) => {
                    let spec = *buf_ref.spec();
                    let frames = buf_ref.frames();
                    self.sample_rate = spec.rate;
                    self.channels = spec.channels.count() as u32;
                    if frames > 0 {
                        let mut sbuf = SampleBuffer::<f32>::new(buf_ref.capacity() as u64, spec);
                        sbuf.copy_interleaved_ref(buf_ref);
                        let samples = sbuf.samples();
                        interleaved_f32_le.reserve(samples.len() * 4);
                        for &sample in samples {
                            interleaved_f32_le.extend_from_slice(&sample.to_le_bytes());
                        }
                        total_frames += frames;
                    }
                }
                Err(SymError::ResetRequired) => {
                    self.decoder.reset();
                }
                Err(SymError::DecodeError(_)) | Err(SymError::IoError(_)) => {
                    // Skip metadata/Xing headers or corrupt padding packets cleanly.
                }
                Err(other) => {
                    return Err(MediaError::DecodeFailed(format!(
                        "symphonia audio decode failed: {other}"
                    )));
                }
            }
        }

        let duration_secs = if self.sample_rate > 0 {
            (total_frames as f64) / f64::from(self.sample_rate)
        } else {
            0.0
        };

        Ok(DecodedAudioPcm {
            sample_rate: self.sample_rate,
            channels: self.channels,
            duration_secs,
            interleaved_f32_le,
        })
    }
}
