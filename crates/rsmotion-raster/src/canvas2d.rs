use std::f32::consts::PI;

use rsmotion_layout::effects_parse::parse_filter_list;
use rsmotion_layout::{parse_color, CssColor};
use rsmotion_scene::{Canvas2DOp, CanvasPaint};
use skia_safe::canvas::{SaveLayerRec, SrcRectConstraint};
use skia_safe::gradient::{
    shaders as gradient_shaders, Colors as GradientColors, Gradient, Interpolation,
};
use skia_safe::paint::{Cap, Join, Style};
use skia_safe::path::AddPathMode;
use skia_safe::{
    image_filters, BlendMode, Canvas, ClipOp, Color4f, Matrix, Paint, Path, PathBuilder,
    PathEffect, PathFillType, Point, RRect, Rect, Shader, TileMode, Vector,
};

use crate::effects::{build_css_image_filter, to_color4f};
use crate::font_cache::FontCache;
use crate::image_cache::ImageCache;

#[derive(Clone)]
struct CanvasState {
    transform: Matrix,
    fill_style: CanvasPaint,
    stroke_style: CanvasPaint,
    line_width: f32,
    line_cap: Cap,
    line_join: Join,
    miter_limit: f32,
    line_dash: Vec<f32>,
    line_dash_offset: f32,
    global_alpha: f32,
    composite_op: BlendMode,
    shadow_blur: f32,
    shadow_color: CssColor,
    shadow_offset_x: f32,
    shadow_offset_y: f32,
    font: String,
    text_align: String,
    text_baseline: String,
    filter: String,
}

impl Default for CanvasState {
    fn default() -> Self {
        Self {
            transform: Matrix::new_identity(),
            fill_style: CanvasPaint::Color {
                value: "#000000".to_string(),
            },
            stroke_style: CanvasPaint::Color {
                value: "#000000".to_string(),
            },
            line_width: 1.0,
            line_cap: Cap::Butt,
            line_join: Join::Miter,
            miter_limit: 10.0,
            line_dash: Vec::new(),
            line_dash_offset: 0.0,
            global_alpha: 1.0,
            composite_op: BlendMode::SrcOver,
            shadow_blur: 0.0,
            shadow_color: CssColor::TRANSPARENT,
            shadow_offset_x: 0.0,
            shadow_offset_y: 0.0,
            font: "10px sans-serif".to_string(),
            text_align: "start".to_string(),
            text_baseline: "alphabetic".to_string(),
            filter: "none".to_string(),
        }
    }
}

fn matrix_from_2d(m: [f32; 6]) -> Matrix {
    // Canvas 2D [a, b, c, d, e, f] -> Skia row-major [scaleX, skewX, transX, skewY, scaleY, transY, 0, 0, 1]
    Matrix::new_all(m[0], m[2], m[4], m[1], m[3], m[5], 0.0, 0.0, 1.0)
}

fn parse_blend_op(raw: &str) -> BlendMode {
    match raw.trim().to_ascii_lowercase().as_str() {
        "source-over" => BlendMode::SrcOver,
        "source-in" => BlendMode::SrcIn,
        "source-out" => BlendMode::SrcOut,
        "source-atop" => BlendMode::SrcATop,
        "destination-over" => BlendMode::DstOver,
        "destination-in" => BlendMode::DstIn,
        "destination-out" => BlendMode::DstOut,
        "destination-atop" => BlendMode::DstATop,
        "lighter" => BlendMode::Plus,
        "copy" => BlendMode::Src,
        "xor" => BlendMode::Xor,
        "multiply" => BlendMode::Multiply,
        "screen" => BlendMode::Screen,
        "overlay" => BlendMode::Overlay,
        "darken" => BlendMode::Darken,
        "lighten" => BlendMode::Lighten,
        "color-dodge" => BlendMode::ColorDodge,
        "color-burn" => BlendMode::ColorBurn,
        "hard-light" => BlendMode::HardLight,
        "soft-light" => BlendMode::SoftLight,
        "difference" => BlendMode::Difference,
        "exclusion" => BlendMode::Exclusion,
        "hue" => BlendMode::Hue,
        "saturation" => BlendMode::Saturation,
        "color" => BlendMode::Color,
        "luminosity" => BlendMode::Luminosity,
        _ => BlendMode::SrcOver,
    }
}

fn build_gradient_shader(
    stops: &[(f32, String)],
    make_shader: impl FnOnce(&[Color4f], &[f32]) -> Option<Shader>,
) -> Option<Shader> {
    if stops.is_empty() {
        return None;
    }
    let mut pairs: Vec<(f32, Color4f)> = stops
        .iter()
        .map(|(offset, css)| {
            let c = parse_color(css, CssColor::BLACK).unwrap_or(CssColor::BLACK);
            (offset.clamp(0.0, 1.0), to_color4f(c))
        })
        .collect();
    pairs.sort_by(|a, b| a.0.total_cmp(&b.0));
    let pos: Vec<f32> = pairs.iter().map(|(o, _)| *o).collect();
    let cols: Vec<Color4f> = pairs.iter().map(|(_, c)| *c).collect();
    make_shader(&cols, &pos)
}

fn apply_canvas_paint(source: &CanvasPaint, state: &CanvasState, is_stroke: bool) -> Paint {
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    paint.set_blend_mode(state.composite_op);

    let alpha = state.global_alpha.clamp(0.0, 1.0);
    match source {
        CanvasPaint::Color { value } => {
            let c = parse_color(value, CssColor::BLACK).unwrap_or(CssColor::BLACK);
            let mut cf = to_color4f(c);
            cf.a *= alpha;
            paint.set_color4f(cf, None);
        }
        CanvasPaint::LinearGradient {
            x0,
            y0,
            x1,
            y1,
            stops,
        } => {
            paint.set_color4f(Color4f::new(0.0, 0.0, 0.0, alpha), None);
            if let Some(sh) = build_gradient_shader(stops, |cols, pos| {
                let gcolors = GradientColors::new(cols, Some(pos), TileMode::Clamp, None);
                let grad = Gradient::new(gcolors, Interpolation::default());
                gradient_shaders::linear_gradient(
                    (Point::new(*x0, *y0), Point::new(*x1, *y1)),
                    &grad,
                    None,
                )
            }) {
                paint.set_shader(sh);
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
            paint.set_color4f(Color4f::new(0.0, 0.0, 0.0, alpha), None);
            if let Some(sh) = build_gradient_shader(stops, |cols, pos| {
                let gcolors = GradientColors::new(cols, Some(pos), TileMode::Clamp, None);
                let grad = Gradient::new(gcolors, Interpolation::default());
                gradient_shaders::two_point_conical_gradient(
                    (Point::new(*x0, *y0), (*r0).max(0.0)),
                    (Point::new(*x1, *y1), (*r1).max(0.0)),
                    &grad,
                    None,
                )
            }) {
                paint.set_shader(sh);
            }
        }
    }

    if is_stroke {
        paint.set_style(Style::Stroke);
        paint.set_stroke_width(state.line_width.max(0.0));
        paint.set_stroke_cap(state.line_cap);
        paint.set_stroke_join(state.line_join);
        paint.set_stroke_miter(state.miter_limit.max(0.0));
        if state.line_dash.len() >= 2 || (state.line_dash.len() == 1 && state.line_dash[0] > 0.0) {
            let mut intervals = state.line_dash.clone();
            if intervals.len() % 2 == 1 {
                intervals.extend_from_within(..);
            }
            if intervals.iter().any(|&v| v > 0.0) {
                if let Some(pe) = PathEffect::dash(&intervals, state.line_dash_offset) {
                    paint.set_path_effect(pe);
                }
            }
        }
    } else {
        paint.set_style(Style::Fill);
    }

    let mut img_filter = None;
    let ftrim = state.filter.trim();
    if !ftrim.is_empty() && !ftrim.eq_ignore_ascii_case("none") {
        let parsed = parse_filter_list(ftrim, CssColor::BLACK, 16.0, 0.0, 0.0);
        img_filter = build_css_image_filter(&parsed, false);
    }
    if state.shadow_color.a > 0
        && (state.shadow_blur > 0.0
            || state.shadow_offset_x.abs() > 0.0
            || state.shadow_offset_y.abs() > 0.0)
    {
        let sigma = (state.shadow_blur * 0.5).max(0.0);
        img_filter = image_filters::drop_shadow(
            (state.shadow_offset_x, state.shadow_offset_y),
            (sigma, sigma),
            to_color4f(state.shadow_color),
            None,
            img_filter,
            image_filters::CropRect::NO_CROP_RECT,
        );
    }
    if let Some(f) = img_filter {
        paint.set_image_filter(f);
    }

    paint
}

fn canonical_arc_sweep(start: f32, end: f32, counterclockwise: bool) -> f32 {
    let two_pi = 2.0 * PI;
    let diff = end - start;
    if diff.abs() >= two_pi - 1e-4 && diff.abs() > 1e-4 {
        return if counterclockwise { -two_pi } else { two_pi };
    }
    if !counterclockwise {
        diff.rem_euclid(two_pi)
    } else {
        -(start - end).rem_euclid(two_pi)
    }
}

fn append_arc_to_builder(
    builder: &mut PathBuilder,
    oval: Rect,
    start_rad: f32,
    end_rad: f32,
    counterclockwise: bool,
) {
    let sweep = canonical_arc_sweep(start_rad, end_rad, counterclockwise);
    let start_deg = start_rad.to_degrees();
    let sweep_deg = sweep.to_degrees();
    let is_empty = builder.is_empty();
    let two_pi = 2.0 * PI;

    if sweep.abs() >= two_pi - 1e-4 {
        let half = sweep_deg * 0.5;
        builder.arc_to(oval, start_deg, half, is_empty);
        builder.arc_to(oval, start_deg + half, sweep_deg - half, false);
    } else if sweep.abs() > 1e-5 {
        builder.arc_to(oval, start_deg, sweep_deg, is_empty);
    } else {
        let cx = oval.center_x();
        let cy = oval.center_y();
        let rx = oval.width() * 0.5;
        let ry = oval.height() * 0.5;
        let pt = Point::new(cx + rx * start_rad.cos(), cy + ry * start_rad.sin());
        if is_empty {
            builder.move_to(pt);
        } else {
            builder.line_to(pt);
        }
    }
}

fn apply_path_op(builder: &mut PathBuilder, op: &Canvas2DOp, transform: &Matrix) -> bool {
    match op {
        Canvas2DOp::BeginPath => {
            *builder = PathBuilder::new();
            true
        }
        Canvas2DOp::ClosePath => {
            builder.close();
            true
        }
        Canvas2DOp::MoveTo { x, y } => {
            builder.move_to(transform.map_point(Point::new(*x, *y)));
            true
        }
        Canvas2DOp::LineTo { x, y } => {
            let pt = transform.map_point(Point::new(*x, *y));
            if builder.is_empty() {
                builder.move_to(pt);
            } else {
                builder.line_to(pt);
            }
            true
        }
        Canvas2DOp::BezierCurveTo {
            cp1x,
            cp1y,
            cp2x,
            cp2y,
            x,
            y,
        } => {
            let p1 = transform.map_point(Point::new(*cp1x, *cp1y));
            let p2 = transform.map_point(Point::new(*cp2x, *cp2y));
            let p3 = transform.map_point(Point::new(*x, *y));
            if builder.is_empty() {
                builder.move_to(p1);
            }
            builder.cubic_to(p1, p2, p3);
            true
        }
        Canvas2DOp::QuadraticCurveTo { cpx, cpy, x, y } => {
            let p1 = transform.map_point(Point::new(*cpx, *cpy));
            let p2 = transform.map_point(Point::new(*x, *y));
            if builder.is_empty() {
                builder.move_to(p1);
            }
            builder.quad_to(p1, p2);
            true
        }
        Canvas2DOp::Arc {
            x,
            y,
            radius,
            start_angle,
            end_angle,
            counterclockwise,
        } => {
            if *radius < 0.0 || !x.is_finite() || !y.is_finite() || !radius.is_finite() {
                return true;
            }
            if transform.is_identity() {
                let oval = Rect::from_xywh(x - radius, y - radius, radius * 2.0, radius * 2.0);
                append_arc_to_builder(builder, oval, *start_angle, *end_angle, *counterclockwise);
            } else {
                let mut local = PathBuilder::new();
                let oval = Rect::from_xywh(x - radius, y - radius, radius * 2.0, radius * 2.0);
                append_arc_to_builder(
                    &mut local,
                    oval,
                    *start_angle,
                    *end_angle,
                    *counterclockwise,
                );
                let sub = local.detach();
                let mode = if builder.is_empty() {
                    AddPathMode::Append
                } else {
                    AddPathMode::Extend
                };
                builder.add_path_with_transform(&sub, transform, mode);
            }
            true
        }
        Canvas2DOp::ArcTo {
            x1,
            y1,
            x2,
            y2,
            radius,
        } => {
            if *radius < 0.0 || !radius.is_finite() {
                return true;
            }
            let p1 = transform.map_point(Point::new(*x1, *y1));
            let p2 = transform.map_point(Point::new(*x2, *y2));
            if transform.is_identity() {
                builder.arc_to_tangent(p1, p2, *radius);
            } else {
                // Scale radius roughly by matrix average axis length when non-uniformly scaled.
                let sx = transform.scale_x().hypot(transform.skew_y());
                let sy = transform.skew_x().hypot(transform.scale_y());
                builder.arc_to_tangent(p1, p2, *radius * (sx * sy).sqrt().max(0.001));
            }
            true
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
            if *radius_x < 0.0 || *radius_y < 0.0 {
                return true;
            }
            let mut local = PathBuilder::new();
            let oval = Rect::from_xywh(-radius_x, -radius_y, radius_x * 2.0, radius_y * 2.0);
            append_arc_to_builder(
                &mut local,
                oval,
                *start_angle,
                *end_angle,
                *counterclockwise,
            );
            let sub = local.detach();
            let mut m = *transform;
            m.pre_translate((*x, *y));
            if rotation.abs() > 1e-6 {
                m.pre_rotate(rotation.to_degrees(), None);
            }
            let mode = if builder.is_empty() {
                AddPathMode::Append
            } else {
                AddPathMode::Extend
            };
            builder.add_path_with_transform(&sub, &m, mode);
            true
        }
        Canvas2DOp::Rect { x, y, w, h } => {
            let r = Rect::from_xywh(*x, *y, *w, *h);
            if transform.is_identity() {
                builder.add_rect(r, None, None);
            } else {
                let mut local = PathBuilder::new();
                local.add_rect(r, None, None);
                builder.add_path_with_transform(&local.detach(), transform, AddPathMode::Append);
            }
            true
        }
        Canvas2DOp::RoundRect { x, y, w, h, radii } => {
            let r = Rect::from_xywh(*x, *y, *w, *h);
            let vectors = [
                Vector::new(radii[0].max(0.0), radii[0].max(0.0)),
                Vector::new(radii[1].max(0.0), radii[1].max(0.0)),
                Vector::new(radii[2].max(0.0), radii[2].max(0.0)),
                Vector::new(radii[3].max(0.0), radii[3].max(0.0)),
            ];
            let rrect = RRect::new_rect_radii(r, &vectors);
            if transform.is_identity() {
                builder.add_rrect(rrect, None, None);
            } else {
                let mut local = PathBuilder::new();
                local.add_rrect(rrect, None, None);
                builder.add_path_with_transform(&local.detach(), transform, AddPathMode::Append);
            }
            true
        }
        _ => false,
    }
}

fn build_path2d(path_ops: &[Canvas2DOp], transform: &Matrix) -> Path {
    let mut pb = PathBuilder::new();
    let ident = Matrix::new_identity();
    for op in path_ops {
        apply_path_op(&mut pb, op, &ident);
    }
    let path = pb.detach();
    if transform.is_identity() {
        path
    } else {
        path.with_transform(transform)
    }
}

fn resolve_fill_type(rule: &str) -> PathFillType {
    if rule.eq_ignore_ascii_case("evenodd") {
        PathFillType::EvenOdd
    } else {
        PathFillType::Winding
    }
}

fn draw_canvas_text(
    canvas: &Canvas,
    font_cache: &FontCache,
    state: &CanvasState,
    paint: &Paint,
    text: &str,
    pos: Point,
    max_width: Option<f32>,
) {
    if text.is_empty() {
        return;
    }
    let (para, text_w, ascent, descent) =
        font_cache.build_canvas_paragraph(text, &state.font, Some(paint));
    let mut draw_scale_x = 1.0_f32;
    let effective_w = if let Some(mw) = max_width.filter(|mw| mw.is_finite() && *mw > 0.0) {
        if text_w > mw && text_w > 0.0 {
            draw_scale_x = mw / text_w;
            mw
        } else {
            text_w
        }
    } else {
        text_w
    };

    let dx = match state.text_align.trim().to_ascii_lowercase().as_str() {
        "center" => -effective_w * 0.5,
        "right" | "end" => -effective_w,
        _ => 0.0,
    };
    let dy = match state.text_baseline.trim().to_ascii_lowercase().as_str() {
        "top" | "hanging" => 0.0,
        "middle" => -0.5 * (ascent + descent),
        "bottom" | "ideographic" => -(ascent + descent),
        _ => -ascent, // alphabetic default
    };

    canvas.save();
    canvas.concat(&state.transform);
    canvas.translate((pos.x + dx, pos.y + dy));
    if (draw_scale_x - 1.0).abs() > 1e-4 {
        canvas.scale((draw_scale_x, 1.0));
    }
    para.paint(canvas, Point::new(0.0, 0.0));
    canvas.restore();
}

/// Executes recorded HTML5 Canvas 2D operations directly onto the hardware/CPU Skia surface.
pub(crate) fn draw_canvas2d(
    canvas: &Canvas,
    dst_rect: Rect,
    canvas_w: u32,
    canvas_h: u32,
    ops: &[Canvas2DOp],
    font_cache: &FontCache,
    image_cache: &ImageCache,
) {
    if canvas_w == 0 || canvas_h == 0 || dst_rect.width() <= 0.0 || dst_rect.height() <= 0.0 {
        return;
    }
    let bw = canvas_w as f32;
    let bh = canvas_h as f32;
    let bounds = Rect::from_wh(bw, bh);

    canvas.save();
    canvas.translate((dst_rect.left, dst_rect.top));
    canvas.scale((dst_rect.width() / bw, dst_rect.height() / bh));
    canvas.clip_rect(bounds, ClipOp::Intersect, true);
    // Isolate Canvas 2D compositing (`clearRect` / `globalCompositeOperation`) from prior DOM backdrop.
    canvas.save_layer(&SaveLayerRec::default().bounds(&bounds));

    let mut state = CanvasState::default();
    let mut stack: Vec<CanvasState> = Vec::new();
    let mut path_builder = PathBuilder::new();
    let mut save_depth = 0usize;

    for op in ops {
        if apply_path_op(&mut path_builder, op, &state.transform) {
            continue;
        }
        match op {
            Canvas2DOp::Save => {
                canvas.save();
                stack.push(state.clone());
                save_depth += 1;
            }
            Canvas2DOp::Restore => {
                if let Some(prev) = stack.pop() {
                    state = prev;
                    canvas.restore();
                    save_depth = save_depth.saturating_sub(1);
                }
            }
            Canvas2DOp::Translate { x, y } => {
                state.transform.pre_translate((*x, *y));
            }
            Canvas2DOp::Rotate { angle } => {
                state.transform.pre_rotate(angle.to_degrees(), None);
            }
            Canvas2DOp::Scale { x, y } => {
                state.transform.pre_scale((*x, *y), None);
            }
            Canvas2DOp::Transform { matrix } => {
                let m = matrix_from_2d(*matrix);
                state.transform.pre_concat(&m);
            }
            Canvas2DOp::SetTransform { matrix } => {
                state.transform = matrix_from_2d(*matrix);
            }
            Canvas2DOp::ResetTransform => {
                state.transform = Matrix::new_identity();
            }
            Canvas2DOp::SetFillStyle { paint } => {
                state.fill_style = paint.clone();
            }
            Canvas2DOp::SetStrokeStyle { paint } => {
                state.stroke_style = paint.clone();
            }
            Canvas2DOp::SetLineWidth { width } => {
                if width.is_finite() && *width > 0.0 {
                    state.line_width = *width;
                }
            }
            Canvas2DOp::SetLineCap { cap } => {
                state.line_cap = match cap.trim().to_ascii_lowercase().as_str() {
                    "round" => Cap::Round,
                    "square" => Cap::Square,
                    _ => Cap::Butt,
                };
            }
            Canvas2DOp::SetLineJoin { join } => {
                state.line_join = match join.trim().to_ascii_lowercase().as_str() {
                    "round" => Join::Round,
                    "bevel" => Join::Bevel,
                    _ => Join::Miter,
                };
            }
            Canvas2DOp::SetMiterLimit { limit } => {
                if limit.is_finite() && *limit > 0.0 {
                    state.miter_limit = *limit;
                }
            }
            Canvas2DOp::SetLineDash { segments } => {
                state.line_dash = segments.clone();
            }
            Canvas2DOp::SetLineDashOffset { offset } => {
                if offset.is_finite() {
                    state.line_dash_offset = *offset;
                }
            }
            Canvas2DOp::SetGlobalAlpha { alpha } => {
                if alpha.is_finite() {
                    state.global_alpha = alpha.clamp(0.0, 1.0);
                }
            }
            Canvas2DOp::SetGlobalCompositeOperation { mode } => {
                state.composite_op = parse_blend_op(mode);
            }
            Canvas2DOp::SetShadow {
                blur,
                color,
                offset_x,
                offset_y,
            } => {
                state.shadow_blur = blur.max(0.0);
                state.shadow_color =
                    parse_color(color, CssColor::TRANSPARENT).unwrap_or(CssColor::TRANSPARENT);
                state.shadow_offset_x = *offset_x;
                state.shadow_offset_y = *offset_y;
            }
            Canvas2DOp::SetFont { font } => {
                state.font = font.clone();
            }
            Canvas2DOp::SetTextAlign { align } => {
                state.text_align = align.clone();
            }
            Canvas2DOp::SetTextBaseline { baseline } => {
                state.text_baseline = baseline.clone();
            }
            Canvas2DOp::SetFilter { filter } => {
                state.filter = filter.clone();
            }
            Canvas2DOp::ClearRect { x, y, w, h } => {
                let mut clear_paint = Paint::default();
                clear_paint.set_blend_mode(BlendMode::Clear);
                clear_paint.set_color4f(Color4f::new(0.0, 0.0, 0.0, 0.0), None);
                canvas.save();
                canvas.concat(&state.transform);
                canvas.draw_rect(Rect::from_xywh(*x, *y, *w, *h), &clear_paint);
                canvas.restore();
            }
            Canvas2DOp::FillRect { x, y, w, h } => {
                let paint = apply_canvas_paint(&state.fill_style, &state, false);
                canvas.save();
                canvas.concat(&state.transform);
                canvas.draw_rect(Rect::from_xywh(*x, *y, *w, *h), &paint);
                canvas.restore();
            }
            Canvas2DOp::StrokeRect { x, y, w, h } => {
                let paint = apply_canvas_paint(&state.stroke_style, &state, true);
                canvas.save();
                canvas.concat(&state.transform);
                canvas.draw_rect(Rect::from_xywh(*x, *y, *w, *h), &paint);
                canvas.restore();
            }
            Canvas2DOp::Fill { rule, path_ops } => {
                let mut path = match path_ops {
                    Some(sub) => build_path2d(sub, &state.transform),
                    None => path_builder.snapshot(),
                };
                path.set_fill_type(resolve_fill_type(rule));
                let paint = apply_canvas_paint(&state.fill_style, &state, false);
                if state.transform.is_identity() {
                    canvas.draw_path(&path, &paint);
                } else if let Some(inv) = state.transform.invert() {
                    let local_path = path.with_transform(&inv);
                    canvas.save();
                    canvas.concat(&state.transform);
                    canvas.draw_path(&local_path, &paint);
                    canvas.restore();
                } else {
                    canvas.draw_path(&path, &paint);
                }
            }
            Canvas2DOp::Stroke { path_ops } => {
                let path = match path_ops {
                    Some(sub) => build_path2d(sub, &state.transform),
                    None => path_builder.snapshot(),
                };
                let paint = apply_canvas_paint(&state.stroke_style, &state, true);
                if state.transform.is_identity() {
                    canvas.draw_path(&path, &paint);
                } else if let Some(inv) = state.transform.invert() {
                    let local_path = path.with_transform(&inv);
                    canvas.save();
                    canvas.concat(&state.transform);
                    canvas.draw_path(&local_path, &paint);
                    canvas.restore();
                } else {
                    canvas.draw_path(&path, &paint);
                }
            }
            Canvas2DOp::Clip { rule, path_ops } => {
                let mut path = match path_ops {
                    Some(sub) => build_path2d(sub, &state.transform),
                    None => path_builder.snapshot(),
                };
                path.set_fill_type(resolve_fill_type(rule));
                canvas.clip_path(&path, ClipOp::Intersect, true);
            }
            Canvas2DOp::FillText {
                text,
                x,
                y,
                max_width,
            } => {
                let paint = apply_canvas_paint(&state.fill_style, &state, false);
                draw_canvas_text(
                    canvas,
                    font_cache,
                    &state,
                    &paint,
                    text,
                    Point::new(*x, *y),
                    *max_width,
                );
            }
            Canvas2DOp::StrokeText {
                text,
                x,
                y,
                max_width,
            } => {
                let paint = apply_canvas_paint(&state.stroke_style, &state, true);
                draw_canvas_text(
                    canvas,
                    font_cache,
                    &state,
                    &paint,
                    text,
                    Point::new(*x, *y),
                    *max_width,
                );
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
                if let Some(img) = image_cache.get_cached(src) {
                    let iw = img.width() as f32;
                    let ih = img.height() as f32;
                    let src_rect = Rect::from_xywh(
                        sx.unwrap_or(0.0),
                        sy.unwrap_or(0.0),
                        sw.unwrap_or(iw),
                        sh.unwrap_or(ih),
                    );
                    let dst = Rect::from_xywh(
                        *dx,
                        *dy,
                        dw.unwrap_or(src_rect.width()),
                        dh.unwrap_or(src_rect.height()),
                    );
                    let mut paint = Paint::default();
                    paint.set_anti_alias(true);
                    paint.set_alpha_f(state.global_alpha.clamp(0.0, 1.0));
                    paint.set_blend_mode(state.composite_op);
                    let sampling = skia_safe::SamplingOptions::new(
                        skia_safe::FilterMode::Linear,
                        skia_safe::MipmapMode::Linear,
                    );
                    canvas.save();
                    canvas.concat(&state.transform);
                    canvas.draw_image_rect_with_sampling_options(
                        img,
                        Some((&src_rect, SrcRectConstraint::Strict)),
                        dst,
                        sampling,
                        &paint,
                    );
                    canvas.restore();
                }
            }
            _ => {}
        }
    }

    for _ in 0..save_depth {
        canvas.restore();
    }
    canvas.restore(); // save_layer
    canvas.restore(); // outer save
}
