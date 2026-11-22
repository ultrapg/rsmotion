use crate::style::{
    expand_4_sides, parse_angle_deg, parse_color, parse_css_number, parse_length_percent,
    parse_length_px, parse_linear_gradient, parse_origin, parse_radial_gradient, split_top_level,
    split_top_level_ws, CssColor, LengthPercent, LinearGradient, RadialGradient, TransformOp,
};

#[derive(Debug, Clone, PartialEq)]
pub enum CssFilter {
    Blur(f32),
    Brightness(f32),
    Contrast(f32),
    Grayscale(f32),
    Sepia(f32),
    Saturate(f32),
    HueRotateDeg(f32),
    Invert(f32),
    Opacity(f32),
    DropShadow {
        offset_x: f32,
        offset_y: f32,
        blur_radius: f32,
        color: CssColor,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum CssBlendMode {
    #[default]
    Normal,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    ColorDodge,
    ColorBurn,
    HardLight,
    SoftLight,
    Difference,
    Exclusion,
    Hue,
    Saturation,
    Color,
    Luminosity,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ClipRadius {
    LengthPercent(LengthPercent),
    ClosestSide,
    FarthestSide,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub enum CssClipPath {
    #[default]
    None,
    Inset {
        top: LengthPercent,
        right: LengthPercent,
        bottom: LengthPercent,
        left: LengthPercent,
        radii: [LengthPercent; 4],
    },
    Circle {
        radius: ClipRadius,
        cx: LengthPercent,
        cy: LengthPercent,
    },
    Ellipse {
        rx: ClipRadius,
        ry: ClipRadius,
        cx: LengthPercent,
        cy: LengthPercent,
    },
    Polygon {
        even_odd: bool,
        points: Vec<(LengthPercent, LengthPercent)>,
    },
    Path {
        even_odd: bool,
        d: String,
    },
}

#[derive(Debug, Clone, PartialEq, Default)]
pub enum CssMaskImage {
    #[default]
    None,
    Linear(LinearGradient),
    Radial(RadialGradient),
    Url(String),
}

fn find_matching_paren(s: &str, open_idx: usize) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut depth = 0usize;
    let mut quote: Option<u8> = None;
    let mut i = open_idx;
    while i < bytes.len() {
        let b = bytes[i];
        if let Some(q) = quote {
            if b == q {
                quote = None;
            }
            i += 1;
            continue;
        }
        match b {
            b'"' | b'\'' => quote = Some(b),
            b'(' => depth += 1,
            b')' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

fn parse_ratio_or_pct(raw: &str, default_val: f32) -> Option<f32> {
    let t = raw.trim();
    if t.is_empty() {
        return Some(default_val);
    }
    if let Some(p) = t.strip_suffix('%') {
        p.trim().parse::<f32>().ok().map(|v| (v * 0.01).max(0.0))
    } else {
        t.parse::<f32>().ok().map(|v| v.max(0.0))
    }
}

pub fn parse_filter_list(
    raw: &str,
    current_color: CssColor,
    fs: f32,
    vw: f32,
    vh: f32,
) -> Vec<CssFilter> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("none") {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut rest = trimmed;
    while let Some(open) = rest.find('(') {
        let name = rest[..open].trim().to_ascii_lowercase();
        let Some(close) = find_matching_paren(rest, open) else {
            break;
        };
        let arg = rest[open + 1..close].trim();
        rest = &rest[close + 1..];

        match name.as_str() {
            "blur" => {
                let r = if arg.is_empty() {
                    0.0
                } else {
                    parse_length_px(arg, fs, vw, vh).unwrap_or(0.0).max(0.0)
                };
                out.push(CssFilter::Blur(r));
            }
            "brightness" => {
                if let Some(v) = parse_ratio_or_pct(arg, 1.0) {
                    out.push(CssFilter::Brightness(v));
                }
            }
            "contrast" => {
                if let Some(v) = parse_ratio_or_pct(arg, 1.0) {
                    out.push(CssFilter::Contrast(v));
                }
            }
            "grayscale" => {
                if let Some(v) = parse_ratio_or_pct(arg, 1.0) {
                    out.push(CssFilter::Grayscale(v.clamp(0.0, 1.0)));
                }
            }
            "sepia" => {
                if let Some(v) = parse_ratio_or_pct(arg, 1.0) {
                    out.push(CssFilter::Sepia(v.clamp(0.0, 1.0)));
                }
            }
            "saturate" => {
                if let Some(v) = parse_ratio_or_pct(arg, 1.0) {
                    out.push(CssFilter::Saturate(v));
                }
            }
            "hue-rotate" => {
                let deg = if arg.is_empty() {
                    Some(0.0)
                } else {
                    parse_angle_deg(arg)
                };
                if let Some(d) = deg {
                    out.push(CssFilter::HueRotateDeg(d));
                }
            }
            "invert" => {
                if let Some(v) = parse_ratio_or_pct(arg, 1.0) {
                    out.push(CssFilter::Invert(v.clamp(0.0, 1.0)));
                }
            }
            "opacity" => {
                if let Some(v) = parse_ratio_or_pct(arg, 1.0) {
                    out.push(CssFilter::Opacity(v.clamp(0.0, 1.0)));
                }
            }
            "drop-shadow" => {
                let toks = split_top_level_ws(arg);
                let mut color = current_color;
                let mut lengths = Vec::new();
                for tok in toks {
                    if let Some(c) = parse_color(tok, current_color) {
                        color = c;
                    } else if let Some(px) = parse_length_px(tok, fs, vw, vh) {
                        lengths.push(px);
                    }
                }
                if lengths.len() >= 2 {
                    out.push(CssFilter::DropShadow {
                        offset_x: lengths[0],
                        offset_y: lengths[1],
                        blur_radius: lengths.get(2).copied().unwrap_or(0.0).max(0.0),
                        color,
                    });
                }
            }
            _ => {}
        }
    }
    out
}

pub fn parse_blend_mode(raw: &str) -> CssBlendMode {
    match raw.trim().to_ascii_lowercase().as_str() {
        "multiply" => CssBlendMode::Multiply,
        "screen" => CssBlendMode::Screen,
        "overlay" => CssBlendMode::Overlay,
        "darken" => CssBlendMode::Darken,
        "lighten" => CssBlendMode::Lighten,
        "color-dodge" => CssBlendMode::ColorDodge,
        "color-burn" => CssBlendMode::ColorBurn,
        "hard-light" => CssBlendMode::HardLight,
        "soft-light" => CssBlendMode::SoftLight,
        "difference" => CssBlendMode::Difference,
        "exclusion" => CssBlendMode::Exclusion,
        "hue" => CssBlendMode::Hue,
        "saturation" => CssBlendMode::Saturation,
        "color" => CssBlendMode::Color,
        "luminosity" => CssBlendMode::Luminosity,
        _ => CssBlendMode::Normal,
    }
}

fn parse_clip_radius(tok: &str, fs: f32, vw: f32, vh: f32) -> Option<ClipRadius> {
    match tok.trim().to_ascii_lowercase().as_str() {
        "closest-side" => Some(ClipRadius::ClosestSide),
        "farthest-side" => Some(ClipRadius::FarthestSide),
        _ => parse_length_percent(tok, fs, vw, vh).map(ClipRadius::LengthPercent),
    }
}

pub fn parse_clip_path(raw: &str, fs: f32, vw: f32, vh: f32) -> CssClipPath {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("none") {
        return CssClipPath::None;
    }
    let Some(open) = trimmed.find('(') else {
        return CssClipPath::None;
    };
    let Some(close) = find_matching_paren(trimmed, open) else {
        return CssClipPath::None;
    };
    let name = trimmed[..open].trim().to_ascii_lowercase();
    let inner = trimmed[open + 1..close].trim();

    match name.as_str() {
        "inset" => {
            let (rect_str, round_str) = if let Some(idx) = inner.to_ascii_lowercase().find("round")
            {
                (&inner[..idx], Some(inner[idx + 5..].trim()))
            } else {
                (inner, None)
            };
            let sides: Vec<LengthPercent> = split_top_level_ws(rect_str)
                .into_iter()
                .filter_map(|t| parse_length_percent(t, fs, vw, vh))
                .collect();
            let [top, right, bottom, left] =
                expand_4_sides(&sides).unwrap_or([LengthPercent::Px(0.0); 4]);
            let radii = if let Some(rs) = round_str {
                let r_before = rs.split('/').next().unwrap_or(rs);
                let rvals: Vec<LengthPercent> = split_top_level_ws(r_before)
                    .into_iter()
                    .filter_map(|t| parse_length_percent(t, fs, vw, vh))
                    .collect();
                expand_4_sides(&rvals).unwrap_or([LengthPercent::Px(0.0); 4])
            } else {
                [LengthPercent::Px(0.0); 4]
            };
            CssClipPath::Inset {
                top,
                right,
                bottom,
                left,
                radii,
            }
        }
        "circle" => {
            let (rad_str, at_str) = if let Some((l, r)) = split_once_at_kw(inner) {
                (l.trim(), Some(r.trim()))
            } else {
                (inner, None)
            };
            let radius = if rad_str.is_empty() {
                ClipRadius::ClosestSide
            } else {
                parse_clip_radius(rad_str, fs, vw, vh).unwrap_or(ClipRadius::ClosestSide)
            };
            let (cx, cy) = at_str
                .map(|a| parse_origin(a, fs, vw, vh))
                .unwrap_or((LengthPercent::Percent(50.0), LengthPercent::Percent(50.0)));
            CssClipPath::Circle { radius, cx, cy }
        }
        "ellipse" => {
            let (rad_str, at_str) = if let Some((l, r)) = split_once_at_kw(inner) {
                (l.trim(), Some(r.trim()))
            } else {
                (inner, None)
            };
            let rtoks = split_top_level_ws(rad_str);
            let rx = rtoks
                .first()
                .and_then(|t| parse_clip_radius(t, fs, vw, vh))
                .unwrap_or(ClipRadius::ClosestSide);
            let ry = rtoks
                .get(1)
                .and_then(|t| parse_clip_radius(t, fs, vw, vh))
                .unwrap_or(ClipRadius::ClosestSide);
            let (cx, cy) = at_str
                .map(|a| parse_origin(a, fs, vw, vh))
                .unwrap_or((LengthPercent::Percent(50.0), LengthPercent::Percent(50.0)));
            CssClipPath::Ellipse { rx, ry, cx, cy }
        }
        "polygon" => {
            let args = split_top_level(inner, ',');
            let mut even_odd = false;
            let mut start = 0usize;
            if let Some(first) = args.first() {
                let fl = first.trim().to_ascii_lowercase();
                if fl == "evenodd" {
                    even_odd = true;
                    start = 1;
                } else if fl == "nonzero" {
                    start = 1;
                }
            }
            let mut points = Vec::new();
            for pair in &args[start..] {
                let coords = split_top_level_ws(pair);
                if coords.len() >= 2 {
                    if let (Some(x), Some(y)) = (
                        parse_length_percent(coords[0], fs, vw, vh),
                        parse_length_percent(coords[1], fs, vw, vh),
                    ) {
                        points.push((x, y));
                    }
                }
            }
            if points.is_empty() {
                CssClipPath::None
            } else {
                CssClipPath::Polygon { even_odd, points }
            }
        }
        "path" => {
            let args = split_top_level(inner, ',');
            let (even_odd, path_tok) = if args.len() >= 2 {
                (
                    args[0].trim().eq_ignore_ascii_case("evenodd"),
                    args[1].trim(),
                )
            } else if let Some(first) = args.first() {
                (false, first.trim())
            } else {
                return CssClipPath::None;
            };
            let d = path_tok.trim_matches(|c| c == '"' || c == '\'').to_string();
            if d.is_empty() {
                CssClipPath::None
            } else {
                CssClipPath::Path { even_odd, d }
            }
        }
        _ => CssClipPath::None,
    }
}

fn split_once_at_kw(s: &str) -> Option<(&str, &str)> {
    let lower = s.to_ascii_lowercase();
    if let Some(after) = lower.strip_prefix("at ") {
        let _ = after;
        return Some(("", s[3..].trim()));
    }
    lower.find(" at ").map(|idx| (&s[..idx], &s[idx + 4..]))
}

pub fn parse_mask_image(
    raw: &str,
    current_color: CssColor,
    fs: f32,
    vw: f32,
    vh: f32,
) -> CssMaskImage {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("none") {
        return CssMaskImage::None;
    }
    if let Some(lg) = parse_linear_gradient(trimmed, current_color, fs, vw, vh) {
        return CssMaskImage::Linear(lg);
    }
    if let Some(rg) = parse_radial_gradient(trimmed, current_color, fs, vw, vh) {
        return CssMaskImage::Radial(rg);
    }
    let lower = trimmed.to_ascii_lowercase();
    if lower.starts_with("url(") && trimmed.ends_with(')') {
        let url = trimmed[4..trimmed.len() - 1]
            .trim()
            .trim_matches(|c| c == '"' || c == '\'')
            .to_string();
        if !url.is_empty() {
            return CssMaskImage::Url(url);
        }
    }
    CssMaskImage::None
}

/// Parses 2D + 3D CSS `transform` lists including `perspective()`, `rotateX/Y/Z/3d`, `translate3d/Z`, `scale3d/Z`, `matrix3d`.
pub fn parse_transforms_ext(raw: &str, font_size: f32, vw: f32, vh: f32) -> Vec<TransformOp> {
    let s = raw.trim();
    if s.is_empty() || s.eq_ignore_ascii_case("none") {
        return Vec::new();
    }

    let mut out = Vec::new();
    let mut rest = s;
    while let Some(open) = rest.find('(') {
        let name = rest[..open].trim().to_ascii_lowercase();
        let Some(close) = find_matching_paren(rest, open) else {
            break;
        };
        let arg_str = &rest[open + 1..close];
        rest = &rest[close + 1..];
        let args = split_top_level(arg_str, ',');

        match name.as_str() {
            "translate" => {
                if let Some(tx) = args
                    .first()
                    .and_then(|a| parse_length_percent(a, font_size, vw, vh))
                {
                    let ty = args
                        .get(1)
                        .and_then(|a| parse_length_percent(a, font_size, vw, vh))
                        .unwrap_or(LengthPercent::Px(0.0));
                    out.push(TransformOp::Translate(tx, ty));
                }
            }
            "translatex" => {
                if let Some(tx) = args
                    .first()
                    .and_then(|a| parse_length_percent(a, font_size, vw, vh))
                {
                    out.push(TransformOp::TranslateX(tx));
                }
            }
            "translatey" => {
                if let Some(ty) = args
                    .first()
                    .and_then(|a| parse_length_percent(a, font_size, vw, vh))
                {
                    out.push(TransformOp::TranslateY(ty));
                }
            }
            "translatez" => {
                if let Some(tz) = args
                    .first()
                    .and_then(|a| parse_length_px(a, font_size, vw, vh))
                {
                    out.push(TransformOp::TranslateZ(tz));
                }
            }
            "translate3d" if args.len() == 3 => {
                if let (Some(tx), Some(ty), Some(tz)) = (
                    parse_length_percent(args[0], font_size, vw, vh),
                    parse_length_percent(args[1], font_size, vw, vh),
                    parse_length_px(args[2], font_size, vw, vh),
                ) {
                    out.push(TransformOp::Translate3d(tx, ty, tz));
                }
            }
            "scale" => {
                if let Some(sx) = args.first().and_then(|a| parse_css_number(a)) {
                    let sy = args.get(1).and_then(|a| parse_css_number(a)).unwrap_or(sx);
                    out.push(TransformOp::Scale(sx, sy));
                }
            }
            "scalex" => {
                if let Some(sx) = args.first().and_then(|a| parse_css_number(a)) {
                    out.push(TransformOp::Scale(sx, 1.0));
                }
            }
            "scaley" => {
                if let Some(sy) = args.first().and_then(|a| parse_css_number(a)) {
                    out.push(TransformOp::Scale(1.0, sy));
                }
            }
            "scalez" => {
                if let Some(sz) = args.first().and_then(|a| parse_css_number(a)) {
                    out.push(TransformOp::ScaleZ(sz));
                }
            }
            "scale3d" if args.len() == 3 => {
                if let (Some(sx), Some(sy), Some(sz)) = (
                    parse_css_number(args[0]),
                    parse_css_number(args[1]),
                    parse_css_number(args[2]),
                ) {
                    out.push(TransformOp::Scale3d(sx, sy, sz));
                }
            }
            "rotate" => {
                if let Some(deg) = args.first().and_then(|a| parse_angle_deg(a)) {
                    out.push(TransformOp::RotateDeg(deg));
                }
            }
            "rotatex" => {
                if let Some(deg) = args.first().and_then(|a| parse_angle_deg(a)) {
                    out.push(TransformOp::RotateXDeg(deg));
                }
            }
            "rotatey" => {
                if let Some(deg) = args.first().and_then(|a| parse_angle_deg(a)) {
                    out.push(TransformOp::RotateYDeg(deg));
                }
            }
            "rotatez" => {
                if let Some(deg) = args.first().and_then(|a| parse_angle_deg(a)) {
                    out.push(TransformOp::RotateZDeg(deg));
                }
            }
            "rotate3d" if args.len() == 4 => {
                if let (Some(x), Some(y), Some(z), Some(deg)) = (
                    parse_css_number(args[0]),
                    parse_css_number(args[1]),
                    parse_css_number(args[2]),
                    parse_angle_deg(args[3]),
                ) {
                    out.push(TransformOp::Rotate3d(x, y, z, deg));
                }
            }
            "perspective" => {
                if let Some(d) = args
                    .first()
                    .and_then(|a| parse_length_px(a, font_size, vw, vh))
                    .filter(|&d| d > 0.0)
                {
                    out.push(TransformOp::Perspective(d));
                }
            }
            "skewx" => {
                if let Some(deg) = args.first().and_then(|a| parse_angle_deg(a)) {
                    out.push(TransformOp::SkewDeg(deg, 0.0));
                }
            }
            "skewy" => {
                if let Some(deg) = args.first().and_then(|a| parse_angle_deg(a)) {
                    out.push(TransformOp::SkewDeg(0.0, deg));
                }
            }
            "skew" => {
                if let Some(dx) = args.first().and_then(|a| parse_angle_deg(a)) {
                    let dy = args.get(1).and_then(|a| parse_angle_deg(a)).unwrap_or(0.0);
                    out.push(TransformOp::SkewDeg(dx, dy));
                }
            }
            "matrix" if args.len() == 6 => {
                let nums: Option<Vec<f32>> = args.iter().map(|a| parse_css_number(a)).collect();
                if let Some(n) = nums {
                    out.push(TransformOp::Matrix([n[0], n[1], n[2], n[3], n[4], n[5]]));
                }
            }
            "matrix3d" if args.len() == 16 => {
                let nums: Option<Vec<f32>> = args.iter().map(|a| parse_css_number(a)).collect();
                if let Some(n) = nums {
                    let mut m = [0.0_f32; 16];
                    m.copy_from_slice(&n);
                    out.push(TransformOp::Matrix3d(m));
                }
            }
            _ => {}
        }
    }
    out
}
