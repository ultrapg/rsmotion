use std::path::PathBuf;

use rsmotion_core::VideoConfig;
use rsmotion_encode::AudioTrackFrame;
use serde::de::{self, DeserializeOwned};
use serde::{Deserialize, Deserializer, Serialize};

/// Font registered dynamically via `new FontFace(...)` + `document.fonts.add(...)`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegisteredFont {
    pub family: String,
    pub src: String,
    pub resolved_path: Option<PathBuf>,
    pub weight: Option<String>,
    pub style: Option<String>,
    #[serde(default)]
    pub unicode_range: Option<String>,
}

/// Lottie JSON asset registered from `@remotion/lottie` (`lottie-web`) for native `skottie` playback.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LottieSource {
    pub id: String,
    pub resolved_path: PathBuf,
    #[serde(default)]
    pub assets_path: Option<String>,
}

/// Paint source (`fillStyle` / `strokeStyle`) captured from HTML5 Canvas 2D.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum CanvasPaint {
    Color {
        value: String,
    },
    LinearGradient {
        x0: f32,
        y0: f32,
        x1: f32,
        y1: f32,
        stops: Vec<(f32, String)>,
    },
    RadialGradient {
        x0: f32,
        y0: f32,
        r0: f32,
        x1: f32,
        y1: f32,
        r1: f32,
        stops: Vec<(f32, String)>,
    },
}

fn req_field<T: DeserializeOwned, E: de::Error>(
    map: &mut serde_json::Map<String, serde_json::Value>,
    key: &'static str,
) -> Result<T, E> {
    let val = map
        .remove(key)
        .ok_or_else(|| de::Error::missing_field(key))?;
    serde_json::from_value(val).map_err(de::Error::custom)
}

fn def_field<T: DeserializeOwned + Default, E: de::Error>(
    map: &mut serde_json::Map<String, serde_json::Value>,
    key: &'static str,
) -> Result<T, E> {
    match map.remove(key) {
        None | Some(serde_json::Value::Null) => Ok(T::default()),
        Some(val) => serde_json::from_value(val).map_err(de::Error::custom),
    }
}

fn opt_field<T: DeserializeOwned, E: de::Error>(
    map: &mut serde_json::Map<String, serde_json::Value>,
    key: &'static str,
) -> Result<Option<T>, E> {
    match map.remove(key) {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(val) => serde_json::from_value(val)
            .map(Some)
            .map_err(de::Error::custom),
    }
}

// Manual Deserialize avoids serde's `Content` buffering (`deserialize_any`), which breaks
// typed numbers when workspace feature unification enables `serde_json/arbitrary_precision`.
impl<'de> Deserialize<'de> for CanvasPaint {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let mut map = serde_json::Map::<String, serde_json::Value>::deserialize(deserializer)?;
        let kind: String = req_field(&mut map, "kind")?;
        match kind.as_str() {
            "color" => Ok(CanvasPaint::Color {
                value: req_field(&mut map, "value")?,
            }),
            "linearGradient" => Ok(CanvasPaint::LinearGradient {
                x0: req_field(&mut map, "x0")?,
                y0: req_field(&mut map, "y0")?,
                x1: req_field(&mut map, "x1")?,
                y1: req_field(&mut map, "y1")?,
                stops: req_field(&mut map, "stops")?,
            }),
            "radialGradient" => Ok(CanvasPaint::RadialGradient {
                x0: req_field(&mut map, "x0")?,
                y0: req_field(&mut map, "y0")?,
                r0: req_field(&mut map, "r0")?,
                x1: req_field(&mut map, "x1")?,
                y1: req_field(&mut map, "y1")?,
                r1: req_field(&mut map, "r1")?,
                stops: req_field(&mut map, "stops")?,
            }),
            other => Err(de::Error::unknown_variant(
                other,
                &["color", "linearGradient", "radialGradient"],
            )),
        }
    }
}

/// Recorded HTML5 `CanvasRenderingContext2D` vector drawing/state command executed natively on GPU Skia.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "op", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Canvas2DOp {
    Save,
    Restore,
    Translate {
        x: f32,
        y: f32,
    },
    Rotate {
        angle: f32,
    },
    Scale {
        x: f32,
        y: f32,
    },
    Transform {
        matrix: [f32; 6],
    },
    SetTransform {
        matrix: [f32; 6],
    },
    ResetTransform,
    SetFillStyle {
        paint: CanvasPaint,
    },
    SetStrokeStyle {
        paint: CanvasPaint,
    },
    SetLineWidth {
        width: f32,
    },
    SetLineCap {
        cap: String,
    },
    SetLineJoin {
        join: String,
    },
    SetMiterLimit {
        limit: f32,
    },
    SetLineDash {
        segments: Vec<f32>,
    },
    SetLineDashOffset {
        offset: f32,
    },
    SetGlobalAlpha {
        alpha: f32,
    },
    SetGlobalCompositeOperation {
        mode: String,
    },
    SetShadow {
        blur: f32,
        color: String,
        offset_x: f32,
        offset_y: f32,
    },
    SetFont {
        font: String,
    },
    SetTextAlign {
        align: String,
    },
    SetTextBaseline {
        baseline: String,
    },
    SetFilter {
        filter: String,
    },
    BeginPath,
    ClosePath,
    MoveTo {
        x: f32,
        y: f32,
    },
    LineTo {
        x: f32,
        y: f32,
    },
    BezierCurveTo {
        cp1x: f32,
        cp1y: f32,
        cp2x: f32,
        cp2y: f32,
        x: f32,
        y: f32,
    },
    QuadraticCurveTo {
        cpx: f32,
        cpy: f32,
        x: f32,
        y: f32,
    },
    Arc {
        x: f32,
        y: f32,
        radius: f32,
        start_angle: f32,
        end_angle: f32,
        counterclockwise: bool,
    },
    ArcTo {
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        radius: f32,
    },
    Ellipse {
        x: f32,
        y: f32,
        radius_x: f32,
        radius_y: f32,
        rotation: f32,
        start_angle: f32,
        end_angle: f32,
        counterclockwise: bool,
    },
    Rect {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
    },
    RoundRect {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        radii: [f32; 4],
    },
    Fill {
        rule: String,
        path_ops: Option<Vec<Canvas2DOp>>,
    },
    Stroke {
        path_ops: Option<Vec<Canvas2DOp>>,
    },
    Clip {
        rule: String,
        path_ops: Option<Vec<Canvas2DOp>>,
    },
    ClearRect {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
    },
    FillRect {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
    },
    StrokeRect {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
    },
    FillText {
        text: String,
        x: f32,
        y: f32,
        max_width: Option<f32>,
    },
    StrokeText {
        text: String,
        x: f32,
        y: f32,
        max_width: Option<f32>,
    },
    DrawImage {
        src: String,
        sx: Option<f32>,
        sy: Option<f32>,
        sw: Option<f32>,
        sh: Option<f32>,
        dx: f32,
        dy: f32,
        dw: Option<f32>,
        dh: Option<f32>,
    },
}

impl<'de> Deserialize<'de> for Canvas2DOp {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let mut map = serde_json::Map::<String, serde_json::Value>::deserialize(deserializer)?;
        let op: String = req_field(&mut map, "op")?;
        match op.as_str() {
            "save" => Ok(Canvas2DOp::Save),
            "restore" => Ok(Canvas2DOp::Restore),
            "translate" => Ok(Canvas2DOp::Translate {
                x: req_field(&mut map, "x")?,
                y: req_field(&mut map, "y")?,
            }),
            "rotate" => Ok(Canvas2DOp::Rotate {
                angle: req_field(&mut map, "angle")?,
            }),
            "scale" => Ok(Canvas2DOp::Scale {
                x: req_field(&mut map, "x")?,
                y: req_field(&mut map, "y")?,
            }),
            "transform" => Ok(Canvas2DOp::Transform {
                matrix: req_field(&mut map, "matrix")?,
            }),
            "setTransform" => Ok(Canvas2DOp::SetTransform {
                matrix: req_field(&mut map, "matrix")?,
            }),
            "resetTransform" => Ok(Canvas2DOp::ResetTransform),
            "setFillStyle" => Ok(Canvas2DOp::SetFillStyle {
                paint: req_field(&mut map, "paint")?,
            }),
            "setStrokeStyle" => Ok(Canvas2DOp::SetStrokeStyle {
                paint: req_field(&mut map, "paint")?,
            }),
            "setLineWidth" => Ok(Canvas2DOp::SetLineWidth {
                width: req_field(&mut map, "width")?,
            }),
            "setLineCap" => Ok(Canvas2DOp::SetLineCap {
                cap: req_field(&mut map, "cap")?,
            }),
            "setLineJoin" => Ok(Canvas2DOp::SetLineJoin {
                join: req_field(&mut map, "join")?,
            }),
            "setMiterLimit" => Ok(Canvas2DOp::SetMiterLimit {
                limit: req_field(&mut map, "limit")?,
            }),
            "setLineDash" => Ok(Canvas2DOp::SetLineDash {
                segments: req_field(&mut map, "segments")?,
            }),
            "setLineDashOffset" => Ok(Canvas2DOp::SetLineDashOffset {
                offset: req_field(&mut map, "offset")?,
            }),
            "setGlobalAlpha" => Ok(Canvas2DOp::SetGlobalAlpha {
                alpha: req_field(&mut map, "alpha")?,
            }),
            "setGlobalCompositeOperation" => Ok(Canvas2DOp::SetGlobalCompositeOperation {
                mode: req_field(&mut map, "mode")?,
            }),
            "setShadow" => Ok(Canvas2DOp::SetShadow {
                blur: req_field(&mut map, "blur")?,
                color: req_field(&mut map, "color")?,
                offset_x: req_field(&mut map, "offsetX")?,
                offset_y: req_field(&mut map, "offsetY")?,
            }),
            "setFont" => Ok(Canvas2DOp::SetFont {
                font: req_field(&mut map, "font")?,
            }),
            "setTextAlign" => Ok(Canvas2DOp::SetTextAlign {
                align: req_field(&mut map, "align")?,
            }),
            "setTextBaseline" => Ok(Canvas2DOp::SetTextBaseline {
                baseline: req_field(&mut map, "baseline")?,
            }),
            "setFilter" => Ok(Canvas2DOp::SetFilter {
                filter: req_field(&mut map, "filter")?,
            }),
            "beginPath" => Ok(Canvas2DOp::BeginPath),
            "closePath" => Ok(Canvas2DOp::ClosePath),
            "moveTo" => Ok(Canvas2DOp::MoveTo {
                x: req_field(&mut map, "x")?,
                y: req_field(&mut map, "y")?,
            }),
            "lineTo" => Ok(Canvas2DOp::LineTo {
                x: req_field(&mut map, "x")?,
                y: req_field(&mut map, "y")?,
            }),
            "bezierCurveTo" => Ok(Canvas2DOp::BezierCurveTo {
                cp1x: req_field(&mut map, "cp1x")?,
                cp1y: req_field(&mut map, "cp1y")?,
                cp2x: req_field(&mut map, "cp2x")?,
                cp2y: req_field(&mut map, "cp2y")?,
                x: req_field(&mut map, "x")?,
                y: req_field(&mut map, "y")?,
            }),
            "quadraticCurveTo" => Ok(Canvas2DOp::QuadraticCurveTo {
                cpx: req_field(&mut map, "cpx")?,
                cpy: req_field(&mut map, "cpy")?,
                x: req_field(&mut map, "x")?,
                y: req_field(&mut map, "y")?,
            }),
            "arc" => Ok(Canvas2DOp::Arc {
                x: req_field(&mut map, "x")?,
                y: req_field(&mut map, "y")?,
                radius: req_field(&mut map, "radius")?,
                start_angle: req_field(&mut map, "startAngle")?,
                end_angle: req_field(&mut map, "endAngle")?,
                counterclockwise: def_field(&mut map, "counterclockwise")?,
            }),
            "arcTo" => Ok(Canvas2DOp::ArcTo {
                x1: req_field(&mut map, "x1")?,
                y1: req_field(&mut map, "y1")?,
                x2: req_field(&mut map, "x2")?,
                y2: req_field(&mut map, "y2")?,
                radius: req_field(&mut map, "radius")?,
            }),
            "ellipse" => Ok(Canvas2DOp::Ellipse {
                x: req_field(&mut map, "x")?,
                y: req_field(&mut map, "y")?,
                radius_x: req_field(&mut map, "radiusX")?,
                radius_y: req_field(&mut map, "radiusY")?,
                rotation: req_field(&mut map, "rotation")?,
                start_angle: req_field(&mut map, "startAngle")?,
                end_angle: req_field(&mut map, "endAngle")?,
                counterclockwise: def_field(&mut map, "counterclockwise")?,
            }),
            "rect" => Ok(Canvas2DOp::Rect {
                x: req_field(&mut map, "x")?,
                y: req_field(&mut map, "y")?,
                w: req_field(&mut map, "w")?,
                h: req_field(&mut map, "h")?,
            }),
            "roundRect" => Ok(Canvas2DOp::RoundRect {
                x: req_field(&mut map, "x")?,
                y: req_field(&mut map, "y")?,
                w: req_field(&mut map, "w")?,
                h: req_field(&mut map, "h")?,
                radii: req_field(&mut map, "radii")?,
            }),
            "fill" => Ok(Canvas2DOp::Fill {
                rule: def_field(&mut map, "rule")?,
                path_ops: opt_field(&mut map, "pathOps")?,
            }),
            "stroke" => Ok(Canvas2DOp::Stroke {
                path_ops: opt_field(&mut map, "pathOps")?,
            }),
            "clip" => Ok(Canvas2DOp::Clip {
                rule: def_field(&mut map, "rule")?,
                path_ops: opt_field(&mut map, "pathOps")?,
            }),
            "clearRect" => Ok(Canvas2DOp::ClearRect {
                x: req_field(&mut map, "x")?,
                y: req_field(&mut map, "y")?,
                w: req_field(&mut map, "w")?,
                h: req_field(&mut map, "h")?,
            }),
            "fillRect" => Ok(Canvas2DOp::FillRect {
                x: req_field(&mut map, "x")?,
                y: req_field(&mut map, "y")?,
                w: req_field(&mut map, "w")?,
                h: req_field(&mut map, "h")?,
            }),
            "strokeRect" => Ok(Canvas2DOp::StrokeRect {
                x: req_field(&mut map, "x")?,
                y: req_field(&mut map, "y")?,
                w: req_field(&mut map, "w")?,
                h: req_field(&mut map, "h")?,
            }),
            "fillText" => Ok(Canvas2DOp::FillText {
                text: req_field(&mut map, "text")?,
                x: req_field(&mut map, "x")?,
                y: req_field(&mut map, "y")?,
                max_width: opt_field(&mut map, "maxWidth")?,
            }),
            "strokeText" => Ok(Canvas2DOp::StrokeText {
                text: req_field(&mut map, "text")?,
                x: req_field(&mut map, "x")?,
                y: req_field(&mut map, "y")?,
                max_width: opt_field(&mut map, "maxWidth")?,
            }),
            "drawImage" => Ok(Canvas2DOp::DrawImage {
                src: req_field(&mut map, "src")?,
                sx: opt_field(&mut map, "sx")?,
                sy: opt_field(&mut map, "sy")?,
                sw: opt_field(&mut map, "sw")?,
                sh: opt_field(&mut map, "sh")?,
                dx: req_field(&mut map, "dx")?,
                dy: req_field(&mut map, "dy")?,
                dw: opt_field(&mut map, "dw")?,
                dh: opt_field(&mut map, "dh")?,
            }),
            other => Err(de::Error::custom(format!("unknown canvas op `{other}`"))),
        }
    }
}

/// CSS stylesheet block (`<style>` or resolved local `<link rel="stylesheet">`) captured from DOM.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StyleSheetSource {
    pub source: String,
    pub css: String,
}

/// DOM or CSS feature encountered in a frame that Tier 1 cannot natively rasterize.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnsupportedFeature {
    pub feature: String,
    pub tag: String,
    pub detail: String,
}

/// Node payload inside a flat arena [`SceneTree`].
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(
    tag = "kind",
    content = "data",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum NodeKind {
    Element {
        tag: String,
        classes: Vec<String>,
        inline_style: Vec<(String, String)>,
        attrs: Vec<(String, String)>,
        children: Vec<usize>,
    },
    Text(String),
    Svg {
        xml: String,
        classes: Vec<String>,
        attrs: Vec<(String, String)>,
        inline_style: Vec<(String, String)>,
        width_attr: Option<f32>,
        height_attr: Option<f32>,
        view_box: Option<(f32, f32, f32, f32)>,
    },
    Canvas2d {
        width: u32,
        height: u32,
        classes: Vec<String>,
        attrs: Vec<(String, String)>,
        inline_style: Vec<(String, String)>,
        ops: Vec<Canvas2DOp>,
    },
    Lottie {
        animation_id: String,
        frame: f64,
        width: f32,
        height: f32,
        preserve_aspect_ratio: Option<String>,
        classes: Vec<String>,
        attrs: Vec<(String, String)>,
        inline_style: Vec<(String, String)>,
    },
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ElementPayload {
    tag: String,
    #[serde(default)]
    classes: Vec<String>,
    #[serde(default)]
    inline_style: Vec<(String, String)>,
    #[serde(default)]
    attrs: Vec<(String, String)>,
    #[serde(default)]
    children: Vec<usize>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SvgPayload {
    xml: String,
    #[serde(default)]
    classes: Vec<String>,
    #[serde(default)]
    attrs: Vec<(String, String)>,
    #[serde(default)]
    inline_style: Vec<(String, String)>,
    #[serde(default)]
    width_attr: Option<f32>,
    #[serde(default)]
    height_attr: Option<f32>,
    #[serde(default)]
    view_box: Option<(f32, f32, f32, f32)>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Canvas2dPayload {
    width: u32,
    height: u32,
    #[serde(default)]
    classes: Vec<String>,
    #[serde(default)]
    attrs: Vec<(String, String)>,
    #[serde(default)]
    inline_style: Vec<(String, String)>,
    #[serde(default)]
    ops: Vec<Canvas2DOp>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LottiePayload {
    animation_id: String,
    frame: f64,
    width: f32,
    height: f32,
    #[serde(default)]
    preserve_aspect_ratio: Option<String>,
    #[serde(default)]
    classes: Vec<String>,
    #[serde(default)]
    attrs: Vec<(String, String)>,
    #[serde(default)]
    inline_style: Vec<(String, String)>,
}

fn deserialize_node_kind<E: de::Error>(kind: &str, data: serde_json::Value) -> Result<NodeKind, E> {
    match kind {
        "element" => {
            let p: ElementPayload = serde_json::from_value(data).map_err(de::Error::custom)?;
            Ok(NodeKind::Element {
                tag: p.tag,
                classes: p.classes,
                inline_style: p.inline_style,
                attrs: p.attrs,
                children: p.children,
            })
        }
        "text" => {
            let s: String = serde_json::from_value(data).map_err(de::Error::custom)?;
            Ok(NodeKind::Text(s))
        }
        "svg" => {
            let p: SvgPayload = serde_json::from_value(data).map_err(de::Error::custom)?;
            Ok(NodeKind::Svg {
                xml: p.xml,
                classes: p.classes,
                attrs: p.attrs,
                inline_style: p.inline_style,
                width_attr: p.width_attr,
                height_attr: p.height_attr,
                view_box: p.view_box,
            })
        }
        "canvas2d" => {
            let p: Canvas2dPayload = serde_json::from_value(data).map_err(de::Error::custom)?;
            Ok(NodeKind::Canvas2d {
                width: p.width,
                height: p.height,
                classes: p.classes,
                attrs: p.attrs,
                inline_style: p.inline_style,
                ops: p.ops,
            })
        }
        "lottie" => {
            let p: LottiePayload = serde_json::from_value(data).map_err(de::Error::custom)?;
            Ok(NodeKind::Lottie {
                animation_id: p.animation_id,
                frame: p.frame,
                width: p.width,
                height: p.height,
                preserve_aspect_ratio: p.preserve_aspect_ratio,
                classes: p.classes,
                attrs: p.attrs,
                inline_style: p.inline_style,
            })
        }
        other => Err(de::Error::unknown_variant(
            other,
            &["element", "text", "svg", "canvas2d", "lottie"],
        )),
    }
}

impl<'de> Deserialize<'de> for NodeKind {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let mut map = serde_json::Map::<String, serde_json::Value>::deserialize(deserializer)?;
        let kind: String = req_field(&mut map, "kind")?;
        let data = map
            .remove("data")
            .ok_or_else(|| de::Error::missing_field("data"))?;
        deserialize_node_kind(&kind, data)
    }
}

/// Single node inside a [`SceneTree`].
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneNode {
    pub id: usize,
    #[serde(flatten)]
    pub node: NodeKind,
}

impl<'de> Deserialize<'de> for SceneNode {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let mut map = serde_json::Map::<String, serde_json::Value>::deserialize(deserializer)?;
        let id: usize = req_field(&mut map, "id")?;
        let kind: String = req_field(&mut map, "kind")?;
        let data = map
            .remove("data")
            .ok_or_else(|| de::Error::missing_field("data"))?;
        Ok(SceneNode {
            id,
            node: deserialize_node_kind(&kind, data)?,
        })
    }
}

/// Extracted DOM/style/SVG/media snapshot for a single evaluated Remotion frame.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneTree {
    pub width: u32,
    pub height: u32,
    pub frame: u32,
    pub root: usize,
    pub nodes: Vec<SceneNode>,
    #[serde(default)]
    pub fonts: Vec<RegisteredFont>,
    #[serde(default)]
    pub stylesheets: Vec<StyleSheetSource>,
    #[serde(default)]
    pub audio_assets: Vec<AudioTrackFrame>,
    #[serde(default)]
    pub lottie_assets: Vec<LottieSource>,
    #[serde(default)]
    pub unsupported_features: Vec<UnsupportedFeature>,
}

/// Composition metadata returned by `window.getStaticCompositions()` in Tier 1.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tier1Composition {
    pub config: VideoConfig,
    pub serialized_resolved_props: String,
    pub default_out_name: Option<String>,
    pub default_video_image_format: Option<String>,
    pub default_pixel_format: Option<String>,
    pub default_prores_profile: Option<String>,
    pub default_sample_rate: Option<u32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scene_tree_roundtrip_serde() {
        let tree = SceneTree {
            width: 1920,
            height: 1080,
            frame: 30,
            root: 0,
            nodes: vec![
                SceneNode {
                    id: 0,
                    node: NodeKind::Element {
                        tag: "div".into(),
                        classes: vec![],
                        inline_style: vec![("background-color".into(), "#0f172a".into())],
                        attrs: vec![("id".into(), "remotion-canvas".into())],
                        children: vec![1, 2, 3, 4],
                    },
                },
                SceneNode {
                    id: 1,
                    node: NodeKind::Text("Hello".into()),
                },
                SceneNode {
                    id: 2,
                    node: NodeKind::Svg {
                        xml: "<svg viewBox=\"0 0 10 10\"></svg>".into(),
                        classes: vec!["icon".into()],
                        attrs: vec![],
                        inline_style: vec![],
                        width_attr: Some(100.0),
                        height_attr: Some(100.0),
                        view_box: Some((0.0, 0.0, 10.0, 10.0)),
                    },
                },
                SceneNode {
                    id: 3,
                    node: NodeKind::Canvas2d {
                        width: 400,
                        height: 300,
                        classes: vec![],
                        attrs: vec![],
                        inline_style: vec![],
                        ops: vec![
                            Canvas2DOp::Translate { x: 10.5, y: 20.25 },
                            Canvas2DOp::SetFillStyle {
                                paint: CanvasPaint::LinearGradient {
                                    x0: 0.0,
                                    y0: 0.0,
                                    x1: 100.0,
                                    y1: 50.0,
                                    stops: vec![(0.0, "#fff".into()), (1.0, "#000".into())],
                                },
                            },
                            Canvas2DOp::SetStrokeStyle {
                                paint: CanvasPaint::RadialGradient {
                                    x0: 10.0,
                                    y0: 10.0,
                                    r0: 2.0,
                                    x1: 20.0,
                                    y1: 20.0,
                                    r1: 30.0,
                                    stops: vec![(0.0, "#f00".into())],
                                },
                            },
                            Canvas2DOp::Arc {
                                x: 50.0,
                                y: 50.0,
                                radius: 25.0,
                                start_angle: 0.0,
                                end_angle: 3.125,
                                counterclockwise: false,
                            },
                            Canvas2DOp::RoundRect {
                                x: 0.0,
                                y: 0.0,
                                w: 80.0,
                                h: 40.0,
                                radii: [4.0, 4.0, 8.0, 8.0],
                            },
                            Canvas2DOp::Fill {
                                rule: "evenodd".into(),
                                path_ops: Some(vec![Canvas2DOp::MoveTo { x: 1.0, y: 2.0 }]),
                            },
                            Canvas2DOp::DrawImage {
                                src: "./public/logo.png".into(),
                                sx: Some(0.0),
                                sy: Some(0.0),
                                sw: Some(16.0),
                                sh: Some(16.0),
                                dx: 5.0,
                                dy: 6.0,
                                dw: Some(32.0),
                                dh: Some(32.0),
                            },
                        ],
                    },
                },
                SceneNode {
                    id: 4,
                    node: NodeKind::Lottie {
                        animation_id: "lottie-01".into(),
                        frame: 14.5,
                        width: 500.0,
                        height: 500.0,
                        preserve_aspect_ratio: Some("xMidYMid meet".into()),
                        classes: vec![],
                        attrs: vec![],
                        inline_style: vec![],
                    },
                },
            ],
            fonts: vec![],
            stylesheets: vec![],
            audio_assets: vec![],
            lottie_assets: vec![],
            unsupported_features: vec![],
        };

        let json = serde_json::to_string(&tree).unwrap();
        let parsed: SceneTree = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, tree);
    }
}
