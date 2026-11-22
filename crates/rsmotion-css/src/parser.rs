use std::collections::{HashMap, HashSet};

use cssparser::{
    AtRuleParser, CowRcStr, DeclarationParser, ParseError, Parser, ParserState,
    QualifiedRuleParser, RuleBodyItemParser, RuleBodyParser, StyleSheetParser, Token,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CssDecl {
    pub name: String,
    pub value: String,
    pub important: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StyleRuleAst {
    pub selectors_raw: String,
    pub declarations: Vec<CssDecl>,
    pub layer_rank: Option<usize>,
    pub source_order: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FontFaceDecl {
    pub family: String,
    pub src_url: String,
    pub weight: Option<String>,
    pub style: Option<String>,
    pub unicode_range: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ParsedStylesheetAst {
    pub rules: Vec<StyleRuleAst>,
    pub font_faces: Vec<FontFaceDecl>,
    pub property_initials: HashMap<String, String>,
    pub layer_names: Vec<String>,
}

impl ParsedStylesheetAst {
    pub fn empty() -> Self {
        Self {
            rules: Vec::new(),
            font_faces: Vec::new(),
            property_initials: HashMap::new(),
            layer_names: Vec::new(),
        }
    }
}

struct ParseCtx {
    vw: f32,
    vh: f32,
    rules: Vec<StyleRuleAst>,
    font_faces: Vec<FontFaceDecl>,
    property_initials: HashMap<String, String>,
    layer_names: Vec<String>,
    next_order: usize,
    anon_layer_seq: usize,
}

impl ParseCtx {
    fn new(vw: f32, vh: f32) -> Self {
        Self {
            vw,
            vh,
            rules: Vec::new(),
            font_faces: Vec::new(),
            property_initials: HashMap::new(),
            layer_names: Vec::new(),
            next_order: 0,
            anon_layer_seq: 0,
        }
    }

    fn intern_layer(&mut self, name: &str) -> usize {
        if let Some(idx) = self.layer_names.iter().position(|n| n == name) {
            return idx;
        }
        let idx = self.layer_names.len();
        self.layer_names.push(name.to_string());
        idx
    }

    fn alloc_order(&mut self) -> usize {
        let ord = self.next_order;
        self.next_order += 1;
        ord
    }
}

#[derive(Clone)]
struct RuleScope {
    active: bool,
    layer_rank: Option<usize>,
    layer_prefix: Option<String>,
}

struct SheetParser<'ctx> {
    ctx: &'ctx mut ParseCtx,
    scope: RuleScope,
}

enum SheetPrelude {
    LayerStatement(Vec<String>),
    LayerBlock(Option<usize>, Option<String>),
    Media(bool),
    Supports(bool),
    FontFace,
    Property(String),
    Skip,
}

enum BodyItem {
    Decl(CssDecl),
    NestedAtRule,
}

fn drain_to_string<'i>(input: &mut Parser<'i>) -> String {
    input.skip_whitespace();
    let start = input.position();
    while input.next_including_whitespace_and_comments().is_ok() {}
    let end = input.position();
    strip_css_comments(input.slice(start..end).trim())
}

fn strip_css_comments(raw: &str) -> String {
    if !raw.contains("/*") {
        return raw.to_string();
    }
    let bytes = raw.as_bytes();
    let mut out = String::with_capacity(raw.len());
    let mut i = 0;
    let mut quote: Option<u8> = None;
    while i < bytes.len() {
        let b = bytes[i];
        if let Some(q) = quote {
            out.push(b as char);
            if b == b'\\' && i + 1 < bytes.len() {
                i += 1;
                out.push(bytes[i] as char);
            } else if b == q {
                quote = None;
            }
            i += 1;
            continue;
        }
        if b == b'"' || b == b'\'' {
            quote = Some(b);
            out.push(b as char);
            i += 1;
            continue;
        }
        if b == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'*' {
            i += 2;
            while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                i += 1;
            }
            i = (i + 2).min(bytes.len());
            continue;
        }
        // Handle multi-byte UTF-8 safely by advancing char boundary
        let ch = raw[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out.trim().to_string()
}

fn extract_important_and_value(raw: &str) -> (String, bool) {
    let trimmed = raw.trim();
    let bytes = trimmed.as_bytes();
    let lower = trimmed.to_ascii_lowercase();
    if let Some(idx) = lower.rfind("important") {
        if lower[idx..].trim_end() == "important" {
            let before = trimmed[..idx].trim_end();
            if let Some(val_part) = before.strip_suffix('!') {
                // Verify `!important` is not inside quotes
                let mut in_quote: Option<u8> = None;
                for &b in &bytes[..before.len() - 1] {
                    match (in_quote, b) {
                        (None, b'"' | b'\'') => in_quote = Some(b),
                        (Some(q), c) if q == c => in_quote = None,
                        _ => {}
                    }
                }
                if in_quote.is_none() {
                    return (val_part.trim().to_string(), true);
                }
            }
        }
    }
    (trimmed.to_string(), false)
}

struct DeclBlockParser;

impl<'i> DeclarationParser<'i> for DeclBlockParser {
    type Declaration = BodyItem;
    type Error = ();

    fn parse_value(
        &mut self,
        name: CowRcStr<'i>,
        input: &mut Parser<'i>,
        _declaration_start: &ParserState,
    ) -> Result<Self::Declaration, ParseError<Self::Error>> {
        let raw_val = drain_to_string(input);
        let (value, important) = extract_important_and_value(&raw_val);
        let prop_name = if name.starts_with("--") {
            name.to_string()
        } else {
            name.to_ascii_lowercase()
        };
        Ok(BodyItem::Decl(CssDecl {
            name: prop_name,
            value,
            important,
        }))
    }
}

impl<'i> AtRuleParser<'i> for DeclBlockParser {
    type Prelude = ();
    type AtRule = BodyItem;
    type Error = ();

    fn parse_prelude(
        &mut self,
        _name: CowRcStr<'i>,
        input: &mut Parser<'i>,
    ) -> Result<Self::Prelude, ParseError<Self::Error>> {
        while input.next_including_whitespace_and_comments().is_ok() {}
        Ok(())
    }

    fn rule_without_block(
        &mut self,
        _prelude: Self::Prelude,
        _start: &ParserState,
    ) -> Result<Self::AtRule, ()> {
        Ok(BodyItem::NestedAtRule)
    }

    fn parse_block(
        &mut self,
        _prelude: Self::Prelude,
        _start: &ParserState,
        input: &mut Parser<'i>,
    ) -> Result<Self::AtRule, ParseError<Self::Error>> {
        while input.next_including_whitespace_and_comments().is_ok() {}
        Ok(BodyItem::NestedAtRule)
    }
}

impl<'i> QualifiedRuleParser<'i> for DeclBlockParser {
    type Prelude = ();
    type QualifiedRule = BodyItem;
    type Error = ();
}

impl<'i> RuleBodyItemParser<'i, BodyItem, ()> for DeclBlockParser {
    fn parse_declarations(&self) -> bool {
        true
    }
    fn parse_qualified(&self) -> bool {
        false
    }
}

fn parse_decl_list<'i>(input: &mut Parser<'i>) -> Vec<CssDecl> {
    let mut decl_parser = DeclBlockParser;
    let body = RuleBodyParser::new(input, &mut decl_parser);
    let mut out = Vec::new();
    for item in body {
        if let Ok(BodyItem::Decl(d)) = item {
            out.push(d);
        }
    }
    out
}

fn parse_media_length_px(raw: &str) -> Option<f32> {
    let s = raw.trim().to_ascii_lowercase();
    if let Some(n) = s.strip_suffix("px") {
        n.trim().parse::<f32>().ok()
    } else if let Some(n) = s.strip_suffix("rem").or_else(|| s.strip_suffix("em")) {
        n.trim().parse::<f32>().ok().map(|v| v * 16.0)
    } else {
        s.parse::<f32>().ok()
    }
}

fn eval_media_feature(feat: &str, vw: f32, vh: f32) -> bool {
    let s = feat
        .trim()
        .trim_start_matches('(')
        .trim_end_matches(')')
        .trim()
        .to_ascii_lowercase();
    if let Some((lhs, rhs)) = s.split_once(':') {
        let k = lhs.trim();
        let v = rhs.trim();
        return match k {
            "min-width" => parse_media_length_px(v).is_some_and(|px| vw >= px),
            "max-width" => parse_media_length_px(v).is_some_and(|px| vw <= px),
            "width" => parse_media_length_px(v).is_some_and(|px| (vw - px).abs() < 0.5),
            "min-height" => parse_media_length_px(v).is_some_and(|px| vh >= px),
            "max-height" => parse_media_length_px(v).is_some_and(|px| vh <= px),
            "height" => parse_media_length_px(v).is_some_and(|px| (vh - px).abs() < 0.5),
            "orientation" => match v {
                "landscape" => vw >= vh,
                "portrait" => vh > vw,
                _ => true,
            },
            "prefers-color-scheme" => v == "light",
            "prefers-reduced-motion" => v == "no-preference",
            _ => true,
        };
    }

    // Range syntax e.g. `(width >= 48rem)` or `(width < 1280px)`
    for (op, cmp) in [(">=", 0u8), ("<=", 1), (">", 2), ("<", 3), ("=", 4)] {
        if let Some((lhs, rhs)) = s.split_once(op) {
            let dim_val = match lhs.trim() {
                "width" => Some(vw),
                "height" => Some(vh),
                _ => None,
            };
            if let (Some(actual), Some(target)) = (dim_val, parse_media_length_px(rhs)) {
                return match cmp {
                    0 => actual >= target,
                    1 => actual <= target,
                    2 => actual > target,
                    3 => actual < target,
                    _ => (actual - target).abs() < 0.5,
                };
            }
        }
    }
    true
}

fn split_top_level_commas(s: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    for (i, b) in s.bytes().enumerate() {
        match b {
            b'(' => depth += 1,
            b')' => depth = depth.saturating_sub(1),
            b',' if depth == 0 => {
                parts.push(s[start..i].trim());
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(s[start..].trim());
    parts
}

fn eval_single_media_query(q: &str, vw: f32, vh: f32) -> bool {
    let trimmed = q.trim();
    if trimmed.is_empty() {
        return true;
    }
    let lower = trimmed.to_ascii_lowercase();
    let (negated, rest) = if let Some(after) = lower.strip_prefix("not ") {
        (true, after.trim())
    } else {
        (false, lower.as_str())
    };
    let rest = rest.strip_prefix("only ").unwrap_or(rest).trim();

    let mut result = true;
    // Split at top-level `and` tokens
    let mut tokens = Vec::new();
    let mut depth = 0usize;
    let bytes = rest.as_bytes();
    let mut start = 0usize;
    let mut i = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b'(' => depth += 1,
            b')' => depth = depth.saturating_sub(1),
            _ if depth == 0 && rest[i..].starts_with("and") => {
                let before_ok =
                    i == 0 || bytes[i - 1].is_ascii_whitespace() || bytes[i - 1] == b')';
                let after_idx = i + 3;
                let after_ok = after_idx == bytes.len()
                    || bytes[after_idx].is_ascii_whitespace()
                    || bytes[after_idx] == b'(';
                if before_ok && after_ok {
                    let piece = rest[start..i].trim();
                    if !piece.is_empty() {
                        tokens.push(piece);
                    }
                    start = after_idx;
                    i = after_idx;
                    continue;
                }
            }
            _ => {}
        }
        i += 1;
    }
    let tail = rest[start..].trim();
    if !tail.is_empty() {
        tokens.push(tail);
    }

    for tok in tokens {
        if tok == "all" || tok == "screen" {
            continue;
        }
        if tok == "print" || tok == "speech" {
            result = false;
            break;
        }
        if tok.starts_with('(') && !eval_media_feature(tok, vw, vh) {
            result = false;
            break;
        }
    }
    if negated {
        !result
    } else {
        result
    }
}

fn eval_media_query_list(raw: &str, vw: f32, vh: f32) -> bool {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return true;
    }
    split_top_level_commas(trimmed)
        .into_iter()
        .any(|q| eval_single_media_query(q, vw, vh))
}

fn eval_supports_condition(raw: &str) -> bool {
    let s = raw.trim().to_ascii_lowercase();
    if s.is_empty() {
        return true;
    }
    if let Some(rest) = s.strip_prefix("not ") {
        return !eval_supports_condition(rest);
    }
    // Common Tailwind v4 feature queries: oklch, color-mix, display: grid, backdrop-filter, etc.
    !s.contains("hanging-punctuation") && !s.contains("paint-order: unsupported")
}

fn parse_font_face(decls: &[CssDecl]) -> Option<FontFaceDecl> {
    let mut family = None;
    let mut src_url = None;
    let mut weight = None;
    let mut style = None;
    let mut unicode_range = None;

    for d in decls {
        match d.name.as_str() {
            "font-family" => {
                let fam = d.value.trim().trim_matches(|c| c == '"' || c == '\'');
                if !fam.is_empty() {
                    family = Some(fam.to_string());
                }
            }
            "src" => {
                // Extract first `url(...)` argument
                let lower = d.value.to_ascii_lowercase();
                if let Some(pos) = lower.find("url(") {
                    let after = &d.value[pos + 4..];
                    if let Some(end) = after.find(')') {
                        let url = after[..end].trim().trim_matches(|c| c == '"' || c == '\'');
                        if !url.is_empty() {
                            src_url = Some(url.to_string());
                        }
                    }
                }
            }
            "font-weight" => weight = Some(d.value.trim().to_string()),
            "font-style" => style = Some(d.value.trim().to_string()),
            "unicode-range" => unicode_range = Some(d.value.trim().to_string()),
            _ => {}
        }
    }

    Some(FontFaceDecl {
        family: family?,
        src_url: src_url?,
        weight,
        style,
        unicode_range,
    })
}

fn resolve_layer_name(prefix: Option<&str>, raw: &str) -> Option<String> {
    let name = raw.trim();
    if name.is_empty() {
        return None;
    }
    Some(match prefix {
        Some(p) => format!("{p}.{name}"),
        None => name.to_string(),
    })
}

impl<'i> QualifiedRuleParser<'i> for SheetParser<'_> {
    type Prelude = String;
    type QualifiedRule = ();
    type Error = ();

    fn parse_prelude(
        &mut self,
        input: &mut Parser<'i>,
    ) -> Result<Self::Prelude, ParseError<Self::Error>> {
        let sel = drain_to_string(input);
        if sel.is_empty() {
            return Err(ParseError::unexpected_token());
        }
        Ok(sel)
    }

    fn parse_block(
        &mut self,
        prelude: Self::Prelude,
        _start: &ParserState,
        input: &mut Parser<'i>,
    ) -> Result<Self::QualifiedRule, ParseError<Self::Error>> {
        if !self.scope.active {
            while input.next_including_whitespace_and_comments().is_ok() {}
            return Ok(());
        }
        let declarations = parse_decl_list(input);
        if !declarations.is_empty() {
            let source_order = self.ctx.alloc_order();
            self.ctx.rules.push(StyleRuleAst {
                selectors_raw: prelude,
                declarations,
                layer_rank: self.scope.layer_rank,
                source_order,
            });
        }
        Ok(())
    }
}

impl<'i> AtRuleParser<'i> for SheetParser<'_> {
    type Prelude = SheetPrelude;
    type AtRule = ();
    type Error = ();

    fn parse_prelude(
        &mut self,
        name: CowRcStr<'i>,
        input: &mut Parser<'i>,
    ) -> Result<Self::Prelude, ParseError<Self::Error>> {
        let prelude_raw = drain_to_string(input);
        match name.to_ascii_lowercase().as_str() {
            "layer" => {
                if prelude_raw.contains(',') {
                    let names: Vec<String> = prelude_raw
                        .split(',')
                        .filter_map(|part| {
                            resolve_layer_name(self.scope.layer_prefix.as_deref(), part)
                        })
                        .collect();
                    return Ok(SheetPrelude::LayerStatement(names));
                }
                let full_name = if let Some(n) =
                    resolve_layer_name(self.scope.layer_prefix.as_deref(), &prelude_raw)
                {
                    n
                } else {
                    self.ctx.anon_layer_seq += 1;
                    let anon = format!("__anon_{}", self.ctx.anon_layer_seq);
                    resolve_layer_name(self.scope.layer_prefix.as_deref(), &anon).unwrap()
                };
                let rank = self.ctx.intern_layer(&full_name);
                Ok(SheetPrelude::LayerBlock(Some(rank), Some(full_name)))
            }
            "media" => {
                let matched = eval_media_query_list(&prelude_raw, self.ctx.vw, self.ctx.vh);
                Ok(SheetPrelude::Media(matched))
            }
            "supports" => Ok(SheetPrelude::Supports(eval_supports_condition(
                &prelude_raw,
            ))),
            "font-face" => Ok(SheetPrelude::FontFace),
            "property" => Ok(SheetPrelude::Property(prelude_raw)),
            _ => Ok(SheetPrelude::Skip),
        }
    }

    fn rule_without_block(
        &mut self,
        prelude: Self::Prelude,
        _start: &ParserState,
    ) -> Result<Self::AtRule, ()> {
        if self.scope.active {
            match prelude {
                SheetPrelude::LayerStatement(names) => {
                    for name in names {
                        self.ctx.intern_layer(&name);
                    }
                }
                SheetPrelude::LayerBlock(Some(_), Some(name)) => {
                    self.ctx.intern_layer(&name);
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn parse_block(
        &mut self,
        prelude: Self::Prelude,
        _start: &ParserState,
        input: &mut Parser<'i>,
    ) -> Result<Self::AtRule, ParseError<Self::Error>> {
        match prelude {
            SheetPrelude::FontFace => {
                let decls = parse_decl_list(input);
                if self.scope.active {
                    if let Some(ff) = parse_font_face(&decls) {
                        self.ctx.font_faces.push(ff);
                    }
                }
            }
            SheetPrelude::Property(prop_name) => {
                let decls = parse_decl_list(input);
                if self.scope.active && prop_name.starts_with("--") {
                    if let Some(init_decl) = decls.iter().find(|d| d.name == "initial-value") {
                        self.ctx
                            .property_initials
                            .insert(prop_name, init_decl.value.clone());
                    }
                }
            }
            SheetPrelude::Media(matched) | SheetPrelude::Supports(matched) => {
                let mut child = SheetParser {
                    ctx: self.ctx,
                    scope: RuleScope {
                        active: self.scope.active && matched,
                        layer_rank: self.scope.layer_rank,
                        layer_prefix: self.scope.layer_prefix.clone(),
                    },
                };
                let iter = StyleSheetParser::new(input, &mut child);
                for _ in iter {}
            }
            SheetPrelude::LayerBlock(rank, prefix) => {
                let mut child = SheetParser {
                    ctx: self.ctx,
                    scope: RuleScope {
                        active: self.scope.active,
                        layer_rank: rank.or(self.scope.layer_rank),
                        layer_prefix: prefix.or_else(|| self.scope.layer_prefix.clone()),
                    },
                };
                let iter = StyleSheetParser::new(input, &mut child);
                for _ in iter {}
            }
            SheetPrelude::LayerStatement(_) | SheetPrelude::Skip => {
                while input.next_including_whitespace_and_comments().is_ok() {}
            }
        }
        Ok(())
    }
}

/// Parses one or more stylesheets in document order against composition dimensions `(vw, vh)`.
pub fn parse_stylesheets<'a>(
    css_blocks: impl IntoIterator<Item = &'a str>,
    vw: f32,
    vh: f32,
) -> ParsedStylesheetAst {
    let mut ctx = ParseCtx::new(vw, vh);
    for css in css_blocks {
        let mut parser_input = Parser::new(css);
        let mut sheet_parser = SheetParser {
            ctx: &mut ctx,
            scope: RuleScope {
                active: true,
                layer_rank: None,
                layer_prefix: None,
            },
        };
        let iter = StyleSheetParser::new(&mut parser_input, &mut sheet_parser);
        for _ in iter {}
    }

    // Deduplicate identical `@font-face` rules while preserving distinct weight/style/subset entries
    // that share the same variable font URL (e.g. Google Fonts CSS2 responses).
    type FontFaceKey = (
        String,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
    );
    let mut seen_fonts: HashSet<FontFaceKey> = HashSet::new();
    let mut deduped_fonts = Vec::with_capacity(ctx.font_faces.len());
    for ff in ctx.font_faces {
        if seen_fonts.insert((
            ff.family.clone(),
            ff.src_url.clone(),
            ff.weight.clone(),
            ff.style.clone(),
            ff.unicode_range.clone(),
        )) {
            deduped_fonts.push(ff);
        }
    }

    let _ = Token::WhiteSpace("");
    ParsedStylesheetAst {
        rules: ctx.rules,
        font_faces: deduped_fonts,
        property_initials: ctx.property_initials,
        layer_names: ctx.layer_names,
    }
}
