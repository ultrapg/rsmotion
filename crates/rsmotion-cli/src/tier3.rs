use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};
use rsmotion_core::{
    AudioCodec, Codec, ColorSpace, FrameRange, ImageFormat, PixelFormat, ProResProfile, VideoConfig,
};
use serde_json::{json, Value};

fn resolve_remotion_renderer_dir(serve_url: &str) -> Result<PathBuf> {
    if let Ok(custom) = std::env::var("RSMOTION_TIER3_MODULE") {
        let p = PathBuf::from(custom);
        if p.join("package.json").is_file() {
            return Ok(std::fs::canonicalize(&p)?);
        }
        bail!(
            "RSMOTION_TIER3_MODULE points to invalid package directory: {}",
            p.display()
        );
    }

    let mut candidates = Vec::new();
    let serve_path = Path::new(serve_url);
    if serve_path.exists() {
        let base = if serve_path.is_file() {
            serve_path.parent().unwrap_or(Path::new("."))
        } else {
            serve_path
        };
        candidates.push(base.join("node_modules/@remotion/renderer"));
        if let Some(parent) = base.parent() {
            candidates.push(parent.join("node_modules/@remotion/renderer"));
        }
    }

    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join("node_modules/@remotion/renderer"));
        candidates.push(cwd.join("tests/fixtures/node_modules/@remotion/renderer"));
    }

    for cand in candidates {
        if cand.join("package.json").is_file() {
            return Ok(std::fs::canonicalize(&cand)?);
        }
    }

    bail!(
        "Tier 3 fallback requires `@remotion/renderer` installed (tried local node_modules, tests/fixtures/node_modules, and RSMOTION_TIER3_MODULE)"
    )
}

fn resolve_serve_url(serve_url: &str) -> String {
    if serve_url.starts_with("http://") || serve_url.starts_with("https://") {
        return serve_url.to_string();
    }
    std::fs::canonicalize(serve_url)
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| serve_url.to_string())
}

const NODE_TIER3_RUNNER: &str = r#"
const fs = require("node:fs");
const path = require("node:path");
const args = JSON.parse(process.env.RSMOTION_TIER3_ARGS || "{}");
const mod = require(args.moduleDir);

function pickGl(gl) {
  if (!gl || gl === "none") return undefined;
  return gl;
}

(async () => {
  const common = {
    serveUrl: args.serveUrl,
    inputProps: args.inputProps || {},
    timeoutInMilliseconds: args.timeoutMs,
    browserExecutable: args.browserExecutable || undefined,
    chromiumOptions: { gl: pickGl(args.gl) },
    logLevel: "warn",
  };

  if (args.mode === "compositions") {
    const comps = await mod.getCompositions(args.serveUrl, common);
    process.stdout.write(JSON.stringify(comps));
    return;
  }

  const comps = await mod.getCompositions(args.serveUrl, common);
  const comp = comps.find((c) => c.id === args.compositionId);
  if (!comp) {
    throw new Error("Composition not found: " + args.compositionId);
  }

  if (args.mode === "still") {
    const t0 = performance.now();
    await mod.renderStill({
      ...common,
      composition: comp,
      output: args.outputPath,
      frame: args.frame,
      imageFormat: args.imageFormat,
      jpegQuality: args.imageFormat === "jpeg" ? args.jpegQuality : undefined,
      scale: args.scale,
      overwrite: true,
    });
    process.stdout.write(JSON.stringify({
      tier: 3,
      composition_id: comp.id,
      frame: args.frame,
      width: Math.max(1, Math.round(comp.width * args.scale)),
      height: Math.max(1, Math.round(comp.height * args.scale)),
      wall_ms: performance.now() - t0,
      output_path: args.outputPath,
    }));
    return;
  }

  if (args.mode === "render") {
    const t0 = performance.now();
    await mod.renderMedia({
      ...common,
      composition: comp,
      codec: args.codec,
      outputLocation: args.outputPath,
      crf: args.crf ?? undefined,
      concurrency: args.concurrency,
      frameRange: args.frameRange ?? undefined,
      everyNthFrame: args.everyNthFrame,
      scale: args.scale,
      muted: args.muted,
      pixelFormat: args.pixelFormat,
      x264Preset: args.x264Preset ?? undefined,
      videoBitrate: args.videoBitrate ?? undefined,
      audioBitrate: args.audioBitrate ?? undefined,
      audioCodec: args.audioCodec ?? undefined,
      colorSpace: args.colorSpace ?? undefined,
      proResProfile: args.proResProfile ?? undefined,
      numberOfGifLoops: args.numberOfGifLoops ?? undefined,
      overwrite: true,
    });
    const wallMs = performance.now() - t0;
    const framesRendered = args.frameRange ? args.expectedFrames : Math.ceil(comp.durationInFrames / (args.everyNthFrame || 1));
    process.stdout.write(JSON.stringify({
      tier: 3,
      composition_id: comp.id,
      output_path: args.outputPath,
      width: Math.max(1, Math.round(comp.width * args.scale)),
      height: Math.max(1, Math.round(comp.height * args.scale)),
      frames_rendered: framesRendered,
      wall_ms: wallMs,
      fps: wallMs > 0 ? framesRendered / (wallMs / 1000) : 0,
      encoder_used: "remotion-stock",
      exists: fs.existsSync(args.outputPath),
    }));
  }
})().catch((err) => {
  console.error(err && err.stack ? err.stack : String(err));
  process.exit(1);
});
"#;

fn run_node_tier3(payload: Value) -> Result<Value> {
    let payload_str = serde_json::to_string(&payload)?;
    let out = Command::new("node")
        .arg("-e")
        .arg(NODE_TIER3_RUNNER)
        .env("RSMOTION_TIER3_ARGS", payload_str)
        .output()
        .context("invoking `node` for Tier 3 @remotion/renderer fallback")?;

    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        bail!("Tier 3 @remotion/renderer failed: {stderr}");
    }

    let stdout = String::from_utf8_lossy(&out.stdout);
    let parsed: Value = serde_json::from_str(stdout.trim())
        .with_context(|| format!("parsing Tier 3 JSON output: {stdout}"))?;
    Ok(parsed)
}

pub fn list_compositions_tier3(
    serve_url: &str,
    input_props: &Value,
    timeout_ms: u64,
    gl: &str,
    browser_executable: Option<&Path>,
) -> Result<Vec<VideoConfig>> {
    let module_dir = resolve_remotion_renderer_dir(serve_url)?;
    let resolved_serve = resolve_serve_url(serve_url);
    let val = run_node_tier3(json!({
        "mode": "compositions",
        "moduleDir": module_dir,
        "serveUrl": resolved_serve,
        "inputProps": input_props,
        "timeoutMs": timeout_ms,
        "gl": gl,
        "browserExecutable": browser_executable.map(|p| p.to_string_lossy().into_owned()),
    }))?;
    let comps: Vec<VideoConfig> = serde_json::from_value(val)?;
    Ok(comps)
}

#[allow(clippy::too_many_arguments)]
pub fn render_still_tier3(
    serve_url: &str,
    composition_id: &str,
    output_path: &Path,
    frame: u32,
    image_format: ImageFormat,
    jpeg_quality: u8,
    scale: f64,
    gl: &str,
    timeout_ms: u64,
    input_props: &Value,
    browser_executable: Option<&Path>,
) -> Result<Value> {
    let module_dir = resolve_remotion_renderer_dir(serve_url)?;
    let resolved_serve = resolve_serve_url(serve_url);
    let fmt_str = match image_format {
        ImageFormat::Png => "png",
        ImageFormat::Jpeg => "jpeg",
        ImageFormat::Webp => "webp",
    };
    let abs_out = if output_path.is_absolute() {
        output_path.to_path_buf()
    } else {
        std::env::current_dir()?.join(output_path)
    };
    run_node_tier3(json!({
        "mode": "still",
        "moduleDir": module_dir,
        "serveUrl": resolved_serve,
        "compositionId": composition_id,
        "outputPath": abs_out,
        "frame": frame,
        "imageFormat": fmt_str,
        "jpegQuality": jpeg_quality,
        "scale": scale,
        "gl": gl,
        "timeoutMs": timeout_ms,
        "inputProps": input_props,
        "browserExecutable": browser_executable.map(|p| p.to_string_lossy().into_owned()),
    }))
}

#[allow(clippy::too_many_arguments)]
pub fn render_media_tier3(
    serve_url: &str,
    composition_id: &str,
    output_path: &Path,
    codec: Codec,
    crf: Option<u8>,
    pixel_format: PixelFormat,
    gl: &str,
    concurrency: usize,
    frame_range: Option<FrameRange>,
    every_nth_frame: u32,
    scale: f64,
    muted: bool,
    timeout_ms: u64,
    input_props: &Value,
    browser_executable: Option<&Path>,
    x264_preset: Option<&str>,
    video_bitrate: Option<&str>,
    audio_bitrate: Option<&str>,
    audio_codec: Option<AudioCodec>,
    color_space: ColorSpace,
    prores_profile: Option<ProResProfile>,
    number_of_gif_loops: Option<i32>,
) -> Result<Value> {
    let module_dir = resolve_remotion_renderer_dir(serve_url)?;
    let resolved_serve = resolve_serve_url(serve_url);
    let abs_out = if output_path.is_absolute() {
        output_path.to_path_buf()
    } else {
        std::env::current_dir()?.join(output_path)
    };
    let range_json = frame_range.map(|r| json!([r.start, r.end]));
    let expected_frames = frame_range
        .map(|r| ((r.end - r.start) / every_nth_frame.max(1)) + 1)
        .unwrap_or(60);

    run_node_tier3(json!({
        "mode": "render",
        "moduleDir": module_dir,
        "serveUrl": resolved_serve,
        "compositionId": composition_id,
        "outputPath": abs_out,
        "codec": codec.as_str(),
        "crf": crf,
        "pixelFormat": pixel_format.as_str(),
        "gl": gl,
        "concurrency": concurrency.clamp(1, 3),
        "frameRange": range_json,
        "everyNthFrame": every_nth_frame,
        "scale": scale,
        "muted": muted,
        "timeoutMs": timeout_ms,
        "inputProps": input_props,
        "browserExecutable": browser_executable.map(|p| p.to_string_lossy().into_owned()),
        "x264Preset": x264_preset,
        "videoBitrate": video_bitrate,
        "audioBitrate": audio_bitrate,
        "audioCodec": audio_codec.map(|c| c.as_str()),
        "colorSpace": match color_space {
            ColorSpace::Default => None,
            cs => Some(cs.as_str()),
        },
        "proResProfile": prores_profile.map(|p| p.as_str()),
        "numberOfGifLoops": number_of_gif_loops,
        "expectedFrames": expected_frames,
    }))
}
