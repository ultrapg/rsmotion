use std::path::{Path, PathBuf};
use std::time::Instant;

use rsmotion_core::{Codec, HardwareAccel, PixelFormat, RasterBackend};
use rsmotion_encode::{save_rgba_png, EncodeOptions, FfmpegPipeEncoder, RawFrame};
use serde::{Deserialize, Serialize};
use skia_safe::gradient::{
    shaders as gradient_shaders, Colors as GradientColors, Gradient, Interpolation,
};
use skia_safe::textlayout::{FontCollection, ParagraphBuilder, ParagraphStyle, TextStyle};
use skia_safe::{
    image_filters, surfaces, AlphaType, Canvas, Color, Color4f, ColorType, Data, FontMgr,
    FontStyle, Image, ImageInfo, Paint, PathBuilder, Point, RRect, Rect, TileMode,
};

use crate::diff::{compare_rgba, DiffReport};
use crate::error::{RasterError, Result};
use crate::gpu_egl::{GpuInfo, HeadlessGpuSurface};

#[derive(Debug, Clone)]
pub struct SpikeConfig {
    pub backend: RasterBackend,
    pub frames: u32,
    pub width: u32,
    pub height: u32,
    pub encode: bool,
    pub hw_accel: HardwareAccel,
    pub out_dir: PathBuf,
    pub sample_image_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendSpikeMetrics {
    pub backend: String,
    pub frames: u32,
    pub width: u32,
    pub height: u32,
    pub gpu_info: Option<GpuInfo>,
    pub draw_ms_avg: f64,
    pub readback_ms_avg: f64,
    pub encode_push_ms_avg: f64,
    pub encode_wall_ms: Option<f64>,
    pub encoder_used: Option<String>,
    pub total_ms: f64,
    pub effective_fps: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RasterSpikeReport {
    pub cpu: Option<BackendSpikeMetrics>,
    pub gpu: Option<BackendSpikeMetrics>,
    pub cpu_vs_gpu_frame30: Option<DiffReport>,
}

fn load_sample_image(path_opt: Option<&Path>) -> Option<Image> {
    let candidates = [
        path_opt.map(PathBuf::from),
        Some(PathBuf::from("tests/fixtures/public/sample-image.png")),
        Some(PathBuf::from("../tests/fixtures/public/sample-image.png")),
    ];
    for cand in candidates.into_iter().flatten() {
        if let Ok(bytes) = std::fs::read(&cand) {
            let data = Data::new_copy(&bytes);
            if let Some(img) = Image::from_encoded(data) {
                return Some(img);
            }
        }
    }
    None
}

/// Draws a deterministic Remotion-style 1080p scene (gradient, cards + drop shadow, vector path, image, text).
pub fn draw_spike_scene(
    canvas: &Canvas,
    width: i32,
    height: i32,
    frame: u32,
    font_collection: &FontCollection,
    sample_img: Option<&Image>,
) {
    let w = width as f32;
    let h = height as f32;
    let t = (frame as f32) / 60.0;

    // 1. Linear gradient background
    let mut bg_paint = Paint::default();
    let colors = [
        Color4f::from(Color::from_rgb(15, 23, 42)),
        Color4f::from(Color::from_rgb(30, 27, 75)),
        Color4f::from(Color::from_rgb(15, 23, 42)),
    ];
    let shift_x = (t * std::f32::consts::TAU).sin() * (w * 0.15);
    let grad_colors = GradientColors::new_evenly_spaced(&colors, TileMode::Clamp, None);
    let grad = Gradient::new(grad_colors, Interpolation::default());
    bg_paint.set_shader(gradient_shaders::linear_gradient(
        (Point::new(shift_x, 0.0), Point::new(w, h)),
        &grad,
        None,
    ));
    canvas.draw_rect(Rect::from_wh(w, h), &bg_paint);

    // 2. 4 rounded cards with drop shadows
    let card_w = w * 0.19;
    let card_h = h * 0.44;
    let gap = w * 0.03;
    let start_x = (w - (4.0 * card_w + 3.0 * gap)) * 0.5;
    let base_y = h * 0.34;

    let palette = [
        Color::from_rgb(56, 189, 248),
        Color::from_rgb(168, 85, 247),
        Color::from_rgb(16, 185, 129),
        Color::from_rgb(244, 63, 94),
    ];

    for (idx, accent) in palette.iter().enumerate() {
        let phase = t * std::f32::consts::TAU + (idx as f32) * 0.8;
        let dy = phase.sin() * 24.0;
        let x = start_x + (idx as f32) * (card_w + gap);
        let y = base_y + dy;
        let rect = Rect::from_xywh(x, y, card_w, card_h);
        let rrect = RRect::new_rect_xy(rect, 28.0, 28.0);

        let mut card_paint = Paint::default();
        card_paint.set_anti_alias(true);
        card_paint.set_color(Color::from_argb(240, 30, 41, 59));
        card_paint.set_image_filter(image_filters::drop_shadow(
            (0.0, 18.0),
            (18.0, 18.0),
            Color::from_argb(150, 0, 0, 0),
            None,
            None,
            None,
        ));
        canvas.draw_rrect(rrect, &card_paint);

        let mut stripe_paint = Paint::default();
        stripe_paint.set_anti_alias(true);
        stripe_paint.set_color(*accent);
        let bar = RRect::new_rect_xy(
            Rect::from_xywh(x + 24.0, y + 24.0, card_w - 48.0, 14.0),
            7.0,
            7.0,
        );
        canvas.draw_rrect(bar, &stripe_paint);
    }

    // 3. Embedded sample image on first card
    if let Some(img) = sample_img {
        let phase0 = (t * std::f32::consts::TAU).sin() * 24.0;
        let dst = Rect::from_xywh(start_x + 24.0, base_y + phase0 + 60.0, card_w - 48.0, 210.0);
        let clip_rrect = RRect::new_rect_xy(dst, 16.0, 16.0);
        canvas.save();
        canvas.clip_rrect(clip_rrect, skia_safe::ClipOp::Intersect, true);
        let img_paint = Paint::default();
        canvas.draw_image_rect(img, None, dst, &img_paint);
        canvas.restore();
    }

    // 4. Animated rotating vector star/polygon path
    canvas.save();
    let cx = w * 0.82;
    let cy = h * 0.16;
    canvas.translate((cx, cy));
    canvas.rotate(t * 360.0, None);
    let mut path_builder = PathBuilder::new();
    let points = 6;
    for i in 0..points {
        let angle = (i as f32) * std::f32::consts::TAU / (points as f32);
        let r = if i % 2 == 0 { 68.0 } else { 32.0 };
        let px = angle.cos() * r;
        let py = angle.sin() * r;
        if i == 0 {
            path_builder.move_to((px, py));
        } else {
            path_builder.line_to((px, py));
        }
    }
    path_builder.close();
    let path = path_builder.detach();
    let mut vec_paint = Paint::default();
    vec_paint.set_anti_alias(true);
    vec_paint.set_color(Color::from_rgb(250, 204, 21));
    canvas.draw_path(&path, &vec_paint);
    canvas.restore();

    // 5. Multi-line Skia Paragraph text
    let mut para_style = ParagraphStyle::new();
    para_style.set_max_lines(3);
    let mut builder = ParagraphBuilder::new(&para_style, font_collection.clone());

    let mut title_style = TextStyle::new();
    title_style.set_color(Color::from_rgb(248, 250, 252));
    title_style.set_font_size(64.0);
    title_style.set_font_style(FontStyle::bold());
    title_style.set_font_families(&["DejaVu Sans", "Space Grotesk", "sans-serif"]);
    builder.push_style(&title_style);
    builder.add_text("rsmotion Skia Spike\n");

    let mut sub_style = TextStyle::new();
    sub_style.set_color(Color::from_rgb(148, 163, 184));
    sub_style.set_font_size(34.0);
    sub_style.set_font_families(&["DejaVu Sans", "sans-serif"]);
    builder.push_style(&sub_style);
    builder.add_text(format!(
        "Frame {frame:04} • 1080p Skia Paragraph + Gradients + Shadow"
    ));

    let mut paragraph = builder.build();
    paragraph.layout(w * 0.68);
    paragraph.paint(canvas, Point::new(start_x, h * 0.09));
}

fn make_font_collection() -> FontCollection {
    let mut fc = FontCollection::new();
    fc.set_default_font_manager(FontMgr::new(), None);
    fc
}

fn run_single_backend(
    backend_label: &str,
    use_gpu: bool,
    cfg: &SpikeConfig,
    font_collection: &FontCollection,
    sample_img: Option<&Image>,
) -> Result<(BackendSpikeMetrics, Option<Vec<u8>>)> {
    let width = cfg.width;
    let height = cfg.height;
    let buf_len = (width as usize) * (height as usize) * 4;
    let image_info = ImageInfo::new(
        (width as i32, height as i32),
        ColorType::RGBA8888,
        AlphaType::Premul,
        None,
    );

    let mut gpu_surface = if use_gpu {
        Some(HeadlessGpuSurface::new(width, height)?)
    } else {
        None
    };

    let mut cpu_surface =
        if !use_gpu {
            Some(surfaces::raster(&image_info, None, None).ok_or_else(|| {
                RasterError::SkiaFailed("surfaces::raster returned None".to_string())
            })?)
        } else {
            None
        };

    let gpu_info = gpu_surface.as_ref().map(|g| g.info().clone());

    let mut encoder = if cfg.encode {
        let mp4_path = cfg.out_dir.join(format!("spike-{backend_label}.mp4"));
        Some(FfmpegPipeEncoder::spawn(EncodeOptions {
            width,
            height,
            fps: 30.0,
            output_path: mp4_path,
            codec: Codec::H264,
            crf: Some(20),
            pixel_format: PixelFormat::Yuv420p,
            hw_accel: cfg.hw_accel,
            audio_input: None,
            muted: true,
            x264_preset: Some("medium".to_string()),
            ring_capacity: 8,
            ..Default::default()
        })?)
    } else {
        None
    };

    let sample_frame_idx = 30.min(cfg.frames.saturating_sub(1));
    let mut frame30_rgba: Option<Vec<u8>> = None;
    let mut scratch_rgba = vec![0u8; buf_len];

    let mut draw_ns_total = 0u128;
    let mut readback_ns_total = 0u128;
    let mut encode_push_ns_total = 0u128;

    let total_start = Instant::now();

    for f in 0..cfg.frames {
        let t_draw = Instant::now();
        if let Some(ref mut gs) = gpu_surface {
            draw_spike_scene(
                gs.canvas(),
                width as i32,
                height as i32,
                f,
                font_collection,
                sample_img,
            );
            gs.flush_and_submit()?;
        } else if let Some(ref mut cs) = cpu_surface {
            draw_spike_scene(
                cs.canvas(),
                width as i32,
                height as i32,
                f,
                font_collection,
                sample_img,
            );
        }
        draw_ns_total += t_draw.elapsed().as_nanos();

        let t_read = Instant::now();
        if let Some(ref mut gs) = gpu_surface {
            gs.read_pixels_rgba(width, height, &mut scratch_rgba)?;
        } else if let Some(ref mut cs) = cpu_surface {
            let row_bytes = (width as usize) * 4;
            if !cs.read_pixels(&image_info, &mut scratch_rgba, row_bytes, (0, 0)) {
                return Err(RasterError::SkiaFailed(
                    "CPU surface read_pixels failed".to_string(),
                ));
            }
        }
        readback_ns_total += t_read.elapsed().as_nanos();

        if f == sample_frame_idx {
            frame30_rgba = Some(scratch_rgba.clone());
            let png_path = cfg.out_dir.join(format!(
                "spike-{backend_label}-frame_{sample_frame_idx:04}.png"
            ));
            save_rgba_png(&png_path, width, height, &scratch_rgba)?;
        }

        if let Some(ref enc) = encoder {
            let t_enc = Instant::now();
            enc.push_frame(RawFrame {
                index: f,
                width,
                height,
                data: scratch_rgba.clone(),
            })?;
            encode_push_ns_total += t_enc.elapsed().as_nanos();
        }
    }

    let (encode_wall_ms, encoder_used) = if let Some(enc) = encoder.take() {
        let stats = enc.finish()?;
        (Some(stats.wall_ms), Some(stats.encoder_used))
    } else {
        (None, None)
    };

    let total_ms = total_start.elapsed().as_secs_f64() * 1000.0;
    let n = f64::from(cfg.frames.max(1));

    let metrics = BackendSpikeMetrics {
        backend: backend_label.to_string(),
        frames: cfg.frames,
        width,
        height,
        gpu_info,
        draw_ms_avg: (draw_ns_total as f64) / 1_000_000.0 / n,
        readback_ms_avg: (readback_ns_total as f64) / 1_000_000.0 / n,
        encode_push_ms_avg: (encode_push_ns_total as f64) / 1_000_000.0 / n,
        encode_wall_ms,
        encoder_used,
        total_ms,
        effective_fps: f64::from(cfg.frames) / (total_ms / 1000.0),
    };

    Ok((metrics, frame30_rgba))
}

/// Runs the Skia CPU and/or headless EGL GPU spike benchmark.
pub fn run_raster_spike(cfg: &SpikeConfig) -> Result<RasterSpikeReport> {
    std::fs::create_dir_all(&cfg.out_dir)?;
    let font_collection = make_font_collection();
    let sample_img = load_sample_image(cfg.sample_image_path.as_deref());

    let mut cpu_metrics = None;
    let mut cpu_frame30 = None;
    let mut gpu_metrics = None;
    let mut gpu_frame30 = None;

    match cfg.backend {
        RasterBackend::Cpu => {
            let (m, f30) =
                run_single_backend("cpu", false, cfg, &font_collection, sample_img.as_ref())?;
            cpu_metrics = Some(m);
            cpu_frame30 = f30;
        }
        RasterBackend::Gpu => {
            let (m, f30) =
                run_single_backend("gpu", true, cfg, &font_collection, sample_img.as_ref())?;
            gpu_metrics = Some(m);
            gpu_frame30 = f30;
        }
        RasterBackend::Auto => {
            let (mc, fc) =
                run_single_backend("cpu", false, cfg, &font_collection, sample_img.as_ref())?;
            cpu_metrics = Some(mc);
            cpu_frame30 = fc;

            let (mg, fg) =
                run_single_backend("gpu", true, cfg, &font_collection, sample_img.as_ref())?;
            gpu_metrics = Some(mg);
            gpu_frame30 = fg;
        }
    }

    let cpu_vs_gpu_frame30 = match (cpu_frame30.as_ref(), gpu_frame30.as_ref()) {
        (Some(c), Some(g)) => Some(compare_rgba(c, g, cfg.width, cfg.height)?),
        _ => None,
    };

    Ok(RasterSpikeReport {
        cpu: cpu_metrics,
        gpu: gpu_metrics,
        cpu_vs_gpu_frame30,
    })
}
