use std::io::{BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::sync::OnceLock;
use std::thread::JoinHandle;
use std::time::Instant;

use rsmotion_core::{
    validate_even_dimensions, AudioCodec, Codec, ColorSpace, HardwareAccel, PixelFormat,
    ProResProfile,
};
use serde::{Deserialize, Serialize};

use crate::error::{EncodeError, Result};
use crate::probe::{probe_best_h264_encoder, ResolvedEncoder};
use crate::yuv::convert_rgba_or_bgra_to_nv12;

static VAAPI_BITRATE_SUPPORTED: OnceLock<bool> = OnceLock::new();

fn vaapi_supports_bitrate(device: &Path) -> bool {
    *VAAPI_BITRATE_SUPPORTED.get_or_init(|| {
        Command::new("ffmpeg")
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
                "-b:v",
                "1M",
                "-frames:v",
                "2",
                "-f",
                "null",
                "-",
            ])
            .output()
            .is_ok_and(|o| o.status.success())
    })
}

/// Input raw pixel layout sent to FFmpeg's stdin pipe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum InputPixFmt {
    #[default]
    Rgba,
    Bgra,
    Nv12,
}

impl InputPixFmt {
    pub fn as_str(self) -> &'static str {
        match self {
            InputPixFmt::Rgba => "rgba",
            InputPixFmt::Bgra => "bgra",
            InputPixFmt::Nv12 => "nv12",
        }
    }
}

/// Configuration for piped FFmpeg encoding.
#[derive(Debug, Clone)]
pub struct EncodeOptions {
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub output_path: PathBuf,
    pub codec: Codec,
    pub crf: Option<u8>,
    pub pixel_format: PixelFormat,
    pub input_pix_fmt: InputPixFmt,
    pub hw_accel: HardwareAccel,
    pub audio_input: Option<PathBuf>,
    pub muted: bool,
    pub x264_preset: Option<String>,
    pub ring_capacity: usize,
    pub video_bitrate: Option<String>,
    pub audio_bitrate: Option<String>,
    pub audio_codec: Option<AudioCodec>,
    pub color_space: ColorSpace,
    pub prores_profile: Option<ProResProfile>,
    pub number_of_gif_loops: Option<i32>,
}

impl Default for EncodeOptions {
    fn default() -> Self {
        Self {
            width: 1920,
            height: 1080,
            fps: 30.0,
            output_path: PathBuf::from("out.mp4"),
            codec: Codec::H264,
            crf: None,
            pixel_format: PixelFormat::Yuv420p,
            input_pix_fmt: InputPixFmt::Rgba,
            hw_accel: HardwareAccel::Auto,
            audio_input: None,
            muted: false,
            x264_preset: None,
            ring_capacity: 8,
            video_bitrate: None,
            audio_bitrate: None,
            audio_codec: None,
            color_space: ColorSpace::Default,
            prores_profile: None,
            number_of_gif_loops: None,
        }
    }
}

/// One uncompressed pixel frame ready for encoding.
#[derive(Debug, Clone)]
pub struct RawFrame {
    pub index: u32,
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

/// Summary statistics returned when the pipe encoder finishes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncodeStats {
    pub frames_encoded: u32,
    pub wall_ms: f64,
    pub fps: f64,
    pub encoder_used: String,
    pub hw_accel: HardwareAccel,
}

enum PipeMsg {
    Frame(RawFrame),
    Duplicate,
}

/// Bounded-memory FFmpeg rawvideo pipe encoder with buffer recycling and direct NV12 VAAPI upload.
pub struct FfmpegPipeEncoder {
    tx: Option<SyncSender<PipeMsg>>,
    recycle_rx: Receiver<Vec<u8>>,
    writer_handle: Option<JoinHandle<std::io::Result<u32>>>,
    stderr_handle: Option<JoinHandle<String>>,
    child: Child,
    expected_width: u32,
    expected_height: u32,
    expected_bytes: usize,
    resolved_encoder: ResolvedEncoder,
    started_at: Instant,
}

impl FfmpegPipeEncoder {
    /// Probes the target encoder, spawns `ffmpeg`, and starts the bounded writer thread.
    pub fn spawn(options: EncodeOptions) -> Result<Self> {
        let (width, height) =
            validate_even_dimensions(options.width, options.height, options.codec)?;
        let expected_bytes = match options.input_pix_fmt {
            InputPixFmt::Nv12 => ((width as usize) * (height as usize) * 3) / 2,
            InputPixFmt::Rgba | InputPixFmt::Bgra => (width as usize) * (height as usize) * 4,
        };
        let crf = options
            .crf
            .or_else(|| options.codec.default_crf())
            .unwrap_or(18);
        options.codec.validate_crf(crf)?;

        let resolved = match options.codec {
            Codec::H264 => {
                let first = probe_best_h264_encoder(options.hw_accel)?;
                if options.hw_accel == HardwareAccel::Auto
                    && options.video_bitrate.is_some()
                    && first.kind == HardwareAccel::Vaapi
                    && !first
                        .vaapi_device
                        .as_deref()
                        .is_some_and(vaapi_supports_bitrate)
                {
                    probe_best_h264_encoder(HardwareAccel::Software)?
                } else {
                    first
                }
            }
            _ => ResolvedEncoder {
                kind: HardwareAccel::Software,
                ffmpeg_codec: match options.codec {
                    Codec::H265 => "libx265".to_string(),
                    Codec::Vp8 => "libvpx".to_string(),
                    Codec::Vp9 => "libvpx-vp9".to_string(),
                    Codec::ProRes => "prores_ks".to_string(),
                    Codec::Gif => "gif".to_string(),
                    _ => "libx264".to_string(),
                },
                vaapi_device: None,
            },
        };

        let vaapi_nv12_fastpath = resolved.kind == HardwareAccel::Vaapi
            && options.pixel_format == PixelFormat::Yuv420p
            && matches!(options.input_pix_fmt, InputPixFmt::Rgba | InputPixFmt::Bgra);
        let pipe_pix_fmt = if vaapi_nv12_fastpath {
            InputPixFmt::Nv12
        } else {
            options.input_pix_fmt
        };

        if let Some(parent) = options.output_path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }

        let mut cmd = Command::new("ffmpeg");
        cmd.args(["-y", "-hide_banner", "-loglevel", "error"]);

        if let Some(ref dev) = resolved.vaapi_device {
            cmd.arg("-vaapi_device").arg(dev);
        }

        let size_arg = format!("{width}x{height}");
        let fps_arg = format!("{}", options.fps);
        cmd.args([
            "-f",
            "rawvideo",
            "-pix_fmt",
            pipe_pix_fmt.as_str(),
            "-s",
            &size_arg,
            "-r",
            &fps_arg,
            "-i",
            "pipe:0",
        ]);

        let has_audio = !options.muted && options.audio_input.is_some();
        if let Some(ref audio) = options.audio_input {
            if !options.muted {
                let acodec = options.audio_codec.unwrap_or(AudioCodec::Aac);
                cmd.arg("-i").arg(audio);
                cmd.args([
                    "-map",
                    "0:v:0",
                    "-map",
                    "1:a:0",
                    "-c:a",
                    acodec.ffmpeg_codec(),
                ]);
                if !matches!(acodec, AudioCodec::Pcm16) {
                    let abr = options.audio_bitrate.as_deref().unwrap_or("192k");
                    cmd.args(["-b:a", abr]);
                }
                cmd.arg("-shortest");
            }
        }
        if !has_audio {
            cmd.arg("-an");
        }

        let use_bitrate = options.video_bitrate.is_some() && options.crf.is_none();
        match options.codec {
            Codec::Gif => {
                let loops = options.number_of_gif_loops.unwrap_or(0).to_string();
                cmd.args(["-c:v", "gif", "-loop", &loops]);
            }
            Codec::ProRes => {
                let profile = options.prores_profile.unwrap_or(ProResProfile::Hq);
                let pr_pix_fmt = if matches!(
                    profile,
                    ProResProfile::FourFourFourFour | ProResProfile::FourFourFourFourXq
                ) {
                    "yuva444p10le"
                } else {
                    "yuv422p10le"
                };
                cmd.args([
                    "-pix_fmt",
                    pr_pix_fmt,
                    "-c:v",
                    "prores_ks",
                    "-profile:v",
                    profile.profile_id(),
                ]);
                if let Some(ref br) = options.video_bitrate {
                    cmd.args(["-b:v", br]);
                }
            }
            _ => match resolved.kind {
                HardwareAccel::Vaapi => {
                    let setparams_prefix = match options.color_space {
                        ColorSpace::Default => "",
                        ColorSpace::Bt709 => {
                            "setparams=colorspace=bt709:color_primaries=bt709:color_trc=bt709,"
                        }
                        ColorSpace::Bt2020Ncl => {
                            "setparams=colorspace=bt2020nc:color_primaries=bt2020:color_trc=smpte2084,"
                        }
                    };
                    let vf = if pipe_pix_fmt == InputPixFmt::Nv12 {
                        format!("{setparams_prefix}hwupload")
                    } else {
                        format!("{setparams_prefix}format=nv12,hwupload")
                    };
                    cmd.args(["-vf", &vf, "-c:v", &resolved.ffmpeg_codec]);
                    if !use_bitrate {
                        cmd.args(["-qp", &crf.to_string()]);
                    }
                    if let Some(ref br) = options.video_bitrate {
                        cmd.args(["-b:v", br]);
                    }
                }
                HardwareAccel::Qsv => {
                    cmd.args(["-vf", "format=nv12", "-c:v", &resolved.ffmpeg_codec]);
                    if !use_bitrate {
                        cmd.args(["-global_quality", &crf.to_string()]);
                    }
                    if let Some(ref br) = options.video_bitrate {
                        cmd.args(["-b:v", br]);
                    }
                }
                HardwareAccel::Software | HardwareAccel::Auto => {
                    let preset = options.x264_preset.as_deref().unwrap_or("medium");
                    cmd.args([
                        "-pix_fmt",
                        options.pixel_format.as_str(),
                        "-c:v",
                        &resolved.ffmpeg_codec,
                    ]);
                    if !use_bitrate {
                        cmd.args(["-crf", &crf.to_string()]);
                    }
                    if let Some(ref br) = options.video_bitrate {
                        cmd.args(["-b:v", br]);
                    }
                    if matches!(options.codec, Codec::H264 | Codec::H265) {
                        cmd.args(["-preset", preset]);
                    }
                }
            },
        }

        if resolved.kind != HardwareAccel::Vaapi {
            match options.color_space {
                ColorSpace::Default => {}
                ColorSpace::Bt709 => {
                    cmd.args([
                        "-colorspace",
                        "bt709",
                        "-color_primaries",
                        "bt709",
                        "-color_trc",
                        "bt709",
                    ]);
                }
                ColorSpace::Bt2020Ncl => {
                    cmd.args([
                        "-colorspace",
                        "bt2020nc",
                        "-color_primaries",
                        "bt2020",
                        "-color_trc",
                        "smpte2084",
                    ]);
                }
            }
        }

        if matches!(options.codec, Codec::H264 | Codec::H265) {
            cmd.args(["-movflags", "+faststart"]);
        }

        cmd.arg(&options.output_path);
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());

        let mut child = cmd.spawn()?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| EncodeError::Io(std::io::Error::other("missing ffmpeg stdin")))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| EncodeError::Io(std::io::Error::other("missing ffmpeg stderr")))?;

        let cap = options.ring_capacity.max(1);
        let (tx, rx) = mpsc::sync_channel::<PipeMsg>(cap);
        let (recycle_tx, recycle_rx) = mpsc::sync_channel::<Vec<u8>>(cap + 2);
        let is_bgra = options.input_pix_fmt == InputPixFmt::Bgra;
        let nv12_len = ((width as usize) * (height as usize) * 3) / 2;

        let writer_handle = std::thread::Builder::new()
            .name("rsmotion-ffmpeg-pipe".to_string())
            .spawn(move || -> std::io::Result<u32> {
                let mut writer = BufWriter::with_capacity(512 * 1024, stdin);
                let mut count = 0u32;
                let mut nv12_buf = if vaapi_nv12_fastpath {
                    vec![0u8; nv12_len]
                } else {
                    Vec::new()
                };
                let mut last_raw: Option<Vec<u8>> = None;
                let mut has_prev = false;

                while let Ok(msg) = rx.recv() {
                    match msg {
                        PipeMsg::Frame(frame) => {
                            if vaapi_nv12_fastpath {
                                convert_rgba_or_bgra_to_nv12(
                                    &frame.data,
                                    &mut nv12_buf,
                                    width,
                                    height,
                                    is_bgra,
                                );
                                writer.write_all(&nv12_buf)?;
                                let _ = recycle_tx.try_send(frame.data);
                            } else {
                                writer.write_all(&frame.data)?;
                                if let Some(prev) = last_raw.replace(frame.data) {
                                    let _ = recycle_tx.try_send(prev);
                                }
                            }
                            has_prev = true;
                            count += 1;
                        }
                        PipeMsg::Duplicate => {
                            if !has_prev {
                                return Err(std::io::Error::other(
                                    "duplicate frame sent before first frame",
                                ));
                            }
                            if vaapi_nv12_fastpath {
                                writer.write_all(&nv12_buf)?;
                            } else if let Some(ref raw) = last_raw {
                                writer.write_all(raw)?;
                            }
                            count += 1;
                        }
                    }
                }
                writer.flush()?;
                drop(writer);
                Ok(count)
            })?;

        let stderr_handle = std::thread::Builder::new()
            .name("rsmotion-ffmpeg-stderr".to_string())
            .spawn(move || -> String {
                let mut reader = stderr;
                let mut buf = String::new();
                let _ = reader.read_to_string(&mut buf);
                buf
            })?;

        Ok(Self {
            tx: Some(tx),
            recycle_rx,
            writer_handle: Some(writer_handle),
            stderr_handle: Some(stderr_handle),
            child,
            expected_width: width,
            expected_height: height,
            expected_bytes,
            resolved_encoder: resolved,
            started_at: Instant::now(),
        })
    }

    /// Acquires a recycled warm frame buffer or allocates one matching expected frame bytes.
    pub fn acquire_frame_buffer(&self) -> Vec<u8> {
        match self.recycle_rx.try_recv() {
            Ok(mut buf) => {
                if buf.len() != self.expected_bytes {
                    buf.resize(self.expected_bytes, 0);
                }
                buf
            }
            Err(_) => vec![0u8; self.expected_bytes],
        }
    }

    /// Pushes one frame into the bounded ring buffer (blocks when full for backpressure).
    pub fn push_frame(&self, frame: RawFrame) -> Result<()> {
        if frame.width != self.expected_width || frame.height != self.expected_height {
            return Err(EncodeError::InvalidFrameBuffer {
                index: frame.index,
                expected_bytes: self.expected_bytes,
                actual_bytes: frame.data.len(),
            });
        }
        if frame.data.len() != self.expected_bytes {
            return Err(EncodeError::InvalidFrameBuffer {
                index: frame.index,
                expected_bytes: self.expected_bytes,
                actual_bytes: frame.data.len(),
            });
        }
        let tx = self.tx.as_ref().ok_or(EncodeError::ChannelClosed)?;
        tx.send(PipeMsg::Frame(frame))
            .map_err(|_| EncodeError::ChannelClosed)
    }

    /// Repeats the immediately preceding encoded frame with zero conversion and zero copy.
    pub fn push_duplicate_frame(&self, _index: u32) -> Result<()> {
        let tx = self.tx.as_ref().ok_or(EncodeError::ChannelClosed)?;
        tx.send(PipeMsg::Duplicate)
            .map_err(|_| EncodeError::ChannelClosed)
    }

    /// Closes the pipe, waits for FFmpeg to finalize the file, and returns timing stats.
    pub fn finish(mut self) -> Result<EncodeStats> {
        drop(self.tx.take());

        let frames_encoded = if let Some(handle) = self.writer_handle.take() {
            match handle.join() {
                Ok(Ok(count)) => count,
                Ok(Err(e)) => {
                    let _ = self.child.wait();
                    let stderr = self
                        .stderr_handle
                        .take()
                        .and_then(|h| h.join().ok())
                        .unwrap_or_default();
                    return Err(EncodeError::FfmpegFailed {
                        code: -1,
                        stderr: format!("pipe write error: {e}; ffmpeg stderr: {stderr}"),
                    });
                }
                Err(_) => return Err(EncodeError::ChannelClosed),
            }
        } else {
            0
        };

        let status = self.child.wait()?;
        let stderr = self
            .stderr_handle
            .take()
            .and_then(|h| h.join().ok())
            .unwrap_or_default();

        if !status.success() {
            return Err(EncodeError::FfmpegFailed {
                code: status.code().unwrap_or(-1),
                stderr: stderr.trim().to_string(),
            });
        }

        let wall_ms = self.started_at.elapsed().as_secs_f64() * 1000.0;
        let fps = if wall_ms > 0.0 {
            f64::from(frames_encoded) / (wall_ms / 1000.0)
        } else {
            0.0
        };

        Ok(EncodeStats {
            frames_encoded,
            wall_ms,
            fps,
            encoder_used: self.resolved_encoder.ffmpeg_codec,
            hw_accel: self.resolved_encoder.kind,
        })
    }
}
