use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::Instant;

use anyhow::{bail, Context, Result};
use rsmotion_analyze::PreparedTier1Host;
use rsmotion_core::{
    validate_even_dimensions, AudioCodec, Codec, ColorSpace, FrameRange, HardwareAccel,
    ImageFormat, PixelFormat, ProResProfile, RasterBackend, VideoConfig,
};
use rsmotion_encode::{
    concat_chunks, mix_and_mux_audio_with_options, save_still_image, AudioMuxOptions,
    AudioTrackFrame, EncodeOptions, FfmpegPipeEncoder, InputPixFmt, RawFrame,
};
use rsmotion_js::V8BundleHost;
use rsmotion_media::resolve_or_fetch_asset;
use rsmotion_raster::SceneRenderer;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Marker error emitted when auto-dispatch Tier 1 encounters an unsupported feature mid-render.
#[derive(Debug)]
pub struct LateUnsupportedFeatureError {
    pub frame: u32,
    pub features: Vec<String>,
}

impl fmt::Display for LateUnsupportedFeatureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Tier 1 encountered unsupported feature(s) at frame {}: {}",
            self.frame,
            self.features.join(", ")
        )
    }
}

impl std::error::Error for LateUnsupportedFeatureError {}

/// Options for Tier 1 native browserless video rendering.
#[derive(Debug, Clone)]
pub struct Tier1RenderOptions {
    pub serve_url: String,
    pub composition_id: String,
    pub output_path: PathBuf,
    pub codec: Codec,
    pub crf: Option<u8>,
    pub pixel_format: PixelFormat,
    pub hw_accel: HardwareAccel,
    pub raster_backend: RasterBackend,
    pub frame_range: Option<FrameRange>,
    pub every_nth_frame: u32,
    pub scale: f64,
    pub muted: bool,
    pub timeout_ms: u64,
    pub input_props: Value,
    pub x264_preset: Option<String>,
    pub video_bitrate: Option<String>,
    pub audio_bitrate: Option<String>,
    pub audio_codec: Option<AudioCodec>,
    pub color_space: ColorSpace,
    pub prores_profile: Option<ProResProfile>,
    pub number_of_gif_loops: Option<i32>,
    pub concurrency: usize,
    pub offline: bool,
    pub json_progress: bool,
    pub abort_on_unsupported: bool,
    pub save_keyframes: BTreeMap<u32, PathBuf>,
}

/// Summary report emitted by `render_media_tier1`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tier1RenderReport {
    pub tier: u8,
    pub composition_id: String,
    pub output_path: PathBuf,
    pub width: u32,
    pub height: u32,
    pub frames_rendered: u32,
    pub deduped_frames: u32,
    pub raster_backend: String,
    pub gpu_renderer: Option<String>,
    pub encoder_used: String,
    pub has_audio: bool,
    pub wall_ms: f64,
    pub fps: f64,
    pub ms_per_frame: f64,
    pub js_ms: f64,
    pub layout_ms: f64,
    pub video_decode_ms: f64,
    pub draw_ms: f64,
    pub readback_ms: f64,
    pub encode_wall_ms: f64,
    pub audio_ms: f64,
    pub unsupported_features: Vec<String>,
}

/// Options for Tier 1 single-frame still rendering.
#[derive(Debug, Clone)]
pub struct Tier1StillOptions {
    pub serve_url: String,
    pub composition_id: String,
    pub output_path: PathBuf,
    pub frame: u32,
    pub image_format: ImageFormat,
    pub jpeg_quality: u8,
    pub scale: f64,
    pub raster_backend: RasterBackend,
    pub timeout_ms: u64,
    pub offline: bool,
    pub input_props: Value,
}

/// Summary report emitted by `render_still_tier1`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tier1StillReport {
    pub tier: u8,
    pub composition_id: String,
    pub frame: u32,
    pub width: u32,
    pub height: u32,
    pub raster_backend: String,
    pub gpu_renderer: Option<String>,
    pub wall_ms: f64,
    pub js_ms: f64,
    pub layout_ms: f64,
    pub video_decode_ms: f64,
    pub draw_ms: f64,
    pub readback_ms: f64,
    pub unsupported_features: Vec<String>,
    pub output_path: PathBuf,
}

pub fn resolve_bundle_dir(serve_url: &str) -> Result<PathBuf> {
    if serve_url.starts_with("http://") || serve_url.starts_with("https://") {
        bail!("Tier 1 requires a local bundle directory path (got HTTP URL `{serve_url}`); use Tier 2 for HTTP serveUrl");
    }
    let path = PathBuf::from(serve_url);
    if !path.exists() {
        bail!("bundle path does not exist: {}", path.display());
    }
    if path.is_file() {
        Ok(path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf())
    } else {
        Ok(path)
    }
}

pub fn list_compositions_tier1_with_offline(
    serve_url: &str,
    input_props: &Value,
    timeout_ms: u64,
    offline: bool,
) -> Result<Vec<VideoConfig>> {
    let bundle_dir = resolve_bundle_dir(serve_url)?;
    let mut host = V8BundleHost::new_with_options(&bundle_dir, input_props, timeout_ms, offline)
        .context("initializing V8 bundle host")?;
    let comps = host.get_compositions()?;
    Ok(comps.into_iter().map(|c| c.config).collect())
}

fn prepare_host(
    bundle_dir: &Path,
    composition_id: &str,
    input_props: &Value,
    timeout_ms: u64,
    offline: bool,
    prepared: Option<PreparedTier1Host>,
) -> Result<PreparedTier1Host> {
    if let Some(p) = prepared {
        return Ok(p);
    }
    let mut host = V8BundleHost::new_with_options(bundle_dir, input_props, timeout_ms, offline)?;
    let comps = host.get_compositions()?;
    let comp = comps
        .into_iter()
        .find(|c| c.config.id == composition_id)
        .with_context(|| format!("composition `{composition_id}` not found"))?;
    host.mount_composition(&comp)?;
    Ok(PreparedTier1Host {
        host,
        composition: comp,
    })
}

pub fn render_still_tier1(opts: Tier1StillOptions) -> Result<Tier1StillReport> {
    render_still_tier1_with_host(opts, None)
}

pub fn render_still_tier1_with_host(
    opts: Tier1StillOptions,
    prepared: Option<PreparedTier1Host>,
) -> Result<Tier1StillReport> {
    let t0 = Instant::now();
    let bundle_dir = resolve_bundle_dir(&opts.serve_url)?;

    let PreparedTier1Host {
        mut host,
        composition: comp,
    } = prepare_host(
        &bundle_dir,
        &opts.composition_id,
        &opts.input_props,
        opts.timeout_ms,
        opts.offline,
        prepared,
    )?;

    let width = ((f64::from(comp.config.width) * opts.scale).round() as u32).max(1);
    let height = ((f64::from(comp.config.height) * opts.scale).round() as u32).max(1);

    let t_js = Instant::now();
    let scene = host.render_frame_tree(
        &comp.config.id,
        comp.config.width,
        comp.config.height,
        opts.frame,
    )?;
    let js_ms = t_js.elapsed().as_secs_f64() * 1000.0;

    let mut renderer = SceneRenderer::new(width, height, opts.raster_backend, &bundle_dir)?;
    renderer.set_offline(opts.offline);
    let backend_name = renderer.backend_name().to_string();
    let gpu_renderer = renderer.gpu_renderer().map(str::to_string);
    let frame = renderer.render_scene(&scene, opts.scale)?;

    save_still_image(
        &opts.output_path,
        width,
        height,
        frame.rgba,
        InputPixFmt::Rgba,
        opts.image_format,
        opts.jpeg_quality,
    )?;

    let unsupported_features: Vec<String> = scene
        .unsupported_features
        .iter()
        .map(|u| format!("{}:{}", u.feature, u.tag))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();

    Ok(Tier1StillReport {
        tier: 1,
        composition_id: opts.composition_id,
        frame: opts.frame,
        width,
        height,
        raster_backend: backend_name,
        gpu_renderer,
        wall_ms: t0.elapsed().as_secs_f64() * 1000.0,
        js_ms,
        layout_ms: frame.layout_ms,
        video_decode_ms: frame.video_decode_ms,
        draw_ms: frame.draw_ms,
        readback_ms: frame.readback_ms,
        unsupported_features,
        output_path: opts.output_path,
    })
}

struct ChunkRenderStats {
    frames_encoded: u32,
    deduped_frames: u32,
    backend_name: String,
    gpu_renderer: Option<String>,
    encoder_used: String,
    js_ms: f64,
    layout_ms: f64,
    video_decode_ms: f64,
    draw_ms: f64,
    readback_ms: f64,
    encode_wall_ms: f64,
    audio: Vec<AudioTrackFrame>,
    unsupported_features: BTreeSet<String>,
}

#[allow(clippy::too_many_arguments)]
fn render_tier1_chunk(
    opts: &Tier1RenderOptions,
    bundle_dir: &Path,
    width: u32,
    height: u32,
    chunk_frames: &[u32],
    seq_offset: usize,
    total_frames: usize,
    chunk_output: &Path,
    prepared: Option<PreparedTier1Host>,
    abort_flag: &AtomicBool,
    progress_done: &AtomicUsize,
    progress_lock: &Mutex<()>,
) -> Result<ChunkRenderStats> {
    let PreparedTier1Host {
        mut host,
        composition: comp,
    } = prepare_host(
        bundle_dir,
        &opts.composition_id,
        &opts.input_props,
        opts.timeout_ms,
        opts.offline,
        prepared,
    )?;

    let mut renderer = SceneRenderer::new(width, height, opts.raster_backend, bundle_dir)?;
    renderer.set_offline(opts.offline);
    let backend_name = renderer.backend_name().to_string();
    let gpu_renderer = renderer.gpu_renderer().map(str::to_string);
    let use_gpu_nv12 = renderer.supports_gpu_nv12()
        && opts.pixel_format == PixelFormat::Yuv420p
        && !opts.codec.is_audio_only()
        && opts.codec != Codec::Gif;

    let pipe = FfmpegPipeEncoder::spawn(EncodeOptions {
        width,
        height,
        fps: comp.config.fps,
        output_path: chunk_output.to_path_buf(),
        codec: opts.codec,
        crf: opts.crf,
        pixel_format: opts.pixel_format,
        input_pix_fmt: if use_gpu_nv12 {
            InputPixFmt::Nv12
        } else {
            InputPixFmt::Rgba
        },
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
    })?;

    let mut js_ms = 0.0_f64;
    let mut layout_ms = 0.0_f64;
    let mut video_decode_ms = 0.0_f64;
    let mut draw_ms = 0.0_f64;
    let mut readback_ms = 0.0_f64;
    let mut deduped_frames = 0u32;
    let mut unsupported_set = BTreeSet::new();
    let mut collected_audio: Vec<AudioTrackFrame> = Vec::new();

    let loop_res: Result<()> = (|| {
        for (local_idx, &frame_idx) in chunk_frames.iter().enumerate() {
            if abort_flag.load(Ordering::Relaxed) {
                bail!("Tier 1 chunk worker aborted");
            }

            let t_js = Instant::now();
            let scene = host.render_frame_tree(
                &comp.config.id,
                comp.config.width,
                comp.config.height,
                frame_idx,
            )?;
            js_ms += t_js.elapsed().as_secs_f64() * 1000.0;

            if !scene.unsupported_features.is_empty() {
                let feats: Vec<String> = scene
                    .unsupported_features
                    .iter()
                    .map(|u| format!("{}:{}", u.feature, u.tag))
                    .collect();
                if opts.abort_on_unsupported {
                    abort_flag.store(true, Ordering::Relaxed);
                    return Err(anyhow::Error::new(LateUnsupportedFeatureError {
                        frame: frame_idx,
                        features: feats,
                    }));
                }
                for f in feats {
                    unsupported_set.insert(f);
                }
            }

            let seq_idx = seq_offset + local_idx;
            if !opts.muted {
                for asset in &scene.audio_assets {
                    let mut mapped = asset.clone();
                    mapped.frame = seq_idx as u32;
                    collected_audio.push(mapped);
                }
            }

            let kf_target = opts.save_keyframes.get(&frame_idx);
            if use_gpu_nv12 {
                let rendered =
                    renderer.render_scene_nv12(&scene, opts.scale, kf_target.is_some())?;
                layout_ms += rendered.layout_ms;
                video_decode_ms += rendered.video_decode_ms;
                draw_ms += rendered.draw_ms;
                readback_ms += rendered.readback_ms;
                if rendered.deduped {
                    deduped_frames += 1;
                }

                if let (Some(kf_path), Some(rgba)) = (kf_target, rendered.rgba) {
                    save_still_image(
                        kf_path,
                        width,
                        height,
                        rgba,
                        InputPixFmt::Rgba,
                        ImageFormat::Png,
                        100,
                    )?;
                }

                if rendered.deduped && local_idx > 0 {
                    pipe.push_duplicate_frame(seq_idx as u32)?;
                } else {
                    let mut frame_buf = pipe.acquire_frame_buffer();
                    frame_buf.copy_from_slice(rendered.nv12);
                    pipe.push_frame(RawFrame {
                        index: seq_idx as u32,
                        width,
                        height,
                        data: frame_buf,
                    })?;
                }
            } else {
                let rendered = renderer.render_scene(&scene, opts.scale)?;
                layout_ms += rendered.layout_ms;
                video_decode_ms += rendered.video_decode_ms;
                draw_ms += rendered.draw_ms;
                readback_ms += rendered.readback_ms;
                if rendered.deduped {
                    deduped_frames += 1;
                }

                if let Some(kf_path) = kf_target {
                    save_still_image(
                        kf_path,
                        width,
                        height,
                        rendered.rgba,
                        InputPixFmt::Rgba,
                        ImageFormat::Png,
                        100,
                    )?;
                }

                if rendered.deduped && local_idx > 0 {
                    pipe.push_duplicate_frame(seq_idx as u32)?;
                } else {
                    let mut frame_buf = pipe.acquire_frame_buffer();
                    frame_buf.copy_from_slice(rendered.rgba);
                    pipe.push_frame(RawFrame {
                        index: seq_idx as u32,
                        width,
                        height,
                        data: frame_buf,
                    })?;
                }
            }

            if opts.json_progress {
                let done = progress_done.fetch_add(1, Ordering::Relaxed) + 1;
                let progress = (done as f64) / (total_frames.max(1) as f64);
                let _guard = progress_lock.lock().unwrap();
                println!(
                    "{}",
                    serde_json::json!({
                        "type": "progress",
                        "renderedFrames": done,
                        "totalFrames": total_frames,
                        "progress": progress,
                    })
                );
            }
        }
        Ok(())
    })();

    if let Err(err) = loop_res {
        drop(pipe);
        let _ = std::fs::remove_file(chunk_output);
        return Err(err);
    }

    let encode_stats = match pipe.finish() {
        Ok(stats) => stats,
        Err(err) => {
            let _ = std::fs::remove_file(chunk_output);
            return Err(err.into());
        }
    };

    Ok(ChunkRenderStats {
        frames_encoded: encode_stats.frames_encoded,
        deduped_frames,
        backend_name,
        gpu_renderer,
        encoder_used: encode_stats.encoder_used,
        js_ms,
        layout_ms,
        video_decode_ms,
        draw_ms,
        readback_ms,
        encode_wall_ms: encode_stats.wall_ms,
        audio: collected_audio,
        unsupported_features: unsupported_set,
    })
}

/// Returns how many parallel V8+EGL+VAAPI chunk workers to run for a Tier 1 video render.
///
/// On integrated GPUs (e.g. Iris 540), booting a second V8 isolate + EGL/VAAPI
/// context + chunk concat regresses short 2s (60f) clips overall; enable multi-worker
/// chunking automatically at >= 120 frames (4s+ at 30fps) where chunk overlap pays off.
fn resolve_worker_count(concurrency: usize, total_frames: usize, codec: Codec) -> usize {
    if concurrency > 1 && total_frames >= 120 && !codec.is_audio_only() && codec != Codec::Gif {
        concurrency.clamp(1, 3)
    } else {
        1
    }
}

/// Renders frame chunks across `workers` parallel threads (chunk 0 on the current thread
/// so `prepared_first`'s `!Send` V8 isolate stays pinned) and mux-concats them cleanly.
#[allow(clippy::too_many_arguments)]
fn render_parallel_chunks(
    opts: &Tier1RenderOptions,
    bundle_dir: &Path,
    width: u32,
    height: u32,
    frames_to_render: &[u32],
    workers: usize,
    temp_video_path: &Path,
    prepared_first: PreparedTier1Host,
    abort_flag: &AtomicBool,
    progress_done: &AtomicUsize,
    progress_lock: &Mutex<()>,
) -> Result<ChunkRenderStats> {
    let total_frames = frames_to_render.len();
    let chunk_size = total_frames.div_ceil(workers);
    let mut ranges: Vec<(usize, &[u32], PathBuf)> = Vec::with_capacity(workers);
    let ext = opts
        .output_path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("mp4");
    for (idx, chunk) in frames_to_render.chunks(chunk_size).enumerate() {
        let offset = idx * chunk_size;
        let chunk_path = opts
            .output_path
            .with_extension(format!("rsmotion-chunk-{idx}.{ext}"));
        ranges.push((offset, chunk, chunk_path));
    }

    // Run chunk 0 on the current thread so the !Send V8 isolate (`prepared_first`) stays
    // pinned here while worker threads boot isolates concurrently for chunks `1..`.
    let mut prepared_opt = Some(prepared_first);
    let results: Vec<Result<ChunkRenderStats>> = std::thread::scope(|s| {
        let mut handles = Vec::with_capacity(ranges.len().saturating_sub(1));
        for (offset, chunk_frames, chunk_path) in &ranges[1..] {
            let offset_val = *offset;
            let chunk_slice = *chunk_frames;
            let chunk_out = chunk_path.as_path();

            handles.push(s.spawn(move || {
                render_tier1_chunk(
                    opts,
                    bundle_dir,
                    width,
                    height,
                    chunk_slice,
                    offset_val,
                    total_frames,
                    chunk_out,
                    None,
                    abort_flag,
                    progress_done,
                    progress_lock,
                )
            }));
        }

        let (off0, slice0, ref path0) = ranges[0];
        let first_res = render_tier1_chunk(
            opts,
            bundle_dir,
            width,
            height,
            slice0,
            off0,
            total_frames,
            path0.as_path(),
            prepared_opt.take(),
            abort_flag,
            progress_done,
            progress_lock,
        );

        let mut collected = Vec::with_capacity(ranges.len());
        collected.push(first_res);
        for h in handles {
            match h.join() {
                Ok(r) => collected.push(r),
                Err(_) => collected.push(Err(anyhow::anyhow!("Tier 1 worker thread panicked"))),
            }
        }
        collected
    });

    let chunk_paths: Vec<PathBuf> = ranges.iter().map(|(_, _, p)| p.clone()).collect();
    let mut ok_stats = Vec::with_capacity(results.len());
    let mut late_err: Option<anyhow::Error> = None;
    let mut first_err: Option<anyhow::Error> = None;
    for res in results {
        match res {
            Ok(st) => ok_stats.push(st),
            Err(err) => {
                if err.downcast_ref::<LateUnsupportedFeatureError>().is_some() && late_err.is_none()
                {
                    late_err = Some(err);
                } else if first_err.is_none() {
                    first_err = Some(err);
                }
            }
        }
    }

    if let Some(err) = late_err.or(first_err) {
        for p in &chunk_paths {
            let _ = std::fs::remove_file(p);
        }
        if !opts.muted {
            let _ = std::fs::remove_file(temp_video_path);
        }
        return Err(err);
    }

    let concat_res = concat_chunks(&chunk_paths, temp_video_path);
    for p in &chunk_paths {
        let _ = std::fs::remove_file(p);
    }
    concat_res?;

    let mut acc = ok_stats.remove(0);
    for st in ok_stats {
        acc.frames_encoded += st.frames_encoded;
        acc.deduped_frames += st.deduped_frames;
        acc.js_ms += st.js_ms;
        acc.layout_ms += st.layout_ms;
        acc.video_decode_ms += st.video_decode_ms;
        acc.draw_ms += st.draw_ms;
        acc.readback_ms += st.readback_ms;
        acc.encode_wall_ms = acc.encode_wall_ms.max(st.encode_wall_ms);
        acc.audio.extend(st.audio);
        acc.unsupported_features.extend(st.unsupported_features);
    }
    acc.audio.sort_by_key(|a| a.frame);
    Ok(acc)
}

pub fn render_media_tier1(opts: Tier1RenderOptions) -> Result<Tier1RenderReport> {
    render_media_tier1_with_host(opts, None)
}

pub fn render_media_tier1_with_host(
    opts: Tier1RenderOptions,
    prepared: Option<PreparedTier1Host>,
) -> Result<Tier1RenderReport> {
    let t_total = Instant::now();
    let bundle_dir = resolve_bundle_dir(&opts.serve_url)?;

    let prepared_first = prepare_host(
        &bundle_dir,
        &opts.composition_id,
        &opts.input_props,
        opts.timeout_ms,
        opts.offline,
        prepared,
    )?;
    let comp = prepared_first.composition.clone();

    let raw_w = ((f64::from(comp.config.width) * opts.scale).round() as u32).max(1);
    let raw_h = ((f64::from(comp.config.height) * opts.scale).round() as u32).max(1);
    let (width, height) = validate_even_dimensions(raw_w, raw_h, opts.codec)?;

    let range = match opts.frame_range {
        Some(r) => r,
        None => FrameRange::full(comp.config.duration_in_frames)?,
    };
    let frames_to_render =
        range.resolve_frames(comp.config.duration_in_frames, opts.every_nth_frame)?;
    let total_frames = frames_to_render.len();

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

    let workers = resolve_worker_count(opts.concurrency, total_frames, opts.codec);

    let abort_flag = AtomicBool::new(false);
    let progress_done = AtomicUsize::new(0);
    let progress_lock = Mutex::new(());

    let merged_stats = if workers == 1 {
        render_tier1_chunk(
            &opts,
            &bundle_dir,
            width,
            height,
            &frames_to_render,
            0,
            total_frames,
            &temp_video_path,
            Some(prepared_first),
            &abort_flag,
            &progress_done,
            &progress_lock,
        )?
    } else {
        render_parallel_chunks(
            &opts,
            &bundle_dir,
            width,
            height,
            &frames_to_render,
            workers,
            &temp_video_path,
            prepared_first,
            &abort_flag,
            &progress_done,
            &progress_lock,
        )?
    };

    let t_audio = Instant::now();
    let has_audio = if skip_audio_mux {
        false
    } else if merged_stats.audio.is_empty() {
        if temp_video_path != opts.output_path
            && std::fs::rename(&temp_video_path, &opts.output_path).is_err()
        {
            std::fs::copy(&temp_video_path, &opts.output_path)?;
            let _ = std::fs::remove_file(&temp_video_path);
        }
        false
    } else {
        let sample_rate = comp.default_sample_rate.unwrap_or(48_000);
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
            &merged_stats.audio,
            comp.config.fps,
            total_frames as u32,
            sample_rate,
            &mux_opts,
            |src| resolve_or_fetch_asset(&bundle_dir, src, opts.offline),
        )?
    };
    let audio_ms = if skip_audio_mux {
        0.0
    } else {
        t_audio.elapsed().as_secs_f64() * 1000.0
    };

    let wall_ms = t_total.elapsed().as_secs_f64() * 1000.0;
    let frames_rendered = merged_stats.frames_encoded;

    Ok(Tier1RenderReport {
        tier: 1,
        composition_id: opts.composition_id,
        output_path: opts.output_path,
        width,
        height,
        frames_rendered,
        deduped_frames: merged_stats.deduped_frames,
        raster_backend: merged_stats.backend_name,
        gpu_renderer: merged_stats.gpu_renderer,
        encoder_used: merged_stats.encoder_used,
        has_audio,
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
        js_ms: merged_stats.js_ms,
        layout_ms: merged_stats.layout_ms,
        video_decode_ms: merged_stats.video_decode_ms,
        draw_ms: merged_stats.draw_ms,
        readback_ms: merged_stats.readback_ms,
        encode_wall_ms: merged_stats.encode_wall_ms,
        audio_ms,
        unsupported_features: merged_stats.unsupported_features.into_iter().collect(),
    })
}

/// Options for Tier 1 image-sequence (`frames`) rendering.
#[derive(Debug, Clone)]
pub struct Tier1FramesOptions {
    pub serve_url: String,
    pub composition_id: String,
    pub output_dir: PathBuf,
    pub image_format: ImageFormat,
    pub jpeg_quality: u8,
    pub frame_range: Option<FrameRange>,
    pub every_nth_frame: u32,
    pub scale: f64,
    pub raster_backend: RasterBackend,
    pub timeout_ms: u64,
    pub offline: bool,
    pub input_props: Value,
    pub abort_on_unsupported: bool,
    pub json_progress: bool,
}

/// Summary report emitted by `render_frames_tier1_with_host`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tier1FramesReport {
    pub tier: u8,
    pub composition_id: String,
    pub output_dir: PathBuf,
    pub width: u32,
    pub height: u32,
    pub frames_rendered: u32,
    pub raster_backend: String,
    pub gpu_renderer: Option<String>,
    pub assets: Vec<PathBuf>,
    pub wall_ms: f64,
    pub fps: f64,
}

pub fn image_format_ext(fmt: ImageFormat) -> &'static str {
    match fmt {
        ImageFormat::Png => "png",
        ImageFormat::Jpeg => "jpeg",
        ImageFormat::Webp => "webp",
    }
}

pub fn render_frames_tier1_with_host(
    opts: Tier1FramesOptions,
    prepared: Option<PreparedTier1Host>,
) -> Result<Tier1FramesReport> {
    let t0 = Instant::now();
    let bundle_dir = resolve_bundle_dir(&opts.serve_url)?;

    let PreparedTier1Host {
        mut host,
        composition: comp,
    } = prepare_host(
        &bundle_dir,
        &opts.composition_id,
        &opts.input_props,
        opts.timeout_ms,
        opts.offline,
        prepared,
    )?;

    let width = ((f64::from(comp.config.width) * opts.scale).round() as u32).max(1);
    let height = ((f64::from(comp.config.height) * opts.scale).round() as u32).max(1);

    let range = match opts.frame_range {
        Some(r) => r,
        None => FrameRange::full(comp.config.duration_in_frames)?,
    };
    let frames_to_render =
        range.resolve_frames(comp.config.duration_in_frames, opts.every_nth_frame)?;
    let total_frames = frames_to_render.len();

    std::fs::create_dir_all(&opts.output_dir)?;
    let ext = image_format_ext(opts.image_format);
    let mut renderer = SceneRenderer::new(width, height, opts.raster_backend, &bundle_dir)?;
    renderer.set_offline(opts.offline);
    let backend_name = renderer.backend_name().to_string();
    let gpu_renderer = renderer.gpu_renderer().map(str::to_string);
    let mut assets = Vec::with_capacity(total_frames);

    for (seq_idx, &frame_idx) in frames_to_render.iter().enumerate() {
        let scene = host.render_frame_tree(
            &comp.config.id,
            comp.config.width,
            comp.config.height,
            frame_idx,
        )?;

        if opts.abort_on_unsupported && !scene.unsupported_features.is_empty() {
            let feats = scene
                .unsupported_features
                .iter()
                .map(|u| format!("{}:{}", u.feature, u.tag))
                .collect();
            return Err(anyhow::Error::new(LateUnsupportedFeatureError {
                frame: frame_idx,
                features: feats,
            }));
        }

        let rendered = renderer.render_scene(&scene, opts.scale)?;
        let out_file = opts
            .output_dir
            .join(format!("element-{frame_idx:04}.{ext}"));
        save_still_image(
            &out_file,
            width,
            height,
            rendered.rgba,
            InputPixFmt::Rgba,
            opts.image_format,
            opts.jpeg_quality,
        )?;
        assets.push(out_file);

        if opts.json_progress {
            let done = seq_idx + 1;
            println!(
                "{}",
                serde_json::json!({
                    "type": "progress",
                    "renderedFrames": done,
                    "totalFrames": total_frames,
                    "progress": (done as f64) / (total_frames.max(1) as f64),
                })
            );
        }
    }

    let wall_ms = t0.elapsed().as_secs_f64() * 1000.0;
    let frames_rendered = assets.len() as u32;
    Ok(Tier1FramesReport {
        tier: 1,
        composition_id: opts.composition_id,
        output_dir: opts.output_dir,
        width,
        height,
        frames_rendered,
        raster_backend: backend_name,
        gpu_renderer,
        assets,
        wall_ms,
        fps: if wall_ms > 0.0 {
            f64::from(frames_rendered) / (wall_ms / 1000.0)
        } else {
            0.0
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_worker_count_gates_short_clips_and_caps_workers() {
        assert_eq!(resolve_worker_count(1, 240, Codec::H264), 1);
        assert_eq!(resolve_worker_count(2, 60, Codec::H264), 1);
        assert_eq!(resolve_worker_count(2, 119, Codec::H264), 1);
        assert_eq!(resolve_worker_count(2, 120, Codec::H264), 2);
        assert_eq!(resolve_worker_count(8, 240, Codec::H264), 3);
        assert_eq!(resolve_worker_count(2, 240, Codec::Gif), 1);
        assert_eq!(resolve_worker_count(2, 240, Codec::Mp3), 1);
    }

    #[test]
    fn multi_worker_chunking_renders_and_concats_chunks() {
        let bundle_dir =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/build");
        let tmp_dir =
            std::env::temp_dir().join(format!("rsmotion-tier1-chunk-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp_dir);
        std::fs::create_dir_all(&tmp_dir).unwrap();

        let out_mp4 = tmp_dir.join("chunked.mp4");
        let opts = Tier1RenderOptions {
            serve_url: bundle_dir.to_string_lossy().into_owned(),
            composition_id: "text-basic".to_string(),
            output_path: out_mp4.clone(),
            codec: Codec::H264,
            crf: None,
            pixel_format: PixelFormat::Yuv420p,
            hw_accel: HardwareAccel::Auto,
            raster_backend: RasterBackend::Auto,
            frame_range: None,
            every_nth_frame: 1,
            scale: 1.0,
            muted: true,
            timeout_ms: 30_000,
            input_props: Value::Object(Default::default()),
            x264_preset: None,
            video_bitrate: None,
            audio_bitrate: None,
            audio_codec: None,
            color_space: ColorSpace::Default,
            prores_profile: None,
            number_of_gif_loops: None,
            concurrency: 2,
            offline: false,
            json_progress: false,
            abort_on_unsupported: false,
            save_keyframes: BTreeMap::new(),
        };

        let prepared_first = prepare_host(
            &bundle_dir,
            &opts.composition_id,
            &opts.input_props,
            opts.timeout_ms,
            false,
            None,
        )
        .unwrap();
        let frames: Vec<u32> = (0..10).collect();
        let abort_flag = AtomicBool::new(false);
        let progress_done = AtomicUsize::new(0);
        let progress_lock = Mutex::new(());

        let stats = render_parallel_chunks(
            &opts,
            &bundle_dir,
            prepared_first.composition.config.width,
            prepared_first.composition.config.height,
            &frames,
            2,
            &out_mp4,
            prepared_first,
            &abort_flag,
            &progress_done,
            &progress_lock,
        )
        .unwrap();

        assert_eq!(stats.frames_encoded, 10);
        assert!(!abort_flag.load(Ordering::Relaxed));
        assert!(!out_mp4.with_extension("rsmotion-chunk-0.mp4").exists());
        assert!(!out_mp4.with_extension("rsmotion-chunk-1.mp4").exists());

        let meta = rsmotion_media::probe_media(&out_mp4).unwrap();
        assert!(meta.has_video);
        assert_eq!(meta.width, 1920);
        assert_eq!(meta.height, 1080);
        assert!(
            meta.duration_secs >= 0.30,
            "duration_secs={}",
            meta.duration_secs
        );

        let _ = std::fs::remove_dir_all(&tmp_dir);
    }
}
