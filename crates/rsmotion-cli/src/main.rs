mod tier1;
mod tier3;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use rsmotion_analyze::{
    analyze_bundle_with_offline, analyze_composition_for_dispatch_with_offline,
    format_composition_explanation, resolve_local_bundle_dir, CompositionAnalysis,
};
use rsmotion_bundler::{bundle_async, is_source_entry, BundleOptions};
use rsmotion_cdp::{
    list_compositions_cdp, render_media_cdp, render_still_cdp, CdpCompositionsOptions,
    CdpRenderOptions, CdpStillOptions, GlRenderer,
};
use rsmotion_core::{
    AudioCodec, Codec, ColorSpace, FrameRange, HardwareAccel, ImageFormat, PixelFormat,
    ProResProfile, RasterBackend,
};
use rsmotion_encode::{
    extract_audio_track, generate_synthetic_gradient_frame, probe_best_h264_encoder, EncodeOptions,
    FfmpegPipeEncoder,
};
use rsmotion_media::get_silent_parts;
use rsmotion_raster::{compare_png_files, run_raster_spike, SpikeConfig};
use serde::{Deserialize, Serialize};
use tier1::{
    image_format_ext, list_compositions_tier1_with_offline, render_frames_tier1_with_host,
    render_media_tier1, render_media_tier1_with_host, render_still_tier1,
    render_still_tier1_with_host, LateUnsupportedFeatureError, Tier1FramesOptions,
    Tier1FramesReport, Tier1RenderOptions, Tier1RenderReport, Tier1StillOptions, Tier1StillReport,
};
use tracing::warn;

#[derive(Debug, Parser)]
#[command(name = "rsmotion", version, about = "Fast Rust renderer for Remotion")]
struct Cli {
    #[arg(long, global = true, value_name = "LEVEL")]
    log: Option<String>,
    /// Disallow network downloads and use only cached `~/.cache/rsmotion/assets` + local files.
    #[arg(long, global = true, default_value_t = false)]
    offline: bool,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum BenchmarkTier {
    #[value(name = "auto")]
    Auto,
    #[value(name = "1")]
    Tier1,
    #[value(name = "2")]
    Tier2,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum SpikeBackendArg {
    Cpu,
    Gpu,
    Both,
}

impl From<SpikeBackendArg> for RasterBackend {
    fn from(value: SpikeBackendArg) -> Self {
        match value {
            SpikeBackendArg::Cpu => RasterBackend::Cpu,
            SpikeBackendArg::Gpu => RasterBackend::Gpu,
            SpikeBackendArg::Both => RasterBackend::Auto,
        }
    }
}

#[derive(Debug, Clone, clap::Args)]
struct BundlerCliArgs {
    /// Enable Tailwind CSS compilation when auto-bundling a TypeScript/JavaScript entrypoint.
    #[arg(long, default_value_t = false)]
    enable_tailwind: bool,
    /// Override the `public/` folder path when auto-bundling a source entrypoint.
    #[arg(long)]
    public_dir: Option<PathBuf>,
    /// Disable the incremental SHA-256 bundle cache when auto-bundling source code.
    #[arg(long, default_value_t = false)]
    no_cache: bool,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Phase 1 spike: benchmark raw RGBA pipe encoding into FFmpeg.
    SpikeEncode {
        #[arg(long, default_value_t = 100)]
        frames: u32,
        #[arg(long, default_value_t = 1920)]
        width: u32,
        #[arg(long, default_value_t = 1080)]
        height: u32,
        #[arg(long, value_enum, default_value_t = HardwareAccel::Auto)]
        encoder: HardwareAccel,
        #[arg(long, default_value = "/tmp/spike-encode.mp4")]
        out: PathBuf,
        #[arg(long, default_value = "medium")]
        x264_preset: String,
    },

    /// Phase 1 spike: benchmark Skia CPU vs headless EGL GPU rasterization (+ optional encode).
    SpikeRaster {
        #[arg(long, default_value_t = 100)]
        frames: u32,
        #[arg(long, default_value_t = 1920)]
        width: u32,
        #[arg(long, default_value_t = 1080)]
        height: u32,
        #[arg(long, value_enum, default_value_t = SpikeBackendArg::Both)]
        backend: SpikeBackendArg,
        #[arg(long, default_value_t = false)]
        encode: bool,
        #[arg(long, value_enum, default_value_t = HardwareAccel::Auto)]
        encoder: HardwareAccel,
        #[arg(long, default_value = "/tmp/spike-raster")]
        out_dir: PathBuf,
        #[arg(long)]
        sample_image: Option<PathBuf>,
    },

    /// Compare a PNG or directory of PNGs against golden frames using windowed RGB SSIM.
    Diff {
        golden: PathBuf,
        actual: PathBuf,
        #[arg(long)]
        heatmap: Option<PathBuf>,
        #[arg(long, default_value_t = 0.97)]
        min_ssim: f64,
    },

    /// List compositions in a Remotion bundle.
    Compositions {
        serve_url: String,
        #[arg(long)]
        props: Option<String>,
        #[arg(long)]
        browser_executable: Option<PathBuf>,
        #[arg(long, value_enum, default_value_t = GlRenderer::None)]
        gl: GlRenderer,
        #[arg(long, default_value_t = 30000)]
        timeout: u64,
        #[arg(long)]
        force_tier: Option<u8>,
        #[arg(long, default_value_t = false)]
        json: bool,
        #[command(flatten)]
        bundler: BundlerCliArgs,
    },

    /// Render a single frame to PNG, JPEG, or WebP.
    Still {
        serve_url: String,
        composition_id: String,
        output: PathBuf,
        #[arg(long, default_value_t = 0)]
        frame: u32,
        #[arg(long, value_enum, default_value_t = ImageFormat::Png)]
        image_format: ImageFormat,
        #[arg(long, default_value_t = 80)]
        jpeg_quality: u8,
        #[arg(long, default_value_t = 1.0)]
        scale: f64,
        #[arg(long)]
        props: Option<String>,
        #[arg(long, value_enum, default_value_t = GlRenderer::None)]
        gl: GlRenderer,
        #[arg(long, value_enum, default_value_t = RasterBackend::Auto)]
        raster: RasterBackend,
        #[arg(long)]
        browser_executable: Option<PathBuf>,
        #[arg(long, default_value_t = 30000)]
        timeout: u64,
        #[arg(long, default_value_t = false)]
        overwrite: bool,
        #[arg(long)]
        force_tier: Option<u8>,
        #[arg(long, default_value_t = false)]
        explain: bool,
        #[arg(long, default_value_t = false)]
        json: bool,
        #[command(flatten)]
        bundler: BundlerCliArgs,
    },

    /// Render a Remotion composition to video (or frame sequence with `--sequence`).
    Render {
        serve_url: String,
        composition_id: String,
        output: PathBuf,
        #[arg(long, value_enum, default_value_t = Codec::H264)]
        codec: Codec,
        #[arg(long)]
        crf: Option<u8>,
        #[arg(long)]
        video_bitrate: Option<String>,
        #[arg(long)]
        audio_bitrate: Option<String>,
        #[arg(long, value_enum)]
        audio_codec: Option<AudioCodec>,
        #[arg(long, value_enum, default_value_t = ColorSpace::Default)]
        color_space: ColorSpace,
        #[arg(long, value_enum)]
        prores_profile: Option<ProResProfile>,
        #[arg(long, allow_hyphen_values = true)]
        number_of_gif_loops: Option<i32>,
        #[arg(long, default_value_t = false)]
        sequence: bool,
        #[arg(long, value_enum, default_value_t = ImageFormat::Png)]
        image_format: ImageFormat,
        #[arg(long, default_value_t = 80)]
        jpeg_quality: u8,
        #[arg(long, default_value_t = 2)]
        concurrency: usize,
        #[arg(long)]
        frames: Option<String>,
        #[arg(long, default_value_t = 1)]
        every_nth_frame: u32,
        #[arg(long, default_value_t = 1.0)]
        scale: f64,
        #[arg(long, default_value_t = false)]
        muted: bool,
        #[arg(long, value_enum, default_value_t = PixelFormat::Yuv420p)]
        pixel_format: PixelFormat,
        #[arg(long, value_enum, default_value_t = GlRenderer::None)]
        gl: GlRenderer,
        #[arg(long, value_enum, default_value_t = RasterBackend::Auto)]
        raster: RasterBackend,
        #[arg(long, value_enum, default_value_t = HardwareAccel::Auto)]
        encoder: HardwareAccel,
        #[arg(long)]
        x264_preset: Option<String>,
        #[arg(long, default_value_t = 30000)]
        timeout: u64,
        #[arg(long)]
        props: Option<String>,
        #[arg(long)]
        browser_executable: Option<PathBuf>,
        #[arg(long, default_value_t = false)]
        overwrite: bool,
        #[arg(long)]
        force_tier: Option<u8>,
        #[arg(long, default_value_t = false)]
        explain: bool,
        #[arg(long, default_value_t = false)]
        json_progress: bool,
        #[command(flatten)]
        bundler: BundlerCliArgs,
    },

    /// Render a frame sequence of images (`element-0000.png`, ...).
    Frames {
        serve_url: String,
        composition_id: String,
        output_dir: PathBuf,
        #[arg(long, value_enum, default_value_t = ImageFormat::Png)]
        image_format: ImageFormat,
        #[arg(long, default_value_t = 80)]
        jpeg_quality: u8,
        #[arg(long)]
        frames: Option<String>,
        #[arg(long, default_value_t = 1)]
        every_nth_frame: u32,
        #[arg(long, default_value_t = 1.0)]
        scale: f64,
        #[arg(long, value_enum, default_value_t = GlRenderer::None)]
        gl: GlRenderer,
        #[arg(long, value_enum, default_value_t = RasterBackend::Auto)]
        raster: RasterBackend,
        #[arg(long, default_value_t = 30000)]
        timeout: u64,
        #[arg(long)]
        props: Option<String>,
        #[arg(long)]
        browser_executable: Option<PathBuf>,
        #[arg(long, default_value_t = false)]
        overwrite: bool,
        #[arg(long)]
        force_tier: Option<u8>,
        #[arg(long, default_value_t = false)]
        explain: bool,
        #[arg(long, default_value_t = false)]
        json_progress: bool,
        #[command(flatten)]
        bundler: BundlerCliArgs,
    },

    /// Run Tier 1, Tier 2, or automatic dispatch benchmarks against baseline.json and verify golden PNG SSIM.
    Benchmark {
        #[arg(long, value_enum, default_value_t = BenchmarkTier::Auto)]
        tier: BenchmarkTier,
        #[arg(long)]
        fixtures: Option<String>,
        #[arg(long, default_value = "tests/fixtures/build")]
        bundle_dir: PathBuf,
        #[arg(long, default_value = "tests/golden")]
        golden_dir: PathBuf,
        #[arg(long, default_value = "bench/baseline.json")]
        baseline_json: PathBuf,
        #[arg(long, default_value = "/tmp/rsmotion-bench")]
        out_dir: PathBuf,
        #[arg(long, default_value_t = 2)]
        concurrency: usize,
        #[arg(long, value_enum, default_value_t = GlRenderer::None)]
        gl: GlRenderer,
        #[arg(long, value_enum, default_value_t = RasterBackend::Auto)]
        raster: RasterBackend,
        #[arg(long, value_enum, default_value_t = HardwareAccel::Auto)]
        encoder: HardwareAccel,
        #[arg(long, default_value_t = 30000)]
        timeout: u64,
        #[arg(long)]
        browser_executable: Option<PathBuf>,
        #[arg(long, default_value_t = 0.98)]
        min_ssim: f64,
    },

    /// Explain which rendering tier would be chosen for each composition.
    Explain {
        serve_url: String,
        #[arg(long)]
        composition: Option<String>,
        #[arg(long)]
        props: Option<String>,
        #[arg(long, default_value_t = 30000)]
        timeout: u64,
        #[arg(long, default_value_t = false)]
        json: bool,
        #[command(flatten)]
        bundler: BundlerCliArgs,
    },

    /// Bundle a Remotion entrypoint (TypeScript/JS/CSS) into index.html + bundle.js using Rolldown.
    Bundle {
        entry_point: PathBuf,
        out_dir: Option<PathBuf>,
        #[arg(long, default_value_t = false)]
        enable_tailwind: bool,
        #[arg(long)]
        public_dir: Option<PathBuf>,
        #[arg(long, default_value_t = false)]
        no_cache: bool,
        #[arg(long, default_value_t = false)]
        json: bool,
    },

    /// Detect silent and audible intervals in an audio or video file via FFmpeg silencedetect.
    SilentParts {
        src: PathBuf,
        #[arg(long, default_value_t = -20.0, allow_hyphen_values = true)]
        noise_threshold: f64,
        #[arg(long, default_value_t = 0.5)]
        min_duration: f64,
        #[arg(long, default_value_t = false)]
        json: bool,
    },

    /// Extract the audio track from a video file without re-encoding video frames.
    ExtractAudio {
        src: PathBuf,
        output: PathBuf,
        #[arg(long, default_value_t = false)]
        overwrite: bool,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
}

fn parse_props_arg(raw: Option<&str>) -> Result<serde_json::Value> {
    let Some(val) = raw else {
        return Ok(serde_json::json!({}));
    };
    let p = Path::new(val);
    let content = if p.is_file() {
        std::fs::read_to_string(p).with_context(|| format!("reading props file {}", p.display()))?
    } else {
        val.to_string()
    };
    let parsed: serde_json::Value =
        serde_json::from_str(&content).context("parsing --props JSON")?;
    Ok(parsed)
}

fn ensure_writable(output: &Path, overwrite: bool) -> Result<()> {
    if output.exists() && !overwrite {
        bail!(
            "File at {} already exists. Use --overwrite to overwrite.",
            output.display()
        );
    }
    Ok(())
}

async fn resolve_serve_url_or_bundle(
    serve_url: String,
    bundler: &BundlerCliArgs,
    explain: bool,
) -> Result<String> {
    if !is_source_entry(&serve_url) {
        return Ok(serve_url);
    }
    let report = bundle_async(BundleOptions {
        entry_point: PathBuf::from(&serve_url),
        out_dir: None,
        public_dir: bundler.public_dir.clone(),
        enable_tailwind: bundler.enable_tailwind,
        use_cache: !bundler.no_cache,
        symlink_public_dir: true,
    })
    .await?;
    if explain {
        eprintln!(
            "[bundle] {} -> {} (cache_hit={}, {:.1} ms)",
            serve_url,
            report.out_dir.display(),
            report.cache_hit,
            report.wall_ms
        );
    }
    Ok(report.out_dir.to_string_lossy().into_owned())
}

fn collect_pngs(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
        let entry = entry?;
        let p = entry.path();
        if p.is_file() && p.extension().and_then(|e| e.to_str()) == Some("png") {
            files.push(p);
        }
    }
    files.sort();
    Ok(files)
}

fn run_spike_encode(
    frames: u32,
    width: u32,
    height: u32,
    encoder: HardwareAccel,
    out: PathBuf,
    x264_preset: String,
) -> Result<()> {
    let resolved = probe_best_h264_encoder(encoder)?;
    println!(
        "Probed encoder: {} ({:?})",
        resolved.ffmpeg_codec, resolved.kind
    );

    let pattern_len = frames.clamp(1, 30);
    let pool: Vec<_> = (0..pattern_len)
        .map(|i| generate_synthetic_gradient_frame(width, height, i))
        .collect();

    let pipe = FfmpegPipeEncoder::spawn(EncodeOptions {
        width,
        height,
        fps: 30.0,
        output_path: out.clone(),
        codec: Codec::H264,
        crf: Some(20),
        pixel_format: PixelFormat::Yuv420p,
        hw_accel: encoder,
        audio_input: None,
        muted: true,
        x264_preset: Some(x264_preset),
        ring_capacity: 8,
        ..Default::default()
    })?;

    for f in 0..frames {
        let mut frame = pool[(f % pattern_len) as usize].clone();
        frame.index = f;
        pipe.push_frame(frame)?;
    }

    let stats = pipe.finish()?;
    println!("{}", serde_json::to_string_pretty(&stats)?);
    println!("Wrote {}", out.display());
    Ok(())
}

fn run_diff(golden: &Path, actual: &Path, heatmap: Option<&Path>, min_ssim: f64) -> Result<bool> {
    if golden.is_file() && actual.is_file() {
        let report = compare_png_files(golden, actual, heatmap)?;
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(report.ssim >= min_ssim);
    }

    if golden.is_dir() && actual.is_dir() {
        let golden_files = collect_pngs(golden)?;
        if golden_files.is_empty() {
            bail!("no PNG files found in {}", golden.display());
        }
        if let Some(hm_dir) = heatmap {
            std::fs::create_dir_all(hm_dir)?;
        }

        let mut all_ok = true;
        let mut min_seen = f64::INFINITY;
        for g_path in &golden_files {
            let file_name = g_path.file_name().unwrap();
            let a_path = actual.join(file_name);
            if !a_path.exists() {
                bail!("missing actual frame {}", a_path.display());
            }
            let hm_file = heatmap.map(|d| d.join(file_name));
            let rep = compare_png_files(g_path, &a_path, hm_file.as_deref())?;
            min_seen = min_seen.min(rep.ssim);
            let ok = rep.ssim >= min_ssim;
            if !ok {
                all_ok = false;
            }
            println!(
                "{}: ssim={:.5} psnr={:.2} max_diff={} diff_ratio={:.4} [{}]",
                file_name.to_string_lossy(),
                rep.ssim,
                rep.psnr,
                rep.max_channel_diff,
                rep.diff_pixel_ratio,
                if ok { "PASS" } else { "FAIL" }
            );
        }
        println!("Minimum SSIM: {min_seen:.5} (threshold {min_ssim:.5})");
        return Ok(all_ok);
    }

    bail!(
        "golden ({}) and actual ({}) must both be files or both be directories",
        golden.display(),
        actual.display()
    );
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BaselineRow {
    id: String,
    frames: u32,
    render_media_ms: f64,
    fps: f64,
    ms_per_frame: f64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BaselineFile {
    keyframes: Vec<u32>,
    results: Vec<BaselineRow>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BenchFixtureRow {
    id: String,
    frames: u32,
    baseline_ms: f64,
    baseline_fps: f64,
    tier2_ms: f64,
    tier2_fps: f64,
    tier2_ms_per_frame: f64,
    speedup: f64,
    capture_mode: String,
    encoder_used: String,
    has_audio: bool,
    min_ssim: f64,
    avg_ssim: f64,
}

fn ffprobe_check_mp4(path: &Path, expected_frames: u32) -> Result<bool> {
    let v_out = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-count_frames",
            "-show_entries",
            "stream=width,height,nb_read_frames",
            "-of",
            "csv=p=0",
        ])
        .arg(path)
        .output()?;
    if !v_out.status.success() {
        bail!(
            "ffprobe failed on {}: {}",
            path.display(),
            String::from_utf8_lossy(&v_out.stderr)
        );
    }
    let v_text = String::from_utf8_lossy(&v_out.stdout).trim().to_string();
    let expected = format!("1920,1080,{expected_frames}");
    if v_text != expected {
        bail!(
            "unexpected ffprobe video stream for {}: expected `{expected}`, got `{v_text}`",
            path.display()
        );
    }

    let a_out = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "a:0",
            "-show_entries",
            "stream=codec_name",
            "-of",
            "csv=p=0",
        ])
        .arg(path)
        .output()?;
    Ok(!String::from_utf8_lossy(&a_out.stdout).trim().is_empty())
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Tier1BenchFixtureRow {
    id: String,
    frames: u32,
    deduped_frames: u32,
    baseline_ms: f64,
    baseline_fps: f64,
    tier1_ms: f64,
    tier1_fps: f64,
    tier1_ms_per_frame: f64,
    speedup: f64,
    raster_backend: String,
    gpu_renderer: Option<String>,
    encoder_used: String,
    has_audio: bool,
    js_ms: f64,
    layout_ms: f64,
    video_decode_ms: f64,
    draw_ms: f64,
    readback_ms: f64,
    encode_wall_ms: f64,
    audio_ms: f64,
    min_ssim: f64,
    avg_ssim: f64,
}

struct BenchmarkConfig {
    tier: BenchmarkTier,
    fixtures: Option<String>,
    bundle_dir: PathBuf,
    golden_dir: PathBuf,
    baseline_json: PathBuf,
    out_dir: PathBuf,
    concurrency: usize,
    gl: GlRenderer,
    raster: RasterBackend,
    encoder: HardwareAccel,
    timeout: u64,
    browser_executable: Option<PathBuf>,
    min_ssim: f64,
    offline: bool,
}

fn filter_baseline_rows<'a>(
    baseline: &'a BaselineFile,
    fixtures_opt: Option<&str>,
) -> Result<Vec<&'a BaselineRow>> {
    let Some(raw) = fixtures_opt else {
        return Ok(baseline.results.iter().collect());
    };
    let wanted: Vec<&str> = raw
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    if wanted.is_empty() {
        return Ok(baseline.results.iter().collect());
    }
    let mut selected = Vec::new();
    for id in wanted {
        let row = baseline
            .results
            .iter()
            .find(|r| r.id == id)
            .with_context(|| format!("fixture `{id}` not found in baseline.json"))?;
        selected.push(row);
    }
    Ok(selected)
}

fn run_tier1_benchmark(cfg: &BenchmarkConfig, baseline: &BaselineFile) -> Result<bool> {
    let selected = filter_baseline_rows(baseline, cfg.fixtures.as_deref())?;
    std::fs::create_dir_all(&cfg.out_dir)?;
    let mp4_dir = cfg.out_dir.join("mp4");
    let kf_root = cfg.out_dir.join("keyframes");
    std::fs::create_dir_all(&mp4_dir)?;
    std::fs::create_dir_all(&kf_root)?;

    let serve_url = cfg.bundle_dir.to_string_lossy().to_string();
    let mut rows = Vec::with_capacity(selected.len());
    let mut all_passed = true;

    for b in selected {
        let out_mp4 = mp4_dir.join(format!("{}.mp4", b.id));
        let kf_dir = kf_root.join(&b.id);
        std::fs::create_dir_all(&kf_dir)?;

        let mut save_keyframes = BTreeMap::new();
        for &kf in &baseline.keyframes {
            save_keyframes.insert(kf, kf_dir.join(format!("frame_{kf:04}.png")));
        }

        let report = render_media_tier1(Tier1RenderOptions {
            serve_url: serve_url.clone(),
            composition_id: b.id.clone(),
            output_path: out_mp4.clone(),
            codec: Codec::H264,
            crf: None,
            pixel_format: PixelFormat::Yuv420p,
            hw_accel: cfg.encoder,
            raster_backend: cfg.raster,
            frame_range: None,
            every_nth_frame: 1,
            scale: 1.0,
            muted: false,
            timeout_ms: cfg.timeout,
            input_props: serde_json::json!({}),
            x264_preset: None,
            video_bitrate: None,
            audio_bitrate: None,
            audio_codec: None,
            color_space: ColorSpace::Default,
            prores_profile: None,
            number_of_gif_loops: None,
            concurrency: cfg.concurrency,
            offline: cfg.offline,
            json_progress: false,
            abort_on_unsupported: false,
            save_keyframes,
        })
        .with_context(|| format!("Tier 1 rendering fixture {}", b.id))?;

        let verified_audio = ffprobe_check_mp4(&out_mp4, b.frames)?;

        let mut min_ssim = f64::INFINITY;
        let mut sum_ssim = 0.0_f64;
        for &kf in &baseline.keyframes {
            let gold = cfg
                .golden_dir
                .join(&b.id)
                .join(format!("frame_{kf:04}.png"));
            let actual = kf_dir.join(format!("frame_{kf:04}.png"));
            let diff = compare_png_files(&gold, &actual, None)?;
            min_ssim = min_ssim.min(diff.ssim);
            sum_ssim += diff.ssim;
        }
        let avg_ssim = sum_ssim / (baseline.keyframes.len().max(1) as f64);
        if min_ssim < cfg.min_ssim {
            all_passed = false;
        }

        let speedup = b.render_media_ms / report.wall_ms.max(0.001);
        let row = Tier1BenchFixtureRow {
            id: b.id.clone(),
            frames: report.frames_rendered,
            deduped_frames: report.deduped_frames,
            baseline_ms: b.render_media_ms,
            baseline_fps: b.fps,
            tier1_ms: report.wall_ms,
            tier1_fps: report.fps,
            tier1_ms_per_frame: report.ms_per_frame,
            speedup,
            raster_backend: report.raster_backend,
            gpu_renderer: report.gpu_renderer,
            encoder_used: report.encoder_used,
            has_audio: report.has_audio && verified_audio,
            js_ms: report.js_ms,
            layout_ms: report.layout_ms,
            video_decode_ms: report.video_decode_ms,
            draw_ms: report.draw_ms,
            readback_ms: report.readback_ms,
            encode_wall_ms: report.encode_wall_ms,
            audio_ms: report.audio_ms,
            min_ssim,
            avg_ssim,
        };
        eprintln!(
            "{:<20} | base {:>6.2} fps | tier1 {:>6.2} fps ({:>5.2}x) | min_ssim {:.5} | {} | {} | js={:.1}ms lay={:.1}ms vid={:.1}ms draw={:.1}ms rb={:.1}ms aud={:.1}ms audio={}",
            row.id,
            row.baseline_fps,
            row.tier1_fps,
            row.speedup,
            row.min_ssim,
            row.raster_backend,
            row.encoder_used,
            row.js_ms,
            row.layout_ms,
            row.video_decode_ms,
            row.draw_ms,
            row.readback_ms,
            row.audio_ms,
            row.has_audio,
        );
        rows.push(row);
    }

    let total_base_ms: f64 = rows.iter().map(|r| r.baseline_ms).sum();
    let total_t1_ms: f64 = rows.iter().map(|r| r.tier1_ms).sum();
    let total_frames: u32 = rows.iter().map(|r| r.frames).sum();
    let base_avg_fps = f64::from(total_frames) / (total_base_ms / 1000.0);
    let t1_avg_fps = f64::from(total_frames) / (total_t1_ms / 1000.0);
    let total_speedup = total_base_ms / total_t1_ms.max(0.001);
    let global_min_ssim = rows
        .iter()
        .map(|r| r.min_ssim)
        .fold(f64::INFINITY, f64::min);

    let summary = serde_json::json!({
        "tier": 1,
        "raster": format!("{:?}", cfg.raster).to_lowercase(),
        "totalFrames": total_frames,
        "baselineTotalMs": total_base_ms,
        "baselineAvgFps": base_avg_fps,
        "tier1TotalMs": total_t1_ms,
        "tier1AvgFps": t1_avg_fps,
        "overallSpeedup": total_speedup,
        "globalMinSsim": global_min_ssim,
        "allSsimPassed": all_passed,
        "rows": rows,
    });

    let out_json = cfg.out_dir.join("tier1-benchmark.json");
    std::fs::write(&out_json, serde_json::to_string_pretty(&summary)? + "\n")?;
    println!("{}", serde_json::to_string_pretty(&summary)?);

    Ok(all_passed)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AutoBenchFixtureRow {
    id: String,
    frames: u32,
    chosen_tier: u8,
    tier_label: String,
    reasons: Vec<String>,
    baseline_ms: f64,
    baseline_fps: f64,
    rsmotion_ms: f64,
    rsmotion_fps: f64,
    rsmotion_ms_per_frame: f64,
    speedup: f64,
    encoder_used: String,
    has_audio: bool,
    min_ssim: f64,
    avg_ssim: f64,
}

fn try_analyze_and_render_still_tier1(
    opts: Tier1StillOptions,
    explain: bool,
) -> Result<(CompositionAnalysis, Option<Tier1StillReport>)> {
    let outcome = analyze_composition_for_dispatch_with_offline(
        &opts.serve_url,
        &opts.composition_id,
        &opts.input_props,
        opts.timeout_ms,
        None,
        Some(opts.frame),
        opts.offline,
    )?;
    if explain {
        eprintln!(
            "{}",
            format_composition_explanation(&outcome.analysis, &outcome.static_scan)
        );
    }

    let mut analysis = outcome.analysis;
    if analysis.chosen_tier != 1 {
        return Ok((analysis, None));
    }

    match render_still_tier1_with_host(opts, outcome.prepared_tier1) {
        Ok(rep) => Ok((analysis, Some(rep))),
        Err(err) => {
            warn!("Tier 1 still failed, falling back to Tier 2: {err:#}");
            analysis.chosen_tier = 2;
            analysis.tier_label = "tier2-cdp-fallback".to_string();
            analysis
                .reasons
                .push(format!("runtime fallback to Tier 2: {err}"));
            Ok((analysis, None))
        }
    }
}

fn try_analyze_and_render_media_tier1(
    opts: Tier1RenderOptions,
    explain: bool,
) -> Result<(CompositionAnalysis, Option<Tier1RenderReport>)> {
    let outcome = analyze_composition_for_dispatch_with_offline(
        &opts.serve_url,
        &opts.composition_id,
        &opts.input_props,
        opts.timeout_ms,
        opts.frame_range,
        None,
        opts.offline,
    )?;
    if explain {
        eprintln!(
            "{}",
            format_composition_explanation(&outcome.analysis, &outcome.static_scan)
        );
    }

    let mut analysis = outcome.analysis;
    if analysis.chosen_tier != 1 {
        return Ok((analysis, None));
    }

    match render_media_tier1_with_host(opts, outcome.prepared_tier1) {
        Ok(rep) => Ok((analysis, Some(rep))),
        Err(err) => {
            if let Some(late) = err.downcast_ref::<LateUnsupportedFeatureError>() {
                warn!("{late}; falling back cleanly to Tier 2 Chrome CDP");
            } else {
                warn!("Tier 1 render failed, falling back to Tier 2: {err:#}");
            }
            analysis.chosen_tier = 2;
            analysis.tier_label = "tier2-cdp-fallback".to_string();
            analysis
                .reasons
                .push(format!("runtime fallback to Tier 2: {err}"));
            Ok((analysis, None))
        }
    }
}

fn try_analyze_and_render_frames_tier1(
    opts: Tier1FramesOptions,
    explain: bool,
) -> Result<(CompositionAnalysis, Option<Tier1FramesReport>)> {
    let outcome = analyze_composition_for_dispatch_with_offline(
        &opts.serve_url,
        &opts.composition_id,
        &opts.input_props,
        opts.timeout_ms,
        opts.frame_range,
        None,
        opts.offline,
    )?;
    if explain {
        eprintln!(
            "{}",
            format_composition_explanation(&outcome.analysis, &outcome.static_scan)
        );
    }

    let mut analysis = outcome.analysis;
    if analysis.chosen_tier != 1 {
        return Ok((analysis, None));
    }

    match render_frames_tier1_with_host(opts, outcome.prepared_tier1) {
        Ok(rep) => Ok((analysis, Some(rep))),
        Err(err) => {
            if let Some(late) = err.downcast_ref::<LateUnsupportedFeatureError>() {
                warn!("{late}; falling back to Tier 2 frames render");
            } else {
                warn!("Tier 1 frames render failed, falling back to Tier 2: {err:#}");
            }
            analysis.chosen_tier = 2;
            analysis.tier_label = "tier2-cdp-fallback".to_string();
            analysis
                .reasons
                .push(format!("runtime fallback to Tier 2: {err}"));
            Ok((analysis, None))
        }
    }
}

async fn run_auto_benchmark(cfg: &BenchmarkConfig, baseline: &BaselineFile) -> Result<bool> {
    let selected = filter_baseline_rows(baseline, cfg.fixtures.as_deref())?;
    std::fs::create_dir_all(&cfg.out_dir)?;
    let mp4_dir = cfg.out_dir.join("mp4");
    let kf_root = cfg.out_dir.join("keyframes");
    std::fs::create_dir_all(&mp4_dir)?;
    std::fs::create_dir_all(&kf_root)?;

    let serve_url = cfg.bundle_dir.to_string_lossy().to_string();
    let mut rows = Vec::with_capacity(selected.len());
    let mut all_passed = true;

    for b in selected {
        let out_mp4 = mp4_dir.join(format!("{}.mp4", b.id));
        let kf_dir = kf_root.join(&b.id);
        std::fs::create_dir_all(&kf_dir)?;

        let mut save_keyframes = BTreeMap::new();
        for &kf in &baseline.keyframes {
            save_keyframes.insert(kf, kf_dir.join(format!("frame_{kf:04}.png")));
        }

        let t1_opts = Tier1RenderOptions {
            serve_url: serve_url.clone(),
            composition_id: b.id.clone(),
            output_path: out_mp4.clone(),
            codec: Codec::H264,
            crf: None,
            pixel_format: PixelFormat::Yuv420p,
            hw_accel: cfg.encoder,
            raster_backend: cfg.raster,
            frame_range: None,
            every_nth_frame: 1,
            scale: 1.0,
            muted: false,
            timeout_ms: cfg.timeout,
            input_props: serde_json::json!({}),
            x264_preset: None,
            video_bitrate: None,
            audio_bitrate: None,
            audio_codec: None,
            color_space: ColorSpace::Default,
            prores_profile: None,
            number_of_gif_loops: None,
            concurrency: cfg.concurrency,
            offline: cfg.offline,
            json_progress: false,
            abort_on_unsupported: true,
            save_keyframes: save_keyframes.clone(),
        };

        let (analysis, tier1_opt) =
            tokio::task::spawn_blocking(move || try_analyze_and_render_media_tier1(t1_opts, false))
                .await
                .context("auto dispatch task panicked")??;

        let chosen_tier = analysis.chosen_tier;
        let tier_label = analysis.tier_label;
        let reasons = analysis.reasons;

        let (wall_ms, fps, ms_per_frame, encoder_used, report_has_audio) = match tier1_opt {
            Some(rep) => (
                rep.wall_ms,
                rep.fps,
                rep.ms_per_frame,
                rep.encoder_used,
                rep.has_audio,
            ),
            None => {
                let rep = render_media_cdp(CdpRenderOptions {
                    serve_url: serve_url.clone(),
                    composition_id: b.id.clone(),
                    output_path: out_mp4.clone(),
                    codec: Codec::H264,
                    crf: None,
                    pixel_format: PixelFormat::Yuv420p,
                    hw_accel: cfg.encoder,
                    gl: cfg.gl,
                    concurrency: cfg.concurrency,
                    frame_range: None,
                    every_nth_frame: 1,
                    scale: 1.0,
                    muted: false,
                    timeout_ms: cfg.timeout,
                    input_props: serde_json::json!({}),
                    browser_executable: cfg.browser_executable.clone(),
                    x264_preset: None,
                    video_bitrate: None,
                    audio_bitrate: None,
                    audio_codec: None,
                    color_space: ColorSpace::Default,
                    prores_profile: None,
                    number_of_gif_loops: None,
                    json_progress: false,
                    save_keyframes,
                })
                .await
                .with_context(|| format!("Tier 2 rendering fixture {}", b.id))?;
                (
                    rep.wall_ms,
                    rep.fps,
                    rep.ms_per_frame,
                    rep.encoder_used,
                    rep.has_audio,
                )
            }
        };

        let verified_audio = ffprobe_check_mp4(&out_mp4, b.frames)?;

        let mut min_ssim = f64::INFINITY;
        let mut sum_ssim = 0.0_f64;
        for &kf in &baseline.keyframes {
            let gold = cfg
                .golden_dir
                .join(&b.id)
                .join(format!("frame_{kf:04}.png"));
            let actual = kf_dir.join(format!("frame_{kf:04}.png"));
            let diff = compare_png_files(&gold, &actual, None)?;
            min_ssim = min_ssim.min(diff.ssim);
            sum_ssim += diff.ssim;
        }
        let avg_ssim = sum_ssim / (baseline.keyframes.len().max(1) as f64);
        if min_ssim < cfg.min_ssim {
            all_passed = false;
        }

        let speedup = b.render_media_ms / wall_ms.max(0.001);
        let row = AutoBenchFixtureRow {
            id: b.id.clone(),
            frames: b.frames,
            chosen_tier,
            tier_label,
            reasons,
            baseline_ms: b.render_media_ms,
            baseline_fps: b.fps,
            rsmotion_ms: wall_ms,
            rsmotion_fps: fps,
            rsmotion_ms_per_frame: ms_per_frame,
            speedup,
            encoder_used,
            has_audio: report_has_audio && verified_audio,
            min_ssim,
            avg_ssim,
        };
        eprintln!(
            "{:<20} | tier {} | base {:>6.2} fps | auto {:>6.2} fps ({:>5.2}x) | min_ssim {:.5} | {} | audio={}",
            row.id,
            row.chosen_tier,
            row.baseline_fps,
            row.rsmotion_fps,
            row.speedup,
            row.min_ssim,
            row.encoder_used,
            row.has_audio,
        );
        rows.push(row);
    }

    let total_base_ms: f64 = rows.iter().map(|r| r.baseline_ms).sum();
    let total_auto_ms: f64 = rows.iter().map(|r| r.rsmotion_ms).sum();
    let total_frames: u32 = rows.iter().map(|r| r.frames).sum();
    let base_avg_fps = f64::from(total_frames) / (total_base_ms / 1000.0);
    let auto_avg_fps = f64::from(total_frames) / (total_auto_ms / 1000.0);
    let total_speedup = total_base_ms / total_auto_ms.max(0.001);
    let global_min_ssim = rows
        .iter()
        .map(|r| r.min_ssim)
        .fold(f64::INFINITY, f64::min);
    let tier1_count = rows.iter().filter(|r| r.chosen_tier == 1).count();
    let tier2_count = rows.iter().filter(|r| r.chosen_tier == 2).count();

    let summary = serde_json::json!({
        "tier": "auto",
        "tier1Fixtures": tier1_count,
        "tier2Fixtures": tier2_count,
        "raster": format!("{:?}", cfg.raster).to_lowercase(),
        "totalFrames": total_frames,
        "baselineTotalMs": total_base_ms,
        "baselineAvgFps": base_avg_fps,
        "autoTotalMs": total_auto_ms,
        "autoAvgFps": auto_avg_fps,
        "overallSpeedup": total_speedup,
        "globalMinSsim": global_min_ssim,
        "allSsimPassed": all_passed,
        "rows": rows,
    });

    let out_json = cfg.out_dir.join("auto-benchmark.json");
    std::fs::write(&out_json, serde_json::to_string_pretty(&summary)? + "\n")?;
    println!("{}", serde_json::to_string_pretty(&summary)?);

    Ok(all_passed)
}

async fn run_benchmark(cfg: BenchmarkConfig) -> Result<bool> {
    let raw = std::fs::read_to_string(&cfg.baseline_json)
        .with_context(|| format!("reading {}", cfg.baseline_json.display()))?;
    let baseline: BaselineFile = serde_json::from_str(&raw)?;

    match cfg.tier {
        BenchmarkTier::Auto => return run_auto_benchmark(&cfg, &baseline).await,
        BenchmarkTier::Tier1 => {
            return tokio::task::spawn_blocking(move || run_tier1_benchmark(&cfg, &baseline))
                .await
                .context("Tier 1 benchmark task panicked")?;
        }
        BenchmarkTier::Tier2 => {}
    }

    let selected = filter_baseline_rows(&baseline, cfg.fixtures.as_deref())?;
    std::fs::create_dir_all(&cfg.out_dir)?;
    let mp4_dir = cfg.out_dir.join("mp4");
    let kf_root = cfg.out_dir.join("keyframes");
    std::fs::create_dir_all(&mp4_dir)?;
    std::fs::create_dir_all(&kf_root)?;

    let serve_url = cfg.bundle_dir.to_string_lossy().to_string();
    let mut rows = Vec::with_capacity(selected.len());
    let mut all_passed = true;

    for b in selected {
        let out_mp4 = mp4_dir.join(format!("{}.mp4", b.id));
        let kf_dir = kf_root.join(&b.id);
        std::fs::create_dir_all(&kf_dir)?;

        let mut save_keyframes = BTreeMap::new();
        for &kf in &baseline.keyframes {
            save_keyframes.insert(kf, kf_dir.join(format!("frame_{kf:04}.png")));
        }

        let report = render_media_cdp(CdpRenderOptions {
            serve_url: serve_url.clone(),
            composition_id: b.id.clone(),
            output_path: out_mp4.clone(),
            codec: Codec::H264,
            crf: None,
            pixel_format: PixelFormat::Yuv420p,
            hw_accel: cfg.encoder,
            gl: cfg.gl,
            concurrency: cfg.concurrency,
            frame_range: None,
            every_nth_frame: 1,
            scale: 1.0,
            muted: false,
            timeout_ms: cfg.timeout,
            input_props: serde_json::json!({}),
            browser_executable: cfg.browser_executable.clone(),
            x264_preset: None,
            video_bitrate: None,
            audio_bitrate: None,
            audio_codec: None,
            color_space: ColorSpace::Default,
            prores_profile: None,
            number_of_gif_loops: None,
            json_progress: false,
            save_keyframes,
        })
        .await
        .with_context(|| format!("rendering fixture {}", b.id))?;

        let verified_audio = ffprobe_check_mp4(&out_mp4, b.frames)?;

        let mut min_ssim = f64::INFINITY;
        let mut sum_ssim = 0.0_f64;
        for &kf in &baseline.keyframes {
            let gold = cfg
                .golden_dir
                .join(&b.id)
                .join(format!("frame_{kf:04}.png"));
            let actual = kf_dir.join(format!("frame_{kf:04}.png"));
            let diff = compare_png_files(&gold, &actual, None)?;
            min_ssim = min_ssim.min(diff.ssim);
            sum_ssim += diff.ssim;
        }
        let avg_ssim = sum_ssim / (baseline.keyframes.len().max(1) as f64);
        if min_ssim < cfg.min_ssim {
            all_passed = false;
        }

        let speedup = b.render_media_ms / report.wall_ms.max(0.001);
        let row = BenchFixtureRow {
            id: b.id.clone(),
            frames: report.frames_rendered,
            baseline_ms: b.render_media_ms,
            baseline_fps: b.fps,
            tier2_ms: report.wall_ms,
            tier2_fps: report.fps,
            tier2_ms_per_frame: report.ms_per_frame,
            speedup,
            capture_mode: report.capture_mode,
            encoder_used: report.encoder_used,
            has_audio: report.has_audio && verified_audio,
            min_ssim,
            avg_ssim,
        };
        eprintln!(
            "{:<20} | base {:>6.2} fps | tier2 {:>6.2} fps ({:>5.2}x) | min_ssim {:.5} | {} | {} | audio={}",
            row.id,
            row.baseline_fps,
            row.tier2_fps,
            row.speedup,
            row.min_ssim,
            row.capture_mode,
            row.encoder_used,
            row.has_audio
        );
        rows.push(row);
    }

    let total_base_ms: f64 = rows.iter().map(|r| r.baseline_ms).sum();
    let total_t2_ms: f64 = rows.iter().map(|r| r.tier2_ms).sum();
    let total_frames: u32 = rows.iter().map(|r| r.frames).sum();
    let base_avg_fps = f64::from(total_frames) / (total_base_ms / 1000.0);
    let t2_avg_fps = f64::from(total_frames) / (total_t2_ms / 1000.0);
    let total_speedup = total_base_ms / total_t2_ms.max(0.001);
    let global_min_ssim = rows
        .iter()
        .map(|r| r.min_ssim)
        .fold(f64::INFINITY, f64::min);

    let summary = serde_json::json!({
        "concurrency": cfg.concurrency,
        "gl": cfg.gl,
        "totalFrames": total_frames,
        "baselineTotalMs": total_base_ms,
        "baselineAvgFps": base_avg_fps,
        "tier2TotalMs": total_t2_ms,
        "tier2AvgFps": t2_avg_fps,
        "overallSpeedup": total_speedup,
        "globalMinSsim": global_min_ssim,
        "allSsimPassed": all_passed,
        "rows": rows,
    });

    let out_json = cfg.out_dir.join("tier2-benchmark.json");
    std::fs::write(&out_json, serde_json::to_string_pretty(&summary)? + "\n")?;
    println!("{}", serde_json::to_string_pretty(&summary)?);

    let _ = baseline.results.first().map(|r| r.ms_per_frame);

    Ok(all_passed)
}

async fn run(cli: Cli) -> Result<bool> {
    let offline = cli.offline
        || std::env::var("RSMOTION_OFFLINE")
            .is_ok_and(|v| v == "1" || v.eq_ignore_ascii_case("true"));
    match cli.command {
        Commands::SpikeEncode {
            frames,
            width,
            height,
            encoder,
            out,
            x264_preset,
        } => run_spike_encode(frames, width, height, encoder, out, x264_preset).map(|()| true),

        Commands::SpikeRaster {
            frames,
            width,
            height,
            backend,
            encode,
            encoder,
            out_dir,
            sample_image,
        } => {
            let cfg = SpikeConfig {
                backend: backend.into(),
                frames,
                width,
                height,
                encode,
                hw_accel: encoder,
                out_dir,
                sample_image_path: sample_image,
            };
            let report = run_raster_spike(&cfg)?;
            println!("{}", serde_json::to_string_pretty(&report)?);
            Ok(true)
        }

        Commands::Diff {
            golden,
            actual,
            heatmap,
            min_ssim,
        } => run_diff(&golden, &actual, heatmap.as_deref(), min_ssim),

        Commands::Compositions {
            serve_url,
            props,
            browser_executable,
            gl,
            timeout,
            force_tier,
            json,
            bundler,
        } => {
            let serve_url = resolve_serve_url_or_bundle(serve_url, &bundler, false).await?;
            let input_props = parse_props_arg(props.as_deref())?;
            let comps = match force_tier {
                Some(1) => tokio::task::spawn_blocking(move || {
                    list_compositions_tier1_with_offline(&serve_url, &input_props, timeout, offline)
                })
                .await
                .context("Tier 1 compositions task panicked")??,
                Some(2) => {
                    list_compositions_cdp(CdpCompositionsOptions {
                        serve_url,
                        gl,
                        timeout_ms: timeout,
                        input_props,
                        browser_executable,
                    })
                    .await?
                }
                Some(3) => {
                    let gl_str = match gl {
                        GlRenderer::None => "none",
                        GlRenderer::AngleEgl => "angle-egl",
                        GlRenderer::Swangle => "swangle",
                        GlRenderer::Angle => "angle",
                    };
                    tokio::task::spawn_blocking(move || {
                        tier3::list_compositions_tier3(
                            &serve_url,
                            &input_props,
                            timeout,
                            gl_str,
                            browser_executable.as_deref(),
                        )
                    })
                    .await
                    .context("Tier 3 compositions task panicked")??
                }
                None => {
                    if resolve_local_bundle_dir(&serve_url).is_some() {
                        let s_url = serve_url.clone();
                        let p_clone = input_props.clone();
                        let t1_res = tokio::task::spawn_blocking(move || {
                            list_compositions_tier1_with_offline(&s_url, &p_clone, timeout, offline)
                        })
                        .await
                        .context("Tier 1 compositions task panicked")?;
                        match t1_res {
                            Ok(c) => c,
                            Err(err) => {
                                warn!(
                                    "Tier 1 compositions failed, falling back to Tier 2: {err:#}"
                                );
                                list_compositions_cdp(CdpCompositionsOptions {
                                    serve_url,
                                    gl,
                                    timeout_ms: timeout,
                                    input_props,
                                    browser_executable,
                                })
                                .await?
                            }
                        }
                    } else {
                        list_compositions_cdp(CdpCompositionsOptions {
                            serve_url,
                            gl,
                            timeout_ms: timeout,
                            input_props,
                            browser_executable,
                        })
                        .await?
                    }
                }
                Some(t) => bail!("--force-tier must be 1, 2, or 3 (got {t})"),
            };
            if json {
                println!(
                    "{}",
                    serde_json::to_string(&serde_json::json!({
                        "type": "result",
                        "data": comps,
                    }))?
                );
            } else {
                for c in comps {
                    println!(
                        "{:<24} {}x{} @ {} fps ({} frames)",
                        c.id, c.width, c.height, c.fps, c.duration_in_frames
                    );
                }
            }
            Ok(true)
        }

        Commands::Still {
            serve_url,
            composition_id,
            output,
            frame,
            image_format,
            jpeg_quality,
            scale,
            props,
            gl,
            raster,
            browser_executable,
            timeout,
            overwrite,
            force_tier,
            explain,
            json,
            bundler,
        } => {
            ensure_writable(&output, overwrite)?;
            let serve_url = resolve_serve_url_or_bundle(serve_url, &bundler, explain).await?;
            let input_props = parse_props_arg(props.as_deref())?;
            let gl_str = match gl {
                GlRenderer::None => "none",
                GlRenderer::AngleEgl => "angle-egl",
                GlRenderer::Swangle => "swangle",
                GlRenderer::Angle => "angle",
            };

            let val = match force_tier {
                Some(1) => {
                    if explain {
                        eprintln!("[explain] forced tier=1 (--force-tier 1)");
                    }
                    let rep = tokio::task::spawn_blocking(move || {
                        render_still_tier1(Tier1StillOptions {
                            serve_url,
                            composition_id,
                            output_path: output,
                            frame,
                            image_format,
                            jpeg_quality,
                            scale,
                            raster_backend: raster,
                            timeout_ms: timeout,
                            offline,
                            input_props,
                        })
                    })
                    .await
                    .context("Tier 1 still task panicked")??;
                    serde_json::to_value(&rep)?
                }
                Some(2) => {
                    if explain {
                        eprintln!("[explain] forced tier=2 (--force-tier 2)");
                    }
                    let rep = render_still_cdp(CdpStillOptions {
                        serve_url,
                        composition_id,
                        output_path: output,
                        frame,
                        image_format,
                        jpeg_quality,
                        scale,
                        gl,
                        timeout_ms: timeout,
                        input_props,
                        browser_executable,
                    })
                    .await?;
                    serde_json::to_value(&rep)?
                }
                Some(3) => {
                    if explain {
                        eprintln!("[explain] forced tier=3 (--force-tier 3 @remotion/renderer)");
                    }
                    tokio::task::spawn_blocking(move || {
                        tier3::render_still_tier3(
                            &serve_url,
                            &composition_id,
                            &output,
                            frame,
                            image_format,
                            jpeg_quality,
                            scale,
                            gl_str,
                            timeout,
                            &input_props,
                            browser_executable.as_deref(),
                        )
                    })
                    .await
                    .context("Tier 3 still task panicked")??
                }
                None => {
                    let opts = Tier1StillOptions {
                        serve_url: serve_url.clone(),
                        composition_id: composition_id.clone(),
                        output_path: output.clone(),
                        frame,
                        image_format,
                        jpeg_quality,
                        scale,
                        raster_backend: raster,
                        timeout_ms: timeout,
                        offline,
                        input_props: input_props.clone(),
                    };
                    let (analysis, tier1_opt) = tokio::task::spawn_blocking(move || {
                        try_analyze_and_render_still_tier1(opts, explain)
                    })
                    .await
                    .context("auto still dispatch task panicked")??;

                    let mut rep_val = match tier1_opt {
                        Some(rep) => serde_json::to_value(&rep)?,
                        None => {
                            let rep = render_still_cdp(CdpStillOptions {
                                serve_url,
                                composition_id,
                                output_path: output,
                                frame,
                                image_format,
                                jpeg_quality,
                                scale,
                                gl,
                                timeout_ms: timeout,
                                input_props,
                                browser_executable,
                            })
                            .await?;
                            serde_json::to_value(&rep)?
                        }
                    };

                    if explain {
                        if let Some(obj) = rep_val.as_object_mut() {
                            obj.insert("analysis".to_string(), serde_json::to_value(&analysis)?);
                        }
                    }
                    rep_val
                }
                Some(t) => bail!("--force-tier must be 1, 2, or 3 (got {t})"),
            };

            if json {
                println!(
                    "{}",
                    serde_json::to_string(&serde_json::json!({
                        "type": "result",
                        "data": val,
                    }))?
                );
            } else {
                println!("{}", serde_json::to_string_pretty(&val)?);
            }
            Ok(true)
        }

        Commands::Render {
            serve_url,
            composition_id,
            output,
            codec,
            crf,
            video_bitrate,
            audio_bitrate,
            audio_codec,
            color_space,
            prores_profile,
            number_of_gif_loops,
            sequence,
            image_format,
            jpeg_quality,
            concurrency,
            frames,
            every_nth_frame,
            scale,
            muted,
            pixel_format,
            gl,
            raster,
            encoder,
            x264_preset,
            timeout,
            props,
            browser_executable,
            overwrite,
            force_tier,
            explain,
            json_progress,
            bundler,
        } => {
            if sequence {
                return run_frames_command(
                    serve_url,
                    composition_id,
                    output,
                    image_format,
                    jpeg_quality,
                    frames,
                    every_nth_frame,
                    scale,
                    gl,
                    raster,
                    timeout,
                    props,
                    browser_executable,
                    overwrite,
                    force_tier,
                    explain,
                    json_progress,
                    &bundler,
                    offline,
                )
                .await;
            }
            ensure_writable(&output, overwrite)?;
            let serve_url = resolve_serve_url_or_bundle(serve_url, &bundler, explain).await?;
            let frame_range = match frames {
                Some(ref s) => Some(s.parse::<FrameRange>()?),
                None => None,
            };
            let input_props = parse_props_arg(props.as_deref())?;
            let gl_str = match gl {
                GlRenderer::None => "none",
                GlRenderer::AngleEgl => "angle-egl",
                GlRenderer::Swangle => "swangle",
                GlRenderer::Angle => "angle",
            };

            let val = match force_tier {
                Some(1) => {
                    if explain {
                        eprintln!("[explain] forced tier=1 (--force-tier 1)");
                    }
                    let rep = tokio::task::spawn_blocking(move || {
                        render_media_tier1(Tier1RenderOptions {
                            serve_url,
                            composition_id,
                            output_path: output,
                            codec,
                            crf,
                            pixel_format,
                            hw_accel: encoder,
                            raster_backend: raster,
                            frame_range,
                            every_nth_frame,
                            scale,
                            muted,
                            timeout_ms: timeout,
                            input_props,
                            x264_preset,
                            video_bitrate,
                            audio_bitrate,
                            audio_codec,
                            color_space,
                            prores_profile,
                            number_of_gif_loops,
                            concurrency,
                            offline,
                            json_progress,
                            abort_on_unsupported: false,
                            save_keyframes: BTreeMap::new(),
                        })
                    })
                    .await
                    .context("Tier 1 render task panicked")??;
                    serde_json::to_value(&rep)?
                }
                Some(2) => {
                    if explain {
                        eprintln!("[explain] forced tier=2 (--force-tier 2)");
                    }
                    let rep = render_media_cdp(CdpRenderOptions {
                        serve_url,
                        composition_id,
                        output_path: output,
                        codec,
                        crf,
                        pixel_format,
                        hw_accel: encoder,
                        gl,
                        concurrency,
                        frame_range,
                        every_nth_frame,
                        scale,
                        muted,
                        timeout_ms: timeout,
                        input_props,
                        browser_executable,
                        x264_preset,
                        video_bitrate,
                        audio_bitrate,
                        audio_codec,
                        color_space,
                        prores_profile,
                        number_of_gif_loops,
                        json_progress,
                        save_keyframes: BTreeMap::new(),
                    })
                    .await?;
                    serde_json::to_value(&rep)?
                }
                Some(3) => {
                    if explain {
                        eprintln!("[explain] forced tier=3 (--force-tier 3 @remotion/renderer)");
                    }
                    tokio::task::spawn_blocking(move || {
                        tier3::render_media_tier3(
                            &serve_url,
                            &composition_id,
                            &output,
                            codec,
                            crf,
                            pixel_format,
                            gl_str,
                            concurrency,
                            frame_range,
                            every_nth_frame,
                            scale,
                            muted,
                            timeout,
                            &input_props,
                            browser_executable.as_deref(),
                            x264_preset.as_deref(),
                            video_bitrate.as_deref(),
                            audio_bitrate.as_deref(),
                            audio_codec,
                            color_space,
                            prores_profile,
                            number_of_gif_loops,
                        )
                    })
                    .await
                    .context("Tier 3 render task panicked")??
                }
                None => {
                    let opts = Tier1RenderOptions {
                        serve_url: serve_url.clone(),
                        composition_id: composition_id.clone(),
                        output_path: output.clone(),
                        codec,
                        crf,
                        pixel_format,
                        hw_accel: encoder,
                        raster_backend: raster,
                        frame_range,
                        every_nth_frame,
                        scale,
                        muted,
                        timeout_ms: timeout,
                        input_props: input_props.clone(),
                        x264_preset: x264_preset.clone(),
                        video_bitrate: video_bitrate.clone(),
                        audio_bitrate: audio_bitrate.clone(),
                        audio_codec,
                        color_space,
                        prores_profile,
                        number_of_gif_loops,
                        concurrency,
                        offline,
                        json_progress,
                        abort_on_unsupported: true,
                        save_keyframes: BTreeMap::new(),
                    };
                    let (analysis, tier1_opt) = tokio::task::spawn_blocking(move || {
                        try_analyze_and_render_media_tier1(opts, explain)
                    })
                    .await
                    .context("auto render dispatch task panicked")??;

                    let mut rep_val = match tier1_opt {
                        Some(rep) => serde_json::to_value(&rep)?,
                        None => {
                            let rep = render_media_cdp(CdpRenderOptions {
                                serve_url,
                                composition_id,
                                output_path: output,
                                codec,
                                crf,
                                pixel_format,
                                hw_accel: encoder,
                                gl,
                                concurrency,
                                frame_range,
                                every_nth_frame,
                                scale,
                                muted,
                                timeout_ms: timeout,
                                input_props,
                                browser_executable,
                                x264_preset,
                                video_bitrate,
                                audio_bitrate,
                                audio_codec,
                                color_space,
                                prores_profile,
                                number_of_gif_loops,
                                json_progress,
                                save_keyframes: BTreeMap::new(),
                            })
                            .await?;
                            serde_json::to_value(&rep)?
                        }
                    };

                    if explain {
                        if let Some(obj) = rep_val.as_object_mut() {
                            obj.insert("analysis".to_string(), serde_json::to_value(&analysis)?);
                        }
                    }
                    rep_val
                }
                Some(t) => bail!("--force-tier must be 1, 2, or 3 (got {t})"),
            };

            if json_progress {
                println!(
                    "{}",
                    serde_json::to_string(&serde_json::json!({
                        "type": "result",
                        "data": val,
                    }))?
                );
            } else {
                println!("{}", serde_json::to_string_pretty(&val)?);
            }
            Ok(true)
        }

        Commands::Benchmark {
            tier,
            fixtures,
            bundle_dir,
            golden_dir,
            baseline_json,
            out_dir,
            concurrency,
            gl,
            raster,
            encoder,
            timeout,
            browser_executable,
            min_ssim,
        } => {
            run_benchmark(BenchmarkConfig {
                tier,
                fixtures,
                bundle_dir,
                golden_dir,
                baseline_json,
                out_dir,
                concurrency,
                gl,
                raster,
                encoder,
                timeout,
                browser_executable,
                min_ssim,
                offline,
            })
            .await
        }

        Commands::Explain {
            serve_url,
            composition,
            props,
            timeout,
            json,
            bundler,
        } => {
            let serve_url = resolve_serve_url_or_bundle(serve_url, &bundler, false).await?;
            let input_props = parse_props_arg(props.as_deref())?;
            let report = tokio::task::spawn_blocking(move || {
                analyze_bundle_with_offline(
                    &serve_url,
                    composition.as_deref(),
                    &input_props,
                    timeout,
                    offline,
                )
            })
            .await
            .context("explain analyzer task panicked")??;

            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                println!(
                    "Bundle: {} (scanned in {:.1} ms)",
                    report.serve_url, report.total_ms
                );
                if !report.static_scan.signals.is_empty() {
                    let hints: Vec<String> = report
                        .static_scan
                        .signals
                        .iter()
                        .map(|s| format!("{}@{}", s.feature, s.source))
                        .collect();
                    println!("Static bundle hints: {}", hints.join(", "));
                }
                println!();
                for c in &report.compositions {
                    println!(
                        "{:<20} | {}x{}@{} ({}f) | tier {} ({:<12}) | {:<32} | {}",
                        c.composition_id,
                        c.width,
                        c.height,
                        c.fps,
                        c.duration_in_frames,
                        c.chosen_tier,
                        c.tier_label,
                        c.expected_fps_range,
                        c.reasons.join("; ")
                    );
                }
            }
            Ok(true)
        }

        Commands::Frames {
            serve_url,
            composition_id,
            output_dir,
            image_format,
            jpeg_quality,
            frames,
            every_nth_frame,
            scale,
            gl,
            raster,
            timeout,
            props,
            browser_executable,
            overwrite,
            force_tier,
            explain,
            json_progress,
            bundler,
        } => {
            run_frames_command(
                serve_url,
                composition_id,
                output_dir,
                image_format,
                jpeg_quality,
                frames,
                every_nth_frame,
                scale,
                gl,
                raster,
                timeout,
                props,
                browser_executable,
                overwrite,
                force_tier,
                explain,
                json_progress,
                &bundler,
                offline,
            )
            .await
        }

        Commands::Bundle {
            entry_point,
            out_dir,
            enable_tailwind,
            public_dir,
            no_cache,
            json,
        } => {
            let symlink_public_dir = out_dir.is_none();
            let report = bundle_async(BundleOptions {
                entry_point,
                out_dir,
                public_dir,
                enable_tailwind,
                use_cache: !no_cache,
                symlink_public_dir,
            })
            .await?;
            if json {
                println!(
                    "{}",
                    serde_json::to_string(&serde_json::json!({
                        "type": "result",
                        "data": report,
                    }))?
                );
            } else {
                println!("{}", serde_json::to_string_pretty(&report)?);
            }
            Ok(true)
        }

        Commands::SilentParts {
            src,
            noise_threshold,
            min_duration,
            json,
        } => {
            let report = tokio::task::spawn_blocking(move || {
                get_silent_parts(&src, noise_threshold, min_duration)
            })
            .await
            .context("silent-parts task panicked")??;
            if json {
                println!(
                    "{}",
                    serde_json::to_string(&serde_json::json!({
                        "type": "result",
                        "data": report,
                    }))?
                );
            } else {
                println!("{}", serde_json::to_string_pretty(&report)?);
            }
            Ok(true)
        }

        Commands::ExtractAudio {
            src,
            output,
            overwrite,
            json,
        } => {
            ensure_writable(&output, overwrite)?;
            let src_clone = src.clone();
            let out_clone = output.clone();
            let t0 = std::time::Instant::now();
            tokio::task::spawn_blocking(move || extract_audio_track(&src_clone, &out_clone))
                .await
                .context("extract-audio task panicked")??;
            let wall_ms = t0.elapsed().as_secs_f64() * 1000.0;
            let bytes = std::fs::metadata(&output).map(|m| m.len()).unwrap_or(0);
            let report = serde_json::json!({
                "src": src,
                "output": output,
                "bytes": bytes,
                "wall_ms": wall_ms,
            });
            if json {
                println!(
                    "{}",
                    serde_json::to_string(&serde_json::json!({
                        "type": "result",
                        "data": report,
                    }))?
                );
            } else {
                println!("{}", serde_json::to_string_pretty(&report)?);
            }
            Ok(true)
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn run_frames_command(
    serve_url: String,
    composition_id: String,
    output_dir: PathBuf,
    image_format: ImageFormat,
    jpeg_quality: u8,
    frames: Option<String>,
    every_nth_frame: u32,
    scale: f64,
    gl: GlRenderer,
    raster: RasterBackend,
    timeout: u64,
    props: Option<String>,
    browser_executable: Option<PathBuf>,
    overwrite: bool,
    force_tier: Option<u8>,
    explain: bool,
    json_progress: bool,
    bundler: &BundlerCliArgs,
    offline: bool,
) -> Result<bool> {
    let frame_range = match frames {
        Some(ref s) => Some(s.parse::<FrameRange>()?),
        None => None,
    };
    let input_props = parse_props_arg(props.as_deref())?;
    if output_dir.exists() && !overwrite && std::fs::read_dir(&output_dir)?.next().is_some() {
        bail!(
            "Directory {} is non-empty. Use --overwrite to overwrite.",
            output_dir.display()
        );
    }
    let serve_url = resolve_serve_url_or_bundle(serve_url, bundler, explain).await?;

    let val = match force_tier {
        Some(1) => {
            let opts = Tier1FramesOptions {
                serve_url,
                composition_id,
                output_dir,
                image_format,
                jpeg_quality,
                frame_range,
                every_nth_frame,
                scale,
                raster_backend: raster,
                timeout_ms: timeout,
                offline,
                input_props,
                abort_on_unsupported: false,
                json_progress,
            };
            let rep =
                tokio::task::spawn_blocking(move || render_frames_tier1_with_host(opts, None))
                    .await
                    .context("Tier 1 frames task panicked")??;
            serde_json::to_value(&rep)?
        }
        Some(2) => {
            render_frames_cdp(
                &serve_url,
                &composition_id,
                &output_dir,
                image_format,
                jpeg_quality,
                frame_range,
                every_nth_frame,
                scale,
                gl,
                timeout,
                input_props,
                browser_executable,
                json_progress,
            )
            .await?
        }
        None => {
            let opts = Tier1FramesOptions {
                serve_url: serve_url.clone(),
                composition_id: composition_id.clone(),
                output_dir: output_dir.clone(),
                image_format,
                jpeg_quality,
                frame_range,
                every_nth_frame,
                scale,
                raster_backend: raster,
                timeout_ms: timeout,
                offline,
                input_props: input_props.clone(),
                abort_on_unsupported: true,
                json_progress,
            };
            let (analysis, tier1_opt) = tokio::task::spawn_blocking(move || {
                try_analyze_and_render_frames_tier1(opts, explain)
            })
            .await
            .context("auto frames dispatch task panicked")??;
            let mut rep_val = match tier1_opt {
                Some(rep) => serde_json::to_value(&rep)?,
                None => {
                    render_frames_cdp(
                        &serve_url,
                        &composition_id,
                        &output_dir,
                        image_format,
                        jpeg_quality,
                        frame_range,
                        every_nth_frame,
                        scale,
                        gl,
                        timeout,
                        input_props,
                        browser_executable,
                        json_progress,
                    )
                    .await?
                }
            };
            if explain {
                if let Some(obj) = rep_val.as_object_mut() {
                    obj.insert("analysis".to_string(), serde_json::to_value(&analysis)?);
                }
            }
            rep_val
        }
        Some(t) => bail!("--force-tier for `frames` supports 1 or 2 (got {t})"),
    };

    if json_progress {
        println!(
            "{}",
            serde_json::to_string(&serde_json::json!({
                "type": "result",
                "data": val,
            }))?
        );
    } else {
        println!("{}", serde_json::to_string_pretty(&val)?);
    }
    Ok(true)
}

#[allow(clippy::too_many_arguments)]
async fn render_frames_cdp(
    serve_url: &str,
    composition_id: &str,
    output_dir: &Path,
    image_format: ImageFormat,
    jpeg_quality: u8,
    frame_range: Option<FrameRange>,
    every_nth_frame: u32,
    scale: f64,
    gl: GlRenderer,
    timeout: u64,
    input_props: serde_json::Value,
    browser_executable: Option<PathBuf>,
    json_progress: bool,
) -> Result<serde_json::Value> {
    let t0 = std::time::Instant::now();
    let comps = list_compositions_cdp(CdpCompositionsOptions {
        serve_url: serve_url.to_string(),
        gl,
        timeout_ms: timeout,
        input_props: input_props.clone(),
        browser_executable: browser_executable.clone(),
    })
    .await?;
    let comp = comps
        .into_iter()
        .find(|c| c.id == composition_id)
        .with_context(|| format!("composition `{composition_id}` not found"))?;
    let range = match frame_range {
        Some(r) => r,
        None => FrameRange::full(comp.duration_in_frames)?,
    };
    let frames_list = range.resolve_frames(comp.duration_in_frames, every_nth_frame)?;
    let total = frames_list.len();
    std::fs::create_dir_all(output_dir)?;
    let ext = image_format_ext(image_format);
    let mut assets = Vec::with_capacity(total);

    for (seq_idx, &frame_idx) in frames_list.iter().enumerate() {
        let out_file = output_dir.join(format!("element-{frame_idx:04}.{ext}"));
        render_still_cdp(CdpStillOptions {
            serve_url: serve_url.to_string(),
            composition_id: composition_id.to_string(),
            output_path: out_file.clone(),
            frame: frame_idx,
            image_format,
            jpeg_quality,
            scale,
            gl,
            timeout_ms: timeout,
            input_props: input_props.clone(),
            browser_executable: browser_executable.clone(),
        })
        .await?;
        assets.push(out_file);
        if json_progress {
            let done = seq_idx + 1;
            println!(
                "{}",
                serde_json::json!({
                    "type": "progress",
                    "renderedFrames": done,
                    "totalFrames": total,
                    "progress": (done as f64) / (total.max(1) as f64),
                })
            );
        }
    }

    let wall_ms = t0.elapsed().as_secs_f64() * 1000.0;
    let frames_rendered = assets.len() as u32;
    Ok(serde_json::json!({
        "tier": 2,
        "composition_id": composition_id,
        "output_dir": output_dir,
        "width": ((f64::from(comp.width) * scale).round() as u32).max(1),
        "height": ((f64::from(comp.height) * scale).round() as u32).max(1),
        "frames_rendered": frames_rendered,
        "assets": assets,
        "wall_ms": wall_ms,
        "fps": if wall_ms > 0.0 { f64::from(frames_rendered) / (wall_ms / 1000.0) } else { 0.0 },
    }))
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let filter = if std::env::var_os("RUST_LOG").is_some() {
        tracing_subscriber::EnvFilter::from_default_env()
    } else if let Some(ref lvl) = cli.log {
        let lower = lvl.to_ascii_lowercase();
        let mapped = match lower.as_str() {
            "verbose" | "debug" => "debug",
            "info" => "info",
            "warn" => "warn",
            "error" => "error",
            other => other,
        };
        tracing_subscriber::EnvFilter::new(mapped)
    } else {
        tracing_subscriber::EnvFilter::from_default_env()
    };

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();

    let rt = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(err) => {
            eprintln!("Error creating tokio runtime: {err}");
            return ExitCode::from(1);
        }
    };

    match rt.block_on(run(cli)) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(1),
        Err(err) => {
            eprintln!("Error: {err:#}");
            ExitCode::from(1)
        }
    }
}
