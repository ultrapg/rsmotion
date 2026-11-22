use std::collections::HashMap;
use std::sync::Arc;

use rsmotion_css::{
    compile_stylesheets, precompute_dom_meta, resolve_node_declarations, CompiledStyles,
    NodeDomMeta,
};
use rsmotion_scene::{Canvas2DOp, NodeKind, SceneTree};
use taffy::prelude::*;

use crate::error::{LayoutError, Result};
use crate::style::{ComputedStyle, TextTransformMode, WhiteSpaceMode};

/// Text and replaced-image measurement callback implemented by `rsmotion-raster`.
pub trait LayoutMeasurer {
    fn measure_text(&self, text: &str, style: &ComputedStyle, max_width: Option<f32>)
        -> (f32, f32);

    fn measure_image(&self, _src: &str) -> Option<(u32, u32)> {
        None
    }

    fn measure_video(&self, _src: &str) -> Option<(u32, u32)> {
        None
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum LayoutContent {
    Container,
    Text(String),
    Image {
        src: String,
    },
    VideoFrame {
        src: String,
        time_secs: f64,
        transparent: bool,
    },
    Svg {
        xml: String,
        intrinsic_w: f32,
        intrinsic_h: f32,
    },
    Canvas2d {
        width: u32,
        height: u32,
        ops: Vec<Canvas2DOp>,
    },
    Lottie {
        animation_id: String,
        frame: f64,
        width: f32,
        height: f32,
        preserve_aspect_ratio: Option<String>,
    },
}

#[derive(Debug, Clone)]
pub struct LayoutNode {
    pub scene_node_id: usize,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    /// `[top, right, bottom, left]` resolved padding in px.
    pub padding: [f32; 4],
    /// `[top, right, bottom, left]` resolved border width in px.
    pub border: [f32; 4],
    pub style: ComputedStyle,
    pub content: LayoutContent,
    pub children: Vec<usize>,
}

#[derive(Debug, Clone)]
pub struct LayoutTree {
    pub width: u32,
    pub height: u32,
    pub root: usize,
    pub nodes: Vec<LayoutNode>,
}

#[derive(Debug, Clone)]
enum MeasurePayload {
    None,
    Text {
        text: String,
        style: Box<ComputedStyle>,
    },
    Image {
        src: String,
        fallback_w: Option<f32>,
        fallback_h: Option<f32>,
    },
    Video {
        src: String,
        fallback_w: Option<f32>,
        fallback_h: Option<f32>,
    },
    Svg {
        intrinsic_w: f32,
        intrinsic_h: f32,
    },
    IntrinsicBox {
        intrinsic_w: f32,
        intrinsic_h: f32,
    },
}

struct BuildNode {
    scene_node_id: usize,
    style: ComputedStyle,
    content: LayoutContent,
    taffy_id: NodeId,
    children: Vec<usize>,
}

fn dim_px(d: taffy::LengthPercentageAuto) -> Option<f32> {
    match d.expand() {
        taffy::style::ExpandedLengthPercentageAuto::Length(v) => Some(v),
        _ => None,
    }
}

fn scale_with_aspect(known: Size<Option<f32>>, iw: f32, ih: f32) -> Size<f32> {
    match (known.width, known.height) {
        (Some(w), None) if iw > 0.0 => Size {
            width: w,
            height: w * (ih / iw),
        },
        (None, Some(h)) if ih > 0.0 => Size {
            width: h * (iw / ih),
            height: h,
        },
        (w, h) => Size {
            width: w.unwrap_or(iw),
            height: h.unwrap_or(ih),
        },
    }
}

pub fn collapse_css_whitespace(raw: &str, mode: WhiteSpaceMode) -> String {
    match mode {
        WhiteSpaceMode::Pre | WhiteSpaceMode::PreWrap => raw.to_string(),
        WhiteSpaceMode::Normal | WhiteSpaceMode::Nowrap => {
            let mut out = String::with_capacity(raw.len());
            let mut prev_ws = false;
            for ch in raw.chars() {
                if ch.is_whitespace() {
                    if !prev_ws {
                        out.push(' ');
                        prev_ws = true;
                    }
                } else {
                    out.push(ch);
                    prev_ws = false;
                }
            }
            out
        }
    }
}

fn apply_text_transform(text: &str, mode: TextTransformMode) -> String {
    match mode {
        TextTransformMode::None => text.to_string(),
        TextTransformMode::Uppercase => text.to_uppercase(),
        TextTransformMode::Lowercase => text.to_lowercase(),
        TextTransformMode::Capitalize => {
            let mut out = String::with_capacity(text.len());
            let mut new_word = true;
            for ch in text.chars() {
                if ch.is_whitespace() {
                    new_word = true;
                    out.push(ch);
                } else if new_word {
                    for uc in ch.to_uppercase() {
                        out.push(uc);
                    }
                    new_word = false;
                } else {
                    out.push(ch);
                }
            }
            out
        }
    }
}

fn find_attr<'a>(attrs: &'a [(String, String)], name: &str) -> Option<&'a str> {
    attrs
        .iter()
        .find(|(k, _)| k == name)
        .map(|(_, v)| v.as_str())
}

fn parse_attr_px(attrs: &[(String, String)], name: &str) -> Option<f32> {
    find_attr(attrs, name).and_then(|v| v.trim().trim_end_matches("px").parse::<f32>().ok())
}

fn normalize_text(raw: &str, style: &ComputedStyle) -> String {
    let collapsed = collapse_css_whitespace(raw, style.white_space);
    let trimmed = match style.white_space {
        WhiteSpaceMode::Pre | WhiteSpaceMode::PreWrap => collapsed,
        _ => collapsed
            .trim_matches(|c| c == '\n' || c == '\r')
            .to_string(),
    };
    apply_text_transform(&trimmed, style.text_transform)
}

enum PendingChild {
    Text(String),
    Node(usize),
}

struct CssEvalCtx<'a> {
    scene: &'a SceneTree,
    compiled: Option<Arc<CompiledStyles>>,
    dom_meta: Option<Vec<NodeDomMeta>>,
}

fn has_dynamic_inline_css(inline_style: &[(String, String)]) -> bool {
    inline_style.iter().any(|(k, v)| {
        k.starts_with("--") || {
            let l = v.to_ascii_lowercase();
            l.contains("var(")
                || l.contains("calc(")
                || l.contains("min(")
                || l.contains("max(")
                || l.contains("clamp(")
        }
    })
}

#[allow(clippy::too_many_arguments)]
fn resolve_element_style(
    css_ctx: &CssEvalCtx<'_>,
    parent_style: Option<&ComputedStyle>,
    parent_vars: &HashMap<String, String>,
    node_id: usize,
    tag: &str,
    classes: &[String],
    inline_style: &[(String, String)],
    vw: f32,
    vh: f32,
) -> (ComputedStyle, HashMap<String, String>) {
    if css_ctx.compiled.is_none() && parent_vars.is_empty() && !has_dynamic_inline_css(inline_style)
    {
        return (
            ComputedStyle::for_element(parent_style, tag, classes, inline_style, vw, vh),
            HashMap::new(),
        );
    }

    let compiled = css_ctx
        .compiled
        .clone()
        .unwrap_or_else(|| compile_stylesheets(&[], vw, vh));
    let meta_slice = css_ctx.dom_meta.as_deref().unwrap_or(&[]);
    let parent_fs = parent_style.map_or(16.0, |p| p.font_size);
    let (resolved_decls, node_vars) = if compiled.index.rules.is_empty() || meta_slice.is_empty() {
        let empty = compile_stylesheets(&[], vw, vh);
        resolve_node_declarations(
            &empty,
            css_ctx.scene,
            meta_slice,
            node_id,
            inline_style,
            parent_vars,
            parent_fs,
            vw,
            vh,
        )
    } else {
        resolve_node_declarations(
            &compiled,
            css_ctx.scene,
            meta_slice,
            node_id,
            inline_style,
            parent_vars,
            parent_fs,
            vw,
            vh,
        )
    };
    let st = ComputedStyle::for_element(parent_style, tag, classes, &resolved_decls, vw, vh);
    (st, node_vars)
}

#[allow(clippy::too_many_arguments)]
fn build_taffy_subtree(
    css_ctx: &CssEvalCtx<'_>,
    scene_id: usize,
    parent_style: Option<&ComputedStyle>,
    parent_vars: &HashMap<String, String>,
    is_root: bool,
    taffy: &mut TaffyTree<MeasurePayload>,
    built: &mut Vec<BuildNode>,
) -> Result<Option<usize>> {
    let scene = css_ctx.scene;
    let vw = scene.width as f32;
    let vh = scene.height as f32;
    let scene_node = scene
        .nodes
        .get(scene_id)
        .ok_or(LayoutError::InvalidRoot(scene_id))?;

    match &scene_node.node {
        NodeKind::Text(raw) => {
            let text_style = parent_style
                .map(ComputedStyle::inherited_text_only)
                .unwrap_or_default();
            let text = normalize_text(raw, &text_style);
            if text.is_empty() {
                return Ok(None);
            }
            let mut st = text_style.taffy.clone();
            st.display = Display::Block;
            let tid = taffy.new_leaf_with_context(
                st,
                MeasurePayload::Text {
                    text: text.clone(),
                    style: Box::new(text_style.clone()),
                },
            )?;
            let idx = built.len();
            built.push(BuildNode {
                scene_node_id: scene_id,
                style: text_style,
                content: LayoutContent::Text(text),
                taffy_id: tid,
                children: Vec::new(),
            });
            Ok(Some(idx))
        }

        NodeKind::Svg {
            xml,
            classes,
            inline_style,
            width_attr,
            height_attr,
            view_box,
            ..
        } => {
            let (style, _) = resolve_element_style(
                css_ctx,
                parent_style,
                parent_vars,
                scene_id,
                "svg",
                classes,
                inline_style,
                vw,
                vh,
            );
            if style.display_none {
                return Ok(None);
            }
            let intrinsic_w = width_attr
                .or_else(|| view_box.map(|vb| vb.2))
                .unwrap_or(300.0);
            let intrinsic_h = height_attr
                .or_else(|| view_box.map(|vb| vb.3))
                .unwrap_or(150.0);

            let mut tstyle = style.taffy.clone();
            tstyle.display = Display::Block;
            tstyle.item_is_replaced = true;
            if tstyle.size.width == Dimension::auto() {
                if let Some(w) = width_attr {
                    tstyle.size.width = Dimension::length(*w);
                }
            }
            if tstyle.size.height == Dimension::auto() {
                if let Some(h) = height_attr {
                    tstyle.size.height = Dimension::length(*h);
                }
            }

            let tid = taffy.new_leaf_with_context(
                tstyle,
                MeasurePayload::Svg {
                    intrinsic_w,
                    intrinsic_h,
                },
            )?;
            let idx = built.len();
            built.push(BuildNode {
                scene_node_id: scene_id,
                style,
                content: LayoutContent::Svg {
                    xml: xml.clone(),
                    intrinsic_w,
                    intrinsic_h,
                },
                taffy_id: tid,
                children: Vec::new(),
            });
            Ok(Some(idx))
        }

        NodeKind::Canvas2d {
            width,
            height,
            classes,
            inline_style,
            ops,
            ..
        } => {
            let (style, _) = resolve_element_style(
                css_ctx,
                parent_style,
                parent_vars,
                scene_id,
                "canvas",
                classes,
                inline_style,
                vw,
                vh,
            );
            if style.display_none {
                return Ok(None);
            }
            let intrinsic_w = (*width as f32).max(0.0);
            let intrinsic_h = (*height as f32).max(0.0);
            let mut tstyle = style.taffy.clone();
            tstyle.display = Display::Block;
            tstyle.item_is_replaced = true;
            let tid = taffy.new_leaf_with_context(
                tstyle,
                MeasurePayload::IntrinsicBox {
                    intrinsic_w,
                    intrinsic_h,
                },
            )?;
            let idx = built.len();
            built.push(BuildNode {
                scene_node_id: scene_id,
                style,
                content: LayoutContent::Canvas2d {
                    width: *width,
                    height: *height,
                    ops: ops.clone(),
                },
                taffy_id: tid,
                children: Vec::new(),
            });
            Ok(Some(idx))
        }

        NodeKind::Lottie {
            animation_id,
            frame,
            width,
            height,
            preserve_aspect_ratio,
            classes,
            inline_style,
            ..
        } => {
            let (style, _) = resolve_element_style(
                css_ctx,
                parent_style,
                parent_vars,
                scene_id,
                "div",
                classes,
                inline_style,
                vw,
                vh,
            );
            if style.display_none {
                return Ok(None);
            }
            let intrinsic_w = width.max(0.0);
            let intrinsic_h = height.max(0.0);
            let mut tstyle = style.taffy.clone();
            tstyle.display = Display::Block;
            tstyle.item_is_replaced = true;
            let tid = taffy.new_leaf_with_context(
                tstyle,
                MeasurePayload::IntrinsicBox {
                    intrinsic_w,
                    intrinsic_h,
                },
            )?;
            let idx = built.len();
            built.push(BuildNode {
                scene_node_id: scene_id,
                style,
                content: LayoutContent::Lottie {
                    animation_id: animation_id.clone(),
                    frame: *frame,
                    width: *width,
                    height: *height,
                    preserve_aspect_ratio: preserve_aspect_ratio.clone(),
                },
                taffy_id: tid,
                children: Vec::new(),
            });
            Ok(Some(idx))
        }

        NodeKind::Element {
            tag,
            classes,
            inline_style,
            attrs,
            children,
        } => {
            let (mut style, node_vars) = resolve_element_style(
                css_ctx,
                parent_style,
                parent_vars,
                scene_id,
                tag,
                classes,
                inline_style,
                vw,
                vh,
            );
            if style.display_none {
                return Ok(None);
            }
            if is_root {
                style.taffy.size = Size {
                    width: Dimension::length(vw),
                    height: Dimension::length(vh),
                };
            }

            if tag == "img" {
                let src = find_attr(attrs, "src").unwrap_or_default().to_string();
                let mut tstyle = style.taffy.clone();
                tstyle.display = Display::Block;
                tstyle.item_is_replaced = true;
                let fallback_w = parse_attr_px(attrs, "width");
                let fallback_h = parse_attr_px(attrs, "height");
                if let Some(req) = rsmotion_media::parse_video_frame_uri(&src) {
                    let tid = taffy.new_leaf_with_context(
                        tstyle,
                        MeasurePayload::Video {
                            src: req.src.clone(),
                            fallback_w,
                            fallback_h,
                        },
                    )?;
                    let idx = built.len();
                    built.push(BuildNode {
                        scene_node_id: scene_id,
                        style,
                        content: LayoutContent::VideoFrame {
                            src: req.src,
                            time_secs: req.time_secs,
                            transparent: req.transparent,
                        },
                        taffy_id: tid,
                        children: Vec::new(),
                    });
                    return Ok(Some(idx));
                }

                let tid = taffy.new_leaf_with_context(
                    tstyle,
                    MeasurePayload::Image {
                        src: src.clone(),
                        fallback_w,
                        fallback_h,
                    },
                )?;
                let idx = built.len();
                built.push(BuildNode {
                    scene_node_id: scene_id,
                    style,
                    content: LayoutContent::Image { src },
                    taffy_id: tid,
                    children: Vec::new(),
                });
                return Ok(Some(idx));
            }

            if tag == "video" {
                let raw_src = find_attr(attrs, "src").unwrap_or_default();
                let (src, time_secs, transparent) =
                    match rsmotion_media::parse_video_frame_uri(raw_src) {
                        Some(req) => (req.src, req.time_secs, req.transparent),
                        None => (raw_src.to_string(), 0.0, false),
                    };
                let mut tstyle = style.taffy.clone();
                tstyle.display = Display::Block;
                tstyle.item_is_replaced = true;
                let fallback_w = parse_attr_px(attrs, "width");
                let fallback_h = parse_attr_px(attrs, "height");
                let tid = taffy.new_leaf_with_context(
                    tstyle,
                    MeasurePayload::Video {
                        src: src.clone(),
                        fallback_w,
                        fallback_h,
                    },
                )?;
                let idx = built.len();
                built.push(BuildNode {
                    scene_node_id: scene_id,
                    style,
                    content: LayoutContent::VideoFrame {
                        src,
                        time_secs,
                        transparent,
                    },
                    taffy_id: tid,
                    children: Vec::new(),
                });
                return Ok(Some(idx));
            }

            // Group consecutive Text children together so React interpolations (`Frame {frame}`)
            // lay out as a single text run inside this element.
            let mut grouped: Vec<PendingChild> = Vec::new();
            let mut text_run = String::new();
            for &cid in children {
                match scene.nodes.get(cid).map(|n| &n.node) {
                    Some(NodeKind::Text(t)) => text_run.push_str(t),
                    Some(_) => {
                        if !text_run.is_empty() {
                            grouped.push(PendingChild::Text(std::mem::take(&mut text_run)));
                        }
                        grouped.push(PendingChild::Node(cid));
                    }
                    None => {}
                }
            }
            if !text_run.is_empty() {
                if grouped.is_empty() {
                    let normalized = normalize_text(&text_run, &style);
                    if !normalized.is_empty() {
                        let tid = taffy.new_leaf_with_context(
                            style.taffy.clone(),
                            MeasurePayload::Text {
                                text: normalized.clone(),
                                style: Box::new(style.clone()),
                            },
                        )?;
                        let idx = built.len();
                        built.push(BuildNode {
                            scene_node_id: scene_id,
                            style,
                            content: LayoutContent::Text(normalized),
                            taffy_id: tid,
                            children: Vec::new(),
                        });
                        return Ok(Some(idx));
                    }
                    let tid =
                        taffy.new_leaf_with_context(style.taffy.clone(), MeasurePayload::None)?;
                    let idx = built.len();
                    built.push(BuildNode {
                        scene_node_id: scene_id,
                        style,
                        content: LayoutContent::Container,
                        taffy_id: tid,
                        children: Vec::new(),
                    });
                    return Ok(Some(idx));
                }
                grouped.push(PendingChild::Text(text_run));
            }

            let mut child_build_ids = Vec::new();
            let mut child_taffy_ids = Vec::new();
            for item in grouped {
                match item {
                    PendingChild::Text(raw_run) => {
                        let text = normalize_text(&raw_run, &style);
                        if text.is_empty() {
                            continue;
                        }
                        let text_style = ComputedStyle::inherited_text_only(&style);
                        let mut st = text_style.taffy.clone();
                        st.display = Display::Block;
                        let tid = taffy.new_leaf_with_context(
                            st,
                            MeasurePayload::Text {
                                text: text.clone(),
                                style: Box::new(text_style.clone()),
                            },
                        )?;
                        let cidx = built.len();
                        built.push(BuildNode {
                            scene_node_id: scene_id,
                            style: text_style,
                            content: LayoutContent::Text(text),
                            taffy_id: tid,
                            children: Vec::new(),
                        });
                        child_build_ids.push(cidx);
                        child_taffy_ids.push(tid);
                    }
                    PendingChild::Node(cid) => {
                        if let Some(cidx) = build_taffy_subtree(
                            css_ctx,
                            cid,
                            Some(&style),
                            &node_vars,
                            false,
                            taffy,
                            built,
                        )? {
                            child_taffy_ids.push(built[cidx].taffy_id);
                            child_build_ids.push(cidx);
                        }
                    }
                }
            }

            let tid = if child_taffy_ids.is_empty() {
                taffy.new_leaf_with_context(style.taffy.clone(), MeasurePayload::None)?
            } else {
                taffy.new_with_children(style.taffy.clone(), &child_taffy_ids)?
            };

            let idx = built.len();
            built.push(BuildNode {
                scene_node_id: scene_id,
                style,
                content: LayoutContent::Container,
                taffy_id: tid,
                children: child_build_ids,
            });
            Ok(Some(idx))
        }
    }
}

pub fn compute_layout(scene: &SceneTree, measurer: &dyn LayoutMeasurer) -> Result<LayoutTree> {
    if scene.root >= scene.nodes.len() {
        return Err(LayoutError::InvalidRoot(scene.root));
    }

    let mut taffy: TaffyTree<MeasurePayload> = TaffyTree::new();
    let mut built = Vec::with_capacity(scene.nodes.len());

    let (compiled, dom_meta, root_vars) = if scene.stylesheets.is_empty() {
        (None, None, HashMap::new())
    } else {
        let comp = compile_stylesheets(&scene.stylesheets, scene.width as f32, scene.height as f32);
        let init_vars = comp.property_initials().clone();
        let meta = precompute_dom_meta(scene);
        (Some(comp), Some(meta), init_vars)
    };
    let css_ctx = CssEvalCtx {
        scene,
        compiled,
        dom_meta,
    };

    let Some(root_build_idx) = build_taffy_subtree(
        &css_ctx, scene.root, None, &root_vars, true, &mut taffy, &mut built,
    )?
    else {
        return Ok(LayoutTree {
            width: scene.width,
            height: scene.height,
            root: 0,
            nodes: vec![LayoutNode {
                scene_node_id: scene.root,
                x: 0.0,
                y: 0.0,
                width: scene.width as f32,
                height: scene.height as f32,
                padding: [0.0; 4],
                border: [0.0; 4],
                style: ComputedStyle::default(),
                content: LayoutContent::Container,
                children: Vec::new(),
            }],
        });
    };

    let root_tid = built[root_build_idx].taffy_id;

    // Assign intrinsic aspect ratios to replaced elements when at least one box dimension
    // is `auto`, and transfer `max_size`/`min_size` constraints across the aspect ratio so
    // `max-height: 250px` scales flex-item width proportionally before flex shrinking.
    for bnode in &built {
        let Ok(mut st) = taffy.style(bnode.taffy_id).cloned() else {
            continue;
        };
        let (w_auto, h_auto) = (st.size.width.is_auto(), st.size.height.is_auto());
        if !w_auto && !h_auto {
            continue;
        }
        let dims: Option<(f32, f32)> = match &bnode.content {
            LayoutContent::Image { src } => measurer
                .measure_image(src)
                .map(|(w, h)| (w as f32, h as f32)),
            LayoutContent::VideoFrame { src, .. } => measurer
                .measure_video(src)
                .map(|(w, h)| (w as f32, h as f32)),
            LayoutContent::Svg {
                intrinsic_w,
                intrinsic_h,
                ..
            } => Some((*intrinsic_w, *intrinsic_h)),
            LayoutContent::Canvas2d { width, height, .. } => Some((*width as f32, *height as f32)),
            LayoutContent::Lottie { width, height, .. } => Some((*width, *height)),
            _ => None,
        };
        let Some((iw, ih)) = dims.filter(|(iw, ih)| *iw > 0.0 && *ih > 0.0) else {
            continue;
        };
        let ratio = st.aspect_ratio.unwrap_or(iw / ih);
        match (w_auto, h_auto) {
            (true, true) => {
                let mut max_w = dim_px(st.max_size.width);
                let mut max_h = dim_px(st.max_size.height);
                if let Some(mh) = max_h {
                    let tw = mh * ratio;
                    max_w = Some(max_w.map_or(tw, |w| w.min(tw)));
                }
                if let Some(mw) = max_w {
                    let th = mw / ratio;
                    max_h = Some(max_h.map_or(th, |h| h.min(th)));
                }
                if let Some(mw) = max_w {
                    st.max_size.width = taffy::LengthPercentageAuto::length(mw);
                }
                if let Some(mh) = max_h {
                    st.max_size.height = taffy::LengthPercentageAuto::length(mh);
                }
                if st.min_size.width.is_auto() {
                    if let Some(min_h) = dim_px(st.min_size.height) {
                        let tw = max_w.map_or(min_h * ratio, |mw| (min_h * ratio).min(mw));
                        st.min_size.width = taffy::LengthPercentageAuto::length(tw);
                    }
                }
                if st.min_size.height.is_auto() {
                    if let Some(min_w) = dim_px(st.min_size.width) {
                        let th = max_h.map_or(min_w / ratio, |mh| (min_w / ratio).min(mh));
                        st.min_size.height = taffy::LengthPercentageAuto::length(th);
                    }
                }
                st.aspect_ratio = Some(ratio);
            }
            (true, false) => {
                if st.aspect_ratio.is_none() && st.max_size.width.is_auto() {
                    st.aspect_ratio = Some(ratio);
                }
            }
            (false, true) => {
                if st.aspect_ratio.is_none() && st.max_size.height.is_auto() {
                    st.aspect_ratio = Some(ratio);
                }
            }
            (false, false) => unreachable!(),
        }
        let _ = taffy.set_style(bnode.taffy_id, st);
    }

    taffy.compute_layout_with_measure(
        root_tid,
        Size {
            width: AvailableSpace::Definite(scene.width as f32),
            height: AvailableSpace::Definite(scene.height as f32),
        },
        |inputs, _tid, ctx, taffy_style| {
            taffy::compute_leaf_layout(
                inputs,
                taffy_style,
                |_, _| 0.0,
                |known, avail| {
                    if let Size {
                        width: Some(w),
                        height: Some(h),
                    } = known
                    {
                        return Size {
                            width: w,
                            height: h,
                        };
                    }
                    match ctx {
                        None | Some(MeasurePayload::None) => Size {
                            width: known.width.unwrap_or(0.0),
                            height: known.height.unwrap_or(0.0),
                        },
                        Some(MeasurePayload::Text { text, style }) => {
                            let max_w = known.width.or(match avail.width {
                                AvailableSpace::Definite(w) => Some(w),
                                AvailableSpace::MinContent => Some(0.0),
                                AvailableSpace::MaxContent => None,
                            });
                            let (mw, mh) = measurer.measure_text(text, style, max_w);
                            Size {
                                width: known.width.unwrap_or(mw),
                                height: known.height.unwrap_or(mh),
                            }
                        }
                        Some(MeasurePayload::Image {
                            src,
                            fallback_w,
                            fallback_h,
                        }) => {
                            let (iw, ih) = measurer
                                .measure_image(src)
                                .map(|(w, h)| (w as f32, h as f32))
                                .unwrap_or((fallback_w.unwrap_or(0.0), fallback_h.unwrap_or(0.0)));
                            scale_with_aspect(known, iw, ih)
                        }
                        Some(MeasurePayload::Video {
                            src,
                            fallback_w,
                            fallback_h,
                        }) => {
                            let (vw_vid, vh_vid) = measurer
                                .measure_video(src)
                                .map(|(w, h)| (w as f32, h as f32))
                                .unwrap_or((
                                    fallback_w.unwrap_or(300.0),
                                    fallback_h.unwrap_or(150.0),
                                ));
                            scale_with_aspect(known, vw_vid, vh_vid)
                        }
                        Some(MeasurePayload::Svg {
                            intrinsic_w,
                            intrinsic_h,
                        }) => scale_with_aspect(known, *intrinsic_w, *intrinsic_h),
                        Some(MeasurePayload::IntrinsicBox {
                            intrinsic_w,
                            intrinsic_h,
                        }) => scale_with_aspect(known, *intrinsic_w, *intrinsic_h),
                    }
                },
            )
        },
    )?;

    let mut nodes: Vec<LayoutNode> = Vec::with_capacity(built.len());
    for b in built {
        let lay = taffy.layout(b.taffy_id)?;
        let mut children = b.children;
        // Stable sort children by CSS z-index so painting preserves DOM order for z-index=0.
        nodes.push(LayoutNode {
            scene_node_id: b.scene_node_id,
            x: lay.location.x,
            y: lay.location.y,
            width: lay.size.width,
            height: lay.size.height,
            padding: [
                lay.padding.top,
                lay.padding.right,
                lay.padding.bottom,
                lay.padding.left,
            ],
            border: [
                lay.border.top,
                lay.border.right,
                lay.border.bottom,
                lay.border.left,
            ],
            style: b.style,
            content: b.content,
            children: std::mem::take(&mut children),
        });
    }

    let z_map: Vec<i32> = nodes.iter().map(|n| n.style.z_index).collect();
    for node in &mut nodes {
        node.children.sort_by_key(|&cid| z_map[cid]);
    }

    Ok(LayoutTree {
        width: scene.width,
        height: scene.height,
        root: root_build_idx,
        nodes,
    })
}
