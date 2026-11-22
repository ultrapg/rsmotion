use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;

use rsmotion_core::{
    validate_even_dimensions, AudioCodec, Codec, ColorSpace, FrameRange, HardwareAccel,
    ImageFormat, PixelFormat, ProResProfile, VideoConfig,
};
use rsmotion_encode::{
    mix_and_mux_audio_with_options, save_still_image, AudioMuxOptions, AudioTrackFrame,
    EncodeOptions, FfmpegPipeEncoder, InputPixFmt, RawFrame,
};
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, Mutex};

use crate::browser::{discover_browser, BrowserProcess, GlRenderer};
use crate::client::CdpClient;
use crate::error::{CdpError, Result};
use crate::handshake::{
    get_compositions, install_init_script, mount_composition, navigate_and_wait_bundle,
    seek_and_collect, InitScriptOpts, ResolvedComposition,
};
use crate::server::{resolve_local_asset, EmbeddedServer};
use crate::shm::{capture_png_fallback_bgra, ShmFramePool};

/// Options for Tier 2 CDP video rendering.
#[derive(Debug, Clone)]
pub struct CdpRenderOptions {
    pub serve_url: String,
    pub composition_id: String,
    pub output_path: PathBuf,
    pub codec: Codec,
    pub crf: Option<u8>,
    pub pixel_format: PixelFormat,
    pub hw_accel: HardwareAccel,
    pub gl: GlRenderer,
    pub concurrency: usize,
    pub frame_range: Option<FrameRange>,
    pub every_nth_frame: u32,
    pub scale: f64,
    pub muted: bool,
    pub timeout_ms: u64,
    pub input_props: serde_json::Value,
    pub browser_executable: Option<PathBuf>,
    pub x264_preset: Option<String>,
    pub video_bitrate: Option<String>,
    pub audio_bitrate: Option<String>,
    pub audio_codec: Option<AudioCodec>,
    pub color_space: ColorSpace,
    pub prores_profile: Option<ProResProfile>,
    pub number_of_gif_loops: Option<i32>,
    pub json_progress: bool,
    /// Optional keyframe PNGs `(frame_index -> path)` saved directly during the video render pass.
    pub save_keyframes: BTreeMap<u32, PathBuf>,
}

/// Summary report emitted by `render_media_cdp`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderReport {
    pub composition_id: String,
    pub frames_rendered: u32,
    pub width: u32,
    pub height: u32,
    pub wall_ms: f64,
    pub fps: f64,
    pub ms_per_frame: f64,
    pub encoder_used: String,
    pub capture_mode: String,
    pub has_audio: bool,
    pub output_path: PathBuf,
}

/// Options for Tier 2 CDP single-frame still rendering.
#[derive(Debug, Clone)]
pub struct CdpStillOptions {
    pub serve_url: String,
    pub composition_id: String,
    pub output_path: PathBuf,
    pub frame: u32,
    pub image_format: ImageFormat,
    pub jpeg_quality: u8,
    pub scale: f64,
    pub gl: GlRenderer,
    pub timeout_ms: u64,
    pub input_props: serde_json::Value,
    pub browser_executable: Option<PathBuf>,
}

/// Summary report emitted by `render_still_cdp`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StillReport {
    pub composition_id: String,
    pub frame: u32,
    pub width: u32,
    pub height: u32,
    pub wall_ms: f64,
    pub capture_mode: String,
    pub output_path: PathBuf,
}

/// Options for listing compositions from a Remotion bundle.
#[derive(Debug, Clone)]
pub struct CdpCompositionsOptions {
    pub serve_url: String,
    pub gl: GlRenderer,
    pub timeout_ms: u64,
    pub input_props: serde_json::Value,
    pub browser_executable: Option<PathBuf>,
}

fn resolve_serve_target(serve_url: &str) -> Result<(Option<PathBuf>, Option<String>)> {
    if serve_url.starts_with("http://") || serve_url.starts_with("https://") {
        return Ok((None, Some(serve_url.to_string())));
    }
    let path = PathBuf::from(serve_url);
    if !path.exists() {
        return Err(CdpError::Server(format!(
            "serveUrl path does not exist: {}",
            path.display()
        )));
    }
    let dir = if path.is_file() {
        path.parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf()
    } else {
        path
    };
    Ok((Some(dir), None))
}

async fn setup_eval_and_select(
    client: &CdpClient,
    page_url: &str,
    proxy_port: u16,
    input_props: &serde_json::Value,
    audio_enabled: bool,
    video_enabled: bool,
    timeout_ms: u64,
) -> Result<(Vec<ResolvedComposition>, String, String)> {
    let (target_id, session_id) = client.create_page_session().await?;
    let res = async {
        install_init_script(
            client,
            &session_id,
            &InitScriptOpts {
                timeout_ms,
                is_main_tab: true,
                sample_rate: 48000,
                serialized_input_props: serde_json::to_string(input_props)?,
                initial_frame: 0,
                proxy_port,
                audio_enabled,
                video_enabled,
                log_level: "warn".to_string(),
            },
        )
        .await?;
        navigate_and_wait_bundle(client, &session_id, page_url, timeout_ms).await?;
        get_compositions(client, &session_id, timeout_ms).await
    }
    .await;

    match res {
        Ok(comps) => Ok((comps, target_id, session_id)),
        Err(e) => {
            client.close_target(&target_id).await;
            Err(e)
        }
    }
}

/// Lists all compositions from a Remotion bundle via headless Chrome.
pub async fn list_compositions_cdp(opts: CdpCompositionsOptions) -> Result<Vec<VideoConfig>> {
    let (serve_dir, explicit_url) = resolve_serve_target(&opts.serve_url)?;
    let (browser_exe, is_headless_shell) =
        discover_browser(opts.browser_executable.as_deref(), serve_dir.as_deref())?;
    let server = EmbeddedServer::start(serve_dir)?;
    let page_url = explicit_url.unwrap_or_else(|| server.base_url());

    let browser = BrowserProcess::launch(&browser_exe, is_headless_shell, opts.gl).await?;
    let res = async {
        let client = CdpClient::connect(browser.ws_url()).await?;
        let (comps, target_id, _) = setup_eval_and_select(
            &client,
            &page_url,
            server.port(),
            &opts.input_props,
            false,
            false,
            opts.timeout_ms,
        )
        .await?;
        client.close_target(&target_id).await;
        Ok(comps.into_iter().map(|c| c.config).collect())
    }
    .await;

    browser.shutdown().await;
    drop(server);
    res
}

/// Renders one still frame from a Remotion composition to PNG, JPEG, or WebP.
pub async fn render_still_cdp(opts: CdpStillOptions) -> Result<StillReport> {
    let t0 = Instant::now();
    let (serve_dir, explicit_url) = resolve_serve_target(&opts.serve_url)?;
    let (browser_exe, is_headless_shell) =
        discover_browser(opts.browser_executable.as_deref(), serve_dir.as_deref())?;
    let server = EmbeddedServer::start(serve_dir.clone())?;
    let page_url = explicit_url.unwrap_or_else(|| server.base_url());

    let browser = BrowserProcess::launch(&browser_exe, is_headless_shell, opts.gl).await?;
    let res = async {
        let client = CdpClient::connect(browser.ws_url()).await?;
        let (comps, target_id, session_id) = setup_eval_and_select(
            &client,
            &page_url,
            server.port(),
            &opts.input_props,
            false,
            true,
            opts.timeout_ms,
        )
        .await?;

        let cap_res = async {
            let comp = comps
                .into_iter()
                .find(|c| c.config.id == opts.composition_id)
                .ok_or_else(|| CdpError::CompositionNotFound(opts.composition_id.clone()))?;

            let width = ((f64::from(comp.config.width) * opts.scale).round() as u32).max(1);
            let height = ((f64::from(comp.config.height) * opts.scale).round() as u32).max(1);

            mount_composition(&client, &session_id, &comp, opts.scale, opts.timeout_ms).await?;

            let pool = ShmFramePool::try_create(&client, &session_id, width, height).await?;
            let _ = seek_and_collect(
                &client,
                &session_id,
                &comp.config.id,
                opts.frame,
                true,
                false,
                opts.timeout_ms,
            )
            .await?;

            let (bgra, mode) = match &pool {
                Some(p) => (
                    p.capture_bgra(&client, &target_id, &session_id, "still-0")
                        .await?,
                    "shm-bgra",
                ),
                None => (
                    capture_png_fallback_bgra(&client, &target_id, &session_id, width, height)
                        .await?,
                    "png-fallback",
                ),
            };
            if let Some(p) = pool {
                p.destroy(&client, &session_id).await;
            }

            save_still_image(
                &opts.output_path,
                width,
                height,
                &bgra,
                InputPixFmt::Bgra,
                opts.image_format,
                opts.jpeg_quality,
            )?;

            Ok::<_, CdpError>((width, height, mode.to_string()))
        }
        .await;
        client.close_target(&target_id).await;
        let (width, height, capture_mode) = cap_res?;

        Ok(StillReport {
            composition_id: opts.composition_id,
            frame: opts.frame,
            width,
            height,
            wall_ms: t0.elapsed().as_secs_f64() * 1000.0,
            capture_mode,
            output_path: opts.output_path,
        })
    }
    .await;

    browser.shutdown().await;
    drop(server);
    res
}

struct RenderedJobFrame {
    seq_idx: usize,
    frame_idx: u32,
    bgra: Vec<u8>,
}

/// Renders a Remotion composition to video using Tier 2 (multi-page CDP + Remotion SHM BGRA + VAAPI + Rust audio mix).
pub async fn render_media_cdp(opts: CdpRenderOptions) -> Result<RenderReport> {
    let total_start = Instant::now();
    let (serve_dir, explicit_url) = resolve_serve_target(&opts.serve_url)?;
    let (browser_exe, is_headless_shell) =
        discover_browser(opts.browser_executable.as_deref(), serve_dir.as_deref())?;
    let server = EmbeddedServer::start(serve_dir.clone())?;
    let page_url = explicit_url.unwrap_or_else(|| server.base_url());

    let browser = BrowserProcess::launch(&browser_exe, is_headless_shell, opts.gl).await?;
    let res = render_with_running_browser(opts, &browser, &server, &page_url).await;

    browser.shutdown().await;
    drop(server);
    res.map(|mut r| {
        r.wall_ms = total_start.elapsed().as_secs_f64() * 1000.0;
        r.fps = if r.wall_ms > 0.0 {
            f64::from(r.frames_rendered) / (r.wall_ms / 1000.0)
        } else {
            0.0
        };
        r.ms_per_frame = if r.frames_rendered > 0 {
            r.wall_ms / f64::from(r.frames_rendered)
        } else {
            0.0
        };
        r
    })
}

/// Shared helper that renders inside an already-launched browser + server (used by `benchmark` too).
pub async fn render_with_running_browser(
    opts: CdpRenderOptions,
    browser: &BrowserProcess,
    server: &EmbeddedServer,
    page_url: &str,
) -> Result<RenderReport> {
    let start = Instant::now();
    let client = Arc::new(CdpClient::connect(browser.ws_url()).await?);
    let (comps, eval_target_id, eval_session_id) = setup_eval_and_select(
        &client,
        page_url,
        server.port(),
        &opts.input_props,
        !opts.muted,
        true,
        opts.timeout_ms,
    )
    .await?;

    let comp = match comps
        .into_iter()
        .find(|c| c.config.id == opts.composition_id)
    {
        Some(c) => c,
        None => {
            client.close_target(&eval_target_id).await;
            return Err(CdpError::CompositionNotFound(opts.composition_id.clone()));
        }
    };

    let raw_w = ((f64::from(comp.config.width) * opts.scale).round() as u32).max(1);
    let raw_h = ((f64::from(comp.config.height) * opts.scale).round() as u32).max(1);
    let (width, height) = match validate_even_dimensions(raw_w, raw_h, opts.codec) {
        Ok(dims) => dims,
        Err(e) => {
            client.close_target(&eval_target_id).await;
            return Err(e.into());
        }
    };

    let range = match opts.frame_range {
        Some(r) => r,
        None => match FrameRange::full(comp.config.duration_in_frames) {
            Ok(r) => r,
            Err(e) => {
                client.close_target(&eval_target_id).await;
                return Err(e.into());
            }
        },
    };
    let frames_to_render =
        match range.resolve_frames(comp.config.duration_in_frames, opts.every_nth_frame) {
            Ok(f) => f,
            Err(e) => {
                client.close_target(&eval_target_id).await;
                return Err(e.into());
            }
        };
    let total_frames = frames_to_render.len();
    let workers = opts.concurrency.clamp(1, 3).min(total_frames.max(1));

    let skip_audio_mux = opts.muted || opts.codec == Codec::Gif;
    let temp_ext = match opts.codec {
        Codec::Vp8 | Codec::Vp9 => "rsmotion-video-only.webm",
        Codec::ProRes => "rsmotion-video-only.mov",
        _ => "rsmotion-video-only.mp4",
    };
    let temp_video_path = if skip_audio_mux {
        opts.output_path.clone()
    } else {
        opts.output_path.with_extension(temp_ext)
    };

    let pipe = match FfmpegPipeEncoder::spawn(EncodeOptions {
        width,
        height,
        fps: comp.config.fps,
        output_path: temp_video_path.clone(),
        codec: opts.codec,
        crf: opts.crf,
        pixel_format: opts.pixel_format,
        input_pix_fmt: InputPixFmt::Bgra,
        hw_accel: opts.hw_accel,
        audio_input: None,
        muted: true,
        x264_preset: opts.x264_preset.clone(),
        ring_capacity: 8,
        video_bitrate: opts.video_bitrate.clone(),
        audio_bitrate: opts.audio_bitrate.clone(),
        audio_codec: opts.audio_codec,
        color_space: opts.color_space,
        prores_profile: opts.prores_profile,
        number_of_gif_loops: opts.number_of_gif_loops,
    }) {
        Ok(p) => p,
        Err(e) => {
            client.close_target(&eval_target_id).await;
            return Err(e.into());
        }
    };

    let serialized_input_props = serde_json::to_string(&opts.input_props)?;
    let frames_arc = Arc::new(frames_to_render);
    let next_idx = Arc::new(AtomicUsize::new(0));
    let abort = Arc::new(AtomicBool::new(false));
    let audio_tracks: Arc<Mutex<Vec<AudioTrackFrame>>> = Arc::new(Mutex::new(Vec::new()));
    let capture_mode_arc: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let keyframes_arc = Arc::new(opts.save_keyframes.clone());

    let (frame_tx, mut frame_rx) = mpsc::channel::<RenderedJobFrame>(8);

    // Feeder task guarantees strict sequential frame ordering into FFmpeg while keeping bounded memory.
    let json_progress = opts.json_progress;
    let feeder_handle =
        tokio::task::spawn_blocking(move || -> Result<rsmotion_encode::EncodeStats> {
            let mut reorder: BTreeMap<usize, RenderedJobFrame> = BTreeMap::new();
            let mut expected_seq = 0usize;

            while let Some(item) = frame_rx.blocking_recv() {
                reorder.insert(item.seq_idx, item);
                while let Some(ready) = reorder.remove(&expected_seq) {
                    if let Some(kf_path) = keyframes_arc.get(&ready.frame_idx) {
                        save_still_image(
                            kf_path,
                            width,
                            height,
                            &ready.bgra,
                            InputPixFmt::Bgra,
                            ImageFormat::Png,
                            100,
                        )?;
                    }
                    pipe.push_frame(RawFrame {
                        index: expected_seq as u32,
                        width,
                        height,
                        data: ready.bgra,
                    })?;
                    expected_seq += 1;
                    if json_progress {
                        let progress = (expected_seq as f64) / (total_frames.max(1) as f64);
                        println!(
                            "{}",
                            serde_json::json!({
                                "type": "progress",
                                "renderedFrames": expected_seq,
                                "totalFrames": total_frames,
                                "progress": progress,
                            })
                        );
                    }
                }
            }
            Ok(pipe.finish()?)
        });

    let mut eval_session_slot = Some((eval_target_id, eval_session_id));
    let mut worker_handles = Vec::with_capacity(workers);
    for worker_id in 0..workers {
        let client = Arc::clone(&client);
        let page_url = page_url.to_string();
        let comp = comp.clone();
        let frames_arc = Arc::clone(&frames_arc);
        let next_idx = Arc::clone(&next_idx);
        let abort = Arc::clone(&abort);
        let audio_tracks = Arc::clone(&audio_tracks);
        let capture_mode_arc = Arc::clone(&capture_mode_arc);
        let frame_tx = frame_tx.clone();
        let serialized_input_props = serialized_input_props.clone();
        let proxy_port = server.port();
        let timeout_ms = opts.timeout_ms;
        let scale = opts.scale;
        let muted = opts.muted;
        let reused_page = eval_session_slot.take();

        worker_handles.push(tokio::spawn(async move {
            let (target_id, session_id, needs_nav) = match reused_page {
                Some((tid, sid)) => (tid, sid, false),
                None => {
                    let (tid, sid) = client.create_page_session().await?;
                    (tid, sid, true)
                }
            };
            let worker_res = async {
                if needs_nav {
                    let initial_frame = frames_arc.get(worker_id).copied().unwrap_or(0);
                    install_init_script(
                        &client,
                        &session_id,
                        &InitScriptOpts {
                            timeout_ms,
                            is_main_tab: worker_id == 0,
                            sample_rate: comp.default_sample_rate.unwrap_or(48000),
                            serialized_input_props,
                            initial_frame,
                            proxy_port,
                            audio_enabled: !muted,
                            video_enabled: true,
                            log_level: "warn".to_string(),
                        },
                    )
                    .await?;
                    navigate_and_wait_bundle(&client, &session_id, &page_url, timeout_ms).await?;
                }
                mount_composition(&client, &session_id, &comp, scale, timeout_ms).await?;

                let pool = ShmFramePool::try_create(&client, &session_id, width, height).await?;
                {
                    let mut guard = capture_mode_arc.lock().await;
                    if guard.is_none() {
                        *guard = Some(
                            if pool.is_some() {
                                "shm-bgra"
                            } else {
                                "png-fallback"
                            }
                            .to_string(),
                        );
                    }
                }

                let inner = async {
                    loop {
                        if abort.load(Ordering::Relaxed) {
                            break;
                        }
                        let seq_idx = next_idx.fetch_add(1, Ordering::Relaxed);
                        if seq_idx >= frames_arc.len() {
                            break;
                        }
                        let frame_idx = frames_arc[seq_idx];
                        let collected = seek_and_collect(
                            &client,
                            &session_id,
                            &comp.config.id,
                            frame_idx,
                            true,
                            !muted,
                            timeout_ms,
                        )
                        .await?;

                        if !muted && !collected.is_empty() {
                            audio_tracks.lock().await.extend(collected);
                        }

                        let frame_id = format!("f-{worker_id}-{seq_idx}");
                        let bgra = match &pool {
                            Some(p) => {
                                p.capture_bgra(&client, &target_id, &session_id, &frame_id)
                                    .await?
                            }
                            None => {
                                capture_png_fallback_bgra(
                                    &client,
                                    &target_id,
                                    &session_id,
                                    width,
                                    height,
                                )
                                .await?
                            }
                        };

                        if frame_tx
                            .send(RenderedJobFrame {
                                seq_idx,
                                frame_idx,
                                bgra,
                            })
                            .await
                            .is_err()
                        {
                            abort.store(true, Ordering::Relaxed);
                            return Err(CdpError::Protocol("frame feeder channel closed".into()));
                        }
                    }
                    Ok::<(), CdpError>(())
                }
                .await;
                if inner.is_err() {
                    abort.store(true, Ordering::Relaxed);
                }

                if let Some(p) = pool {
                    p.destroy(&client, &session_id).await;
                }
                inner
            }
            .await;

            client.close_target(&target_id).await;
            worker_res
        }));
    }

    drop(frame_tx);

    let mut first_err: Option<CdpError> = None;
    for handle in worker_handles {
        match handle.await {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                if first_err.is_none() {
                    first_err = Some(e);
                }
            }
            Err(e) => {
                if first_err.is_none() {
                    first_err = Some(CdpError::Protocol(format!("worker task panicked: {e}")));
                }
            }
        }
    }

    let encode_stats = feeder_handle
        .await
        .map_err(|e| CdpError::Protocol(format!("feeder panicked: {e}")))??;

    if let Some(err) = first_err {
        let _ = std::fs::remove_file(&temp_video_path);
        return Err(err);
    }

    let has_audio = if skip_audio_mux {
        false
    } else {
        let collected = audio_tracks.lock().await;
        let serve_dir = server.serve_dir().map(Path::to_path_buf);
        let sample_rate = comp.default_sample_rate.unwrap_or(48000);
        let mux_opts = AudioMuxOptions {
            audio_codec: opts.audio_codec.unwrap_or(match opts.codec {
                Codec::Vp8 | Codec::Vp9 => AudioCodec::Opus,
                _ => AudioCodec::Aac,
            }),
            audio_bitrate: opts.audio_bitrate.clone(),
        };
        mix_and_mux_audio_with_options(
            &temp_video_path,
            &opts.output_path,
            &collected,
            comp.config.fps,
            comp.config.duration_in_frames,
            sample_rate,
            &mux_opts,
            |src| resolve_local_asset(src, serve_dir.as_deref()),
        )?
    };

    let wall_ms = start.elapsed().as_secs_f64() * 1000.0;
    let frames_rendered = encode_stats.frames_encoded;
    let capture_mode = capture_mode_arc
        .lock()
        .await
        .clone()
        .unwrap_or_else(|| "shm-bgra".to_string());

    Ok(RenderReport {
        composition_id: opts.composition_id,
        frames_rendered,
        width,
        height,
        wall_ms,
        fps: if wall_ms > 0.0 {
            f64::from(frames_rendered) / (wall_ms / 1000.0)
        } else {
            0.0
        },
        ms_per_frame: if frames_rendered > 0 {
            wall_ms / f64::from(frames_rendered)
        } else {
            0.0
        },
        encoder_used: encode_stats.encoder_used,
        capture_mode,
        has_audio,
        output_path: opts.output_path,
    })
}
