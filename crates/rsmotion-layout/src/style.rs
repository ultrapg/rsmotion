use taffy::prelude::*;
use taffy::style::Overflow;

pub use crate::effects_parse::{ClipRadius, CssBlendMode, CssClipPath, CssFilter, CssMaskImage};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CssColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl CssColor {
    pub const TRANSPARENT: Self = Self {
        r: 0,
        g: 0,
        b: 0,
        a: 0,
    };
    pub const BLACK: Self = Self {
        r: 0,
        g: 0,
        b: 0,
        a: 255,
    };
    pub const WHITE: Self = Self {
        r: 255,
        g: 255,
        b: 255,
        a: 255,
    };

    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LengthPercent {
    Px(f32),
    Percent(f32),
}

impl LengthPercent {
    pub fn resolve(self, reference: f32) -> f32 {
        match self {
            Self::Px(px) => px,
            Self::Percent(pct) => reference * (pct * 0.01),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ColorStop {
    pub color: CssColor,
    pub pos: Option<LengthPercent>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LinearGradient {
    pub angle_deg: f32,
    pub stops: Vec<ColorStop>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RadialGradient {
    pub shape_circle: bool,
    pub cx: LengthPercent,
    pub cy: LengthPercent,
    pub stops: Vec<ColorStop>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub enum BackgroundImage {
    #[default]
    None,
    Linear(LinearGradient),
    Radial(RadialGradient),
}

#[derive(Debug, Clone, PartialEq)]
pub struct BoxShadow {
    pub inset: bool,
    pub offset_x: f32,
    pub offset_y: f32,
    pub blur_radius: f32,
    pub spread_radius: f32,
    pub color: CssColor,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TextShadow {
    pub offset_x: f32,
    pub offset_y: f32,
    pub blur_radius: f32,
    pub color: CssColor,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TransformOp {
    Translate(LengthPercent, LengthPercent),
    TranslateX(LengthPercent),
    TranslateY(LengthPercent),
    TranslateZ(f32),
    Translate3d(LengthPercent, LengthPercent, f32),
    Scale(f32, f32),
    ScaleZ(f32),
    Scale3d(f32, f32, f32),
    RotateDeg(f32),
    RotateXDeg(f32),
    RotateYDeg(f32),
    RotateZDeg(f32),
    Rotate3d(f32, f32, f32, f32),
    SkewDeg(f32, f32),
    Perspective(f32),
    Matrix([f32; 6]),
    Matrix3d([f32; 16]),
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum LineHeight {
    #[default]
    Normal,
    Number(f32),
    Px(f32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ObjectFit {
    #[default]
    Fill,
    Contain,
    Cover,
    None,
    ScaleDown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TextAlignMode {
    #[default]
    Left,
    Center,
    Right,
    Justify,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum FontSlantMode {
    #[default]
    Normal,
    Italic,
    Oblique,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum WhiteSpaceMode {
    #[default]
    Normal,
    Nowrap,
    Pre,
    PreWrap,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TextTransformMode {
    #[default]
    None,
    Uppercase,
    Lowercase,
    Capitalize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TextDecorationMode {
    #[default]
    None,
    Underline,
    LineThrough,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum VisibilityMode {
    #[default]
    Visible,
    Hidden,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BorderSide {
    pub width: f32,
    pub color: CssColor,
}

impl Default for BorderSide {
    fn default() -> Self {
        Self {
            width: 0.0,
            color: CssColor::BLACK,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ComputedStyle {
    pub taffy: Style,
    pub display_none: bool,
    pub overflow_hidden: bool,
    pub visibility: VisibilityMode,
    pub z_index: i32,
    pub background_color: CssColor,
    pub background_image: BackgroundImage,
    pub border_top: BorderSide,
    pub border_right: BorderSide,
    pub border_bottom: BorderSide,
    pub border_left: BorderSide,
    pub border_radii: [LengthPercent; 4], // top-left, top-right, bottom-right, bottom-left
    pub box_shadows: Vec<BoxShadow>,
    pub text_shadows: Vec<TextShadow>,
    pub opacity: f32,
    pub transforms: Vec<TransformOp>,
    pub transform_origin: (LengthPercent, LengthPercent),
    pub standalone_translate: Option<(LengthPercent, LengthPercent, f32)>,
    pub standalone_rotate: Option<TransformOp>,
    pub standalone_scale: Option<(f32, f32, f32)>,
    pub perspective: Option<f32>,
    pub perspective_origin: (LengthPercent, LengthPercent),
    pub filters: Vec<CssFilter>,
    pub backdrop_filters: Vec<CssFilter>,
    pub mix_blend_mode: CssBlendMode,
    pub clip_path: CssClipPath,
    pub mask_image: CssMaskImage,
    pub background_clip_text: bool,
    pub color: CssColor,
    pub font_family: Vec<String>,
    pub font_size: f32,
    pub font_weight: u16,
    pub font_style: FontSlantMode,
    pub line_height: LineHeight,
    pub letter_spacing: f32,
    pub text_align: TextAlignMode,
    pub white_space: WhiteSpaceMode,
    pub text_transform: TextTransformMode,
    pub text_decoration: TextDecorationMode,
    pub object_fit: ObjectFit,
    pub object_position: (LengthPercent, LengthPercent),
}

impl Default for ComputedStyle {
    fn default() -> Self {
        Self {
            taffy: Style {
                display: Display::Block,
                box_sizing: BoxSizing::BorderBox,
                ..Default::default()
            },
            display_none: false,
            overflow_hidden: false,
            visibility: VisibilityMode::Visible,
            z_index: 0,
            background_color: CssColor::TRANSPARENT,
            background_image: BackgroundImage::None,
            border_top: BorderSide::default(),
            border_right: BorderSide::default(),
            border_bottom: BorderSide::default(),
            border_left: BorderSide::default(),
            border_radii: [LengthPercent::Px(0.0); 4],
            box_shadows: Vec::new(),
            text_shadows: Vec::new(),
            opacity: 1.0,
            transforms: Vec::new(),
            transform_origin: (LengthPercent::Percent(50.0), LengthPercent::Percent(50.0)),
            standalone_translate: None,
            standalone_rotate: None,
            standalone_scale: None,
            perspective: None,
            perspective_origin: (LengthPercent::Percent(50.0), LengthPercent::Percent(50.0)),
            filters: Vec::new(),
            backdrop_filters: Vec::new(),
            mix_blend_mode: CssBlendMode::Normal,
            clip_path: CssClipPath::None,
            mask_image: CssMaskImage::None,
            background_clip_text: false,
            color: CssColor::BLACK,
            font_family: vec!["DejaVu Sans".to_string(), "sans-serif".to_string()],
            font_size: 16.0,
            font_weight: 400,
            font_style: FontSlantMode::Normal,
            line_height: LineHeight::Normal,
            letter_spacing: 0.0,
            text_align: TextAlignMode::Left,
            white_space: WhiteSpaceMode::Normal,
            text_transform: TextTransformMode::None,
            text_decoration: TextDecorationMode::None,
            object_fit: ObjectFit::Fill,
            object_position: (LengthPercent::Percent(50.0), LengthPercent::Percent(50.0)),
        }
    }
}

pub fn split_top_level(s: &str, delim: char) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut in_quote: Option<char> = None;
    let mut start = 0usize;

    for (i, c) in s.char_indices() {
        if let Some(q) = in_quote {
            if c == q {
                in_quote = None;
            }
            continue;
        }
        match c {
            '"' | '\'' => in_quote = Some(c),
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            ch if ch == delim && depth == 0 => {
                let piece = s[start..i].trim();
                if !piece.is_empty() {
                    parts.push(piece);
                }
                start = i + ch.len_utf8();
            }
            _ => {}
        }
    }
    let tail = s[start..].trim();
    if !tail.is_empty() {
        parts.push(tail);
    }
    parts
}

pub(crate) fn split_top_level_ws(s: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut in_quote: Option<char> = None;
    let mut start: Option<usize> = None;

    for (i, c) in s.char_indices() {
        if let Some(q) = in_quote {
            if c == q {
                in_quote = None;
            }
            continue;
        }
        match c {
            '"' | '\'' => {
                if start.is_none() {
                    start = Some(i);
                }
                in_quote = Some(c);
            }
            '(' => {
                if start.is_none() {
                    start = Some(i);
                }
                depth += 1;
            }
            ')' => depth = depth.saturating_sub(1),
            ch if ch.is_whitespace() && depth == 0 => {
                if let Some(st) = start.take() {
                    parts.push(&s[st..i]);
                }
            }
            _ => {
                if start.is_none() {
                    start = Some(i);
                }
            }
        }
    }
    if let Some(st) = start {
        parts.push(&s[st..]);
    }
    parts
}

pub(crate) fn parse_css_number(s: &str) -> Option<f32> {
    let t = s.trim();
    if let Ok(n) = t.parse::<f32>() {
        return Some(n);
    }
    if t.contains('(') {
        if let Some(v) = rsmotion_css::eval_css_math(t, 16.0, 16.0, 1920.0, 1080.0, None) {
            if v.dim == rsmotion_css::MathDim::Number {
                return Some(v.value);
            }
        }
    }
    None
}

pub(crate) fn parse_angle_deg(raw: &str) -> Option<f32> {
    let s = raw.trim().to_ascii_lowercase();
    if let Some(v) = s.strip_suffix("deg") {
        v.trim().parse::<f32>().ok()
    } else if let Some(v) = s.strip_suffix("rad") {
        v.trim().parse::<f32>().ok().map(|r| r.to_degrees())
    } else if let Some(v) = s.strip_suffix("turn") {
        v.trim().parse::<f32>().ok().map(|t| t * 360.0)
    } else if let Some(v) = s.strip_suffix("grad") {
        v.trim().parse::<f32>().ok().map(|g| g * 0.9)
    } else if s == "0" {
        Some(0.0)
    } else if s.contains('(') {
        rsmotion_css::eval_css_math(raw, 16.0, 16.0, 1920.0, 1080.0, None)
            .filter(|v| v.dim == rsmotion_css::MathDim::Deg)
            .map(|v| v.value)
    } else {
        None
    }
}

pub(crate) fn parse_length_px(raw: &str, font_size: f32, vw: f32, vh: f32) -> Option<f32> {
    let s = raw.trim().to_ascii_lowercase();
    if s == "0" {
        return Some(0.0);
    }
    if let Some(v) = s.strip_suffix("px") {
        return parse_css_number(v);
    }
    if let Some(v) = s.strip_suffix("rem") {
        return parse_css_number(v).map(|n| n * 16.0);
    }
    if let Some(v) = s.strip_suffix("em") {
        return parse_css_number(v).map(|n| n * font_size);
    }
    if let Some(v) = s.strip_suffix("vw") {
        return parse_css_number(v).map(|n| n * vw * 0.01);
    }
    if let Some(v) = s.strip_suffix("vh") {
        return parse_css_number(v).map(|n| n * vh * 0.01);
    }
    if s.contains('(') {
        if let Some(val) = rsmotion_css::eval_css_math(raw, font_size, 16.0, vw, vh, None) {
            if matches!(
                val.dim,
                rsmotion_css::MathDim::Px | rsmotion_css::MathDim::Number
            ) {
                return Some(val.value);
            }
        }
    }
    parse_css_number(&s)
}

pub(crate) fn parse_length_percent(
    raw: &str,
    font_size: f32,
    vw: f32,
    vh: f32,
) -> Option<LengthPercent> {
    let s = raw.trim().to_ascii_lowercase();
    if let Some(p) = s.strip_suffix('%') {
        return parse_css_number(p).map(LengthPercent::Percent);
    }
    if s.contains('(') {
        if let Some(val) = rsmotion_css::eval_css_math(raw, font_size, 16.0, vw, vh, None) {
            return match val.dim {
                rsmotion_css::MathDim::Percent => Some(LengthPercent::Percent(val.value)),
                rsmotion_css::MathDim::Px | rsmotion_css::MathDim::Number => {
                    Some(LengthPercent::Px(val.value))
                }
                rsmotion_css::MathDim::Deg => None,
            };
        }
    }
    parse_length_px(&s, font_size, vw, vh).map(LengthPercent::Px)
}

fn parse_dim_or_auto(raw: &str, font_size: f32, vw: f32, vh: f32) -> Option<Dimension> {
    let s = raw.trim().to_ascii_lowercase();
    if s == "auto" {
        return Some(Dimension::auto());
    }
    if let Some(lp) = parse_length_percent(&s, font_size, vw, vh) {
        return Some(match lp {
            LengthPercent::Px(px) => Dimension::length(px),
            LengthPercent::Percent(p) => Dimension::percent(p * 0.01),
        });
    }
    None
}

fn parse_lpa(raw: &str, font_size: f32, vw: f32, vh: f32) -> Option<LengthPercentageAuto> {
    let s = raw.trim().to_ascii_lowercase();
    if s == "auto" {
        return Some(LengthPercentageAuto::auto());
    }
    if let Some(lp) = parse_length_percent(&s, font_size, vw, vh) {
        return Some(match lp {
            LengthPercent::Px(px) => LengthPercentageAuto::length(px),
            LengthPercent::Percent(p) => LengthPercentageAuto::percent(p * 0.01),
        });
    }
    None
}

pub(crate) fn parse_lp(raw: &str, font_size: f32, vw: f32, vh: f32) -> Option<LengthPercentage> {
    parse_length_percent(raw, font_size, vw, vh).map(|lp| match lp {
        LengthPercent::Px(px) => LengthPercentage::length(px),
        LengthPercent::Percent(p) => LengthPercentage::percent(p * 0.01),
    })
}

pub(crate) fn expand_4_sides<T: Copy>(vals: &[T]) -> Option<[T; 4]> {
    match vals.len() {
        1 => Some([vals[0], vals[0], vals[0], vals[0]]),
        2 => Some([vals[0], vals[1], vals[0], vals[1]]),
        3 => Some([vals[0], vals[1], vals[2], vals[1]]),
        4 => Some([vals[0], vals[1], vals[2], vals[3]]),
        _ => None,
    }
}

fn hsl_to_rgb(h_deg: f32, s_pct: f32, l_pct: f32) -> (u8, u8, u8) {
    let h = ((h_deg % 360.0) + 360.0) % 360.0 / 360.0;
    let s = (s_pct / 100.0).clamp(0.0, 1.0);
    let l = (l_pct / 100.0).clamp(0.0, 1.0);
    if s == 0.0 {
        let v = (l * 255.0).round() as u8;
        return (v, v, v);
    }
    let q = if l < 0.5 {
        l * (1.0 + s)
    } else {
        l + s - l * s
    };
    let p = 2.0 * l - q;
    let hue2rgb = |mut t: f32| {
        if t < 0.0 {
            t += 1.0;
        }
        if t > 1.0 {
            t -= 1.0;
        }
        if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 0.5 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        }
    };
    (
        (hue2rgb(h + 1.0 / 3.0) * 255.0).round() as u8,
        (hue2rgb(h) * 255.0).round() as u8,
        (hue2rgb(h - 1.0 / 3.0) * 255.0).round() as u8,
    )
}

fn parse_alpha(s: &str) -> Option<u8> {
    let t = s.trim();
    let f = if let Some(p) = t.strip_suffix('%') {
        p.trim().parse::<f32>().ok()? * 0.01
    } else {
        t.parse::<f32>().ok()?
    };
    Some((f.clamp(0.0, 1.0) * 255.0).round() as u8)
}

pub fn parse_color(raw: &str, current_color: CssColor) -> Option<CssColor> {
    let s = raw.trim();
    if s.is_empty() {
        return None;
    }
    let lower = s.to_ascii_lowercase();
    if lower == "transparent" {
        return Some(CssColor::TRANSPARENT);
    }
    if lower == "currentcolor" {
        return Some(current_color);
    }

    if let Some(hex) = lower.strip_prefix('#') {
        let bytes = hex.as_bytes();
        let hex_nibble = |b: u8| -> Option<u8> {
            match b {
                b'0'..=b'9' => Some(b - b'0'),
                b'a'..=b'f' => Some(b - b'a' + 10),
                _ => None,
            }
        };
        let hex_byte =
            |hi: u8, lo: u8| -> Option<u8> { Some((hex_nibble(hi)? << 4) | hex_nibble(lo)?) };
        return match bytes.len() {
            3 => Some(CssColor::rgba(
                hex_byte(bytes[0], bytes[0])?,
                hex_byte(bytes[1], bytes[1])?,
                hex_byte(bytes[2], bytes[2])?,
                255,
            )),
            4 => Some(CssColor::rgba(
                hex_byte(bytes[0], bytes[0])?,
                hex_byte(bytes[1], bytes[1])?,
                hex_byte(bytes[2], bytes[2])?,
                hex_byte(bytes[3], bytes[3])?,
            )),
            6 => Some(CssColor::rgba(
                hex_byte(bytes[0], bytes[1])?,
                hex_byte(bytes[2], bytes[3])?,
                hex_byte(bytes[4], bytes[5])?,
                255,
            )),
            8 => Some(CssColor::rgba(
                hex_byte(bytes[0], bytes[1])?,
                hex_byte(bytes[2], bytes[3])?,
                hex_byte(bytes[4], bytes[5])?,
                hex_byte(bytes[6], bytes[7])?,
            )),
            _ => None,
        };
    }

    if let Some(inner) = lower
        .strip_prefix("rgba(")
        .or_else(|| lower.strip_prefix("rgb("))
        .and_then(|r| r.strip_suffix(')'))
    {
        let (main, slash_alpha) = if let Some((a, b)) = inner.split_once('/') {
            (a, Some(b))
        } else {
            (inner, None)
        };
        let parts: Vec<&str> = if main.contains(',') {
            main.split(',').map(str::trim).collect()
        } else {
            main.split_whitespace().collect()
        };
        if parts.len() < 3 {
            return None;
        }
        let chan = |p: &str| -> Option<u8> {
            let v = if let Some(pct) = p.strip_suffix('%') {
                pct.trim().parse::<f32>().ok()? * 2.55
            } else {
                p.trim().parse::<f32>().ok()?
            };
            Some(v.clamp(0.0, 255.0).round() as u8)
        };
        let r = chan(parts[0])?;
        let g = chan(parts[1])?;
        let b = chan(parts[2])?;
        let a = if let Some(sa) = slash_alpha {
            parse_alpha(sa)?
        } else if parts.len() >= 4 {
            parse_alpha(parts[3])?
        } else {
            255
        };
        return Some(CssColor::rgba(r, g, b, a));
    }

    if let Some(inner) = lower
        .strip_prefix("hsla(")
        .or_else(|| lower.strip_prefix("hsl("))
        .and_then(|r| r.strip_suffix(')'))
    {
        let (main, slash_alpha) = if let Some((a, b)) = inner.split_once('/') {
            (a, Some(b))
        } else {
            (inner, None)
        };
        let parts: Vec<&str> = if main.contains(',') {
            main.split(',').map(str::trim).collect()
        } else {
            main.split_whitespace().collect()
        };
        if parts.len() < 3 {
            return None;
        }
        let h = parts[0]
            .trim_end_matches("deg")
            .trim()
            .parse::<f32>()
            .ok()?;
        let s_pct = parts[1].trim_end_matches('%').trim().parse::<f32>().ok()?;
        let l_pct = parts[2].trim_end_matches('%').trim().parse::<f32>().ok()?;
        let (r, g, b) = hsl_to_rgb(h, s_pct, l_pct);
        let a = if let Some(sa) = slash_alpha {
            parse_alpha(sa)?
        } else if parts.len() >= 4 {
            parse_alpha(parts[3])?
        } else {
            255
        };
        return Some(CssColor::rgba(r, g, b, a));
    }

    if let Some(c4) = crate::color4::parse_color4(s, current_color) {
        return Some(c4);
    }

    named_color(&lower)
}

fn named_color(name: &str) -> Option<CssColor> {
    Some(match name {
        "black" => CssColor::rgba(0, 0, 0, 255),
        "white" => CssColor::rgba(255, 255, 255, 255),
        "red" => CssColor::rgba(255, 0, 0, 255),
        "green" => CssColor::rgba(0, 128, 0, 255),
        "lime" => CssColor::rgba(0, 255, 0, 255),
        "blue" => CssColor::rgba(0, 0, 255, 255),
        "yellow" => CssColor::rgba(255, 255, 0, 255),
        "cyan" | "aqua" => CssColor::rgba(0, 255, 255, 255),
        "magenta" | "fuchsia" => CssColor::rgba(255, 0, 255, 255),
        "gray" | "grey" => CssColor::rgba(128, 128, 128, 255),
        "silver" => CssColor::rgba(192, 192, 192, 255),
        "orange" => CssColor::rgba(255, 165, 0, 255),
        "purple" => CssColor::rgba(128, 0, 128, 255),
        "pink" => CssColor::rgba(255, 192, 203, 255),
        "navy" => CssColor::rgba(0, 0, 128, 255),
        "teal" => CssColor::rgba(0, 128, 128, 255),
        "olive" => CssColor::rgba(128, 128, 0, 255),
        "maroon" => CssColor::rgba(128, 0, 0, 255),
        "rebeccapurple" => CssColor::rgba(102, 51, 153, 255),
        _ => return None,
    })
}

fn parse_color_stops(
    args: &[&str],
    current_color: CssColor,
    font_size: f32,
    vw: f32,
    vh: f32,
) -> Option<Vec<ColorStop>> {
    let mut stops = Vec::with_capacity(args.len());
    for arg in args {
        let toks = split_top_level_ws(arg);
        if toks.is_empty() {
            continue;
        }
        let color = parse_color(toks[0], current_color)?;
        let pos = if toks.len() >= 2 {
            parse_length_percent(toks[1], font_size, vw, vh)
        } else {
            None
        };
        stops.push(ColorStop { color, pos });
    }
    if stops.len() >= 2 {
        Some(stops)
    } else {
        None
    }
}

pub(crate) fn parse_linear_gradient(
    raw: &str,
    current_color: CssColor,
    font_size: f32,
    vw: f32,
    vh: f32,
) -> Option<LinearGradient> {
    let trimmed = raw.trim();
    let lower = trimmed.to_ascii_lowercase();
    if !lower.starts_with("linear-gradient(") || !trimmed.ends_with(')') {
        return None;
    }
    let inner = &trimmed["linear-gradient(".len()..trimmed.len() - 1];
    let args = split_top_level(inner, ',');
    if args.len() < 2 {
        return None;
    }

    let first_lower = args[0].trim().to_ascii_lowercase();
    let (angle_deg, stop_start) = if let Some(deg) = parse_angle_deg(&first_lower) {
        (deg, 1usize)
    } else if let Some(dir) = first_lower.strip_prefix("to ") {
        let d = match dir.trim() {
            "top" => 0.0,
            "top right" | "right top" => 45.0,
            "right" => 90.0,
            "bottom right" | "right bottom" => 135.0,
            "bottom" => 180.0,
            "bottom left" | "left bottom" => 225.0,
            "left" => 270.0,
            "top left" | "left top" => 315.0,
            _ => 180.0,
        };
        (d, 1usize)
    } else {
        (180.0, 0usize)
    };

    let stops = parse_color_stops(&args[stop_start..], current_color, font_size, vw, vh)?;
    Some(LinearGradient { angle_deg, stops })
}

pub(crate) fn parse_radial_gradient(
    raw: &str,
    current_color: CssColor,
    font_size: f32,
    vw: f32,
    vh: f32,
) -> Option<RadialGradient> {
    let trimmed = raw.trim();
    let lower = trimmed.to_ascii_lowercase();
    if !lower.starts_with("radial-gradient(") || !trimmed.ends_with(')') {
        return None;
    }
    let inner = &trimmed["radial-gradient(".len()..trimmed.len() - 1];
    let args = split_top_level(inner, ',');
    if args.len() < 2 {
        return None;
    }

    let first_lower = args[0].trim().to_ascii_lowercase();
    let is_prelude = parse_color(&first_lower, current_color).is_none();

    let mut shape_circle = false;
    let mut cx = LengthPercent::Percent(50.0);
    let mut cy = LengthPercent::Percent(50.0);
    let stop_start = if is_prelude {
        let (shape_part, at_part) = if let Some((l, r)) = first_lower.split_once(" at ") {
            (l.trim(), Some(r.trim()))
        } else if let Some(r) = first_lower.strip_prefix("at ") {
            ("", Some(r.trim()))
        } else {
            (first_lower.as_str(), None)
        };
        if shape_part.contains("circle") {
            shape_circle = true;
        }
        if let Some(pos_str) = at_part {
            let toks: Vec<&str> = pos_str.split_whitespace().collect();
            if let Some(t0) = toks.first() {
                cx = match *t0 {
                    "left" => LengthPercent::Percent(0.0),
                    "center" => LengthPercent::Percent(50.0),
                    "right" => LengthPercent::Percent(100.0),
                    other => parse_length_percent(other, font_size, vw, vh).unwrap_or(cx),
                };
            }
            if let Some(t1) = toks.get(1) {
                cy = match *t1 {
                    "top" => LengthPercent::Percent(0.0),
                    "center" => LengthPercent::Percent(50.0),
                    "bottom" => LengthPercent::Percent(100.0),
                    other => parse_length_percent(other, font_size, vw, vh).unwrap_or(cy),
                };
            }
        }
        1usize
    } else {
        0usize
    };

    let stops = parse_color_stops(&args[stop_start..], current_color, font_size, vw, vh)?;
    Some(RadialGradient {
        shape_circle,
        cx,
        cy,
        stops,
    })
}

fn parse_box_shadows(
    raw: &str,
    current_color: CssColor,
    font_size: f32,
    vw: f32,
    vh: f32,
) -> Vec<BoxShadow> {
    if raw.trim().eq_ignore_ascii_case("none") {
        return Vec::new();
    }
    let mut out = Vec::new();
    for item in split_top_level(raw, ',') {
        let toks = split_top_level_ws(item);
        let mut inset = false;
        let mut color = current_color;
        let mut lengths = Vec::new();

        for tok in toks {
            if tok.eq_ignore_ascii_case("inset") {
                inset = true;
            } else if let Some(c) = parse_color(tok, current_color) {
                color = c;
            } else if let Some(px) = parse_length_px(tok, font_size, vw, vh) {
                lengths.push(px);
            }
        }
        if lengths.len() >= 2 {
            out.push(BoxShadow {
                inset,
                offset_x: lengths[0],
                offset_y: lengths[1],
                blur_radius: lengths.get(2).copied().unwrap_or(0.0).max(0.0),
                spread_radius: lengths.get(3).copied().unwrap_or(0.0),
                color,
            });
        }
    }
    out
}

fn parse_text_shadows(
    raw: &str,
    current_color: CssColor,
    font_size: f32,
    vw: f32,
    vh: f32,
) -> Vec<TextShadow> {
    if raw.trim().eq_ignore_ascii_case("none") {
        return Vec::new();
    }
    let mut out = Vec::new();
    for item in split_top_level(raw, ',') {
        let toks = split_top_level_ws(item);
        let mut color = current_color;
        let mut lengths = Vec::new();

        for tok in toks {
            if let Some(c) = parse_color(tok, current_color) {
                color = c;
            } else if let Some(px) = parse_length_px(tok, font_size, vw, vh) {
                lengths.push(px);
            }
        }
        if lengths.len() >= 2 {
            out.push(TextShadow {
                offset_x: lengths[0],
                offset_y: lengths[1],
                blur_radius: lengths.get(2).copied().unwrap_or(0.0).max(0.0),
                color,
            });
        }
    }
    out
}

fn parse_border_side(
    raw: &str,
    current_color: CssColor,
    font_size: f32,
    vw: f32,
    vh: f32,
) -> Option<BorderSide> {
    let s = raw.trim();
    if s.eq_ignore_ascii_case("none") || s == "0" {
        return Some(BorderSide {
            width: 0.0,
            color: CssColor::TRANSPARENT,
        });
    }
    let toks = split_top_level_ws(s);
    let mut width = 1.0_f32;
    let mut color = current_color;
    let mut style_none = false;

    for tok in toks {
        let lower = tok.to_ascii_lowercase();
        if lower == "none" || lower == "hidden" {
            style_none = true;
        } else if matches!(
            lower.as_str(),
            "solid" | "dashed" | "dotted" | "double" | "groove" | "ridge"
        ) {
            continue;
        } else if lower == "thin" {
            width = 1.0;
        } else if lower == "medium" {
            width = 3.0;
        } else if lower == "thick" {
            width = 5.0;
        } else if let Some(c) = parse_color(tok, current_color) {
            color = c;
        } else if let Some(px) = parse_length_px(tok, font_size, vw, vh) {
            width = px.max(0.0);
        }
    }
    if style_none {
        width = 0.0;
    }
    Some(BorderSide { width, color })
}

pub(crate) fn parse_origin(
    raw: &str,
    font_size: f32,
    vw: f32,
    vh: f32,
) -> (LengthPercent, LengthPercent) {
    let toks: Vec<&str> = raw.split_whitespace().collect();
    let parse_axis = |tok: &str| match tok.to_ascii_lowercase().as_str() {
        "left" | "top" => Some(LengthPercent::Percent(0.0)),
        "center" => Some(LengthPercent::Percent(50.0)),
        "right" | "bottom" => Some(LengthPercent::Percent(100.0)),
        other => parse_length_percent(other, font_size, vw, vh),
    };
    match toks.len() {
        0 => (LengthPercent::Percent(50.0), LengthPercent::Percent(50.0)),
        1 => match toks[0].to_ascii_lowercase().as_str() {
            "top" => (LengthPercent::Percent(50.0), LengthPercent::Percent(0.0)),
            "bottom" => (LengthPercent::Percent(50.0), LengthPercent::Percent(100.0)),
            _ => (
                parse_axis(toks[0]).unwrap_or(LengthPercent::Percent(50.0)),
                LengthPercent::Percent(50.0),
            ),
        },
        _ => (
            parse_axis(toks[0]).unwrap_or(LengthPercent::Percent(50.0)),
            parse_axis(toks[1]).unwrap_or(LengthPercent::Percent(50.0)),
        ),
    }
}

fn parse_font_family_list(raw: &str) -> Vec<String> {
    let list: Vec<String> = split_top_level(raw, ',')
        .into_iter()
        .map(|s| s.trim().trim_matches(|c| c == '"' || c == '\'').to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if list.is_empty() {
        vec!["DejaVu Sans".to_string(), "sans-serif".to_string()]
    } else {
        list
    }
}

impl ComputedStyle {
    /// Computes child style from `parent`, HTML UA defaults, utility classes, and declarations.
    pub fn for_element(
        parent: Option<&ComputedStyle>,
        tag: &str,
        classes: &[String],
        declarations: &[(String, String)],
        vw: f32,
        vh: f32,
    ) -> Self {
        let mut st = Self::default();
        if let Some(p) = parent {
            st.color = p.color;
            st.font_family = p.font_family.clone();
            st.font_size = p.font_size;
            st.font_weight = p.font_weight;
            st.font_style = p.font_style;
            st.line_height = p.line_height;
            st.letter_spacing = p.letter_spacing;
            st.text_align = p.text_align;
            st.text_shadows = p.text_shadows.clone();
            st.white_space = p.white_space;
            st.text_transform = p.text_transform;
            st.visibility = p.visibility;
        }

        // UA tag defaults (overridden by stylesheet / inline declarations)
        match tag {
            "h1" => {
                st.font_size = 32.0;
                st.font_weight = 700;
                let em = st.font_size * 0.67;
                st.taffy.margin.top = LengthPercentageAuto::length(em);
                st.taffy.margin.bottom = LengthPercentageAuto::length(em);
            }
            "h2" => {
                st.font_size = 24.0;
                st.font_weight = 700;
                let em = st.font_size * 0.83;
                st.taffy.margin.top = LengthPercentageAuto::length(em);
                st.taffy.margin.bottom = LengthPercentageAuto::length(em);
            }
            "h3" => {
                st.font_size = 18.72;
                st.font_weight = 700;
            }
            "p" => {
                let em = st.font_size;
                st.taffy.margin.top = LengthPercentageAuto::length(em);
                st.taffy.margin.bottom = LengthPercentageAuto::length(em);
            }
            "strong" | "b" => {
                st.font_weight = 700;
            }
            "em" | "i" => {
                st.font_style = FontSlantMode::Italic;
            }
            "video" => {
                st.object_fit = ObjectFit::Contain;
            }
            "audio" => {
                st.display_none = true;
                st.taffy.display = Display::None;
            }
            _ => {}
        }

        if classes.iter().any(|c| c == "__remotion_objectfitcontain") {
            st.object_fit = ObjectFit::Contain;
        }

        // Pass 1: resolve color and font-size so `currentColor` and `em` units work.
        for (k, v) in declarations {
            if k == "color" {
                if let Some(c) = parse_color(v, st.color) {
                    st.color = c;
                }
            } else if k == "font-size" {
                let val_lower = v.trim().to_ascii_lowercase();
                if let Some(p) = val_lower.strip_suffix('%') {
                    if let Some(pct) = parse_css_number(p) {
                        st.font_size = (st.font_size * pct * 0.01).max(0.0);
                    }
                } else if let Some(px) = parse_length_px(&val_lower, st.font_size, vw, vh) {
                    st.font_size = px.max(0.0);
                }
            }
        }

        // Pass 2: apply all declarations in exact cascade/insertion order.
        for (k, v) in declarations {
            st.apply_declaration(k, v, vw, vh);
        }
        st
    }

    /// Inherits text/color properties without keeping box margins/padding for anonymous text leaves.
    pub fn inherited_text_only(parent: &ComputedStyle) -> Self {
        Self {
            color: parent.color,
            font_family: parent.font_family.clone(),
            font_size: parent.font_size,
            font_weight: parent.font_weight,
            font_style: parent.font_style,
            line_height: parent.line_height,
            letter_spacing: parent.letter_spacing,
            text_align: parent.text_align,
            text_shadows: parent.text_shadows.clone(),
            white_space: parent.white_space,
            text_transform: parent.text_transform,
            visibility: parent.visibility,
            ..Default::default()
        }
    }

    pub fn apply_declaration(&mut self, key: &str, raw_val: &str, vw: f32, vh: f32) {
        let val = raw_val.trim();
        let lower = val.to_ascii_lowercase();
        let fs = self.font_size;

        match key {
            "display" => match lower.as_str() {
                "none" => {
                    self.display_none = true;
                    self.taffy.display = Display::None;
                }
                "flex" | "inline-flex" => {
                    self.display_none = false;
                    self.taffy.display = Display::Flex;
                }
                "grid" | "inline-grid" => {
                    self.display_none = false;
                    self.taffy.display = Display::Grid;
                }
                _ => {
                    self.display_none = false;
                    self.taffy.display = Display::Block;
                }
            },
            "box-sizing" => {
                self.taffy.box_sizing = if lower == "content-box" {
                    BoxSizing::ContentBox
                } else {
                    BoxSizing::BorderBox
                };
            }
            "position" => {
                self.taffy.position = match lower.as_str() {
                    "absolute" | "fixed" => Position::Absolute,
                    _ => Position::Relative,
                };
            }
            "top" => {
                if let Some(d) = parse_lpa(val, fs, vw, vh) {
                    self.taffy.inset.top = d;
                }
            }
            "right" => {
                if let Some(d) = parse_lpa(val, fs, vw, vh) {
                    self.taffy.inset.right = d;
                }
            }
            "bottom" => {
                if let Some(d) = parse_lpa(val, fs, vw, vh) {
                    self.taffy.inset.bottom = d;
                }
            }
            "left" => {
                if let Some(d) = parse_lpa(val, fs, vw, vh) {
                    self.taffy.inset.left = d;
                }
            }
            "inset" => {
                let vals: Vec<_> = val
                    .split_whitespace()
                    .filter_map(|t| parse_lpa(t, fs, vw, vh))
                    .collect();
                if let Some([t, r, b, l]) = expand_4_sides(&vals) {
                    self.taffy.inset = Rect {
                        top: t,
                        right: r,
                        bottom: b,
                        left: l,
                    };
                }
            }
            "width" => {
                if let Some(d) = parse_dim_or_auto(val, fs, vw, vh) {
                    self.taffy.size.width = d;
                }
            }
            "height" => {
                if let Some(d) = parse_dim_or_auto(val, fs, vw, vh) {
                    self.taffy.size.height = d;
                }
            }
            "min-width" => {
                if let Some(d) = parse_lpa(val, fs, vw, vh) {
                    self.taffy.min_size.width = d;
                }
            }
            "min-height" => {
                if let Some(d) = parse_lpa(val, fs, vw, vh) {
                    self.taffy.min_size.height = d;
                }
            }
            "max-width" => {
                if lower == "none" {
                    self.taffy.max_size.width = LengthPercentageAuto::auto();
                } else if let Some(d) = parse_lpa(val, fs, vw, vh) {
                    self.taffy.max_size.width = d;
                }
            }
            "max-height" => {
                if lower == "none" {
                    self.taffy.max_size.height = LengthPercentageAuto::auto();
                } else if let Some(d) = parse_lpa(val, fs, vw, vh) {
                    self.taffy.max_size.height = d;
                }
            }
            "aspect-ratio" => {
                if lower == "auto" {
                    self.taffy.aspect_ratio = None;
                } else if let Some((a, b)) = lower.split_once('/') {
                    if let (Some(x), Some(y)) = (parse_css_number(a), parse_css_number(b)) {
                        if y > 0.0 {
                            self.taffy.aspect_ratio = Some(x / y);
                        }
                    }
                } else if let Some(r) = parse_css_number(&lower) {
                    self.taffy.aspect_ratio = Some(r);
                }
            }
            "margin" => {
                let vals: Vec<_> = val
                    .split_whitespace()
                    .filter_map(|t| parse_lpa(t, fs, vw, vh))
                    .collect();
                if let Some([t, r, b, l]) = expand_4_sides(&vals) {
                    self.taffy.margin = Rect {
                        top: t,
                        right: r,
                        bottom: b,
                        left: l,
                    };
                }
            }
            "margin-top" => {
                if let Some(d) = parse_lpa(val, fs, vw, vh) {
                    self.taffy.margin.top = d;
                }
            }
            "margin-right" => {
                if let Some(d) = parse_lpa(val, fs, vw, vh) {
                    self.taffy.margin.right = d;
                }
            }
            "margin-bottom" => {
                if let Some(d) = parse_lpa(val, fs, vw, vh) {
                    self.taffy.margin.bottom = d;
                }
            }
            "margin-left" => {
                if let Some(d) = parse_lpa(val, fs, vw, vh) {
                    self.taffy.margin.left = d;
                }
            }
            "padding" => {
                let vals: Vec<_> = val
                    .split_whitespace()
                    .filter_map(|t| parse_lp(t, fs, vw, vh))
                    .collect();
                if let Some([t, r, b, l]) = expand_4_sides(&vals) {
                    self.taffy.padding = Rect {
                        top: t,
                        right: r,
                        bottom: b,
                        left: l,
                    };
                }
            }
            "padding-top" => {
                if let Some(d) = parse_lp(val, fs, vw, vh) {
                    self.taffy.padding.top = d;
                }
            }
            "padding-right" => {
                if let Some(d) = parse_lp(val, fs, vw, vh) {
                    self.taffy.padding.right = d;
                }
            }
            "padding-bottom" => {
                if let Some(d) = parse_lp(val, fs, vw, vh) {
                    self.taffy.padding.bottom = d;
                }
            }
            "padding-left" => {
                if let Some(d) = parse_lp(val, fs, vw, vh) {
                    self.taffy.padding.left = d;
                }
            }
            "flex-direction" => {
                self.taffy.flex_direction = match lower.as_str() {
                    "row" => FlexDirection::Row,
                    "column" => FlexDirection::Column,
                    "row-reverse" => FlexDirection::RowReverse,
                    "column-reverse" => FlexDirection::ColumnReverse,
                    _ => self.taffy.flex_direction,
                };
            }
            "flex-wrap" => {
                self.taffy.flex_wrap = match lower.as_str() {
                    "nowrap" => FlexWrap::NoWrap,
                    "wrap" => FlexWrap::Wrap,
                    "wrap-reverse" => FlexWrap::WrapReverse,
                    _ => self.taffy.flex_wrap,
                };
            }
            "justify-content" => {
                self.taffy.justify_content = match lower.as_str() {
                    "flex-start" => JustifyContent::FLEX_START,
                    "start" => JustifyContent::START,
                    "flex-end" => JustifyContent::FLEX_END,
                    "end" => JustifyContent::END,
                    "center" => JustifyContent::CENTER,
                    "space-between" => JustifyContent::SPACE_BETWEEN,
                    "space-around" => JustifyContent::SPACE_AROUND,
                    "space-evenly" => JustifyContent::SPACE_EVENLY,
                    _ => self.taffy.justify_content,
                };
            }
            "align-items" => {
                self.taffy.align_items = match lower.as_str() {
                    "stretch" => AlignItems::STRETCH,
                    "flex-start" => AlignItems::FLEX_START,
                    "start" => AlignItems::START,
                    "flex-end" => AlignItems::FLEX_END,
                    "end" => AlignItems::END,
                    "center" => AlignItems::CENTER,
                    "baseline" => AlignItems::BASELINE,
                    _ => self.taffy.align_items,
                };
            }
            "align-self" => {
                self.taffy.align_self = match lower.as_str() {
                    "auto" => None,
                    "stretch" => Some(AlignSelf::STRETCH),
                    "flex-start" => Some(AlignSelf::FLEX_START),
                    "start" => Some(AlignSelf::START),
                    "flex-end" => Some(AlignSelf::FLEX_END),
                    "end" => Some(AlignSelf::END),
                    "center" => Some(AlignSelf::CENTER),
                    "baseline" => Some(AlignSelf::BASELINE),
                    _ => self.taffy.align_self,
                };
            }
            "align-content" => {
                self.taffy.align_content = match lower.as_str() {
                    "flex-start" => AlignContent::FLEX_START,
                    "start" => AlignContent::START,
                    "flex-end" => AlignContent::FLEX_END,
                    "end" => AlignContent::END,
                    "center" => AlignContent::CENTER,
                    "stretch" => AlignContent::STRETCH,
                    "space-between" => AlignContent::SPACE_BETWEEN,
                    "space-around" => AlignContent::SPACE_AROUND,
                    _ => self.taffy.align_content,
                };
            }
            "flex-grow" => {
                if let Some(g) = parse_css_number(&lower) {
                    self.taffy.flex_grow = g;
                }
            }
            "flex-shrink" => {
                if let Some(s) = parse_css_number(&lower) {
                    self.taffy.flex_shrink = s;
                }
            }
            "flex-basis" => {
                if let Some(b) = parse_dim_or_auto(val, fs, vw, vh) {
                    self.taffy.flex_basis = b;
                }
            }
            "flex" => {
                if lower == "none" {
                    self.taffy.flex_grow = 0.0;
                    self.taffy.flex_shrink = 0.0;
                    self.taffy.flex_basis = Dimension::auto();
                } else if lower == "auto" {
                    self.taffy.flex_grow = 1.0;
                    self.taffy.flex_shrink = 1.0;
                    self.taffy.flex_basis = Dimension::auto();
                } else {
                    let toks: Vec<&str> = val.split_whitespace().collect();
                    if toks.len() == 1 {
                        if let Some(n) = parse_css_number(toks[0]) {
                            self.taffy.flex_grow = n;
                            self.taffy.flex_shrink = 1.0;
                            self.taffy.flex_basis = Dimension::length(0.0);
                        } else if let Some(b) = parse_dim_or_auto(toks[0], fs, vw, vh) {
                            self.taffy.flex_grow = 1.0;
                            self.taffy.flex_shrink = 1.0;
                            self.taffy.flex_basis = b;
                        }
                    } else if toks.len() >= 2 {
                        if let Some(g) = parse_css_number(toks[0]) {
                            self.taffy.flex_grow = g;
                        }
                        if let Some(s) = parse_css_number(toks[1]) {
                            self.taffy.flex_shrink = s;
                        }
                        if let Some(b) = toks.get(2).and_then(|t| parse_dim_or_auto(t, fs, vw, vh))
                        {
                            self.taffy.flex_basis = b;
                        }
                    }
                }
            }
            "gap" => {
                let vals: Vec<_> = val
                    .split_whitespace()
                    .filter_map(|t| parse_lp(t, fs, vw, vh))
                    .collect();
                if vals.len() == 1 {
                    self.taffy.gap = Size {
                        width: vals[0],
                        height: vals[0],
                    };
                } else if vals.len() >= 2 {
                    self.taffy.gap = Size {
                        width: vals[1],
                        height: vals[0],
                    };
                }
            }
            "row-gap" => {
                if let Some(d) = parse_lp(val, fs, vw, vh) {
                    self.taffy.gap.height = d;
                }
            }
            "column-gap" => {
                if let Some(d) = parse_lp(val, fs, vw, vh) {
                    self.taffy.gap.width = d;
                }
            }
            "overflow" | "overflow-x" | "overflow-y" => {
                let hidden = matches!(lower.as_str(), "hidden" | "clip");
                if key == "overflow" {
                    self.overflow_hidden = hidden;
                } else {
                    self.overflow_hidden |= hidden;
                }
                let ov = if hidden {
                    Overflow::Hidden
                } else {
                    Overflow::Visible
                };
                match key {
                    "overflow" => self.taffy.overflow = taffy::geometry::Point { x: ov, y: ov },
                    "overflow-x" => self.taffy.overflow.x = ov,
                    _ => self.taffy.overflow.y = ov,
                }
            }
            "z-index" => {
                if let Ok(z) = lower.parse::<i32>() {
                    self.z_index = z;
                }
            }
            "visibility" => {
                self.visibility = if lower == "hidden" || lower == "collapse" {
                    VisibilityMode::Hidden
                } else {
                    VisibilityMode::Visible
                };
            }
            "background-color" => {
                if let Some(c) = parse_color(val, self.color) {
                    self.background_color = c;
                }
            }
            "background" => {
                if lower == "none" || lower == "transparent" {
                    self.background_color = CssColor::TRANSPARENT;
                    self.background_image = BackgroundImage::None;
                } else if let Some(lg) = parse_linear_gradient(val, self.color, fs, vw, vh) {
                    self.background_image = BackgroundImage::Linear(lg);
                } else if let Some(rg) = parse_radial_gradient(val, self.color, fs, vw, vh) {
                    self.background_image = BackgroundImage::Radial(rg);
                } else if let Some(c) = parse_color(val, self.color) {
                    self.background_color = c;
                    self.background_image = BackgroundImage::None;
                }
            }
            "background-image" => {
                if lower == "none" {
                    self.background_image = BackgroundImage::None;
                } else if let Some(lg) = parse_linear_gradient(val, self.color, fs, vw, vh) {
                    self.background_image = BackgroundImage::Linear(lg);
                } else if let Some(rg) = parse_radial_gradient(val, self.color, fs, vw, vh) {
                    self.background_image = BackgroundImage::Radial(rg);
                }
            }
            "border" => {
                if let Some(side) = parse_border_side(val, self.color, fs, vw, vh) {
                    self.border_top = side;
                    self.border_right = side;
                    self.border_bottom = side;
                    self.border_left = side;
                    let w = LengthPercentage::length(side.width);
                    self.taffy.border = Rect {
                        top: w,
                        right: w,
                        bottom: w,
                        left: w,
                    };
                }
            }
            "border-width" => {
                let vals: Vec<_> = val
                    .split_whitespace()
                    .filter_map(|t| parse_length_px(t, fs, vw, vh))
                    .collect();
                if let Some([t, r, b, l]) = expand_4_sides(&vals) {
                    self.border_top.width = t;
                    self.border_right.width = r;
                    self.border_bottom.width = b;
                    self.border_left.width = l;
                    self.taffy.border = Rect {
                        top: LengthPercentage::length(t),
                        right: LengthPercentage::length(r),
                        bottom: LengthPercentage::length(b),
                        left: LengthPercentage::length(l),
                    };
                }
            }
            "border-color" => {
                let vals: Vec<_> = split_top_level_ws(val)
                    .into_iter()
                    .filter_map(|t| parse_color(t, self.color))
                    .collect();
                if let Some([t, r, b, l]) = expand_4_sides(&vals) {
                    self.border_top.color = t;
                    self.border_right.color = r;
                    self.border_bottom.color = b;
                    self.border_left.color = l;
                }
            }
            "border-top" => {
                if let Some(side) = parse_border_side(val, self.color, fs, vw, vh) {
                    self.border_top = side;
                    self.taffy.border.top = LengthPercentage::length(side.width);
                }
            }
            "border-right" => {
                if let Some(side) = parse_border_side(val, self.color, fs, vw, vh) {
                    self.border_right = side;
                    self.taffy.border.right = LengthPercentage::length(side.width);
                }
            }
            "border-bottom" => {
                if let Some(side) = parse_border_side(val, self.color, fs, vw, vh) {
                    self.border_bottom = side;
                    self.taffy.border.bottom = LengthPercentage::length(side.width);
                }
            }
            "border-left" => {
                if let Some(side) = parse_border_side(val, self.color, fs, vw, vh) {
                    self.border_left = side;
                    self.taffy.border.left = LengthPercentage::length(side.width);
                }
            }
            "border-top-width" => {
                if let Some(w) = parse_length_px(val, fs, vw, vh) {
                    self.border_top.width = w.max(0.0);
                    self.taffy.border.top = LengthPercentage::length(w.max(0.0));
                }
            }
            "border-right-width" => {
                if let Some(w) = parse_length_px(val, fs, vw, vh) {
                    self.border_right.width = w.max(0.0);
                    self.taffy.border.right = LengthPercentage::length(w.max(0.0));
                }
            }
            "border-bottom-width" => {
                if let Some(w) = parse_length_px(val, fs, vw, vh) {
                    self.border_bottom.width = w.max(0.0);
                    self.taffy.border.bottom = LengthPercentage::length(w.max(0.0));
                }
            }
            "border-left-width" => {
                if let Some(w) = parse_length_px(val, fs, vw, vh) {
                    self.border_left.width = w.max(0.0);
                    self.taffy.border.left = LengthPercentage::length(w.max(0.0));
                }
            }
            "border-top-color" => {
                if let Some(c) = parse_color(val, self.color) {
                    self.border_top.color = c;
                }
            }
            "border-right-color" => {
                if let Some(c) = parse_color(val, self.color) {
                    self.border_right.color = c;
                }
            }
            "border-bottom-color" => {
                if let Some(c) = parse_color(val, self.color) {
                    self.border_bottom.color = c;
                }
            }
            "border-left-color" => {
                if let Some(c) = parse_color(val, self.color) {
                    self.border_left.color = c;
                }
            }
            "border-radius" => {
                let before_slash = val.split('/').next().unwrap_or(val);
                let vals: Vec<_> = before_slash
                    .split_whitespace()
                    .filter_map(|t| parse_length_percent(t, fs, vw, vh))
                    .collect();
                if let Some(arr) = expand_4_sides(&vals) {
                    self.border_radii = arr;
                }
            }
            "border-top-left-radius" => {
                if let Some(lp) = parse_length_percent(val, fs, vw, vh) {
                    self.border_radii[0] = lp;
                }
            }
            "border-top-right-radius" => {
                if let Some(lp) = parse_length_percent(val, fs, vw, vh) {
                    self.border_radii[1] = lp;
                }
            }
            "border-bottom-right-radius" => {
                if let Some(lp) = parse_length_percent(val, fs, vw, vh) {
                    self.border_radii[2] = lp;
                }
            }
            "border-bottom-left-radius" => {
                if let Some(lp) = parse_length_percent(val, fs, vw, vh) {
                    self.border_radii[3] = lp;
                }
            }
            "box-shadow" => {
                self.box_shadows = parse_box_shadows(val, self.color, fs, vw, vh);
            }
            "text-shadow" => {
                self.text_shadows = parse_text_shadows(val, self.color, fs, vw, vh);
            }
            "opacity" => {
                if let Some(o) = parse_css_number(&lower) {
                    self.opacity = o.clamp(0.0, 1.0);
                }
            }
            "transform" => {
                self.transforms = crate::effects_parse::parse_transforms_ext(val, fs, vw, vh);
            }
            "transform-origin" => {
                self.transform_origin = parse_origin(val, fs, vw, vh);
            }
            "translate" => {
                if lower == "none" {
                    self.standalone_translate = None;
                } else {
                    let toks = split_top_level_ws(val);
                    if let Some(tx) = toks
                        .first()
                        .and_then(|t| parse_length_percent(t, fs, vw, vh))
                    {
                        let ty = toks
                            .get(1)
                            .and_then(|t| parse_length_percent(t, fs, vw, vh))
                            .unwrap_or(LengthPercent::Px(0.0));
                        let tz = toks
                            .get(2)
                            .and_then(|t| parse_length_px(t, fs, vw, vh))
                            .unwrap_or(0.0);
                        self.standalone_translate = Some((tx, ty, tz));
                    }
                }
            }
            "rotate" => {
                if lower == "none" {
                    self.standalone_rotate = None;
                } else {
                    let toks = split_top_level_ws(val);
                    if toks.len() == 1 {
                        if let Some(deg) = parse_angle_deg(toks[0]) {
                            self.standalone_rotate = Some(TransformOp::RotateDeg(deg));
                        }
                    } else if toks.len() == 2 {
                        if let Some(deg) = parse_angle_deg(toks[1]) {
                            self.standalone_rotate = match toks[0].to_ascii_lowercase().as_str() {
                                "x" => Some(TransformOp::RotateXDeg(deg)),
                                "y" => Some(TransformOp::RotateYDeg(deg)),
                                "z" => Some(TransformOp::RotateZDeg(deg)),
                                _ => self.standalone_rotate,
                            };
                        }
                    } else if toks.len() == 4 {
                        if let (Some(x), Some(y), Some(z), Some(deg)) = (
                            parse_css_number(toks[0]),
                            parse_css_number(toks[1]),
                            parse_css_number(toks[2]),
                            parse_angle_deg(toks[3]),
                        ) {
                            self.standalone_rotate = Some(TransformOp::Rotate3d(x, y, z, deg));
                        }
                    }
                }
            }
            "scale" => {
                if lower == "none" {
                    self.standalone_scale = None;
                } else {
                    let toks = split_top_level_ws(val);
                    if let Some(sx) = toks.first().and_then(|t| parse_css_number(t)) {
                        let sy = toks.get(1).and_then(|t| parse_css_number(t)).unwrap_or(sx);
                        let sz = toks.get(2).and_then(|t| parse_css_number(t)).unwrap_or(1.0);
                        self.standalone_scale = Some((sx, sy, sz));
                    }
                }
            }
            "perspective" => {
                self.perspective = if lower == "none" {
                    None
                } else {
                    parse_length_px(val, fs, vw, vh).filter(|&d| d > 0.0)
                };
            }
            "perspective-origin" => {
                self.perspective_origin = parse_origin(val, fs, vw, vh);
            }
            "filter" => {
                self.filters = crate::effects_parse::parse_filter_list(val, self.color, fs, vw, vh);
            }
            "backdrop-filter" | "-webkit-backdrop-filter" => {
                self.backdrop_filters =
                    crate::effects_parse::parse_filter_list(val, self.color, fs, vw, vh);
            }
            "mix-blend-mode" => {
                self.mix_blend_mode = crate::effects_parse::parse_blend_mode(val);
            }
            "clip-path" => {
                self.clip_path = crate::effects_parse::parse_clip_path(val, fs, vw, vh);
            }
            "mask-image" | "-webkit-mask-image" => {
                self.mask_image =
                    crate::effects_parse::parse_mask_image(val, self.color, fs, vw, vh);
            }
            "background-clip" | "-webkit-background-clip" => {
                self.background_clip_text = lower == "text";
            }
            "-webkit-text-fill-color" | "text-fill-color" => {
                if lower == "transparent" {
                    self.color = CssColor::TRANSPARENT;
                } else if let Some(c) = parse_color(val, self.color) {
                    self.color = c;
                }
            }
            "grid-template-columns" => {
                if let Some(tracks) = crate::grid_parse::parse_grid_template_tracks(val, fs, vw, vh)
                {
                    self.taffy.grid_template_columns = tracks;
                }
            }
            "grid-template-rows" => {
                if let Some(tracks) = crate::grid_parse::parse_grid_template_tracks(val, fs, vw, vh)
                {
                    self.taffy.grid_template_rows = tracks;
                }
            }
            "grid-auto-columns" => {
                self.taffy.grid_auto_columns =
                    crate::grid_parse::parse_grid_auto_tracks(val, fs, vw, vh);
            }
            "grid-auto-rows" => {
                self.taffy.grid_auto_rows =
                    crate::grid_parse::parse_grid_auto_tracks(val, fs, vw, vh);
            }
            "grid-auto-flow" => {
                if let Some(flow) = crate::grid_parse::parse_grid_auto_flow(val) {
                    self.taffy.grid_auto_flow = flow;
                }
            }
            "grid-column" => {
                if let Some(gl) = crate::grid_parse::parse_grid_line(val) {
                    self.taffy.grid_column = gl;
                }
            }
            "grid-column-start" => {
                if let Some(gp) = crate::grid_parse::parse_grid_placement(val) {
                    self.taffy.grid_column.start = gp;
                }
            }
            "grid-column-end" => {
                if let Some(gp) = crate::grid_parse::parse_grid_placement(val) {
                    self.taffy.grid_column.end = gp;
                }
            }
            "grid-row" => {
                if let Some(gl) = crate::grid_parse::parse_grid_line(val) {
                    self.taffy.grid_row = gl;
                }
            }
            "grid-row-start" => {
                if let Some(gp) = crate::grid_parse::parse_grid_placement(val) {
                    self.taffy.grid_row.start = gp;
                }
            }
            "grid-row-end" => {
                if let Some(gp) = crate::grid_parse::parse_grid_placement(val) {
                    self.taffy.grid_row.end = gp;
                }
            }
            "justify-items" => {
                if let Some(ji) = crate::grid_parse::parse_align_items_kw(val) {
                    self.taffy.justify_items = ji;
                }
            }
            "justify-self" => {
                if let Some(js) = crate::grid_parse::parse_align_self_kw(val) {
                    self.taffy.justify_self = js;
                }
            }
            "place-items" => {
                let toks: Vec<&str> = val.split_whitespace().collect();
                if let Some(ai) = toks
                    .first()
                    .and_then(|t| crate::grid_parse::parse_align_items_kw(t))
                {
                    self.taffy.align_items = ai;
                    let ji = toks
                        .get(1)
                        .and_then(|t| crate::grid_parse::parse_align_items_kw(t))
                        .unwrap_or(ai);
                    self.taffy.justify_items = ji;
                }
            }
            "place-content" => {
                let toks: Vec<&str> = val.split_whitespace().collect();
                if let Some(ac) = toks
                    .first()
                    .and_then(|t| crate::grid_parse::parse_align_content_kw(t))
                {
                    self.taffy.align_content = ac;
                    let jc = toks
                        .get(1)
                        .and_then(|t| crate::grid_parse::parse_justify_content_kw(t))
                        .or_else(|| crate::grid_parse::parse_justify_content_kw(toks[0]));
                    if let Some(jc_val) = jc {
                        self.taffy.justify_content = jc_val;
                    }
                }
            }
            "place-self" => {
                let toks: Vec<&str> = val.split_whitespace().collect();
                if let Some(als) = toks
                    .first()
                    .and_then(|t| crate::grid_parse::parse_align_self_kw(t))
                {
                    self.taffy.align_self = als;
                    let js = toks
                        .get(1)
                        .and_then(|t| crate::grid_parse::parse_align_self_kw(t))
                        .unwrap_or(als);
                    self.taffy.justify_self = js;
                }
            }
            "font-family" => {
                self.font_family = parse_font_family_list(val);
            }
            "font-weight" => {
                self.font_weight = match lower.as_str() {
                    "normal" => 400,
                    "bold" => 700,
                    "lighter" => 300,
                    "bolder" => 800,
                    _ => lower
                        .parse::<u16>()
                        .unwrap_or(self.font_weight)
                        .clamp(100, 900),
                };
            }
            "font-style" => {
                self.font_style = match lower.as_str() {
                    "italic" => FontSlantMode::Italic,
                    "oblique" => FontSlantMode::Oblique,
                    _ => FontSlantMode::Normal,
                };
            }
            "line-height" => {
                if lower == "normal" {
                    self.line_height = LineHeight::Normal;
                } else if let Some(p) = lower.strip_suffix('%') {
                    if let Some(pct) = parse_css_number(p) {
                        self.line_height = LineHeight::Number((pct * 0.01).max(0.0));
                    }
                } else if let Some(px) = lower.strip_suffix("px").and_then(parse_css_number) {
                    self.line_height = LineHeight::Px(px.max(0.0));
                } else if let Some(n) = parse_css_number(&lower) {
                    self.line_height = LineHeight::Number(n.max(0.0));
                }
            }
            "letter-spacing" => {
                if lower == "normal" {
                    self.letter_spacing = 0.0;
                } else if let Some(px) = parse_length_px(val, fs, vw, vh) {
                    self.letter_spacing = px;
                }
            }
            "text-align" => {
                self.text_align = match lower.as_str() {
                    "center" => TextAlignMode::Center,
                    "right" | "end" => TextAlignMode::Right,
                    "justify" => TextAlignMode::Justify,
                    _ => TextAlignMode::Left,
                };
            }
            "white-space" => {
                self.white_space = match lower.as_str() {
                    "nowrap" => WhiteSpaceMode::Nowrap,
                    "pre" => WhiteSpaceMode::Pre,
                    "pre-wrap" => WhiteSpaceMode::PreWrap,
                    _ => WhiteSpaceMode::Normal,
                };
            }
            "text-transform" => {
                self.text_transform = match lower.as_str() {
                    "uppercase" => TextTransformMode::Uppercase,
                    "lowercase" => TextTransformMode::Lowercase,
                    "capitalize" => TextTransformMode::Capitalize,
                    _ => TextTransformMode::None,
                };
            }
            "text-decoration" | "text-decoration-line" => {
                if lower.contains("underline") {
                    self.text_decoration = TextDecorationMode::Underline;
                } else if lower.contains("line-through") {
                    self.text_decoration = TextDecorationMode::LineThrough;
                } else if lower == "none" {
                    self.text_decoration = TextDecorationMode::None;
                }
            }
            "object-fit" => {
                self.object_fit = match lower.as_str() {
                    "contain" => ObjectFit::Contain,
                    "cover" => ObjectFit::Cover,
                    "none" => ObjectFit::None,
                    "scale-down" => ObjectFit::ScaleDown,
                    _ => ObjectFit::Fill,
                };
            }
            "object-position" => {
                self.object_position = parse_origin(val, fs, vw, vh);
            }
            _ => {}
        }
    }
}
