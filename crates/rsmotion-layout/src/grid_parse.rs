use taffy::prelude::*;
use taffy::style::GridTemplateRepetition;

use crate::style::{
    parse_css_number, parse_length_px, parse_lp, split_top_level, split_top_level_ws,
};

fn parse_min_track(raw: &str, fs: f32, vw: f32, vh: f32) -> Option<MinTrackSizingFunction> {
    let s = raw.trim().to_ascii_lowercase();
    match s.as_str() {
        "auto" => Some(auto()),
        "min-content" => Some(min_content()),
        "max-content" => Some(max_content()),
        _ if s.ends_with('%') => {
            parse_css_number(s.trim_end_matches('%')).map(|p| percent(p * 0.01))
        }
        _ => parse_length_px(&s, fs, vw, vh).map(|px| length(px.max(0.0))),
    }
}

fn parse_max_track(raw: &str, fs: f32, vw: f32, vh: f32) -> Option<MaxTrackSizingFunction> {
    let s = raw.trim().to_ascii_lowercase();
    match s.as_str() {
        "auto" => Some(auto()),
        "min-content" => Some(min_content()),
        "max-content" => Some(max_content()),
        _ if s.ends_with("fr") => {
            parse_css_number(s.trim_end_matches("fr")).map(|v| fr(v.max(0.0)))
        }
        _ if s.starts_with("fit-content(") && s.ends_with(')') => {
            let inner = &s["fit-content(".len()..s.len() - 1];
            parse_lp(inner, fs, vw, vh).map(fit_content)
        }
        _ if s.ends_with('%') => {
            parse_css_number(s.trim_end_matches('%')).map(|p| percent(p * 0.01))
        }
        _ => parse_length_px(&s, fs, vw, vh).map(|px| length(px.max(0.0))),
    }
}

/// Parses a single `<track-size>` (`1fr`, `200px`, `50%`, `auto`, `min-content`, `minmax(0, 1fr)`).
pub fn parse_track_size(raw: &str, fs: f32, vw: f32, vh: f32) -> Option<TrackSizingFunction> {
    let trimmed = raw.trim();
    let lower = trimmed.to_ascii_lowercase();
    if lower.starts_with("minmax(") && trimmed.ends_with(')') {
        let inner = &trimmed["minmax(".len()..trimmed.len() - 1];
        let args = split_top_level(inner, ',');
        if args.len() != 2 {
            return None;
        }
        let mn = parse_min_track(args[0], fs, vw, vh)?;
        let mx = parse_max_track(args[1], fs, vw, vh)?;
        return Some(minmax(mn, mx));
    }
    if lower.starts_with("fit-content(") && trimmed.ends_with(')') {
        let inner = &trimmed["fit-content(".len()..trimmed.len() - 1];
        let lp = parse_lp(inner, fs, vw, vh)?;
        return Some(minmax(auto(), fit_content(lp)));
    }
    if let Some(fr_part) = lower.strip_suffix("fr") {
        let val = parse_css_number(fr_part)?;
        return Some(minmax(auto(), fr(val.max(0.0))));
    }
    match lower.as_str() {
        "auto" => Some(auto()),
        "min-content" => Some(min_content()),
        "max-content" => Some(max_content()),
        _ if lower.ends_with('%') => {
            let p = parse_css_number(lower.trim_end_matches('%'))?;
            Some(percent(p * 0.01))
        }
        _ => parse_length_px(trimmed, fs, vw, vh).map(|px| length(px.max(0.0))),
    }
}

/// Parses `grid-template-columns` or `grid-template-rows`.
pub fn parse_grid_template_tracks(
    raw: &str,
    fs: f32,
    vw: f32,
    vh: f32,
) -> Option<Vec<GridTemplateComponent<String>>> {
    let trimmed = raw.trim();
    if trimmed.eq_ignore_ascii_case("none") {
        return Some(Vec::new());
    }
    let tokens = split_top_level_ws(trimmed);
    let mut out = Vec::new();
    for tok in tokens {
        // Ignore named grid line lists `[header-start]`
        if tok.starts_with('[') && tok.ends_with(']') {
            continue;
        }
        let lower = tok.to_ascii_lowercase();
        if lower.starts_with("repeat(") && tok.ends_with(')') {
            let inner = &tok["repeat(".len()..tok.len() - 1];
            let args = split_top_level(inner, ',');
            if args.len() != 2 {
                continue;
            }
            let rep_str = args[0].trim().to_ascii_lowercase();
            let count = match rep_str.as_str() {
                "auto-fill" => RepetitionCount::AutoFill,
                "auto-fit" => RepetitionCount::AutoFit,
                _ => match rep_str.parse::<u16>() {
                    Ok(n) if n > 0 => RepetitionCount::Count(n),
                    _ => continue,
                },
            };
            let repeated_tracks: Vec<TrackSizingFunction> = split_top_level_ws(args[1])
                .into_iter()
                .filter(|t| !(t.starts_with('[') && t.ends_with(']')))
                .filter_map(|t| parse_track_size(t, fs, vw, vh))
                .collect();
            if !repeated_tracks.is_empty() {
                out.push(GridTemplateComponent::Repeat(GridTemplateRepetition {
                    count,
                    tracks: repeated_tracks,
                    line_names: Vec::new(),
                }));
            }
        } else if let Some(track) = parse_track_size(tok, fs, vw, vh) {
            out.push(GridTemplateComponent::Single(track));
        }
    }
    Some(out)
}

/// Parses `grid-auto-columns` or `grid-auto-rows`.
pub fn parse_grid_auto_tracks(raw: &str, fs: f32, vw: f32, vh: f32) -> Vec<TrackSizingFunction> {
    split_top_level_ws(raw.trim())
        .into_iter()
        .filter_map(|tok| parse_track_size(tok, fs, vw, vh))
        .collect()
}

pub fn parse_grid_auto_flow(raw: &str) -> Option<GridAutoFlow> {
    let lower = raw.trim().to_ascii_lowercase();
    let has_col = lower.contains("column");
    let has_dense = lower.contains("dense");
    if has_col && has_dense {
        Some(GridAutoFlow::ColumnDense)
    } else if has_col {
        Some(GridAutoFlow::Column)
    } else if has_dense {
        Some(GridAutoFlow::RowDense)
    } else if lower.contains("row") {
        Some(GridAutoFlow::Row)
    } else {
        None
    }
}

pub fn parse_grid_placement(raw: &str) -> Option<GridPlacement<String>> {
    let s = raw.trim();
    if s.eq_ignore_ascii_case("auto") {
        return Some(GridPlacement::Auto);
    }
    if let Some(rest) = s.to_ascii_lowercase().strip_prefix("span") {
        let count_str = rest.trim();
        let n = if count_str.is_empty() {
            1
        } else {
            count_str.parse::<u16>().ok()?.max(1)
        };
        return Some(span(n));
    }
    if let Ok(idx) = s.parse::<i16>() {
        return Some(if idx == 0 {
            GridPlacement::Auto
        } else {
            line(idx)
        });
    }
    None
}

pub fn parse_grid_line(raw: &str) -> Option<Line<GridPlacement<String>>> {
    if let Some((a, b)) = raw.split_once('/') {
        Some(Line {
            start: parse_grid_placement(a)?,
            end: parse_grid_placement(b)?,
        })
    } else {
        Some(Line {
            start: parse_grid_placement(raw)?,
            end: GridPlacement::Auto,
        })
    }
}

pub fn parse_align_items_kw(raw: &str) -> Option<AlignItems> {
    Some(match raw.trim().to_ascii_lowercase().as_str() {
        "stretch" => AlignItems::STRETCH,
        "flex-start" => AlignItems::FLEX_START,
        "start" => AlignItems::START,
        "flex-end" => AlignItems::FLEX_END,
        "end" => AlignItems::END,
        "center" => AlignItems::CENTER,
        "baseline" => AlignItems::BASELINE,
        _ => return None,
    })
}

pub fn parse_align_self_kw(raw: &str) -> Option<Option<AlignSelf>> {
    Some(match raw.trim().to_ascii_lowercase().as_str() {
        "auto" => None,
        "stretch" => Some(AlignSelf::STRETCH),
        "flex-start" => Some(AlignSelf::FLEX_START),
        "start" => Some(AlignSelf::START),
        "flex-end" => Some(AlignSelf::FLEX_END),
        "end" => Some(AlignSelf::END),
        "center" => Some(AlignSelf::CENTER),
        "baseline" => Some(AlignSelf::BASELINE),
        _ => return None,
    })
}

pub fn parse_align_content_kw(raw: &str) -> Option<AlignContent> {
    Some(match raw.trim().to_ascii_lowercase().as_str() {
        "flex-start" => AlignContent::FLEX_START,
        "start" => AlignContent::START,
        "flex-end" => AlignContent::FLEX_END,
        "end" => AlignContent::END,
        "center" => AlignContent::CENTER,
        "stretch" => AlignContent::STRETCH,
        "space-between" => AlignContent::SPACE_BETWEEN,
        "space-around" => AlignContent::SPACE_AROUND,
        "space-evenly" => AlignContent::SPACE_EVENLY,
        _ => return None,
    })
}

pub fn parse_justify_content_kw(raw: &str) -> Option<JustifyContent> {
    Some(match raw.trim().to_ascii_lowercase().as_str() {
        "flex-start" => JustifyContent::FLEX_START,
        "start" => JustifyContent::START,
        "flex-end" => JustifyContent::FLEX_END,
        "end" => JustifyContent::END,
        "center" => JustifyContent::CENTER,
        "stretch" => JustifyContent::STRETCH,
        "space-between" => JustifyContent::SPACE_BETWEEN,
        "space-around" => JustifyContent::SPACE_AROUND,
        "space-evenly" => JustifyContent::SPACE_EVENLY,
        _ => return None,
    })
}
