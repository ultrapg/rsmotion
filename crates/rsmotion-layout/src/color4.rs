use crate::style::{parse_angle_deg, parse_color, split_top_level, split_top_level_ws, CssColor};

fn parse_unit_or_pct_01(raw: &str, pct_scale: f32) -> Option<f32> {
    let t = raw.trim();
    if let Some(p) = t.strip_suffix('%') {
        p.trim().parse::<f32>().ok().map(|v| v * 0.01 * pct_scale)
    } else {
        t.parse::<f32>().ok()
    }
}

fn parse_alpha_01(s: &str) -> Option<f32> {
    let t = s.trim();
    if let Some(p) = t.strip_suffix('%') {
        Some((p.trim().parse::<f32>().ok()? * 0.01).clamp(0.0, 1.0))
    } else {
        Some(t.parse::<f32>().ok()?.clamp(0.0, 1.0))
    }
}

fn linear_to_srgb_channel(x: f32) -> f32 {
    let c = x.clamp(0.0, 1.0);
    if c <= 0.003_130_8 {
        12.92 * c
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

fn srgb_to_linear_channel(x: f32) -> f32 {
    let c = x.clamp(0.0, 1.0);
    if c <= 0.040_45 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn oklab_to_rgba(l: f32, a: f32, b: f32, alpha: f32) -> CssColor {
    let l_ = l + 0.396_337_78 * a + 0.215_803_76 * b;
    let m_ = l - 0.105_561_35 * a - 0.063_854_17 * b;
    let s_ = l - 0.089_484_18 * a - 1.291_485_5 * b;

    let ll = l_ * l_ * l_;
    let mm = m_ * m_ * m_;
    let ss = s_ * s_ * s_;

    let lr = 4.076_741_7 * ll - 3.307_711_6 * mm + 0.230_969_93 * ss;
    let lg = -1.268_438 * ll + 2.609_757_4 * mm - 0.341_319_4 * ss;
    let lb = -0.004_196_086_3 * ll - 0.703_418_6 * mm + 1.707_614_7 * ss;

    let r = (linear_to_srgb_channel(lr).clamp(0.0, 1.0) * 255.0).round() as u8;
    let g = (linear_to_srgb_channel(lg).clamp(0.0, 1.0) * 255.0).round() as u8;
    let bl = (linear_to_srgb_channel(lb).clamp(0.0, 1.0) * 255.0).round() as u8;
    let al = (alpha.clamp(0.0, 1.0) * 255.0).round() as u8;
    CssColor::rgba(r, g, bl, al)
}

fn rgba_to_oklab(c: CssColor) -> (f32, f32, f32, f32) {
    let r = srgb_to_linear_channel(f32::from(c.r) / 255.0);
    let g = srgb_to_linear_channel(f32::from(c.g) / 255.0);
    let b = srgb_to_linear_channel(f32::from(c.b) / 255.0);

    let l = (0.412_221_47 * r + 0.536_332_55 * g + 0.051_445_995 * b).cbrt();
    let m = (0.211_903_5 * r + 0.680_699_5 * g + 0.107_396_96 * b).cbrt();
    let s = (0.088_302_46 * r + 0.281_718_85 * g + 0.629_978_7 * b).cbrt();

    let lab_l = 0.210_454_26 * l + 0.793_617_8 * m - 0.004_072_047 * s;
    let lab_a = 1.977_998_5 * l - 2.428_592_2 * m + 0.450_593_7 * s;
    let lab_b = 0.025_904_037 * l + 0.782_771_77 * m - 0.808_675_77 * s;
    (lab_l, lab_a, lab_b, f32::from(c.a) / 255.0)
}

/// Parses CSS Color Level 4 `oklch(...)`, `oklab(...)`, and `color-mix(...)`.
pub fn parse_color4(raw: &str, current_color: CssColor) -> Option<CssColor> {
    let trimmed = raw.trim();
    let lower = trimmed.to_ascii_lowercase();

    if let Some(inner) = lower
        .strip_prefix("oklab(")
        .and_then(|s| s.strip_suffix(')'))
    {
        let (main, alpha_part) = inner
            .split_once('/')
            .map_or((inner, None), |(a, b)| (a, Some(b)));
        let parts: Vec<&str> = main.split_whitespace().collect();
        if parts.len() < 3 {
            return None;
        }
        let l = parse_unit_or_pct_01(parts[0], 1.0)?.clamp(0.0, 1.0);
        let a = parse_unit_or_pct_01(parts[1], 0.4)?;
        let b = parse_unit_or_pct_01(parts[2], 0.4)?;
        let alpha = match alpha_part {
            Some(ap) => parse_alpha_01(ap)?,
            None => 1.0,
        };
        return Some(oklab_to_rgba(l, a, b, alpha));
    }

    if let Some(inner) = lower
        .strip_prefix("oklch(")
        .and_then(|s| s.strip_suffix(')'))
    {
        let (main, alpha_part) = inner
            .split_once('/')
            .map_or((inner, None), |(a, b)| (a, Some(b)));
        let parts: Vec<&str> = main.split_whitespace().collect();
        if parts.len() < 3 {
            return None;
        }
        let l = parse_unit_or_pct_01(parts[0], 1.0)?.clamp(0.0, 1.0);
        let c = parse_unit_or_pct_01(parts[1], 0.4)?.max(0.0);
        let h_deg = parse_angle_deg(parts[2]).or_else(|| parts[2].parse::<f32>().ok())?;
        let rad = h_deg.to_radians();
        let a = c * rad.cos();
        let b = c * rad.sin();
        let alpha = match alpha_part {
            Some(ap) => parse_alpha_01(ap)?,
            None => 1.0,
        };
        return Some(oklab_to_rgba(l, a, b, alpha));
    }

    if lower.starts_with("color-mix(") && trimmed.ends_with(')') {
        let inner = &trimmed["color-mix(".len()..trimmed.len() - 1];
        let args = split_top_level(inner, ',');
        if args.len() != 3 {
            return None;
        }
        let space = args[0].trim().to_ascii_lowercase();
        let (c1, p1) = parse_mix_arg(args[1], current_color)?;
        let (c2, p2) = parse_mix_arg(args[2], current_color)?;

        let (w1, w2, alpha_mul) = match (p1, p2) {
            (None, None) => (0.5_f32, 0.5_f32, 1.0_f32),
            (Some(a), None) => {
                let a = a.clamp(0.0, 1.0);
                (a, 1.0 - a, 1.0)
            }
            (None, Some(b)) => {
                let b = b.clamp(0.0, 1.0);
                (1.0 - b, b, 1.0)
            }
            (Some(a), Some(b)) => {
                let sum = (a + b).max(0.0);
                if sum <= 1e-6 {
                    return Some(CssColor::TRANSPARENT);
                }
                let mul = sum.min(1.0);
                (a / sum, b / sum, mul)
            }
        };

        if space.contains("oklab") || space.contains("oklch") {
            let (l1, a1, b1, al1) = rgba_to_oklab(c1);
            let (l2, a2, b2, al2) = rgba_to_oklab(c2);
            let a_prem = al1 * w1 + al2 * w2;
            if a_prem <= 1e-6 {
                return Some(CssColor::TRANSPARENT);
            }
            let l = (l1 * al1 * w1 + l2 * al2 * w2) / a_prem;
            let a = (a1 * al1 * w1 + a2 * al2 * w2) / a_prem;
            let b = (b1 * al1 * w1 + b2 * al2 * w2) / a_prem;
            return Some(oklab_to_rgba(l, a, b, (a_prem * alpha_mul).clamp(0.0, 1.0)));
        }

        // sRGB premultiplied interpolation
        let al1 = f32::from(c1.a) / 255.0;
        let al2 = f32::from(c2.a) / 255.0;
        let a_prem = al1 * w1 + al2 * w2;
        if a_prem <= 1e-6 {
            return Some(CssColor::TRANSPARENT);
        }
        let r = ((f32::from(c1.r) * al1 * w1 + f32::from(c2.r) * al2 * w2) / a_prem)
            .clamp(0.0, 255.0)
            .round() as u8;
        let g = ((f32::from(c1.g) * al1 * w1 + f32::from(c2.g) * al2 * w2) / a_prem)
            .clamp(0.0, 255.0)
            .round() as u8;
        let b = ((f32::from(c1.b) * al1 * w1 + f32::from(c2.b) * al2 * w2) / a_prem)
            .clamp(0.0, 255.0)
            .round() as u8;
        let a_out = ((a_prem * alpha_mul).clamp(0.0, 1.0) * 255.0).round() as u8;
        return Some(CssColor::rgba(r, g, b, a_out));
    }

    None
}

fn parse_mix_arg(raw: &str, current_color: CssColor) -> Option<(CssColor, Option<f32>)> {
    let toks = split_top_level_ws(raw.trim());
    if toks.is_empty() {
        return None;
    }
    if toks.len() == 1 {
        return Some((parse_color(toks[0], current_color)?, None));
    }
    if let Some(pct_str) = toks[toks.len() - 1].strip_suffix('%') {
        if let Ok(pct) = pct_str.trim().parse::<f32>() {
            let color_str = toks[..toks.len() - 1].join(" ");
            return Some((parse_color(&color_str, current_color)?, Some(pct * 0.01)));
        }
    }
    if let Some(pct_str) = toks[0].strip_suffix('%') {
        if let Ok(pct) = pct_str.trim().parse::<f32>() {
            let color_str = toks[1..].join(" ");
            return Some((parse_color(&color_str, current_color)?, Some(pct * 0.01)));
        }
    }
    Some((parse_color(raw.trim(), current_color)?, None))
}
