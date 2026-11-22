use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::Path;

use rsmotion_layout::{
    ComputedStyle, CssColor, FontSlantMode, LineHeight, TextAlignMode, TextDecorationMode,
    WhiteSpaceMode,
};
use rsmotion_media::{decode_font_to_sfnt, resolve_or_fetch_asset};
use rsmotion_scene::RegisteredFont;
use skia_safe::font_arguments::{variation_position::Coordinate, VariationPosition};
use skia_safe::font_style::{Slant, Weight, Width};
use skia_safe::textlayout::{
    FontCollection, Paragraph, ParagraphBuilder, ParagraphStyle, TextAlign, TextDecoration,
    TextShadow as SkTextShadow, TextStyle, TypefaceFontProvider,
};
use skia_safe::{
    Canvas, Color, Data, FontArguments, FontHinting, FontMgr, FontStyle, Paint, Point, Typeface,
};
use tracing::warn;

const MAX_MEASURE_CACHE_ENTRIES: usize = 4096;
const MAX_PARA_CACHE_ENTRIES: usize = 512;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct TextMeasureKey {
    text: String,
    font_family: Vec<String>,
    font_size_bits: u32,
    font_weight: u16,
    font_style: FontSlantMode,
    line_height_tag: u8,
    line_height_bits: u32,
    letter_spacing_bits: u32,
    text_align: TextAlignMode,
    max_width_bits: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ParagraphCacheKey {
    measure: TextMeasureKey,
    text_decoration: TextDecorationMode,
    color: CssColor,
    text_shadows: Vec<(u32, u32, u32, CssColor)>,
}

fn build_measure_key(text: &str, style: &ComputedStyle, max_width: Option<f32>) -> TextMeasureKey {
    let (line_height_tag, line_height_bits) = match style.line_height {
        LineHeight::Normal => (0u8, 0u32),
        LineHeight::Number(n) => (1u8, n.to_bits()),
        LineHeight::Px(p) => (2u8, p.to_bits()),
    };
    TextMeasureKey {
        text: text.to_string(),
        font_family: style.font_family.clone(),
        font_size_bits: style.font_size.to_bits(),
        font_weight: style.font_weight,
        font_style: style.font_style,
        line_height_tag,
        line_height_bits,
        letter_spacing_bits: style.letter_spacing.to_bits(),
        text_align: style.text_align,
        max_width_bits: max_width.map(|mw| mw.max(0.0).to_bits()),
    }
}

fn build_para_key(text: &str, style: &ComputedStyle, layout_w: f32) -> ParagraphCacheKey {
    ParagraphCacheKey {
        measure: build_measure_key(text, style, Some(layout_w)),
        text_decoration: style.text_decoration,
        color: style.color,
        text_shadows: style
            .text_shadows
            .iter()
            .map(|sh| {
                (
                    sh.offset_x.to_bits(),
                    sh.offset_y.to_bits(),
                    sh.blur_radius.to_bits(),
                    sh.color,
                )
            })
            .collect(),
    }
}

/// Manages in-memory pre-seeded Skia fonts, custom `FontFace` assets, and cached `Paragraph` layouts.
pub struct FontCache {
    system_font_mgr: FontMgr,
    asset_provider: TypefaceFontProvider,
    fast_provider: RefCell<TypefaceFontProvider>,
    font_collection: RefCell<FontCollection>,
    known_families: RefCell<HashMap<String, bool>>,
    custom_families: HashSet<String>,
    custom_subset_fallbacks: HashMap<String, Vec<String>>,
    custom_subset_aliases: HashMap<(String, String), String>,
    svg_fast_families: HashSet<String>,
    loaded_keys: HashSet<String>,
    has_custom_fonts: bool,
    measure_cache: RefCell<HashMap<TextMeasureKey, (f32, f32)>>,
    para_cache: RefCell<HashMap<ParagraphCacheKey, Paragraph>>,
}

impl Default for FontCache {
    fn default() -> Self {
        Self::new()
    }
}

fn standard_seed_styles() -> [FontStyle; 4] {
    [
        FontStyle::normal(),
        FontStyle::bold(),
        FontStyle::italic(),
        FontStyle::bold_italic(),
    ]
}

impl FontCache {
    pub fn new() -> Self {
        let system_font_mgr = FontMgr::new();
        let asset_provider = TypefaceFontProvider::new();
        let mut fast_provider = TypefaceFontProvider::new();
        let mut known_families = HashMap::new();
        let mut svg_fast_families = HashSet::new();

        // Pre-seed standard CSS & generic font aliases using fontconfig's exact host mapping
        // once at startup so Paragraph shaping hits RAM instead of scanning libfontconfig per frame.
        const STANDARD_FAMILIES: &[&str] = &[
            "DejaVu Sans",
            "sans-serif",
            "Arial",
            "Helvetica",
            "system-ui",
            "-apple-system",
            "BlinkMacSystemFont",
            "Segoe UI",
            "Roboto",
            "DejaVu Serif",
            "serif",
            "Times New Roman",
            "Georgia",
            "DejaVu Sans Mono",
            "monospace",
            "Courier New",
            "ui-monospace",
        ];

        for &alias in STANDARD_FAMILIES {
            let mut registered_any = false;
            for st in standard_seed_styles() {
                if let Some(tf) = system_font_mgr.match_family_style(alias, st) {
                    fast_provider.register_typeface(tf, Some(alias));
                    registered_any = true;
                }
            }
            if registered_any {
                let lower = alias.to_ascii_lowercase();
                known_families.insert(lower.clone(), true);
                svg_fast_families.insert(lower);
            }
        }

        let mut font_collection = FontCollection::new();
        font_collection.set_dynamic_font_manager(Some(fast_provider.clone().into()));
        font_collection.set_default_font_manager(system_font_mgr.clone(), None);

        Self {
            system_font_mgr,
            asset_provider,
            fast_provider: RefCell::new(fast_provider),
            font_collection: RefCell::new(font_collection),
            known_families: RefCell::new(known_families),
            custom_families: HashSet::new(),
            custom_subset_fallbacks: HashMap::new(),
            custom_subset_aliases: HashMap::new(),
            svg_fast_families,
            loaded_keys: HashSet::new(),
            has_custom_fonts: false,
            measure_cache: RefCell::new(HashMap::with_capacity(256)),
            para_cache: RefCell::new(HashMap::with_capacity(128)),
        }
    }

    pub fn svg_font_mgr(&self) -> FontMgr {
        self.system_font_mgr.clone()
    }

    /// Returns fast in-memory `TypefaceFontProvider` when every SVG `font-family` is cached.
    pub fn svg_font_mgr_for_xml(&self, xml: &str) -> FontMgr {
        if self.svg_fast_families.is_empty() {
            return self.system_font_mgr.clone();
        }
        let mut rest = xml;
        while let Some(pos) = rest.find("font-family") {
            let after_key = &rest[pos + "font-family".len()..];
            let trimmed = after_key.trim_start();
            let list_slice = if let Some(after_eq) = trimmed.strip_prefix('=') {
                let s = after_eq.trim_start();
                if let Some(q) = s.chars().next().filter(|&c| c == '"' || c == '\'') {
                    let inner = &s[q.len_utf8()..];
                    inner.split(q).next().unwrap_or("")
                } else {
                    ""
                }
            } else if let Some(after_colon) = trimmed.strip_prefix(':') {
                after_colon.split([';', '"', '\'']).next().unwrap_or("")
            } else {
                ""
            };
            if let Some(primary) = list_slice
                .split(',')
                .map(|s| s.trim().trim_matches(|c| c == '"' || c == '\''))
                .find(|s| !s.is_empty())
            {
                if !self
                    .svg_fast_families
                    .contains(&primary.to_ascii_lowercase())
                {
                    return self.system_font_mgr.clone();
                }
            }
            rest = after_key;
        }
        self.fast_provider.borrow().clone().into()
    }

    pub fn font_collection(&self) -> FontCollection {
        self.font_collection.borrow().clone()
    }

    fn filter_and_resolve_families(&self, requested: &[String]) -> Vec<String> {
        let mut out = Vec::with_capacity(requested.len() + 2);
        let mut updated_fast = false;

        for fam in requested {
            let clean = fam.trim();
            if clean.is_empty() {
                continue;
            }
            let lower = clean.to_ascii_lowercase();
            if self.custom_families.contains(&lower)
                || self.custom_subset_fallbacks.contains_key(&lower)
            {
                if self.custom_families.contains(&lower) {
                    out.push(clean.to_string());
                }
                if let Some(subs) = self.custom_subset_fallbacks.get(&lower) {
                    for sub in subs {
                        out.push(sub.clone());
                    }
                }
                continue;
            }

            if let Some(&known) = self.known_families.borrow().get(&lower) {
                if known {
                    out.push(clean.to_string());
                }
                continue;
            }

            let mut set = self.system_font_mgr.match_family(clean);
            if set.count() > 0 {
                let mut fp = self.fast_provider.borrow_mut();
                for st in standard_seed_styles() {
                    if let Some(tf) = self.system_font_mgr.match_family_style(clean, st) {
                        fp.register_typeface(tf, Some(clean));
                    }
                }
                updated_fast = true;
                self.known_families.borrow_mut().insert(lower, true);
                out.push(clean.to_string());
            } else {
                self.known_families.borrow_mut().insert(lower, false);
            }
        }

        if updated_fast {
            let fp_mgr: FontMgr = self.fast_provider.borrow().clone().into();
            self.font_collection
                .borrow_mut()
                .set_dynamic_font_manager(Some(fp_mgr));
        }

        if !out.iter().any(|f| f.eq_ignore_ascii_case("dejavu sans")) {
            out.push("DejaVu Sans".to_string());
        }
        if !out.iter().any(|f| f.eq_ignore_ascii_case("sans-serif")) {
            out.push("sans-serif".to_string());
        }
        out
    }

    /// Registers any newly discovered `@font-face` / `FontFace` assets (TTF/OTF/WOFF/WOFF2) into Skia.
    pub fn sync_registered_fonts(
        &mut self,
        fonts: &[RegisteredFont],
        bundle_dir: &Path,
        offline: bool,
    ) {
        let mut added = false;
        for f in fonts {
            let resolved = f
                .resolved_path
                .clone()
                .or_else(|| resolve_or_fetch_asset(bundle_dir, &f.src, offline));
            let Some(path) = resolved else {
                continue;
            };
            let clean_family = f.family.trim().trim_matches(|c| c == '"' || c == '\'');
            if clean_family.is_empty() {
                continue;
            }
            let lower = clean_family.to_ascii_lowercase();
            let basic_latin = unicode_range_covers_basic_latin(f.unicode_range.as_deref());
            let weight_range = parse_font_weight_range(f.weight.as_deref());
            let key = format!(
                "{lower}:{}:{}:{}:{basic_latin}",
                path.display(),
                f.weight.as_deref().unwrap_or(""),
                f.style.as_deref().unwrap_or("")
            );
            if !self.loaded_keys.insert(key) {
                continue;
            }
            let Ok(raw_bytes) = std::fs::read(&path) else {
                warn!("failed to read font {}", path.display());
                continue;
            };
            let Some(sfnt_bytes) = decode_font_to_sfnt(&raw_bytes) else {
                warn!(
                    "failed to decode font container (WOFF/WOFF2/SFNT) at {}",
                    path.display()
                );
                continue;
            };
            let data = Data::new_copy(&sfnt_bytes);
            let Some(tf) = self.system_font_mgr.new_from_data(data, None) else {
                warn!("failed to parse SFNT font bytes at {}", path.display());
                continue;
            };

            let target_alias = if basic_latin {
                clean_family.to_string()
            } else {
                let path_key = path.to_string_lossy().into_owned();
                let alias_key = (lower.clone(), path_key);
                if let Some(existing) = self.custom_subset_aliases.get(&alias_key) {
                    existing.clone()
                } else {
                    let subs = self
                        .custom_subset_fallbacks
                        .entry(lower.clone())
                        .or_default();
                    let alias = format!("{lower}__sub_{}", subs.len());
                    subs.push(alias.clone());
                    self.custom_subset_aliases.insert(alias_key, alias.clone());
                    alias
                }
            };

            self.register_typeface_with_variations(&tf, &target_alias, weight_range);
            if basic_latin {
                self.custom_families.insert(lower.clone());
                self.svg_fast_families.insert(lower);
            }
            self.has_custom_fonts = true;
            added = true;
        }
        if added && self.has_custom_fonts {
            let mut fc = self.font_collection.borrow_mut();
            fc.set_asset_font_manager(Some(self.asset_provider.clone().into()));
            fc.set_dynamic_font_manager(Some(self.fast_provider.borrow().clone().into()));
            fc.clear_caches();
            self.measure_cache.borrow_mut().clear();
            self.para_cache.borrow_mut().clear();
        }
    }

    fn register_typeface_with_variations(
        &mut self,
        tf: &Typeface,
        alias: &str,
        weight_range: (u16, u16),
    ) {
        let mut registered_var = false;
        if let Some(axes) = tf.variation_design_parameters() {
            if let Some(wght) = axes.iter().find(|a| a.tag == Coordinate::wght) {
                if wght.max > wght.min {
                    let mut candidates: Vec<u16> = [100, 200, 300, 400, 500, 600, 700, 800, 900]
                        .into_iter()
                        .filter(|w| (weight_range.0..=weight_range.1).contains(w))
                        .collect();
                    if candidates.is_empty() {
                        candidates.push(weight_range.0.clamp(100, 900));
                    }
                    for w in candidates {
                        let wf = w as f32;
                        if wf + 0.5 < wght.min || wf - 0.5 > wght.max {
                            continue;
                        }
                        let coords = [Coordinate {
                            axis: Coordinate::wght,
                            value: wf.clamp(wght.min, wght.max),
                        }];
                        let args =
                            FontArguments::new().set_variation_design_position(VariationPosition {
                                coordinates: &coords,
                            });
                        if let Some(cloned) = tf.clone_with_arguments(&args) {
                            self.asset_provider
                                .register_typeface(cloned.clone(), Some(alias));
                            self.fast_provider
                                .borrow_mut()
                                .register_typeface(cloned, Some(alias));
                            registered_var = true;
                        }
                    }
                }
            }
        }
        if !registered_var {
            self.asset_provider
                .register_typeface(tf.clone(), Some(alias));
            self.fast_provider
                .borrow_mut()
                .register_typeface(tf.clone(), Some(alias));
        }
    }

    /// Parses a CSS `ctx.font` string into `(size_px, weight, slant, families)`.
    pub fn parse_css_canvas_font(spec: &str) -> (f32, u16, FontSlantMode, Vec<String>) {
        let trimmed = spec.trim();
        let bytes = trimmed.as_bytes();
        let mut size_px = 10.0_f32;
        let mut weight = 400_u16;
        let mut slant = FontSlantMode::Normal;

        // Locate `<size>px` or `<size>pt` / `rem` / `em` outside quotes
        let mut in_quote: Option<u8> = None;
        let mut size_span: Option<(usize, usize)> = None;
        let mut tok_start = 0usize;
        let mut i = 0usize;
        while i <= bytes.len() {
            let b = bytes.get(i).copied().unwrap_or(b' ');
            if let Some(q) = in_quote {
                if b == q {
                    in_quote = None;
                }
            } else if b == b'"' || b == b'\'' {
                in_quote = Some(b);
            } else if b.is_ascii_whitespace() || i == bytes.len() {
                if i > tok_start {
                    let tok = &trimmed[tok_start..i];
                    let base = tok.split('/').next().unwrap_or(tok).trim();
                    let lower = base.to_ascii_lowercase();
                    if let Some(num) = lower.strip_suffix("px").and_then(|s| s.parse::<f32>().ok())
                    {
                        size_px = num.max(1.0);
                        size_span = Some((tok_start, i));
                        break;
                    } else if let Some(num) =
                        lower.strip_suffix("pt").and_then(|s| s.parse::<f32>().ok())
                    {
                        size_px = (num * (96.0 / 72.0)).max(1.0);
                        size_span = Some((tok_start, i));
                        break;
                    } else if let Some(num) = lower
                        .strip_suffix("rem")
                        .or_else(|| lower.strip_suffix("em"))
                        .and_then(|s| s.parse::<f32>().ok())
                    {
                        size_px = (num * 16.0).max(1.0);
                        size_span = Some((tok_start, i));
                        break;
                    } else if lower == "italic" {
                        slant = FontSlantMode::Italic;
                    } else if lower == "oblique" {
                        slant = FontSlantMode::Oblique;
                    } else if lower == "bold" {
                        weight = 700;
                    } else if let Ok(w) = lower.parse::<u16>() {
                        if (1..=1000).contains(&w) {
                            weight = w;
                        }
                    }
                }
                tok_start = i + 1;
            }
            i += 1;
        }

        let family_raw = match size_span {
            Some((_, end)) => trimmed[end..].trim(),
            None => "",
        };
        let families: Vec<String> = family_raw
            .split(',')
            .map(|s| s.trim().trim_matches(|c| c == '"' || c == '\'').to_string())
            .filter(|s| !s.is_empty())
            .collect();
        let families = if families.is_empty() {
            vec!["sans-serif".to_string()]
        } else {
            families
        };
        (size_px, weight, slant, families)
    }

    /// Builds a single-line Skia `Paragraph` for Canvas 2D text using `paint` and returns `(para, width, ascent, descent)`.
    pub fn build_canvas_paragraph(
        &self,
        text: &str,
        css_font: &str,
        paint: Option<&Paint>,
    ) -> (Paragraph, f32, f32, f32) {
        let (size_px, weight, slant_mode, req_families) = Self::parse_css_canvas_font(css_font);
        let mut para_style = ParagraphStyle::new();
        para_style.turn_hinting_off();
        para_style.set_letter_spacing_by_css_spec(true);
        para_style.set_fake_missing_font_styles(true);

        let resolved_families = self.filter_and_resolve_families(&req_families);
        let families: Vec<&str> = resolved_families.iter().map(String::as_str).collect();
        let slant = match slant_mode {
            FontSlantMode::Normal => Slant::Upright,
            FontSlantMode::Italic => Slant::Italic,
            FontSlantMode::Oblique => Slant::Oblique,
        };

        let mut ts = TextStyle::new();
        ts.set_font_families(&families);
        ts.set_font_size(size_px.max(0.1));
        ts.set_font_style(FontStyle::new(
            Weight::from(i32::from(weight)),
            Width::NORMAL,
            slant,
        ));
        ts.set_font_hinting(FontHinting::None);
        ts.set_subpixel(true);
        if let Some(p) = paint {
            ts.set_foreground_paint(p);
        }

        let fc = self.font_collection.borrow().clone();
        let mut builder = ParagraphBuilder::new(&para_style, fc);
        builder.push_style(&ts);
        builder.add_text(text);
        let mut para = builder.build();
        para.layout(1e6);

        let width = para.max_intrinsic_width().max(0.0);
        let metrics = para.get_line_metrics();
        let (ascent, descent) = metrics
            .first()
            .map(|lm| (lm.ascent as f32, lm.descent as f32))
            .unwrap_or((size_px * 0.8, size_px * 0.2));
        (para, width, ascent, descent)
    }

    /// Measures text dimensions `(width, ascent, descent)` for HTML5 `ctx.measureText`.
    pub fn measure_canvas_text(&self, text: &str, css_font: &str) -> (f32, f32, f32) {
        let (_, w, a, d) = self.build_canvas_paragraph(text, css_font, None);
        (w, a, d)
    }

    /// Builds and lays out a Skia `Paragraph` matching CSS typography rules.
    pub fn build_paragraph(&self, text: &str, style: &ComputedStyle, max_width: f32) -> Paragraph {
        let mut para_style = ParagraphStyle::new();
        para_style.turn_hinting_off();
        para_style.set_letter_spacing_by_css_spec(true);
        para_style.set_fake_missing_font_styles(true);
        para_style.set_text_align(match style.text_align {
            TextAlignMode::Left => TextAlign::Left,
            TextAlignMode::Center => TextAlign::Center,
            TextAlignMode::Right => TextAlign::Right,
            TextAlignMode::Justify => TextAlign::Justify,
        });

        let resolved_families = self.filter_and_resolve_families(&style.font_family);
        let families: Vec<&str> = resolved_families.iter().map(String::as_str).collect();

        let slant = match style.font_style {
            FontSlantMode::Normal => Slant::Upright,
            FontSlantMode::Italic => Slant::Italic,
            FontSlantMode::Oblique => Slant::Oblique,
        };

        let mut ts = TextStyle::new();
        ts.set_font_families(&families);
        ts.set_font_size(style.font_size.max(0.1));
        ts.set_font_style(FontStyle::new(
            Weight::from(i32::from(style.font_weight)),
            Width::NORMAL,
            slant,
        ));
        ts.set_color(Color::from_argb(
            style.color.a,
            style.color.r,
            style.color.g,
            style.color.b,
        ));
        ts.set_letter_spacing(style.letter_spacing);
        ts.set_font_hinting(FontHinting::None);
        ts.set_subpixel(true);

        match style.line_height {
            LineHeight::Normal => {}
            LineHeight::Number(n) => {
                ts.set_height(n);
                ts.set_height_override(true);
                ts.set_half_leading(true);
            }
            LineHeight::Px(px) => {
                if style.font_size > 0.0 {
                    ts.set_height(px / style.font_size);
                    ts.set_height_override(true);
                    ts.set_half_leading(true);
                }
            }
        }

        match style.text_decoration {
            TextDecorationMode::None => {}
            TextDecorationMode::Underline => ts.set_decoration_type(TextDecoration::UNDERLINE),
            TextDecorationMode::LineThrough => ts.set_decoration_type(TextDecoration::LINE_THROUGH),
        }

        for sh in &style.text_shadows {
            let c = Color::from_argb(sh.color.a, sh.color.r, sh.color.g, sh.color.b);
            ts.add_shadow(SkTextShadow::new(
                c,
                (sh.offset_x, sh.offset_y),
                f64::from(sh.blur_radius * 0.5),
            ));
        }

        let fc = self.font_collection.borrow().clone();
        let mut builder = ParagraphBuilder::new(&para_style, fc);
        builder.push_style(&ts);
        builder.add_text(text);
        let mut para = builder.build();
        para.layout(max_width.max(0.0));
        para
    }

    /// Paints a cached `Paragraph` (building and memoizing on miss) to eliminate redundant shaping.
    pub fn paint_paragraph(
        &self,
        canvas: &Canvas,
        text: &str,
        style: &ComputedStyle,
        max_width: f32,
        origin: Point,
    ) {
        let layout_w = max_width.max(0.0);
        let key = build_para_key(text, style, layout_w);
        if let Some(para) = self.para_cache.borrow().get(&key) {
            para.paint(canvas, origin);
            return;
        }

        let para = self.build_paragraph(text, style, layout_w);
        para.paint(canvas, origin);
        let mut cache = self.para_cache.borrow_mut();
        if cache.len() >= MAX_PARA_CACHE_ENTRIES {
            cache.clear();
        }
        cache.insert(key, para);
    }

    pub fn measure_text_box(
        &self,
        text: &str,
        style: &ComputedStyle,
        max_width: Option<f32>,
    ) -> (f32, f32) {
        let effective_max_width = match style.white_space {
            WhiteSpaceMode::Nowrap | WhiteSpaceMode::Pre => None,
            _ => max_width,
        };
        let key = build_measure_key(text, style, effective_max_width);
        if let Some(&cached) = self.measure_cache.borrow().get(&key) {
            return cached;
        }

        let measured = match effective_max_width {
            None => {
                let para = self.build_paragraph(text, style, 1e6);
                (para.max_intrinsic_width().ceil(), para.height().ceil())
            }
            Some(mw) => {
                let para = self.build_paragraph(text, style, mw.max(0.0));
                (para.longest_line().min(mw).ceil(), para.height().ceil())
            }
        };

        let mut cache = self.measure_cache.borrow_mut();
        if cache.len() >= MAX_MEASURE_CACHE_ENTRIES {
            cache.clear();
        }
        cache.insert(key, measured);
        measured
    }
}

fn unicode_range_covers_basic_latin(range_opt: Option<&str>) -> bool {
    let Some(raw) = range_opt else {
        return true;
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return true;
    }
    for part in trimmed.split(',') {
        let tok = part.trim();
        let hex = tok
            .strip_prefix("U+")
            .or_else(|| tok.strip_prefix("u+"))
            .unwrap_or(tok)
            .trim();
        if hex.is_empty() {
            continue;
        }
        let parsed = if hex.contains('?') {
            let start_s = hex.replace('?', "0");
            let end_s = hex.replace('?', "F");
            u32::from_str_radix(&start_s, 16)
                .ok()
                .zip(u32::from_str_radix(&end_s, 16).ok())
        } else if let Some((a, b)) = hex.split_once('-') {
            u32::from_str_radix(a.trim(), 16)
                .ok()
                .zip(u32::from_str_radix(b.trim(), 16).ok())
        } else {
            u32::from_str_radix(hex, 16).ok().map(|v| (v, v))
        };
        if let Some((start, end)) = parsed {
            if start <= 0x007F && end >= 0x0020 {
                return true;
            }
        }
    }
    false
}

fn parse_font_weight_num(tok: &str) -> Option<u16> {
    let lower = tok.trim().to_ascii_lowercase();
    match lower.as_str() {
        "normal" => Some(400),
        "bold" => Some(700),
        _ => lower
            .parse::<f32>()
            .ok()
            .map(|v| v.round().clamp(100.0, 900.0) as u16),
    }
}

fn parse_font_weight_range(weight_opt: Option<&str>) -> (u16, u16) {
    let Some(raw) = weight_opt.map(str::trim).filter(|s| !s.is_empty()) else {
        return (100, 900);
    };
    let nums: Vec<u16> = raw
        .split_whitespace()
        .filter_map(parse_font_weight_num)
        .collect();
    match nums.as_slice() {
        [single] => (*single, *single),
        [a, b, ..] => ((*a).min(*b), (*a).max(*b)),
        [] => (100, 900),
    }
}
