use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, VecDeque};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::Instant;

use rsmotion_core::RasterBackend;
use rsmotion_css::compile_stylesheets;
use rsmotion_layout::{
    compute_layout, BackgroundImage, ClipRadius, CssBlendMode, CssClipPath, CssColor, CssFilter,
    CssMaskImage, LayoutContent, LayoutNode, LayoutTree, LengthPercent, ObjectFit, TransformOp,
    VisibilityMode, WhiteSpaceMode,
};
use rsmotion_media::{parse_video_frame_uri, resolve_or_fetch_asset, VideoDecoderPool};
use rsmotion_scene::{Canvas2DOp, CanvasPaint, NodeKind, RegisteredFont, SceneTree};
use skia_safe::canvas::{SaveLayerRec, SrcRectConstraint};
use skia_safe::{
    image_filters, images, surfaces, svg, AlphaType, BlendMode, Canvas, ClipOp, Color, ColorType,
    Data, FilterMode, ImageInfo, MipmapMode, Paint, Point, Rect, SamplingOptions, Surface,
};
use tracing::warn;

use crate::canvas2d::draw_canvas2d;
use crate::effects::{
    apply_css_transforms, apply_parent_perspective, build_background_shader, build_clip_path,
    build_css_image_filter, build_rrect, to_sk_blend_mode, to_sk_color,
};
use crate::error::{RasterError, Result};
use crate::font_cache::FontCache;
use crate::gpu_egl::HeadlessGpuSurface;
use crate::image_cache::{ImageCache, SceneMeasurer};
use crate::lottie_cache::LottieCache;

enum RenderTarget {
    Gpu(Box<HeadlessGpuSurface>),
    Cpu(Surface),
}

/// Metrics returned for a single Tier 1 rendered frame (RGBA8888).
pub struct RenderedFrame<'a> {
    pub rgba: &'a [u8],
    pub layout_ms: f64,
    pub video_decode_ms: f64,
    pub draw_ms: f64,
    pub readback_ms: f64,
    pub deduped: bool,
}

/// Metrics returned for a single Tier 1 GPU-converted NV12 frame (`W*H*3/2` bytes).
pub struct RenderedNv12Frame<'a> {
    pub nv12: &'a [u8],
    pub rgba: Option<&'a [u8]>,
    pub layout_ms: f64,
    pub video_decode_ms: f64,
    pub draw_ms: f64,
    pub readback_ms: f64,
    pub deduped: bool,
}

type VideoFrameKey = (String, i64, bool);
type SvgDomKey = (String, u32, u32);
const MAX_SVG_DOM_CACHE: usize = 16;

fn video_frame_key(src: &str, time_secs: f64, transparent: bool) -> VideoFrameKey {
    (
        src.to_string(),
        (time_secs * 1_000_000.0).round() as i64,
        transparent,
    )
}

fn svg_dom_key(xml: &str, iw: f32, ih: f32) -> SvgDomKey {
    (xml.to_string(), iw.to_bits(), ih.to_bits())
}

/// Hardware (EGL/GLES3) or software Skia rasterizer for Tier 1 [`SceneTree`] frames.
pub struct SceneRenderer {
    target: RenderTarget,
    bundle_dir: PathBuf,
    offline: bool,
    width: u32,
    height: u32,
    backend_name: String,
    gpu_renderer: Option<String>,
    font_cache: FontCache,
    image_cache: ImageCache,
    lottie_cache: LottieCache,
    video_pool: VideoDecoderPool,
    frame_video_images: HashMap<VideoFrameKey, skia_safe::Image>,
    svg_dom_cache: HashMap<SvgDomKey, svg::Dom>,
    svg_dom_order: VecDeque<SvgDomKey>,
    last_stylesheet_fonts_hash: Option<u64>,
    last_rgba_hash: Option<u64>,
    last_nv12_hash: Option<u64>,
    last_rgba: Vec<u8>,
    last_nv12: Vec<u8>,
}

impl SceneRenderer {
    pub fn new(
        width: u32,
        height: u32,
        requested_backend: RasterBackend,
        bundle_dir: &Path,
    ) -> Result<Self> {
        let info = ImageInfo::new(
            (width as i32, height as i32),
            ColorType::RGBA8888,
            AlphaType::Premul,
            None,
        );

        let (target, backend_name, gpu_renderer) = match requested_backend {
            RasterBackend::Cpu => {
                let s = surfaces::raster(&info, None, None).ok_or_else(|| {
                    RasterError::SkiaFailed("CPU surfaces::raster returned None".to_string())
                })?;
                (RenderTarget::Cpu(s), "cpu".to_string(), None)
            }
            RasterBackend::Gpu => {
                let gpu = HeadlessGpuSurface::new(width, height)?;
                let renderer_str = gpu.info().renderer.clone();
                (
                    RenderTarget::Gpu(Box::new(gpu)),
                    "gpu".to_string(),
                    Some(renderer_str),
                )
            }
            RasterBackend::Auto => match HeadlessGpuSurface::new(width, height) {
                Ok(gpu) => {
                    let renderer_str = gpu.info().renderer.clone();
                    (
                        RenderTarget::Gpu(Box::new(gpu)),
                        "gpu".to_string(),
                        Some(renderer_str),
                    )
                }
                Err(RasterError::GpuUnavailable(reason)) => {
                    warn!("GPU raster unavailable ({reason}), falling back to CPU Skia");
                    let s = surfaces::raster(&info, None, None).ok_or_else(|| {
                        RasterError::SkiaFailed("CPU surfaces::raster returned None".to_string())
                    })?;
                    (RenderTarget::Cpu(s), "cpu-fallback".to_string(), None)
                }
                Err(e) => return Err(e),
            },
        };

        let buf_len = (width as usize) * (height as usize) * 4;
        Ok(Self {
            target,
            bundle_dir: bundle_dir.to_path_buf(),
            offline: false,
            width,
            height,
            backend_name,
            gpu_renderer,
            font_cache: FontCache::new(),
            image_cache: ImageCache::new(bundle_dir),
            lottie_cache: LottieCache::new(),
            video_pool: VideoDecoderPool::new(bundle_dir),
            frame_video_images: HashMap::new(),
            svg_dom_cache: HashMap::new(),
            svg_dom_order: VecDeque::new(),
            last_stylesheet_fonts_hash: None,
            last_rgba_hash: None,
            last_nv12_hash: None,
            last_rgba: vec![0u8; buf_len],
            last_nv12: Vec::new(),
        })
    }

    pub fn set_offline(&mut self, offline: bool) {
        self.offline = offline;
        self.image_cache.set_offline(offline);
    }

    pub fn backend_name(&self) -> &str {
        &self.backend_name
    }

    pub fn gpu_renderer(&self) -> Option<&str> {
        self.gpu_renderer.as_deref()
    }

    /// Whether GPU-side SkSL NV12 readback is available on this renderer surface.
    pub fn supports_gpu_nv12(&self) -> bool {
        matches!(&self.target, RenderTarget::Gpu(gpu) if gpu.supports_nv12())
    }

    fn compute_scene_layout(
        &mut self,
        scene: &SceneTree,
        scale: f64,
    ) -> Result<(LayoutTree, f64, u64)> {
        if !scene.stylesheets.is_empty() {
            let mut h = DefaultHasher::new();
            scene.stylesheets.hash(&mut h);
            let sheets_hash = h.finish();
            if self.last_stylesheet_fonts_hash != Some(sheets_hash) {
                let compiled = compile_stylesheets(
                    &scene.stylesheets,
                    scene.width as f32,
                    scene.height as f32,
                );
                let mut ff_fonts = Vec::with_capacity(compiled.font_faces().len());
                for ff in compiled.font_faces() {
                    let resolved =
                        resolve_or_fetch_asset(&self.bundle_dir, &ff.src_url, self.offline);
                    ff_fonts.push(RegisteredFont {
                        family: ff.family.clone(),
                        src: ff.src_url.clone(),
                        resolved_path: resolved,
                        weight: ff.weight.clone(),
                        style: ff.style.clone(),
                        unicode_range: ff.unicode_range.clone(),
                    });
                }
                if !ff_fonts.is_empty() {
                    self.font_cache.sync_registered_fonts(
                        &ff_fonts,
                        &self.bundle_dir,
                        self.offline,
                    );
                }
                self.last_stylesheet_fonts_hash = Some(sheets_hash);
            }
        }
        self.font_cache
            .sync_registered_fonts(&scene.fonts, &self.bundle_dir, self.offline);
        if !scene.lottie_assets.is_empty() {
            self.lottie_cache.sync_assets(
                &scene.lottie_assets,
                &self.bundle_dir,
                self.offline,
                self.font_cache.svg_font_mgr(),
            );
        }
        probe_scene_videos(scene, &mut self.video_pool);

        let t_layout = Instant::now();
        let layout = {
            let measurer = SceneMeasurer {
                fonts: &self.font_cache,
                images: &self.image_cache,
                videos: &self.video_pool,
            };
            compute_layout(scene, &measurer)
                .map_err(|e| RasterError::SkiaFailed(format!("layout failed: {e}")))?
        };
        let layout_ms = t_layout.elapsed().as_secs_f64() * 1000.0;
        let frame_hash = hash_layout_tree(&layout, scale);
        Ok((layout, layout_ms, frame_hash))
    }

    fn draw_layout(&mut self, layout: &LayoutTree, scale: f64) -> Result<(f64, f64)> {
        preload_tree_images(layout, &mut self.image_cache);
        preload_tree_svgs(
            layout,
            &self.font_cache,
            &mut self.svg_dom_cache,
            &mut self.svg_dom_order,
        );
        let t_video = Instant::now();
        preload_tree_videos(layout, &mut self.video_pool, &mut self.frame_video_images);
        let video_decode_ms = t_video.elapsed().as_secs_f64() * 1000.0;

        let assets = PaintAssets {
            font_cache: &self.font_cache,
            image_cache: &self.image_cache,
            lottie_cache: &self.lottie_cache,
            video_images: &self.frame_video_images,
            svg_doms: &self.svg_dom_cache,
        };

        let t_draw = Instant::now();
        match &mut self.target {
            RenderTarget::Gpu(gpu) => {
                let canvas = gpu.canvas();
                paint_layout_tree(canvas, layout, scale as f32, &assets);
                gpu.flush_and_submit()?;
            }
            RenderTarget::Cpu(surface) => {
                let canvas = surface.canvas();
                paint_layout_tree(canvas, layout, scale as f32, &assets);
            }
        }
        let draw_ms = t_draw.elapsed().as_secs_f64() * 1000.0;
        Ok((video_decode_ms, draw_ms))
    }

    fn read_current_rgba(&mut self) -> Result<()> {
        match &mut self.target {
            RenderTarget::Gpu(gpu) => {
                gpu.read_pixels_rgba(self.width, self.height, &mut self.last_rgba)?;
            }
            RenderTarget::Cpu(surface) => {
                let info = ImageInfo::new(
                    (self.width as i32, self.height as i32),
                    ColorType::RGBA8888,
                    AlphaType::Premul,
                    None,
                );
                let row_bytes = (self.width as usize) * 4;
                if !surface.read_pixels(&info, &mut self.last_rgba, row_bytes, (0, 0)) {
                    return Err(RasterError::SkiaFailed(
                        "CPU surface read_pixels failed".to_string(),
                    ));
                }
            }
        }
        Ok(())
    }

    /// Lays out and rasterizes `scene` into an RGBA8888 buffer (skipping draw/readback on identical frames).
    pub fn render_scene(&mut self, scene: &SceneTree, scale: f64) -> Result<RenderedFrame<'_>> {
        let (layout, layout_ms, frame_hash) = self.compute_scene_layout(scene, scale)?;
        if self.last_rgba_hash == Some(frame_hash) {
            return Ok(RenderedFrame {
                rgba: &self.last_rgba,
                layout_ms,
                video_decode_ms: 0.0,
                draw_ms: 0.0,
                readback_ms: 0.0,
                deduped: true,
            });
        }

        let (video_decode_ms, draw_ms) = self.draw_layout(&layout, scale)?;
        let t_rb = Instant::now();
        self.read_current_rgba()?;
        let readback_ms = t_rb.elapsed().as_secs_f64() * 1000.0;

        self.last_rgba_hash = Some(frame_hash);
        self.last_nv12_hash = None;
        Ok(RenderedFrame {
            rgba: &self.last_rgba,
            layout_ms,
            video_decode_ms,
            draw_ms,
            readback_ms,
            deduped: false,
        })
    }

    /// Lays out, draws, and converts `scene` on-GPU into NV12 (`need_rgba` also captures keyframe RGBA).
    pub fn render_scene_nv12(
        &mut self,
        scene: &SceneTree,
        scale: f64,
        need_rgba: bool,
    ) -> Result<RenderedNv12Frame<'_>> {
        if !self.supports_gpu_nv12() {
            return Err(RasterError::SkiaFailed(
                "render_scene_nv12 called on surface without GPU NV12 pipeline".to_string(),
            ));
        }

        let nv12_len = ((self.width as usize) * (self.height as usize) * 3) / 2;
        if self.last_nv12.len() != nv12_len {
            self.last_nv12.resize(nv12_len, 0);
        }

        let (layout, layout_ms, frame_hash) = self.compute_scene_layout(scene, scale)?;
        if self.last_nv12_hash == Some(frame_hash) {
            let mut readback_ms = 0.0;
            if need_rgba && self.last_rgba_hash != Some(frame_hash) {
                let t_rb = Instant::now();
                self.read_current_rgba()?;
                readback_ms = t_rb.elapsed().as_secs_f64() * 1000.0;
                self.last_rgba_hash = Some(frame_hash);
            }
            return Ok(RenderedNv12Frame {
                nv12: &self.last_nv12,
                rgba: if need_rgba {
                    Some(&self.last_rgba)
                } else {
                    None
                },
                layout_ms,
                video_decode_ms: 0.0,
                draw_ms: 0.0,
                readback_ms,
                deduped: true,
            });
        }

        let (video_decode_ms, draw_ms) = self.draw_layout(&layout, scale)?;
        let t_rb = Instant::now();
        match &mut self.target {
            RenderTarget::Gpu(gpu) => {
                gpu.read_pixels_nv12(self.width, self.height, &mut self.last_nv12)?;
                if need_rgba {
                    gpu.read_pixels_rgba(self.width, self.height, &mut self.last_rgba)?;
                }
            }
            RenderTarget::Cpu(_) => unreachable!(),
        }
        let readback_ms = t_rb.elapsed().as_secs_f64() * 1000.0;

        self.last_nv12_hash = Some(frame_hash);
        self.last_rgba_hash = if need_rgba { Some(frame_hash) } else { None };
        Ok(RenderedNv12Frame {
            nv12: &self.last_nv12,
            rgba: if need_rgba {
                Some(&self.last_rgba)
            } else {
                None
            },
            layout_ms,
            video_decode_ms,
            draw_ms,
            readback_ms,
            deduped: false,
        })
    }
}

struct PaintAssets<'a> {
    font_cache: &'a FontCache,
    image_cache: &'a ImageCache,
    lottie_cache: &'a LottieCache,
    video_images: &'a HashMap<VideoFrameKey, skia_safe::Image>,
    svg_doms: &'a HashMap<SvgDomKey, svg::Dom>,
}

fn probe_scene_videos(scene: &SceneTree, video_pool: &mut VideoDecoderPool) {
    for node in &scene.nodes {
        if let NodeKind::Element { tag, attrs, .. } = &node.node {
            if tag == "img" || tag == "video" {
                if let Some((_, raw_src)) = attrs.iter().find(|(k, _)| k == "src") {
                    if let Some(req) = parse_video_frame_uri(raw_src) {
                        let _ = video_pool.probe(&req.src);
                    } else if tag == "video" && !raw_src.is_empty() {
                        let _ = video_pool.probe(raw_src);
                    }
                }
            }
        }
    }
}

fn preload_tree_images(tree: &LayoutTree, image_cache: &mut ImageCache) {
    for node in &tree.nodes {
        match &node.content {
            LayoutContent::Image { ref src } => {
                let _ = image_cache.get_or_load(src);
            }
            LayoutContent::Canvas2d { ref ops, .. } => {
                for op in ops {
                    if let Canvas2DOp::DrawImage { ref src, .. } = op {
                        let _ = image_cache.get_or_load(src);
                    }
                }
            }
            _ => {}
        }
        if let CssMaskImage::Url(ref src) = node.style.mask_image {
            let _ = image_cache.get_or_load(src);
        }
    }
}

fn preload_tree_svgs(
    tree: &LayoutTree,
    font_cache: &FontCache,
    svg_cache: &mut HashMap<SvgDomKey, svg::Dom>,
    svg_order: &mut VecDeque<SvgDomKey>,
) {
    for node in &tree.nodes {
        if let LayoutContent::Svg {
            ref xml,
            intrinsic_w,
            intrinsic_h,
        } = node.content
        {
            let iw = intrinsic_w.max(1.0);
            let ih = intrinsic_h.max(1.0);
            let key = svg_dom_key(xml, iw, ih);
            if svg_cache.contains_key(&key) {
                continue;
            }
            if let Ok(mut dom) = svg::Dom::from_str(xml, font_cache.svg_font_mgr_for_xml(xml)) {
                dom.set_container_size((iw, ih));
                if svg_order.len() >= MAX_SVG_DOM_CACHE {
                    if let Some(oldest) = svg_order.pop_front() {
                        svg_cache.remove(&oldest);
                    }
                }
                svg_order.push_back(key.clone());
                svg_cache.insert(key, dom);
            }
        }
    }
}

fn preload_tree_videos(
    tree: &LayoutTree,
    video_pool: &mut VideoDecoderPool,
    frame_video_images: &mut HashMap<VideoFrameKey, skia_safe::Image>,
) {
    frame_video_images.clear();
    for node in &tree.nodes {
        if let LayoutContent::VideoFrame {
            ref src,
            time_secs,
            transparent,
        } = node.content
        {
            let key = video_frame_key(src, time_secs, transparent);
            if frame_video_images.contains_key(&key) {
                continue;
            }
            match video_pool.decode_frame(src, time_secs, transparent) {
                Ok(df) => {
                    let alpha = if transparent {
                        AlphaType::Unpremul
                    } else {
                        AlphaType::Opaque
                    };
                    let info = ImageInfo::new(
                        (df.width as i32, df.height as i32),
                        ColorType::RGBA8888,
                        alpha,
                        None,
                    );
                    let row_bytes = (df.width as usize) * 4;
                    if let Some(img) =
                        images::raster_from_data(&info, Data::new_copy(&df.rgba), row_bytes)
                    {
                        frame_video_images.insert(key, img);
                    } else {
                        warn!("Skia failed to wrap decoded video frame for {src}");
                    }
                }
                Err(err) => {
                    warn!("video frame decode failed for {src} @ {time_secs:.3}s: {err}");
                }
            }
        }
    }
}

fn inset_rrect(
    outer_rect: Rect,
    outer_rrect: &skia_safe::RRect,
    border: [f32; 4],
) -> skia_safe::RRect {
    let [bt, br, bb, bl] = border;
    let inner_rect = Rect::from_xywh(
        outer_rect.left + bl,
        outer_rect.top + bt,
        (outer_rect.width() - bl - br).max(0.0),
        (outer_rect.height() - bt - bb).max(0.0),
    );
    let corners = [
        skia_safe::rrect::Corner::UpperLeft,
        skia_safe::rrect::Corner::UpperRight,
        skia_safe::rrect::Corner::LowerRight,
        skia_safe::rrect::Corner::LowerLeft,
    ];
    let mut vecs = [skia_safe::Vector::default(); 4];
    for (i, c) in corners.iter().enumerate() {
        let rv = outer_rrect.radii(*c);
        let (dx, dy) = match i {
            0 => (bl, bt),
            1 => (br, bt),
            2 => (br, bb),
            _ => (bl, bb),
        };
        vecs[i] = skia_safe::Vector::new((rv.x - dx).max(0.0), (rv.y - dy).max(0.0));
    }
    skia_safe::RRect::new_rect_radii(inner_rect, &vecs)
}

fn paint_layout_tree(canvas: &Canvas, tree: &LayoutTree, scale: f32, assets: &PaintAssets<'_>) {
    canvas.clear(Color::TRANSPARENT);
    canvas.save();
    if (scale - 1.0).abs() > 1e-4 {
        canvas.scale((scale, scale));
    }
    paint_node(canvas, tree, tree.root, assets);
    canvas.restore();
}

fn paint_node(canvas: &Canvas, tree: &LayoutTree, node_idx: usize, assets: &PaintAssets<'_>) {
    paint_node_inner(canvas, tree, node_idx, assets, false);
}

fn paint_content_and_children(
    canvas: &Canvas,
    tree: &LayoutTree,
    node: &LayoutNode,
    inner_rrect: &skia_safe::RRect,
    assets: &PaintAssets<'_>,
    text_mask_mode: bool,
) {
    let w = node.width;
    let h = node.height;
    let st = &node.style;
    let has_radii = st.border_radii.iter().any(|r| {
        matches!(r, LengthPercent::Px(p) if *p > 0.0)
            || matches!(r, LengthPercent::Percent(p) if *p > 0.0)
    });
    let should_clip = st.overflow_hidden
        || (has_radii
            && matches!(
                node.content,
                LayoutContent::Image { .. }
                    | LayoutContent::VideoFrame { .. }
                    | LayoutContent::Svg { .. }
                    | LayoutContent::Canvas2d { .. }
                    | LayoutContent::Lottie { .. }
            ));
    if should_clip {
        canvas.save();
        canvas.clip_rrect(inner_rrect, ClipOp::Intersect, true);
    }

    let pad_l = node.padding[3] + node.border[3];
    let pad_t = node.padding[0] + node.border[0];
    let content_w = (w - pad_l - node.padding[1] - node.border[1]).max(0.0);
    let content_h = (h - pad_t - node.padding[2] - node.border[2]).max(0.0);

    match &node.content {
        LayoutContent::Container => {}
        LayoutContent::Text(text) => {
            if !text.is_empty() && (text_mask_mode || st.color.a > 0) {
                let layout_w = match st.white_space {
                    WhiteSpaceMode::Nowrap | WhiteSpaceMode::Pre => {
                        let (iw, _) = assets.font_cache.measure_text_box(text, st, None);
                        content_w.max(iw + 0.75).max(1.0)
                    }
                    _ => (content_w + 0.75).max(1.0),
                };
                if text_mask_mode {
                    let mut mask_style = st.clone();
                    mask_style.color = CssColor::WHITE;
                    mask_style.text_shadows.clear();
                    assets.font_cache.paint_paragraph(
                        canvas,
                        text,
                        &mask_style,
                        layout_w,
                        Point::new(pad_l, pad_t),
                    );
                } else {
                    assets.font_cache.paint_paragraph(
                        canvas,
                        text,
                        st,
                        layout_w,
                        Point::new(pad_l, pad_t),
                    );
                }
            }
        }
        LayoutContent::Image { src } => {
            if content_w > 0.0 && content_h > 0.0 {
                if let Some(img) = assets.image_cache.get_cached(src) {
                    draw_fitted_image(
                        canvas,
                        img,
                        Rect::from_xywh(pad_l, pad_t, content_w, content_h),
                        st.object_fit,
                        st.object_position,
                    );
                }
            }
        }
        LayoutContent::VideoFrame {
            src,
            time_secs,
            transparent,
        } => {
            if content_w > 0.0 && content_h > 0.0 {
                let key = video_frame_key(src, *time_secs, *transparent);
                if let Some(img) = assets.video_images.get(&key) {
                    draw_fitted_image(
                        canvas,
                        img,
                        Rect::from_xywh(pad_l, pad_t, content_w, content_h),
                        st.object_fit,
                        st.object_position,
                    );
                }
            }
        }
        LayoutContent::Svg {
            xml,
            intrinsic_w,
            intrinsic_h,
        } => {
            if content_w > 0.0 && content_h > 0.0 {
                let iw = (*intrinsic_w).max(1.0);
                let ih = (*intrinsic_h).max(1.0);
                let key = svg_dom_key(xml, iw, ih);
                if let Some(dom) = assets.svg_doms.get(&key) {
                    draw_svg_dom(canvas, dom, pad_l, pad_t, iw, ih, content_w, content_h);
                } else if let Ok(mut dom) =
                    svg::Dom::from_str(xml, assets.font_cache.svg_font_mgr_for_xml(xml))
                {
                    dom.set_container_size((iw, ih));
                    draw_svg_dom(canvas, &dom, pad_l, pad_t, iw, ih, content_w, content_h);
                }
            }
        }
        LayoutContent::Canvas2d { width, height, ops } => {
            if content_w > 0.0 && content_h > 0.0 {
                draw_canvas2d(
                    canvas,
                    Rect::from_xywh(pad_l, pad_t, content_w, content_h),
                    *width,
                    *height,
                    ops,
                    assets.font_cache,
                    assets.image_cache,
                );
            }
        }
        LayoutContent::Lottie {
            animation_id,
            frame,
            preserve_aspect_ratio,
            ..
        } => {
            if content_w > 0.0 && content_h > 0.0 {
                assets.lottie_cache.render(
                    canvas,
                    animation_id,
                    *frame,
                    Rect::from_xywh(pad_l, pad_t, content_w, content_h),
                    preserve_aspect_ratio.as_deref(),
                );
            }
        }
    }

    let has_parent_persp = !text_mask_mode && st.perspective.is_some_and(|d| d > 0.0);
    if has_parent_persp {
        canvas.save();
        apply_parent_perspective(canvas, st.perspective.unwrap(), st.perspective_origin, w, h);
    }
    for &cid in &node.children {
        paint_node_inner(canvas, tree, cid, assets, text_mask_mode);
    }
    if has_parent_persp {
        canvas.restore();
    }

    if should_clip {
        canvas.restore();
    }
}

fn paint_mask_overlay(
    canvas: &Canvas,
    mask: &CssMaskImage,
    rect: Rect,
    w: f32,
    h: f32,
    assets: &PaintAssets<'_>,
) {
    let mut mp = Paint::default();
    mp.set_anti_alias(true);
    mp.set_blend_mode(BlendMode::DstIn);
    match mask {
        CssMaskImage::None => (),
        CssMaskImage::Linear(lg) => {
            let bg = BackgroundImage::Linear(lg.clone());
            if let Some(shader) = build_background_shader(&bg, w, h) {
                mp.set_shader(shader);
                canvas.draw_rect(rect, &mp);
            }
        }
        CssMaskImage::Radial(rg) => {
            let bg = BackgroundImage::Radial(rg.clone());
            if let Some(shader) = build_background_shader(&bg, w, h) {
                mp.set_shader(shader);
                canvas.draw_rect(rect, &mp);
            }
        }
        CssMaskImage::Url(src) => {
            if let Some(img) = assets.image_cache.get_cached(src) {
                let dims = img.dimensions();
                let full_src = Rect::from_wh(dims.width as f32, dims.height as f32);
                let sampling = SamplingOptions::new(FilterMode::Linear, MipmapMode::Linear);
                canvas.draw_image_rect_with_sampling_options(
                    img,
                    Some((&full_src, SrcRectConstraint::Fast)),
                    rect,
                    sampling,
                    &mp,
                );
            }
        }
    }
}

fn paint_node_inner(
    canvas: &Canvas,
    tree: &LayoutTree,
    node_idx: usize,
    assets: &PaintAssets<'_>,
    text_mask_mode: bool,
) {
    let node = &tree.nodes[node_idx];
    let st = &node.style;
    if st.visibility == VisibilityMode::Hidden {
        return;
    }

    let w = node.width;
    let h = node.height;

    canvas.save();
    canvas.translate((node.x, node.y));
    apply_css_transforms(canvas, st, w, h);

    if let Some(cp) = build_clip_path(&st.clip_path, w, h) {
        canvas.clip_path(&cp, ClipOp::Intersect, true);
    }

    let outer_rect = Rect::from_wh(w, h);
    let outer_rrect = build_rrect(outer_rect, &st.border_radii, w, h, 0.0);
    let inner_rrect = inset_rrect(outer_rect, &outer_rrect, node.border);

    // 0. CSS backdrop-filter BEFORE compositing layer so backdrop reads prior framebuffer contents.
    if !text_mask_mode && !st.backdrop_filters.is_empty() && w > 0.0 && h > 0.0 {
        if let Some(bf) = build_css_image_filter(&st.backdrop_filters, true) {
            canvas.save();
            canvas.clip_rrect(outer_rrect, ClipOp::Intersect, true);
            let mut bd_paint = Paint::default();
            if st.opacity < 0.9999 {
                bd_paint.set_alpha_f(st.opacity);
            }
            let rec = SaveLayerRec::default()
                .bounds(&outer_rect)
                .backdrop(&bf)
                .backdrop_tile_mode(skia_safe::TileMode::Clamp)
                .paint(&bd_paint);
            canvas.save_layer(&rec);
            canvas.restore();
            canvas.restore();
        }
    }

    // 1. Element compositing layer (`opacity`, `mix-blend-mode`, `filter`).
    let has_comp_layer = !text_mask_mode
        && (st.opacity < 0.9999
            || st.mix_blend_mode != CssBlendMode::Normal
            || !st.filters.is_empty());
    if has_comp_layer {
        let mut lp = Paint::default();
        lp.set_alpha_f(st.opacity);
        lp.set_blend_mode(to_sk_blend_mode(st.mix_blend_mode));
        if let Some(img_f) = build_css_image_filter(&st.filters, false) {
            lp.set_image_filter(img_f);
        }
        canvas.save_layer(&SaveLayerRec::default().paint(&lp));
    }

    // 2. Optional `mask-image` layer wrapping box + content + borders.
    let has_mask =
        !text_mask_mode && !matches!(st.mask_image, CssMaskImage::None) && w > 0.0 && h > 0.0;
    if has_mask {
        canvas.save_layer(&SaveLayerRec::default().bounds(&outer_rect));
    }

    // 3. Outer CSS box-shadows (painted in reverse declaration order behind the background)
    if !text_mask_mode {
        for sh in st.box_shadows.iter().rev() {
            if sh.inset || sh.color.a == 0 {
                continue;
            }
            canvas.save();
            canvas.clip_rrect(outer_rrect, ClipOp::Difference, true);
            let sw = (w + 2.0 * sh.spread_radius).max(0.0);
            let sh_h = (h + 2.0 * sh.spread_radius).max(0.0);
            let srect = Rect::from_xywh(
                sh.offset_x - sh.spread_radius,
                sh.offset_y - sh.spread_radius,
                sw,
                sh_h,
            );
            let srrect = build_rrect(srect, &st.border_radii, sw, sh_h, sh.spread_radius);
            let mut paint = Paint::default();
            paint.set_anti_alias(true);
            paint.set_color(to_sk_color(sh.color));
            if sh.blur_radius > 0.0 {
                let sigma = sh.blur_radius * 0.5;
                paint.set_image_filter(image_filters::blur(
                    (sigma, sigma),
                    None,
                    None,
                    image_filters::CropRect::NO_CROP_RECT,
                ));
            }
            canvas.draw_rrect(srrect, &paint);
            canvas.restore();
        }
    }

    // 4. Background color & gradient (skipped when `background-clip: text` owns background fill)
    if !text_mask_mode && !st.background_clip_text && w > 0.0 && h > 0.0 {
        if st.background_color.a > 0 {
            let mut paint = Paint::default();
            paint.set_anti_alias(true);
            paint.set_color(to_sk_color(st.background_color));
            canvas.draw_rrect(outer_rrect, &paint);
        }

        if let Some(shader) = build_background_shader(&st.background_image, w, h) {
            let mut paint = Paint::default();
            paint.set_anti_alias(true);
            paint.set_shader(shader);
            canvas.draw_rrect(outer_rrect, &paint);
        }
    }

    // 5. Content & Children (supporting `background-clip: text` glyph masking)
    if st.background_clip_text && !text_mask_mode && w > 0.0 && h > 0.0 {
        let has_bg =
            st.background_color.a > 0 || !matches!(st.background_image, BackgroundImage::None);
        if has_bg {
            canvas.save_layer(&SaveLayerRec::default().bounds(&outer_rect));
            paint_content_and_children(canvas, tree, node, &inner_rrect, assets, true);
            let mut drew_bg = false;
            if st.background_color.a > 0 {
                let mut cp = Paint::default();
                cp.set_anti_alias(true);
                cp.set_blend_mode(BlendMode::SrcIn);
                cp.set_color(to_sk_color(st.background_color));
                canvas.draw_rrect(outer_rrect, &cp);
                drew_bg = true;
            }
            if let Some(shader) = build_background_shader(&st.background_image, w, h) {
                let mut gp = Paint::default();
                gp.set_anti_alias(true);
                gp.set_blend_mode(if drew_bg {
                    BlendMode::SrcATop
                } else {
                    BlendMode::SrcIn
                });
                gp.set_shader(shader);
                canvas.draw_rrect(outer_rrect, &gp);
            }
            canvas.restore();
        }
        if st.color.a > 0 {
            paint_content_and_children(canvas, tree, node, &inner_rrect, assets, false);
        }
    } else {
        paint_content_and_children(canvas, tree, node, &inner_rrect, assets, text_mask_mode);
    }

    // 6. CSS Borders
    if !text_mask_mode {
        let [bt, br, bb, bl] = node.border;
        if bt > 0.0 || br > 0.0 || bb > 0.0 || bl > 0.0 {
            let uniform_color = st.border_top.color == st.border_right.color
                && st.border_top.color == st.border_bottom.color
                && st.border_top.color == st.border_left.color;
            if uniform_color && st.border_top.color.a > 0 {
                let mut bpaint = Paint::default();
                bpaint.set_anti_alias(true);
                bpaint.set_color(to_sk_color(st.border_top.color));
                canvas.draw_drrect(outer_rrect, inner_rrect, &bpaint);
            } else {
                let mut bpaint = Paint::default();
                bpaint.set_anti_alias(true);
                if bt > 0.0 && st.border_top.color.a > 0 {
                    bpaint.set_color(to_sk_color(st.border_top.color));
                    canvas.draw_rect(Rect::from_xywh(0.0, 0.0, w, bt), &bpaint);
                }
                if bb > 0.0 && st.border_bottom.color.a > 0 {
                    bpaint.set_color(to_sk_color(st.border_bottom.color));
                    canvas.draw_rect(Rect::from_xywh(0.0, (h - bb).max(0.0), w, bb), &bpaint);
                }
                if bl > 0.0 && st.border_left.color.a > 0 {
                    bpaint.set_color(to_sk_color(st.border_left.color));
                    canvas.draw_rect(Rect::from_xywh(0.0, 0.0, bl, h), &bpaint);
                }
                if br > 0.0 && st.border_right.color.a > 0 {
                    bpaint.set_color(to_sk_color(st.border_right.color));
                    canvas.draw_rect(Rect::from_xywh((w - br).max(0.0), 0.0, br, h), &bpaint);
                }
            }
        }
    }

    if has_mask {
        paint_mask_overlay(canvas, &st.mask_image, outer_rect, w, h, assets);
        canvas.restore();
    }
    if has_comp_layer {
        canvas.restore();
    }
    canvas.restore();
}

#[allow(clippy::too_many_arguments)]
fn draw_svg_dom(
    canvas: &Canvas,
    dom: &svg::Dom,
    pad_l: f32,
    pad_t: f32,
    iw: f32,
    ih: f32,
    content_w: f32,
    content_h: f32,
) {
    canvas.save();
    canvas.translate((pad_l, pad_t));
    let sx = content_w / iw;
    let sy = content_h / ih;
    if (sx - 1.0).abs() > 1e-4 || (sy - 1.0).abs() > 1e-4 {
        canvas.scale((sx, sy));
    }
    dom.render(canvas);
    canvas.restore();
}

fn draw_fitted_image(
    canvas: &Canvas,
    img: &skia_safe::Image,
    dst: Rect,
    object_fit: ObjectFit,
    object_pos: (LengthPercent, LengthPercent),
) {
    let dims = img.dimensions();
    let iw = dims.width as f32;
    let ih = dims.height as f32;
    let cw = dst.width();
    let ch = dst.height();
    if iw <= 0.0 || ih <= 0.0 || cw <= 0.0 || ch <= 0.0 {
        return;
    }

    let sampling = SamplingOptions::new(FilterMode::Linear, MipmapMode::Linear);
    let mut paint = Paint::default();
    paint.set_anti_alias(true);

    let full_src = Rect::from_wh(iw, ih);
    let pos_x = match object_pos.0 {
        LengthPercent::Percent(p) => p * 0.01,
        LengthPercent::Px(px) => px / cw.max(1.0),
    };
    let pos_y = match object_pos.1 {
        LengthPercent::Percent(p) => p * 0.01,
        LengthPercent::Px(px) => px / ch.max(1.0),
    };

    match object_fit {
        ObjectFit::Fill => {
            canvas.draw_image_rect_with_sampling_options(
                img,
                Some((&full_src, SrcRectConstraint::Fast)),
                dst,
                sampling,
                &paint,
            );
        }
        ObjectFit::Contain => {
            let s = (cw / iw).min(ch / ih);
            let dw = iw * s;
            let dh = ih * s;
            let dx = dst.left + (cw - dw) * pos_x;
            let dy = dst.top + (ch - dh) * pos_y;
            canvas.draw_image_rect_with_sampling_options(
                img,
                Some((&full_src, SrcRectConstraint::Fast)),
                Rect::from_xywh(dx, dy, dw, dh),
                sampling,
                &paint,
            );
        }
        ObjectFit::ScaleDown => {
            let s = (cw / iw).min(ch / ih).min(1.0);
            let dw = iw * s;
            let dh = ih * s;
            let dx = dst.left + (cw - dw) * pos_x;
            let dy = dst.top + (ch - dh) * pos_y;
            canvas.draw_image_rect_with_sampling_options(
                img,
                Some((&full_src, SrcRectConstraint::Fast)),
                Rect::from_xywh(dx, dy, dw, dh),
                sampling,
                &paint,
            );
        }
        ObjectFit::Cover => {
            let s = (cw / iw).max(ch / ih);
            let src_w = (cw / s).min(iw);
            let src_h = (ch / s).min(ih);
            let src_x = (iw - src_w) * pos_x;
            let src_y = (ih - src_h) * pos_y;
            let src_rect = Rect::from_xywh(src_x, src_y, src_w, src_h);
            canvas.draw_image_rect_with_sampling_options(
                img,
                Some((&src_rect, SrcRectConstraint::Strict)),
                dst,
                sampling,
                &paint,
            );
        }
        ObjectFit::None => {
            let dw = iw.min(cw);
            let dh = ih.min(ch);
            let src_x = (iw - dw).max(0.0) * pos_x;
            let src_y = (ih - dh).max(0.0) * pos_y;
            let dst_x = dst.left + (cw - dw).max(0.0) * pos_x;
            let dst_y = dst.top + (ch - dh).max(0.0) * pos_y;
            let src_rect = Rect::from_xywh(src_x, src_y, dw, dh);
            canvas.draw_image_rect_with_sampling_options(
                img,
                Some((&src_rect, SrcRectConstraint::Strict)),
                Rect::from_xywh(dst_x, dst_y, dw, dh),
                sampling,
                &paint,
            );
        }
    }
}

fn hash_f32<H: Hasher>(h: &mut H, v: f32) {
    v.to_bits().hash(h);
}

fn hash_lp<H: Hasher>(h: &mut H, lp: LengthPercent) {
    match lp {
        LengthPercent::Px(v) => {
            0u8.hash(h);
            hash_f32(h, v);
        }
        LengthPercent::Percent(v) => {
            1u8.hash(h);
            hash_f32(h, v);
        }
    }
}

fn hash_bg_image<H: Hasher>(h: &mut H, bg: &BackgroundImage) {
    match bg {
        BackgroundImage::None => 0u8.hash(h),
        BackgroundImage::Linear(lg) => {
            1u8.hash(h);
            hash_f32(h, lg.angle_deg);
            for s in &lg.stops {
                s.color.hash(h);
                if let Some(p) = s.pos {
                    hash_lp(h, p);
                }
            }
        }
        BackgroundImage::Radial(rg) => {
            2u8.hash(h);
            rg.shape_circle.hash(h);
            hash_lp(h, rg.cx);
            hash_lp(h, rg.cy);
            for s in &rg.stops {
                s.color.hash(h);
                if let Some(p) = s.pos {
                    hash_lp(h, p);
                }
            }
        }
    }
}

fn hash_filter_list<H: Hasher>(h: &mut H, list: &[CssFilter]) {
    list.len().hash(h);
    for f in list {
        match *f {
            CssFilter::Blur(r) => {
                0u8.hash(h);
                hash_f32(h, r);
            }
            CssFilter::Brightness(v) => {
                1u8.hash(h);
                hash_f32(h, v);
            }
            CssFilter::Contrast(v) => {
                2u8.hash(h);
                hash_f32(h, v);
            }
            CssFilter::Grayscale(v) => {
                3u8.hash(h);
                hash_f32(h, v);
            }
            CssFilter::Sepia(v) => {
                4u8.hash(h);
                hash_f32(h, v);
            }
            CssFilter::Saturate(v) => {
                5u8.hash(h);
                hash_f32(h, v);
            }
            CssFilter::HueRotateDeg(v) => {
                6u8.hash(h);
                hash_f32(h, v);
            }
            CssFilter::Invert(v) => {
                7u8.hash(h);
                hash_f32(h, v);
            }
            CssFilter::Opacity(v) => {
                8u8.hash(h);
                hash_f32(h, v);
            }
            CssFilter::DropShadow {
                offset_x,
                offset_y,
                blur_radius,
                color,
            } => {
                9u8.hash(h);
                hash_f32(h, offset_x);
                hash_f32(h, offset_y);
                hash_f32(h, blur_radius);
                color.hash(h);
            }
        }
    }
}

fn hash_clip_rad<H: Hasher>(h: &mut H, r: ClipRadius) {
    match r {
        ClipRadius::LengthPercent(lp) => {
            0u8.hash(h);
            hash_lp(h, lp);
        }
        ClipRadius::ClosestSide => 1u8.hash(h),
        ClipRadius::FarthestSide => 2u8.hash(h),
    }
}

fn hash_transform_op<H: Hasher>(h: &mut H, op: &TransformOp) {
    match *op {
        TransformOp::Translate(a, b) => {
            0u8.hash(h);
            hash_lp(h, a);
            hash_lp(h, b);
        }
        TransformOp::TranslateX(a) => {
            1u8.hash(h);
            hash_lp(h, a);
        }
        TransformOp::TranslateY(b) => {
            2u8.hash(h);
            hash_lp(h, b);
        }
        TransformOp::Scale(x, y) => {
            3u8.hash(h);
            hash_f32(h, x);
            hash_f32(h, y);
        }
        TransformOp::RotateDeg(d) => {
            4u8.hash(h);
            hash_f32(h, d);
        }
        TransformOp::SkewDeg(x, y) => {
            5u8.hash(h);
            hash_f32(h, x);
            hash_f32(h, y);
        }
        TransformOp::Matrix(m) => {
            6u8.hash(h);
            for v in m {
                hash_f32(h, v);
            }
        }
        TransformOp::TranslateZ(z) => {
            7u8.hash(h);
            hash_f32(h, z);
        }
        TransformOp::Translate3d(x, y, z) => {
            8u8.hash(h);
            hash_lp(h, x);
            hash_lp(h, y);
            hash_f32(h, z);
        }
        TransformOp::ScaleZ(z) => {
            9u8.hash(h);
            hash_f32(h, z);
        }
        TransformOp::Scale3d(x, y, z) => {
            10u8.hash(h);
            hash_f32(h, x);
            hash_f32(h, y);
            hash_f32(h, z);
        }
        TransformOp::RotateXDeg(d) => {
            11u8.hash(h);
            hash_f32(h, d);
        }
        TransformOp::RotateYDeg(d) => {
            12u8.hash(h);
            hash_f32(h, d);
        }
        TransformOp::RotateZDeg(d) => {
            13u8.hash(h);
            hash_f32(h, d);
        }
        TransformOp::Rotate3d(x, y, z, d) => {
            14u8.hash(h);
            hash_f32(h, x);
            hash_f32(h, y);
            hash_f32(h, z);
            hash_f32(h, d);
        }
        TransformOp::Perspective(d) => {
            15u8.hash(h);
            hash_f32(h, d);
        }
        TransformOp::Matrix3d(m) => {
            16u8.hash(h);
            for v in m {
                hash_f32(h, v);
            }
        }
    }
}

fn hash_canvas_paint<H: Hasher>(h: &mut H, paint: &CanvasPaint) {
    match paint {
        CanvasPaint::Color { value } => {
            0u8.hash(h);
            value.hash(h);
        }
        CanvasPaint::LinearGradient {
            x0,
            y0,
            x1,
            y1,
            stops,
        } => {
            1u8.hash(h);
            for v in [*x0, *y0, *x1, *y1] {
                hash_f32(h, v);
            }
            for (o, c) in stops {
                hash_f32(h, *o);
                c.hash(h);
            }
        }
        CanvasPaint::RadialGradient {
            x0,
            y0,
            r0,
            x1,
            y1,
            r1,
            stops,
        } => {
            2u8.hash(h);
            for v in [*x0, *y0, *r0, *x1, *y1, *r1] {
                hash_f32(h, v);
            }
            for (o, c) in stops {
                hash_f32(h, *o);
                c.hash(h);
            }
        }
    }
}

fn hash_canvas_op<H: Hasher>(h: &mut H, op: &Canvas2DOp) {
    std::mem::discriminant(op).hash(h);
    match op {
        Canvas2DOp::Save
        | Canvas2DOp::Restore
        | Canvas2DOp::ResetTransform
        | Canvas2DOp::BeginPath
        | Canvas2DOp::ClosePath => {}
        Canvas2DOp::Translate { x, y }
        | Canvas2DOp::Scale { x, y }
        | Canvas2DOp::MoveTo { x, y }
        | Canvas2DOp::LineTo { x, y } => {
            hash_f32(h, *x);
            hash_f32(h, *y);
        }
        Canvas2DOp::Rotate { angle }
        | Canvas2DOp::SetLineWidth { width: angle }
        | Canvas2DOp::SetMiterLimit { limit: angle }
        | Canvas2DOp::SetLineDashOffset { offset: angle }
        | Canvas2DOp::SetGlobalAlpha { alpha: angle } => hash_f32(h, *angle),
        Canvas2DOp::Transform { matrix } | Canvas2DOp::SetTransform { matrix } => {
            for v in *matrix {
                hash_f32(h, v);
            }
        }
        Canvas2DOp::SetFillStyle { paint } | Canvas2DOp::SetStrokeStyle { paint } => {
            hash_canvas_paint(h, paint);
        }
        Canvas2DOp::SetLineCap { cap: s }
        | Canvas2DOp::SetLineJoin { join: s }
        | Canvas2DOp::SetGlobalCompositeOperation { mode: s }
        | Canvas2DOp::SetFont { font: s }
        | Canvas2DOp::SetTextAlign { align: s }
        | Canvas2DOp::SetTextBaseline { baseline: s }
        | Canvas2DOp::SetFilter { filter: s } => s.hash(h),
        Canvas2DOp::SetLineDash { segments } => {
            segments.len().hash(h);
            for v in segments {
                hash_f32(h, *v);
            }
        }
        Canvas2DOp::SetShadow {
            blur,
            color,
            offset_x,
            offset_y,
        } => {
            hash_f32(h, *blur);
            color.hash(h);
            hash_f32(h, *offset_x);
            hash_f32(h, *offset_y);
        }
        Canvas2DOp::BezierCurveTo {
            cp1x,
            cp1y,
            cp2x,
            cp2y,
            x,
            y,
        } => {
            for v in [*cp1x, *cp1y, *cp2x, *cp2y, *x, *y] {
                hash_f32(h, v);
            }
        }
        Canvas2DOp::QuadraticCurveTo { cpx, cpy, x, y }
        | Canvas2DOp::ClearRect {
            x: cpx,
            y: cpy,
            w: x,
            h: y,
        }
        | Canvas2DOp::FillRect {
            x: cpx,
            y: cpy,
            w: x,
            h: y,
        }
        | Canvas2DOp::StrokeRect {
            x: cpx,
            y: cpy,
            w: x,
            h: y,
        }
        | Canvas2DOp::Rect {
            x: cpx,
            y: cpy,
            w: x,
            h: y,
        } => {
            for v in [*cpx, *cpy, *x, *y] {
                hash_f32(h, v);
            }
        }
        Canvas2DOp::Arc {
            x,
            y,
            radius,
            start_angle,
            end_angle,
            counterclockwise,
        } => {
            for v in [*x, *y, *radius, *start_angle, *end_angle] {
                hash_f32(h, v);
            }
            counterclockwise.hash(h);
        }
        Canvas2DOp::ArcTo {
            x1,
            y1,
            x2,
            y2,
            radius,
        } => {
            for v in [*x1, *y1, *x2, *y2, *radius] {
                hash_f32(h, v);
            }
        }
        Canvas2DOp::Ellipse {
            x,
            y,
            radius_x,
            radius_y,
            rotation,
            start_angle,
            end_angle,
            counterclockwise,
        } => {
            for v in [
                *x,
                *y,
                *radius_x,
                *radius_y,
                *rotation,
                *start_angle,
                *end_angle,
            ] {
                hash_f32(h, v);
            }
            counterclockwise.hash(h);
        }
        Canvas2DOp::RoundRect {
            x,
            y,
            w,
            h: rh,
            radii,
        } => {
            for v in [*x, *y, *w, *rh] {
                hash_f32(h, v);
            }
            for v in *radii {
                hash_f32(h, v);
            }
        }
        Canvas2DOp::Fill { rule, path_ops } => {
            rule.hash(h);
            if let Some(sub) = path_ops {
                sub.len().hash(h);
                for sop in sub {
                    hash_canvas_op(h, sop);
                }
            }
        }
        Canvas2DOp::Stroke { path_ops } => {
            if let Some(sub) = path_ops {
                sub.len().hash(h);
                for sop in sub {
                    hash_canvas_op(h, sop);
                }
            }
        }
        Canvas2DOp::Clip { rule, path_ops } => {
            rule.hash(h);
            if let Some(sub) = path_ops {
                sub.len().hash(h);
                for sop in sub {
                    hash_canvas_op(h, sop);
                }
            }
        }
        Canvas2DOp::FillText {
            text,
            x,
            y,
            max_width,
        }
        | Canvas2DOp::StrokeText {
            text,
            x,
            y,
            max_width,
        } => {
            text.hash(h);
            hash_f32(h, *x);
            hash_f32(h, *y);
            max_width.map(f32::to_bits).hash(h);
        }
        Canvas2DOp::DrawImage {
            src,
            sx,
            sy,
            sw,
            sh,
            dx,
            dy,
            dw,
            dh,
        } => {
            src.hash(h);
            for v in [sx, sy, sw, sh, dw, dh] {
                v.map(f32::to_bits).hash(h);
            }
            hash_f32(h, *dx);
            hash_f32(h, *dy);
        }
    }
}

fn hash_layout_tree(tree: &LayoutTree, scale: f64) -> u64 {
    let mut h = DefaultHasher::new();
    tree.width.hash(&mut h);
    tree.height.hash(&mut h);
    scale.to_bits().hash(&mut h);
    hash_node(&mut h, tree, tree.root);
    h.finish()
}

fn hash_node<H: Hasher>(h: &mut H, tree: &LayoutTree, idx: usize) {
    let n: &LayoutNode = &tree.nodes[idx];
    hash_f32(h, n.x);
    hash_f32(h, n.y);
    hash_f32(h, n.width);
    hash_f32(h, n.height);
    for p in n.padding {
        hash_f32(h, p);
    }
    for b in n.border {
        hash_f32(h, b);
    }

    let st = &n.style;
    st.visibility.hash(h);
    st.overflow_hidden.hash(h);
    st.z_index.hash(h);
    st.background_color.hash(h);
    hash_bg_image(h, &st.background_image);
    for side in [
        st.border_top,
        st.border_right,
        st.border_bottom,
        st.border_left,
    ] {
        hash_f32(h, side.width);
        side.color.hash(h);
    }
    for r in st.border_radii {
        hash_lp(h, r);
    }
    for sh in &st.box_shadows {
        sh.inset.hash(h);
        hash_f32(h, sh.offset_x);
        hash_f32(h, sh.offset_y);
        hash_f32(h, sh.blur_radius);
        hash_f32(h, sh.spread_radius);
        sh.color.hash(h);
    }
    for sh in &st.text_shadows {
        hash_f32(h, sh.offset_x);
        hash_f32(h, sh.offset_y);
        hash_f32(h, sh.blur_radius);
        sh.color.hash(h);
    }
    hash_f32(h, st.opacity);
    st.transforms.len().hash(h);
    for t in &st.transforms {
        hash_transform_op(h, t);
    }
    if let Some((tx, ty, tz)) = st.standalone_translate {
        1u8.hash(h);
        hash_lp(h, tx);
        hash_lp(h, ty);
        hash_f32(h, tz);
    } else {
        0u8.hash(h);
    }
    if let Some(ref rot) = st.standalone_rotate {
        1u8.hash(h);
        hash_transform_op(h, rot);
    } else {
        0u8.hash(h);
    }
    if let Some((sx, sy, sz)) = st.standalone_scale {
        1u8.hash(h);
        hash_f32(h, sx);
        hash_f32(h, sy);
        hash_f32(h, sz);
    } else {
        0u8.hash(h);
    }
    if let Some(p) = st.perspective {
        1u8.hash(h);
        hash_f32(h, p);
    } else {
        0u8.hash(h);
    }
    hash_lp(h, st.perspective_origin.0);
    hash_lp(h, st.perspective_origin.1);
    hash_filter_list(h, &st.filters);
    hash_filter_list(h, &st.backdrop_filters);
    st.mix_blend_mode.hash(h);
    match &st.clip_path {
        CssClipPath::None => 0u8.hash(h),
        CssClipPath::Inset {
            top,
            right,
            bottom,
            left,
            radii,
        } => {
            1u8.hash(h);
            hash_lp(h, *top);
            hash_lp(h, *right);
            hash_lp(h, *bottom);
            hash_lp(h, *left);
            for r in radii {
                hash_lp(h, *r);
            }
        }
        CssClipPath::Circle { radius, cx, cy } => {
            2u8.hash(h);
            hash_clip_rad(h, *radius);
            hash_lp(h, *cx);
            hash_lp(h, *cy);
        }
        CssClipPath::Ellipse { rx, ry, cx, cy } => {
            3u8.hash(h);
            hash_clip_rad(h, *rx);
            hash_clip_rad(h, *ry);
            hash_lp(h, *cx);
            hash_lp(h, *cy);
        }
        CssClipPath::Polygon { even_odd, points } => {
            4u8.hash(h);
            even_odd.hash(h);
            points.len().hash(h);
            for (x, y) in points {
                hash_lp(h, *x);
                hash_lp(h, *y);
            }
        }
        CssClipPath::Path { even_odd, d } => {
            5u8.hash(h);
            even_odd.hash(h);
            d.hash(h);
        }
    }
    match &st.mask_image {
        CssMaskImage::None => 0u8.hash(h),
        CssMaskImage::Linear(lg) => {
            1u8.hash(h);
            hash_bg_image(h, &BackgroundImage::Linear(lg.clone()));
        }
        CssMaskImage::Radial(rg) => {
            2u8.hash(h);
            hash_bg_image(h, &BackgroundImage::Radial(rg.clone()));
        }
        CssMaskImage::Url(u) => {
            3u8.hash(h);
            u.hash(h);
        }
    }
    st.background_clip_text.hash(h);
    hash_lp(h, st.transform_origin.0);
    hash_lp(h, st.transform_origin.1);
    st.color.hash(h);
    st.font_family.hash(h);
    hash_f32(h, st.font_size);
    st.font_weight.hash(h);
    st.font_style.hash(h);
    match st.line_height {
        rsmotion_layout::LineHeight::Normal => 0u8.hash(h),
        rsmotion_layout::LineHeight::Number(n) => {
            1u8.hash(h);
            hash_f32(h, n);
        }
        rsmotion_layout::LineHeight::Px(p) => {
            2u8.hash(h);
            hash_f32(h, p);
        }
    }
    hash_f32(h, st.letter_spacing);
    st.text_align.hash(h);
    st.white_space.hash(h);
    st.text_transform.hash(h);
    st.text_decoration.hash(h);
    st.object_fit.hash(h);
    hash_lp(h, st.object_position.0);
    hash_lp(h, st.object_position.1);

    match &n.content {
        LayoutContent::Container => 0u8.hash(h),
        LayoutContent::Text(t) => {
            1u8.hash(h);
            t.hash(h);
        }
        LayoutContent::Image { src } => {
            2u8.hash(h);
            src.hash(h);
        }
        LayoutContent::VideoFrame {
            src,
            time_secs,
            transparent,
        } => {
            4u8.hash(h);
            src.hash(h);
            ((*time_secs * 1_000_000.0).round() as i64).hash(h);
            transparent.hash(h);
        }
        LayoutContent::Svg {
            xml,
            intrinsic_w,
            intrinsic_h,
        } => {
            3u8.hash(h);
            xml.hash(h);
            hash_f32(h, *intrinsic_w);
            hash_f32(h, *intrinsic_h);
        }
        LayoutContent::Canvas2d { width, height, ops } => {
            5u8.hash(h);
            width.hash(h);
            height.hash(h);
            ops.len().hash(h);
            for op in ops {
                hash_canvas_op(h, op);
            }
        }
        LayoutContent::Lottie {
            animation_id,
            frame,
            width,
            height,
            preserve_aspect_ratio,
        } => {
            6u8.hash(h);
            animation_id.hash(h);
            frame.to_bits().hash(h);
            hash_f32(h, *width);
            hash_f32(h, *height);
            preserve_aspect_ratio.hash(h);
        }
    }

    n.children.len().hash(h);
    for &cid in &n.children {
        hash_node(h, tree, cid);
    }
}

#[cfg(test)]
mod tests {
    use rsmotion_scene::{NodeKind, SceneNode};

    use super::*;

    #[test]
    fn renders_scene_and_deduplicates_identical_frames() {
        let mut renderer =
            SceneRenderer::new(320, 180, RasterBackend::Auto, Path::new(".")).unwrap();
        let scene = SceneTree {
            width: 320,
            height: 180,
            frame: 0,
            root: 0,
            nodes: vec![
                SceneNode {
                    id: 0,
                    node: NodeKind::Element {
                        tag: "div".into(),
                        classes: vec![],
                        inline_style: vec![
                            ("background-color".into(), "#0f172a".into()),
                            ("display".into(), "flex".into()),
                            ("justify-content".into(), "center".into()),
                            ("align-items".into(), "center".into()),
                        ],
                        attrs: vec![],
                        children: vec![1],
                    },
                },
                SceneNode {
                    id: 1,
                    node: NodeKind::Element {
                        tag: "p".into(),
                        classes: vec![],
                        inline_style: vec![
                            ("margin".into(), "0".into()),
                            ("color".into(), "#f8fafc".into()),
                            ("font-size".into(), "28px".into()),
                        ],
                        attrs: vec![],
                        children: vec![2],
                    },
                },
                SceneNode {
                    id: 2,
                    node: NodeKind::Text("Hello Skia".into()),
                },
            ],
            fonts: vec![],
            stylesheets: vec![],
            audio_assets: vec![],
            lottie_assets: vec![],
            unsupported_features: vec![],
        };

        let first = renderer.render_scene(&scene, 1.0).unwrap();
        assert!(!first.deduped);
        assert_eq!(first.rgba.len(), 320 * 180 * 4);
        // Top-left pixel matches #0f172a (15, 23, 42, 255)
        assert_eq!(&first.rgba[0..4], &[15, 23, 42, 255]);

        let second = renderer.render_scene(&scene, 1.0).unwrap();
        assert!(second.deduped);
    }

    #[test]
    fn renders_gpu_css_effects_3d_and_deduplicates_cleanly() {
        let mut renderer =
            SceneRenderer::new(400, 300, RasterBackend::Auto, Path::new(".")).unwrap();
        let make_scene = |deg: f32| SceneTree {
            width: 400,
            height: 300,
            frame: 0,
            root: 0,
            nodes: vec![
                SceneNode {
                    id: 0,
                    node: NodeKind::Element {
                        tag: "div".into(),
                        classes: vec![],
                        inline_style: vec![
                            ("background-color".into(), "#0f172a".into()),
                            ("perspective".into(), "800px".into()),
                            ("display".into(), "flex".into()),
                            ("justify-content".into(), "center".into()),
                            ("align-items".into(), "center".into()),
                        ],
                        attrs: vec![],
                        children: vec![1, 2, 3],
                    },
                },
                SceneNode {
                    id: 1,
                    node: NodeKind::Element {
                        tag: "div".into(),
                        classes: vec![],
                        inline_style: vec![
                            ("position".into(), "absolute".into()),
                            ("left".into(), "20px".into()),
                            ("top".into(), "20px".into()),
                            ("width".into(), "180px".into()),
                            ("height".into(), "180px".into()),
                            ("background-color".into(), "#ef4444".into()),
                            ("clip-path".into(), "circle(50% at 50% 50%)".into()),
                            ("filter".into(), "blur(2px) brightness(1.1)".into()),
                        ],
                        attrs: vec![],
                        children: vec![],
                    },
                },
                SceneNode {
                    id: 2,
                    node: NodeKind::Element {
                        tag: "div".into(),
                        classes: vec![],
                        inline_style: vec![
                            ("position".into(), "absolute".into()),
                            ("left".into(), "100px".into()),
                            ("top".into(), "60px".into()),
                            ("width".into(), "200px".into()),
                            ("height".into(), "120px".into()),
                            ("border-radius".into(), "16px".into()),
                            ("background-color".into(), "rgba(255, 255, 255, 0.2)".into()),
                            ("backdrop-filter".into(), "blur(10px) saturate(1.4)".into()),
                            ("mix-blend-mode".into(), "screen".into()),
                            (
                                "transform".into(),
                                format!("rotateY({deg}deg) translateZ(20px)"),
                            ),
                        ],
                        attrs: vec![],
                        children: vec![],
                    },
                },
                SceneNode {
                    id: 3,
                    node: NodeKind::Element {
                        tag: "div".into(),
                        classes: vec![],
                        inline_style: vec![
                            ("font-size".into(), "36px".into()),
                            ("font-weight".into(), "700".into()),
                            ("color".into(), "transparent".into()),
                            (
                                "background-image".into(),
                                "linear-gradient(90deg, #38bdf8, #f472b6)".into(),
                            ),
                            ("background-clip".into(), "text".into()),
                            (
                                "mask-image".into(),
                                "linear-gradient(to bottom, #ffffff 0%, #ffffff 100%)".into(),
                            ),
                        ],
                        attrs: vec![],
                        children: vec![4],
                    },
                },
                SceneNode {
                    id: 4,
                    node: NodeKind::Text("CLIP".into()),
                },
            ],
            fonts: vec![],
            stylesheets: vec![],
            audio_assets: vec![],
            lottie_assets: vec![],
            unsupported_features: vec![],
        };

        let s0 = make_scene(15.0);
        let f0 = renderer.render_scene(&s0, 1.0).unwrap().rgba.to_vec();
        let dup = renderer.render_scene(&s0, 1.0).unwrap();
        assert!(dup.deduped);

        let s1 = make_scene(45.0);
        let f1 = renderer.render_scene(&s1, 1.0).unwrap();
        assert!(!f1.deduped);
        assert_ne!(
            f0, f1.rgba,
            "3D rotateY change must update pixels & frame hash"
        );
    }

    #[test]
    fn renders_canvas2d_and_lottie_skottie_and_measures_canvas_text() {
        let fc = FontCache::new();
        let (tw, ta, td) = fc.measure_canvas_text("Remotion", "bold 24px sans-serif");
        assert!(tw > 40.0 && ta > 10.0 && td > 0.0, "w={tw} a={ta} d={td}");

        let lottie_json = r#"{
            "v":"5.5.7","fr":30,"ip":0,"op":60,"w":200,"h":200,
            "layers":[{
                "ddd":0,"ind":1,"ty":4,"nm":"Box","sr":1,
                "ks":{
                    "o":{"a":0,"k":100},
                    "r":{"a":0,"k":0},
                    "p":{"a":1,"k":[{"t":0,"s":[50,100,0],"e":[150,100,0]},{"t":30,"s":[150,100,0]}]},
                    "a":{"a":0,"k":[0,0,0]},
                    "s":{"a":0,"k":[100,100,100]}
                },
                "shapes":[{
                    "ty":"gr","it":[
                        {"ty":"rc","p":{"a":0,"k":[0,0]},"s":{"a":0,"k":[60,60]},"r":{"a":0,"k":8}},
                        {"ty":"fl","c":{"a":0,"k":[0.2,0.8,0.4,1]},"o":{"a":0,"k":100}},
                        {"ty":"tr","p":{"a":0,"k":[0,0]},"a":{"a":0,"k":[0,0]},"s":{"a":0,"k":[100,100]},"r":{"a":0,"k":0},"o":{"a":0,"k":100}}
                    ]
                }],
                "ip":0,"op":60,"st":0
            }]
        }"#;
        let lottie_path =
            rsmotion_media::cache_inline_bytes("test_lottie", "json", lottie_json.as_bytes())
                .unwrap();

        let make_scene = |l_frame: f64, rect_x: f32| SceneTree {
            width: 300,
            height: 200,
            frame: 0,
            root: 0,
            nodes: vec![
                SceneNode {
                    id: 0,
                    node: NodeKind::Element {
                        tag: "div".into(),
                        classes: vec![],
                        inline_style: vec![
                            ("background-color".into(), "#0f172a".into()),
                            ("display".into(), "flex".into()),
                        ],
                        attrs: vec![],
                        children: vec![1, 2],
                    },
                },
                SceneNode {
                    id: 1,
                    node: NodeKind::Canvas2d {
                        width: 150,
                        height: 200,
                        classes: vec![],
                        attrs: vec![],
                        inline_style: vec![
                            ("width".into(), "150px".into()),
                            ("height".into(), "200px".into()),
                        ],
                        ops: vec![
                            Canvas2DOp::SetFillStyle {
                                paint: CanvasPaint::Color {
                                    value: "#f97316".into(),
                                },
                            },
                            Canvas2DOp::FillRect {
                                x: rect_x,
                                y: 40.0,
                                w: 60.0,
                                h: 60.0,
                            },
                        ],
                    },
                },
                SceneNode {
                    id: 2,
                    node: NodeKind::Lottie {
                        animation_id: "test-lot-1".into(),
                        frame: l_frame,
                        width: 150.0,
                        height: 200.0,
                        preserve_aspect_ratio: Some("xMidYMid meet".into()),
                        classes: vec![],
                        attrs: vec![],
                        inline_style: vec![
                            ("width".into(), "150px".into()),
                            ("height".into(), "200px".into()),
                        ],
                    },
                },
            ],
            fonts: vec![],
            stylesheets: vec![],
            audio_assets: vec![],
            lottie_assets: vec![rsmotion_scene::LottieSource {
                id: "test-lot-1".into(),
                resolved_path: lottie_path.clone(),
                assets_path: None,
            }],
            unsupported_features: vec![],
        };

        let mut renderer =
            SceneRenderer::new(300, 200, RasterBackend::Auto, Path::new(".")).unwrap();
        let p0 = renderer
            .render_scene(&make_scene(0.0, 10.0), 1.0)
            .unwrap()
            .rgba
            .to_vec();
        let p1 = renderer.render_scene(&make_scene(25.0, 40.0), 1.0).unwrap();
        assert!(!p1.deduped);
        assert_ne!(
            p0, p1.rgba,
            "Canvas2D + Lottie seek must change rendered output"
        );
    }

    #[test]
    fn renders_right_aligned_white_space_pre_text_within_bounds() {
        let scene = SceneTree {
            width: 400,
            height: 120,
            frame: 0,
            root: 0,
            nodes: vec![
                SceneNode {
                    id: 0,
                    node: NodeKind::Element {
                        tag: "div".into(),
                        classes: vec![],
                        inline_style: vec![("background-color".into(), "#ffffff".into())],
                        attrs: vec![],
                        children: vec![1],
                    },
                },
                SceneNode {
                    id: 1,
                    node: NodeKind::Element {
                        tag: "div".into(),
                        classes: vec![],
                        inline_style: vec![
                            ("position".into(), "absolute".into()),
                            ("right".into(), "20px".into()),
                            ("bottom".into(), "20px".into()),
                            ("font-size".into(), "24px".into()),
                            ("text-align".into(), "right".into()),
                            ("white-space".into(), "pre".into()),
                            ("color".into(), "#000000".into()),
                        ],
                        attrs: vec![],
                        children: vec![2],
                    },
                },
                SceneNode {
                    id: 2,
                    node: NodeKind::Text("RightPre".into()),
                },
            ],
            fonts: vec![],
            stylesheets: vec![],
            audio_assets: vec![],
            lottie_assets: vec![],
            unsupported_features: vec![],
        };

        let mut renderer =
            SceneRenderer::new(400, 120, RasterBackend::Cpu, Path::new(".")).unwrap();
        let out = renderer.render_scene(&scene, 1.0).unwrap();
        let mut dark_right = 0usize;
        for y in 40usize..110 {
            for x in 180usize..390 {
                let idx = (y * 400 + x) * 4;
                if out.rgba[idx] < 128 && out.rgba[idx + 1] < 128 && out.rgba[idx + 2] < 128 {
                    dark_right += 1;
                }
            }
        }
        assert!(
            dark_right > 50,
            "expected visible right-aligned white-space: pre glyphs in x>=180, found {dark_right} pixels"
        );
    }
}
