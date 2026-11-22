use rsmotion_layout::{
    BackgroundImage, ClipRadius, ColorStop, ComputedStyle, CssBlendMode, CssClipPath, CssColor,
    CssFilter, LengthPercent, TransformOp,
};
use skia_safe::gradient::{
    shaders as gradient_shaders, Colors as GradientColors, Gradient, Interpolation,
};
use skia_safe::{
    color_filters, image_filters, BlendMode, Canvas, Color, Color4f, ImageFilter, Matrix, Path,
    PathBuilder, PathFillType, Point, RRect, Rect, Shader, TileMode, Vector, M44,
};

pub(crate) fn to_sk_color(c: CssColor) -> Color {
    Color::from_argb(c.a, c.r, c.g, c.b)
}

pub(crate) fn to_color4f(c: CssColor) -> Color4f {
    Color4f::from(to_sk_color(c))
}

pub(crate) fn to_sk_blend_mode(mode: CssBlendMode) -> BlendMode {
    match mode {
        CssBlendMode::Normal => BlendMode::SrcOver,
        CssBlendMode::Multiply => BlendMode::Multiply,
        CssBlendMode::Screen => BlendMode::Screen,
        CssBlendMode::Overlay => BlendMode::Overlay,
        CssBlendMode::Darken => BlendMode::Darken,
        CssBlendMode::Lighten => BlendMode::Lighten,
        CssBlendMode::ColorDodge => BlendMode::ColorDodge,
        CssBlendMode::ColorBurn => BlendMode::ColorBurn,
        CssBlendMode::HardLight => BlendMode::HardLight,
        CssBlendMode::SoftLight => BlendMode::SoftLight,
        CssBlendMode::Difference => BlendMode::Difference,
        CssBlendMode::Exclusion => BlendMode::Exclusion,
        CssBlendMode::Hue => BlendMode::Hue,
        CssBlendMode::Saturation => BlendMode::Saturation,
        CssBlendMode::Color => BlendMode::Color,
        CssBlendMode::Luminosity => BlendMode::Luminosity,
    }
}

pub(crate) fn build_rrect(
    rect: Rect,
    radii_lp: &[LengthPercent; 4],
    w: f32,
    h: f32,
    extra_spread: f32,
) -> RRect {
    let ref_dim = w.min(h);
    let mut r: [f32; 4] = [0.0; 4];
    for (i, lp) in radii_lp.iter().enumerate() {
        let base = match *lp {
            LengthPercent::Px(px) => px,
            LengthPercent::Percent(pct) => ref_dim * pct * 0.01,
        };
        r[i] = if base > 0.0 {
            (base + extra_spread).max(0.0)
        } else {
            0.0
        };
    }

    let rw = rect.width().max(0.0);
    let rh = rect.height().max(0.0);
    let mut scale = 1.0_f32;
    let pairs = [
        (r[0] + r[1], rw),
        (r[1] + r[2], rh),
        (r[2] + r[3], rw),
        (r[3] + r[0], rh),
    ];
    for (sum, side) in pairs {
        if sum > side && sum > 0.0 {
            scale = scale.min(side / sum);
        }
    }

    let vectors = [
        Vector::new(r[0] * scale, r[0] * scale),
        Vector::new(r[1] * scale, r[1] * scale),
        Vector::new(r[2] * scale, r[2] * scale),
        Vector::new(r[3] * scale, r[3] * scale),
    ];
    RRect::new_rect_radii(rect, &vectors)
}

fn resolve_gradient_stops(stops: &[ColorStop], grad_len_px: f32) -> (Vec<Color4f>, Vec<f32>) {
    let n = stops.len();
    let mut colors = Vec::with_capacity(n);
    let mut pos: Vec<Option<f32>> = Vec::with_capacity(n);

    for s in stops {
        colors.push(to_color4f(s.color));
        let p = s.pos.map(|lp| match lp {
            LengthPercent::Percent(pct) => (pct * 0.01).clamp(0.0, 1.0),
            LengthPercent::Px(px) => {
                if grad_len_px > 0.0 {
                    (px / grad_len_px).clamp(0.0, 1.0)
                } else {
                    0.0
                }
            }
        });
        pos.push(p);
    }

    if !pos.is_empty() {
        if pos[0].is_none() {
            pos[0] = Some(0.0);
        }
        if n > 1 && pos[n - 1].is_none() {
            pos[n - 1] = Some(1.0);
        }
    }

    let mut max_so_far = 0.0_f32;
    for v in pos.iter_mut().flatten() {
        if *v < max_so_far {
            *v = max_so_far;
        } else {
            max_so_far = *v;
        }
    }

    let mut i = 0;
    while i < n {
        if pos[i].is_some() {
            i += 1;
            continue;
        }
        let start_idx = i - 1;
        let start_val = pos[start_idx].unwrap_or(0.0);
        let mut end_idx = i + 1;
        while end_idx < n && pos[end_idx].is_none() {
            end_idx += 1;
        }
        let end_val = pos[end_idx].unwrap_or(1.0);
        let span = (end_idx - start_idx) as f32;
        for (offset, item) in pos[start_idx + 1..end_idx].iter_mut().enumerate() {
            let t = (offset + 1) as f32 / span;
            *item = Some(start_val + (end_val - start_val) * t);
        }
        i = end_idx;
    }

    let resolved: Vec<f32> = pos.into_iter().map(|p| p.unwrap_or(0.0)).collect();
    (colors, resolved)
}

pub(crate) fn build_background_shader(bg: &BackgroundImage, w: f32, h: f32) -> Option<Shader> {
    match bg {
        BackgroundImage::None => None,
        BackgroundImage::Linear(lg) => {
            let rad = lg.angle_deg.to_radians();
            let dx = rad.sin();
            let dy = -rad.cos();
            let half_len = 0.5 * (w * dx.abs() + h * dy.abs());
            let p0 = Point::new(w * 0.5 - dx * half_len, h * 0.5 - dy * half_len);
            let p1 = Point::new(w * 0.5 + dx * half_len, h * 0.5 + dy * half_len);
            let (colors, pos) = resolve_gradient_stops(&lg.stops, 2.0 * half_len);
            let gcolors = GradientColors::new(&colors, Some(&pos), TileMode::Clamp, None);
            let grad = Gradient::new(gcolors, Interpolation::default());
            gradient_shaders::linear_gradient((p0, p1), &grad, None)
        }
        BackgroundImage::Radial(rg) => {
            let cx = rg.cx.resolve(w);
            let cy = rg.cy.resolve(h);
            let fx = cx.max(w - cx);
            let fy = cy.max(h - cy);
            let radius = fx.hypot(fy).max(0.001);
            let (colors, pos) = resolve_gradient_stops(&rg.stops, radius);
            let gcolors = GradientColors::new(&colors, Some(&pos), TileMode::Clamp, None);
            let grad = Gradient::new(gcolors, Interpolation::default());
            let local_mat = if !rg.shape_circle && w > 0.0 && h > 0.0 && (fx - fy).abs() > 0.5 {
                let rx = fx * std::f32::consts::SQRT_2;
                let ry = fy * std::f32::consts::SQRT_2;
                let mut m = Matrix::new_identity();
                m.pre_translate((cx, cy));
                m.pre_scale((rx / radius, ry / radius), None);
                m.pre_translate((-cx, -cy));
                Some(m)
            } else {
                None
            };
            gradient_shaders::radial_gradient(
                (Point::new(cx, cy), radius),
                &grad,
                local_mat.as_ref(),
            )
        }
    }
}

fn w3c_filter_color_matrix(filter: &CssFilter) -> Option<[f32; 20]> {
    Some(match *filter {
        CssFilter::Brightness(b) => [
            b, 0.0, 0.0, 0.0, 0.0, 0.0, b, 0.0, 0.0, 0.0, 0.0, 0.0, b, 0.0, 0.0, 0.0, 0.0, 0.0,
            1.0, 0.0,
        ],
        CssFilter::Contrast(c) => {
            let t = 0.5 * (1.0 - c);
            [
                c, 0.0, 0.0, 0.0, t, 0.0, c, 0.0, 0.0, t, 0.0, 0.0, c, 0.0, t, 0.0, 0.0, 0.0, 1.0,
                0.0,
            ]
        }
        CssFilter::Grayscale(a) => {
            let u = 1.0 - a.clamp(0.0, 1.0);
            [
                0.2126 + 0.7874 * u,
                0.7152 - 0.7152 * u,
                0.0722 - 0.0722 * u,
                0.0,
                0.0,
                0.2126 - 0.2126 * u,
                0.7152 + 0.2848 * u,
                0.0722 - 0.0722 * u,
                0.0,
                0.0,
                0.2126 - 0.2126 * u,
                0.7152 - 0.7152 * u,
                0.0722 + 0.9278 * u,
                0.0,
                0.0,
                0.0,
                0.0,
                0.0,
                1.0,
                0.0,
            ]
        }
        CssFilter::Sepia(a) => {
            let amt = a.clamp(0.0, 1.0);
            let u = 1.0 - amt;
            [
                0.393 + 0.607 * u,
                0.769 * amt,
                0.189 * amt,
                0.0,
                0.0,
                0.349 * amt,
                0.686 + 0.314 * u,
                0.168 * amt,
                0.0,
                0.0,
                0.272 * amt,
                0.534 * amt,
                0.131 + 0.869 * u,
                0.0,
                0.0,
                0.0,
                0.0,
                0.0,
                1.0,
                0.0,
            ]
        }
        CssFilter::Saturate(s) => [
            0.213 + 0.787 * s,
            0.715 - 0.715 * s,
            0.072 - 0.072 * s,
            0.0,
            0.0,
            0.213 - 0.213 * s,
            0.715 + 0.285 * s,
            0.072 - 0.072 * s,
            0.0,
            0.0,
            0.213 - 0.213 * s,
            0.715 - 0.715 * s,
            0.072 + 0.928 * s,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            1.0,
            0.0,
        ],
        CssFilter::HueRotateDeg(deg) => {
            let rad = deg.to_radians();
            let c = rad.cos();
            let s = rad.sin();
            [
                0.213 + c * 0.787 - s * 0.213,
                0.715 - c * 0.715 - s * 0.715,
                0.072 - c * 0.072 + s * 0.928,
                0.0,
                0.0,
                0.213 - c * 0.213 + s * 0.143,
                0.715 + c * 0.285 + s * 0.140,
                0.072 - c * 0.072 - s * 0.283,
                0.0,
                0.0,
                0.213 - c * 0.213 - s * 0.787,
                0.715 - c * 0.715 + s * 0.715,
                0.072 + c * 0.928 + s * 0.072,
                0.0,
                0.0,
                0.0,
                0.0,
                0.0,
                1.0,
                0.0,
            ]
        }
        CssFilter::Invert(a) => {
            let amt = a.clamp(0.0, 1.0);
            let scale = 1.0 - 2.0 * amt;
            [
                scale, 0.0, 0.0, 0.0, amt, 0.0, scale, 0.0, 0.0, amt, 0.0, 0.0, scale, 0.0, amt,
                0.0, 0.0, 0.0, 1.0, 0.0,
            ]
        }
        CssFilter::Opacity(o) => [
            1.0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            1.0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            1.0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            o.clamp(0.0, 1.0),
            0.0,
        ],
        CssFilter::Blur(_) | CssFilter::DropShadow { .. } => return None,
    })
}

pub(crate) fn build_css_image_filter(
    filters: &[CssFilter],
    is_backdrop: bool,
) -> Option<ImageFilter> {
    let mut cur: Option<ImageFilter> = None;
    let blur_tile = if is_backdrop {
        TileMode::Clamp
    } else {
        TileMode::Decal
    };

    for f in filters {
        let next = match *f {
            CssFilter::Blur(sigma) => image_filters::blur(
                (sigma.max(0.0), sigma.max(0.0)),
                Some(blur_tile),
                cur,
                image_filters::CropRect::NO_CROP_RECT,
            ),
            CssFilter::DropShadow {
                offset_x,
                offset_y,
                blur_radius,
                color,
            } => image_filters::drop_shadow(
                (offset_x, offset_y),
                (blur_radius.max(0.0), blur_radius.max(0.0)),
                to_color4f(color),
                None,
                cur,
                image_filters::CropRect::NO_CROP_RECT,
            ),
            ref other => {
                let mat = w3c_filter_color_matrix(other)?;
                let cf = color_filters::matrix_row_major(&mat, None);
                image_filters::color_filter(cf, cur, image_filters::CropRect::NO_CROP_RECT)
            }
        };
        cur = next;
    }
    cur
}

fn resolve_circle_radius(rad: ClipRadius, cx: f32, cy: f32, w: f32, h: f32) -> f32 {
    match rad {
        ClipRadius::ClosestSide => {
            let dx = cx.min((w - cx).max(0.0)).max(0.0);
            let dy = cy.min((h - cy).max(0.0)).max(0.0);
            dx.min(dy)
        }
        ClipRadius::FarthestSide => {
            let dx = cx.max((w - cx).max(0.0)).max(0.0);
            let dy = cy.max((h - cy).max(0.0)).max(0.0);
            dx.max(dy)
        }
        ClipRadius::LengthPercent(lp) => match lp {
            LengthPercent::Px(px) => px.max(0.0),
            LengthPercent::Percent(pct) => {
                ((w.hypot(h) / std::f32::consts::SQRT_2) * pct * 0.01).max(0.0)
            }
        },
    }
}

pub(crate) fn build_clip_path(clip: &CssClipPath, w: f32, h: f32) -> Option<Path> {
    if w <= 0.0 || h <= 0.0 {
        return None;
    }
    match clip {
        CssClipPath::None => None,
        CssClipPath::Inset {
            top,
            right,
            bottom,
            left,
            radii,
        } => {
            let l = left.resolve(w);
            let t = top.resolve(h);
            let r = (w - right.resolve(w)).max(l);
            let b = (h - bottom.resolve(h)).max(t);
            let rect = Rect::from_ltrb(l, t, r, b);
            let rr = build_rrect(rect, radii, rect.width(), rect.height(), 0.0);
            let mut pb = PathBuilder::new();
            pb.add_rrect(rr, None, None);
            Some(pb.detach())
        }
        CssClipPath::Circle { radius, cx, cy } => {
            let x = cx.resolve(w);
            let y = cy.resolve(h);
            let r = resolve_circle_radius(*radius, x, y, w, h);
            let mut pb = PathBuilder::new();
            pb.add_circle((x, y), r, None);
            Some(pb.detach())
        }
        CssClipPath::Ellipse { rx, ry, cx, cy } => {
            let x = cx.resolve(w);
            let y = cy.resolve(h);
            let rad_x = match *rx {
                ClipRadius::ClosestSide => x.min((w - x).max(0.0)).max(0.0),
                ClipRadius::FarthestSide => x.max((w - x).max(0.0)).max(0.0),
                ClipRadius::LengthPercent(lp) => lp.resolve(w).max(0.0),
            };
            let rad_y = match *ry {
                ClipRadius::ClosestSide => y.min((h - y).max(0.0)).max(0.0),
                ClipRadius::FarthestSide => y.max((h - y).max(0.0)).max(0.0),
                ClipRadius::LengthPercent(lp) => lp.resolve(h).max(0.0),
            };
            let mut pb = PathBuilder::new();
            pb.add_oval(
                Rect::from_xywh(x - rad_x, y - rad_y, rad_x * 2.0, rad_y * 2.0),
                None,
                None,
            );
            Some(pb.detach())
        }
        CssClipPath::Polygon { even_odd, points } => {
            if points.len() < 3 {
                return None;
            }
            let pts: Vec<Point> = points
                .iter()
                .map(|(px, py)| Point::new(px.resolve(w), py.resolve(h)))
                .collect();
            let mut pb = PathBuilder::new();
            if *even_odd {
                pb.set_fill_type(PathFillType::EvenOdd);
            }
            pb.add_polygon(&pts, true);
            Some(pb.detach())
        }
        CssClipPath::Path { even_odd, d } => {
            let mut path = Path::from_svg(d)?;
            if *even_odd {
                path.set_fill_type(PathFillType::EvenOdd);
            }
            Some(path)
        }
    }
}

fn collect_all_transform_ops(st: &ComputedStyle) -> Vec<TransformOp> {
    let mut ops = Vec::with_capacity(3 + st.transforms.len());
    if let Some((tx, ty, tz)) = st.standalone_translate {
        if tz.abs() > 1e-4 {
            ops.push(TransformOp::Translate3d(tx, ty, tz));
        } else {
            ops.push(TransformOp::Translate(tx, ty));
        }
    }
    if let Some(rot) = st.standalone_rotate {
        ops.push(rot);
    }
    if let Some((sx, sy, sz)) = st.standalone_scale {
        if (sz - 1.0).abs() > 1e-4 {
            ops.push(TransformOp::Scale3d(sx, sy, sz));
        } else {
            ops.push(TransformOp::Scale(sx, sy));
        }
    }
    ops.extend_from_slice(&st.transforms);
    ops
}

fn is_3d_op(op: &TransformOp) -> bool {
    match *op {
        TransformOp::TranslateZ(tz) | TransformOp::Translate3d(_, _, tz) => tz.abs() > 1e-4,
        TransformOp::ScaleZ(sz) | TransformOp::Scale3d(_, _, sz) => (sz - 1.0).abs() > 1e-4,
        TransformOp::RotateXDeg(_)
        | TransformOp::RotateYDeg(_)
        | TransformOp::Rotate3d(_, _, _, _)
        | TransformOp::Perspective(_)
        | TransformOp::Matrix3d(_) => true,
        _ => false,
    }
}

fn m44_perspective(d: f32) -> M44 {
    if d <= 0.0 {
        M44::new_identity()
    } else {
        M44::row_major(&[
            1.0,
            0.0,
            0.0,
            0.0,
            0.0,
            1.0,
            0.0,
            0.0,
            0.0,
            0.0,
            1.0,
            0.0,
            0.0,
            0.0,
            -1.0 / d,
            1.0,
        ])
    }
}

fn m44_rotate_x(deg: f32) -> M44 {
    let rad = deg.to_radians();
    let c = rad.cos();
    let s = rad.sin();
    M44::row_major(&[
        1.0, 0.0, 0.0, 0.0, 0.0, c, -s, 0.0, 0.0, s, c, 0.0, 0.0, 0.0, 0.0, 1.0,
    ])
}

fn m44_rotate_y(deg: f32) -> M44 {
    let rad = deg.to_radians();
    let c = rad.cos();
    let s = rad.sin();
    M44::row_major(&[
        c, 0.0, s, 0.0, 0.0, 1.0, 0.0, 0.0, -s, 0.0, c, 0.0, 0.0, 0.0, 0.0, 1.0,
    ])
}

fn m44_rotate_z(deg: f32) -> M44 {
    let rad = deg.to_radians();
    let c = rad.cos();
    let s = rad.sin();
    M44::row_major(&[
        c, -s, 0.0, 0.0, s, c, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ])
}

fn m44_rotate_3d(x: f32, y: f32, z: f32, deg: f32) -> M44 {
    let len = (x * x + y * y + z * z).sqrt();
    if len < 1e-6 {
        return M44::new_identity();
    }
    let ux = x / len;
    let uy = y / len;
    let uz = z / len;
    let rad = deg.to_radians();
    let c = rad.cos();
    let s = rad.sin();
    let t = 1.0 - c;
    M44::row_major(&[
        t * ux * ux + c,
        t * ux * uy - s * uz,
        t * ux * uz + s * uy,
        0.0,
        t * ux * uy + s * uz,
        t * uy * uy + c,
        t * uy * uz - s * ux,
        0.0,
        t * ux * uz - s * uy,
        t * uy * uz + s * ux,
        t * uz * uz + c,
        0.0,
        0.0,
        0.0,
        0.0,
        1.0,
    ])
}

pub(crate) fn apply_parent_perspective(
    canvas: &Canvas,
    d: f32,
    origin: (LengthPercent, LengthPercent),
    w: f32,
    h: f32,
) {
    if d <= 0.0 {
        return;
    }
    let px = origin.0.resolve(w);
    let py = origin.1.resolve(h);
    let mut m = M44::translate(px, py, 0.0);
    m = M44::concat(&m, &m44_perspective(d));
    m = M44::concat(&m, &M44::translate(-px, -py, 0.0));
    canvas.concat_44(&m);
}

pub(crate) fn apply_css_transforms(canvas: &Canvas, st: &ComputedStyle, w: f32, h: f32) {
    let ops = collect_all_transform_ops(st);
    if ops.is_empty() {
        return;
    }
    let ox = st.transform_origin.0.resolve(w);
    let oy = st.transform_origin.1.resolve(h);

    if !ops.iter().any(is_3d_op) {
        canvas.translate((ox, oy));
        for op in ops {
            match op {
                TransformOp::Translate(tx, ty) | TransformOp::Translate3d(tx, ty, _) => {
                    canvas.translate((tx.resolve(w), ty.resolve(h)));
                }
                TransformOp::TranslateX(tx) => {
                    canvas.translate((tx.resolve(w), 0.0));
                }
                TransformOp::TranslateY(ty) => {
                    canvas.translate((0.0, ty.resolve(h)));
                }
                TransformOp::Scale(sx, sy) | TransformOp::Scale3d(sx, sy, _) => {
                    canvas.scale((sx, sy));
                }
                TransformOp::RotateDeg(deg) | TransformOp::RotateZDeg(deg) => {
                    canvas.rotate(deg, None);
                }
                TransformOp::SkewDeg(dx, dy) => {
                    canvas.skew((dx.to_radians().tan(), dy.to_radians().tan()));
                }
                TransformOp::Matrix(m) => {
                    let sk_mat = Matrix::new_all(m[0], m[2], m[4], m[1], m[3], m[5], 0.0, 0.0, 1.0);
                    canvas.concat(&sk_mat);
                }
                _ => {}
            }
        }
        canvas.translate((-ox, -oy));
        return;
    }

    let mut m = M44::translate(ox, oy, 0.0);
    for op in ops {
        let step = match op {
            TransformOp::Translate(tx, ty) => M44::translate(tx.resolve(w), ty.resolve(h), 0.0),
            TransformOp::TranslateX(tx) => M44::translate(tx.resolve(w), 0.0, 0.0),
            TransformOp::TranslateY(ty) => M44::translate(0.0, ty.resolve(h), 0.0),
            TransformOp::TranslateZ(tz) => M44::translate(0.0, 0.0, tz),
            TransformOp::Translate3d(tx, ty, tz) => {
                M44::translate(tx.resolve(w), ty.resolve(h), tz)
            }
            TransformOp::Scale(sx, sy) => M44::scale(sx, sy, 1.0),
            TransformOp::ScaleZ(sz) => M44::scale(1.0, 1.0, sz),
            TransformOp::Scale3d(sx, sy, sz) => M44::scale(sx, sy, sz),
            TransformOp::RotateDeg(deg) | TransformOp::RotateZDeg(deg) => m44_rotate_z(deg),
            TransformOp::RotateXDeg(deg) => m44_rotate_x(deg),
            TransformOp::RotateYDeg(deg) => m44_rotate_y(deg),
            TransformOp::Rotate3d(x, y, z, deg) => m44_rotate_3d(x, y, z, deg),
            TransformOp::Perspective(d) => m44_perspective(d),
            TransformOp::SkewDeg(dx, dy) => M44::row_major(&[
                1.0,
                dx.to_radians().tan(),
                0.0,
                0.0,
                dy.to_radians().tan(),
                1.0,
                0.0,
                0.0,
                0.0,
                0.0,
                1.0,
                0.0,
                0.0,
                0.0,
                0.0,
                1.0,
            ]),
            TransformOp::Matrix(mat) => M44::row_major(&[
                mat[0], mat[2], 0.0, mat[4], mat[1], mat[3], 0.0, mat[5], 0.0, 0.0, 1.0, 0.0, 0.0,
                0.0, 0.0, 1.0,
            ]),
            TransformOp::Matrix3d(col) => M44::col_major(&col),
        };
        m = M44::concat(&m, &step);
    }
    m = M44::concat(&m, &M44::translate(-ox, -oy, 0.0));
    canvas.concat_44(&m);
}
