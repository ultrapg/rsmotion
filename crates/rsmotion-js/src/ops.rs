use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::fs::File;
use std::hash::{Hash, Hasher};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use deno_core::{op2, OpState};
use rsmotion_media::{
    cache_inline_bytes, decode_audio_bytes, fetch_remote_asset, is_remote_http_url, probe_media,
    resolve_or_fetch_asset, wrap_aac_frame_adts, MediaMetadata, StreamingAudioDecoder,
    CHROME_WOFF2_UA,
};
use rsmotion_raster::FontCache;
use rsmotion_scene::{LottieSource, RegisteredFont};
use serde::Serialize;
use skia_safe::skottie::Animation;
use tracing::{debug, warn};
use url::Url;

type CachedAudioEntry = (Vec<u8>, (u32, u32, f64, Vec<u8>));

/// Lottie registration metadata returned directly to JS via `serde_v8`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LottieMeta {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ip: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub op: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fr: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_frames: Option<f32>,
}

impl LottieMeta {
    fn failed() -> Self {
        Self {
            ok: false,
            id: None,
            width: None,
            height: None,
            ip: None,
            op: None,
            fr: None,
            total_frames: None,
        }
    }
}

/// Shared per-runtime state accessible from synchronous `deno_core` ops.
pub struct HostState {
    pub bundle_dir: PathBuf,
    pub offline: bool,
    pub fonts: Vec<RegisteredFont>,
    pub lotties: Vec<LottieSource>,
    pub lottie_meta_cache: HashMap<String, LottieMeta>,
    pub font_cache: FontCache,
    pub image_dims_cache: HashMap<String, (u32, u32)>,
    pub media_meta_cache: HashMap<String, Option<MediaMetadata>>,
    pub text_asset_cache: HashMap<String, Option<String>>,
    pub last_pcm_buf: Vec<u8>,
    pub audio_decode_cache: HashMap<(u64, usize, u32), CachedAudioEntry>,
    pub streaming_decoders: HashMap<u32, StreamingAudioDecoder>,
    pub next_decoder_id: u32,
}

impl HostState {
    pub fn new(bundle_dir: PathBuf, offline: bool) -> Self {
        Self {
            bundle_dir,
            offline,
            fonts: Vec::new(),
            lotties: Vec::new(),
            lottie_meta_cache: HashMap::new(),
            font_cache: FontCache::new(),
            image_dims_cache: HashMap::new(),
            media_meta_cache: HashMap::new(),
            text_asset_cache: HashMap::new(),
            last_pcm_buf: Vec::new(),
            audio_decode_cache: HashMap::new(),
            streaming_decoders: HashMap::new(),
            next_decoder_id: 1,
        }
    }
}

/// Resolves a browser asset URL or path strictly inside local `bundle_dir` (no network).
pub fn resolve_bundle_asset(bundle_dir: &Path, raw_src: &str) -> Option<PathBuf> {
    let trimmed = raw_src.trim();
    if trimmed.is_empty()
        || trimmed.starts_with("data:")
        || trimmed.starts_with("blob:")
        || is_remote_http_url(trimmed)
    {
        return None;
    }
    rsmotion_media::resolve_media_path(bundle_dir, trimmed)
}

fn hash_bytes(bytes: &[u8]) -> u64 {
    let mut h = DefaultHasher::new();
    bytes.hash(&mut h);
    h.finish()
}

fn hash_str(s: &str) -> u64 {
    let mut h = DefaultHasher::new();
    s.hash(&mut h);
    h.finish()
}

fn mime_for_path(path: &Path) -> &'static str {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase);
    match ext.as_deref() {
        Some("wav") => "audio/wav",
        Some("mp3") => "audio/mpeg",
        Some("aac") | Some("m4a") => "audio/mp4",
        Some("flac") => "audio/flac",
        Some("ogg") => "audio/ogg",
        Some("mp4") => "video/mp4",
        Some("webm") => "video/webm",
        Some("json") => "application/json",
        Some("css") => "text/css",
        Some("woff2") => "font/woff2",
        Some("woff") => "font/woff",
        Some("ttf") => "font/ttf",
        Some("otf") => "font/otf",
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        Some("svg") => "image/svg+xml",
        _ => "application/octet-stream",
    }
}

fn load_single_text_asset(
    bundle_dir: &Path,
    offline: bool,
    href: &str,
) -> (Option<String>, Option<String>) {
    let trimmed = href.trim();
    if trimmed.is_empty() {
        return (None, None);
    }
    if is_remote_http_url(trimmed) {
        let normalized = if trimmed.starts_with("//") {
            format!("https:{trimmed}")
        } else {
            trimmed.to_string()
        };
        let loaded = fetch_remote_asset(&normalized, offline, Some(CHROME_WOFF2_UA))
            .ok()
            .and_then(|p| std::fs::read_to_string(p).ok());
        return (loaded, Some(normalized));
    }
    let Ok(canon_bundle) = std::fs::canonicalize(bundle_dir) else {
        return (None, None);
    };
    let loaded = resolve_bundle_asset(bundle_dir, trimmed)
        .filter(|p| p.starts_with(&canon_bundle))
        .and_then(|p| std::fs::read_to_string(p).ok());
    (loaded, Some(trimmed.to_string()))
}

fn resolve_import_href(base_href: Option<&str>, raw_sub: &str) -> String {
    let sub = raw_sub.trim().trim_matches(|c| c == '"' || c == '\'');
    if sub.is_empty() || is_remote_http_url(sub) || sub.starts_with('/') || sub.starts_with("data:")
    {
        return sub.to_string();
    }
    if let Some(base) = base_href {
        if let Ok(base_url) = Url::parse(base) {
            if let Ok(joined) = base_url.join(sub) {
                return joined.into();
            }
        }
        if let Some((dir, _)) = base.rsplit_once('/') {
            return format!("{dir}/{sub}");
        }
    }
    sub.to_string()
}

fn parse_css_import_target(stmt_rest: &str) -> Option<(String, usize)> {
    let s = stmt_rest.trim_start();
    let ws = stmt_rest.len() - s.len();
    let (url, after_url_offset) = if s.len() >= 4 && s[..4].eq_ignore_ascii_case("url(") {
        let inner = &s[4..];
        let close = inner.find(')')?;
        (
            inner[..close]
                .trim()
                .trim_matches(|c| c == '"' || c == '\'')
                .to_string(),
            4 + close + 1,
        )
    } else {
        let q = s.chars().next().filter(|&c| c == '"' || c == '\'')?;
        let inner = &s[1..];
        let close = inner.find(q)?;
        (inner[..close].to_string(), 1 + close + 1)
    };

    let tail = &s[after_url_offset..];
    let semi_rel = tail.find(';');
    let nl_rel = tail.find('\n');
    let end_offset = match (semi_rel, nl_rel) {
        (Some(a), Some(b)) => a.min(b) + 1,
        (Some(a), None) => a + 1,
        (None, Some(b)) => b + 1,
        (None, None) => tail.len(),
    };
    Some((url, ws + after_url_offset + end_offset))
}

fn expand_css_imports_inner(
    bundle_dir: &Path,
    offline: bool,
    base_href: Option<&str>,
    css: &str,
    depth: usize,
) -> String {
    if depth >= 4 || !css.to_ascii_lowercase().contains("@import") {
        return css.to_string();
    }
    let lower = css.to_ascii_lowercase();
    let mut out = String::with_capacity(css.len());
    let mut cursor = 0usize;

    while let Some(rel) = lower[cursor..].find("@import") {
        let at_pos = cursor + rel;
        out.push_str(&css[cursor..at_pos]);
        let after_kw = at_pos + "@import".len();
        if let Some((raw_target, consumed)) = parse_css_import_target(&css[after_kw..]) {
            let resolved = resolve_import_href(base_href, &raw_target);
            let (child_css, child_base) = load_single_text_asset(bundle_dir, offline, &resolved);
            if let Some(child) = child_css {
                let expanded = expand_css_imports_inner(
                    bundle_dir,
                    offline,
                    child_base.as_deref(),
                    &child,
                    depth + 1,
                );
                out.push_str(&expanded);
                out.push('\n');
            } else {
                // Keep unresolved @import intact rather than silently corrupting CSS syntax.
                out.push_str(&css[at_pos..after_kw + consumed]);
            }
            cursor = after_kw + consumed;
        } else {
            out.push_str(&css[at_pos..after_kw]);
            cursor = after_kw;
        }
    }
    out.push_str(&css[cursor..]);
    out
}

#[op2]
#[serde]
pub fn op_probe_image(state: &mut OpState, #[string] src: String) -> (u32, u32) {
    let host = state.borrow_mut::<HostState>();
    if let Some(&dims) = host.image_dims_cache.get(&src) {
        return dims;
    }

    let dims = resolve_or_fetch_asset(&host.bundle_dir, &src, host.offline)
        .and_then(|path| image::image_dimensions(&path).ok())
        .unwrap_or((0, 0));

    host.image_dims_cache.insert(src, dims);
    dims
}

#[op2]
#[serde]
pub fn op_probe_media(state: &mut OpState, #[string] src: String) -> (u32, u32, f64, bool, bool) {
    let host = state.borrow_mut::<HostState>();
    if let Some(cached) = host.media_meta_cache.get(&src) {
        return cached
            .as_ref()
            .map(|m| (m.width, m.height, m.duration_secs, m.has_video, m.has_audio))
            .unwrap_or((0, 0, 0.0, false, false));
    }

    let meta = resolve_or_fetch_asset(&host.bundle_dir, &src, host.offline)
        .and_then(|path| probe_media(&path).ok());

    let res = meta
        .as_ref()
        .map(|m| (m.width, m.height, m.duration_secs, m.has_video, m.has_audio))
        .unwrap_or((0, 0, 0.0, false, false));

    host.media_meta_cache.insert(src, meta);
    res
}

fn register_resolved_font(
    host: &mut HostState,
    family: String,
    src: String,
    resolved: Option<PathBuf>,
    weight: String,
    style: String,
    unicode_range: String,
) -> bool {
    let found = resolved.is_some();
    let clean_family = family
        .trim()
        .trim_matches(|c| c == '"' || c == '\'')
        .to_string();
    let weight_opt = (!weight.trim().is_empty()).then_some(weight.trim().to_string());
    let style_opt = (!style.trim().is_empty()).then_some(style.trim().to_string());
    let urange_opt = (!unicode_range.trim().is_empty()).then_some(unicode_range.trim().to_string());

    let duplicate = host.fonts.iter().any(|f| {
        f.family == clean_family
            && f.resolved_path == resolved
            && f.weight == weight_opt
            && f.style == style_opt
            && f.unicode_range == urange_opt
    });
    if !duplicate {
        let rf = RegisteredFont {
            family: clean_family,
            src,
            resolved_path: resolved,
            weight: weight_opt,
            style: style_opt,
            unicode_range: urange_opt,
        };
        let bundle_dir = host.bundle_dir.clone();
        let offline = host.offline;
        host.font_cache
            .sync_registered_fonts(std::slice::from_ref(&rf), &bundle_dir, offline);
        host.fonts.push(rf);
    }
    found
}

#[op2(fast)]
pub fn op_register_font(
    state: &mut OpState,
    #[string] family: String,
    #[string] src: String,
    #[string] weight: String,
    #[string] style: String,
    #[string] unicode_range: String,
) -> bool {
    let host = state.borrow_mut::<HostState>();
    let resolved = resolve_or_fetch_asset(&host.bundle_dir, &src, host.offline);
    register_resolved_font(host, family, src, resolved, weight, style, unicode_range)
}

#[op2]
pub fn op_register_font_bytes(
    state: &mut OpState,
    #[string] family: String,
    #[anybuffer] bytes: &[u8],
    #[string] weight: String,
    #[string] style: String,
    #[string] unicode_range: String,
) -> bool {
    if bytes.is_empty() {
        return false;
    }
    let Ok(path) = cache_inline_bytes("font", "woff2", bytes) else {
        return false;
    };
    let host = state.borrow_mut::<HostState>();
    register_resolved_font(
        host,
        family,
        "arraybuffer".to_string(),
        Some(path),
        weight,
        style,
        unicode_range,
    )
}

#[op2]
#[string]
pub fn op_read_text_asset(state: &mut OpState, #[string] href: String) -> Option<String> {
    let host = state.borrow_mut::<HostState>();
    if let Some(cached) = host.text_asset_cache.get(&href) {
        return cached.clone();
    }

    let (raw, base_href) = load_single_text_asset(&host.bundle_dir, host.offline, &href);
    let expanded = raw.map(|css| {
        expand_css_imports_inner(
            &host.bundle_dir,
            host.offline,
            base_href.as_deref(),
            &css,
            0,
        )
    });

    host.text_asset_cache.insert(href, expanded.clone());
    expanded
}

#[op2]
#[string]
pub fn op_read_script_asset(state: &mut OpState, #[string] href: String) -> Option<String> {
    let host = state.borrow::<HostState>();
    let res = load_single_text_asset(&host.bundle_dir, host.offline, &href).0;
    debug!(
        target: "rsmotion_js",
        "op_read_script_asset href={href} found={}",
        res.is_some()
    );
    res
}

#[op2]
#[string]
pub fn op_expand_css_imports(state: &mut OpState, #[string] css: String) -> String {
    let host = state.borrow::<HostState>();
    expand_css_imports_inner(&host.bundle_dir, host.offline, None, &css, 0)
}

#[op2]
#[serde]
pub fn op_canvas_measure_text(
    state: &mut OpState,
    #[string] text: String,
    #[string] font: String,
) -> (f32, f32, f32) {
    let host = state.borrow::<HostState>();
    host.font_cache.measure_canvas_text(&text, &font)
}

#[op2]
#[serde]
pub fn op_register_lottie(
    state: &mut OpState,
    #[string] json: String,
    #[string] assets_path: String,
) -> LottieMeta {
    let key = format!("{:016x}:{}", hash_str(&json), assets_path.trim());
    {
        let host = state.borrow::<HostState>();
        if let Some(cached) = host.lottie_meta_cache.get(&key) {
            return cached.clone();
        }
    }

    let meta = match Animation::from_str(&json) {
        None => LottieMeta::failed(),
        Some(anim) => {
            let Ok(path) = cache_inline_bytes("lottie", "json", json.as_bytes()) else {
                return LottieMeta::failed();
            };
            let id = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("lottie")
                .to_string();
            let size = anim.size();
            let ip = anim.in_point();
            let op = anim.out_point();
            let fr = anim.fps();
            let total_frames = (op - ip).max(1.0);

            let host = state.borrow_mut::<HostState>();
            if !host.lotties.iter().any(|l| l.id == id) {
                let ap = assets_path.trim();
                host.lotties.push(LottieSource {
                    id: id.clone(),
                    resolved_path: path,
                    assets_path: (!ap.is_empty()).then(|| ap.to_string()),
                });
            }
            LottieMeta {
                ok: true,
                id: Some(id),
                width: Some(size.width),
                height: Some(size.height),
                ip: Some(ip),
                op: Some(op),
                fr: Some(fr),
                total_frames: Some(total_frames),
            }
        }
    };

    let host = state.borrow_mut::<HostState>();
    host.lottie_meta_cache.insert(key, meta.clone());
    meta
}

#[op2]
#[serde]
pub fn op_stat_asset(state: &mut OpState, #[string] url: String) -> (bool, f64, String) {
    let host = state.borrow::<HostState>();
    let Some(path) = resolve_or_fetch_asset(&host.bundle_dir, &url, host.offline) else {
        debug!(target: "rsmotion_js", "op_stat_asset url={url} -> NOT FOUND");
        return (false, 0.0, String::new());
    };
    let Ok(meta) = std::fs::metadata(&path) else {
        debug!(target: "rsmotion_js", "op_stat_asset url={url} -> NO META");
        return (false, 0.0, String::new());
    };
    if !meta.is_file() {
        return (false, 0.0, String::new());
    }
    let mime = mime_for_path(&path);
    debug!(
        target: "rsmotion_js",
        "op_stat_asset url={url} -> len={} mime={mime}",
        meta.len()
    );
    (true, meta.len() as f64, mime.to_string())
}

#[op2]
#[buffer]
pub fn op_read_asset_slice(
    state: &mut OpState,
    #[string] url: String,
    start: f64,
    end: f64,
) -> Vec<u8> {
    let host = state.borrow::<HostState>();
    let Some(path) = resolve_or_fetch_asset(&host.bundle_dir, &url, host.offline) else {
        return Vec::new();
    };
    let Ok(mut file) = File::open(&path) else {
        return Vec::new();
    };
    let Ok(total_len) = file.metadata().map(|m| m.len()) else {
        return Vec::new();
    };
    let start_u = if start.is_finite() && start > 0.0 {
        (start as u64).min(total_len)
    } else {
        0
    };
    let end_u = if end < 0.0 || !end.is_finite() {
        total_len.saturating_sub(1)
    } else {
        (end as u64).min(total_len.saturating_sub(1))
    };
    if start_u >= total_len || end_u < start_u {
        return Vec::new();
    }
    let to_read = (end_u - start_u + 1) as usize;
    if file.seek(SeekFrom::Start(start_u)).is_err() {
        return Vec::new();
    }
    let mut buf = Vec::with_capacity(to_read);
    if file.take(to_read as u64).read_to_end(&mut buf).is_err() {
        return Vec::new();
    }
    buf
}

#[op2]
#[serde]
pub fn op_decode_audio_bytes(
    state: &mut OpState,
    #[anybuffer] bytes: &[u8],
    target_sample_rate: u32,
) -> (bool, u32, u32, f64) {
    if bytes.is_empty() {
        return (false, 0, 0, 0.0);
    }
    let cache_key = (hash_bytes(bytes), bytes.len(), target_sample_rate);
    {
        let host = state.borrow_mut::<HostState>();
        if let Some((orig, (sr, ch, dur, pcm))) = host.audio_decode_cache.get(&cache_key) {
            if orig.as_slice() == bytes {
                host.last_pcm_buf = pcm.clone();
                return (true, *sr, *ch, *dur);
            }
        }
    }

    let target = (target_sample_rate > 0).then_some(target_sample_rate);
    let Ok(decoded) = decode_audio_bytes(bytes, None, target, None) else {
        return (false, 0, 0, 0.0);
    };
    let res = (decoded.sample_rate, decoded.channels, decoded.duration_secs);
    let host = state.borrow_mut::<HostState>();
    host.last_pcm_buf = decoded.interleaved_f32_le.clone();
    host.audio_decode_cache.insert(
        cache_key,
        (
            bytes.to_vec(),
            (res.0, res.1, res.2, decoded.interleaved_f32_le),
        ),
    );
    (true, res.0, res.1, res.2)
}

#[op2]
pub fn op_create_audio_decoder(
    state: &mut OpState,
    #[string] codec: String,
    sample_rate: u32,
    channels: u32,
    #[anybuffer] description: &[u8],
) -> u32 {
    let Some(dec) = StreamingAudioDecoder::new(&codec, sample_rate, channels, description) else {
        return 0;
    };
    let host = state.borrow_mut::<HostState>();
    let id = host.next_decoder_id.max(1);
    host.next_decoder_id = id.wrapping_add(1).max(1);
    host.streaming_decoders.insert(id, dec);
    id
}

#[op2(fast)]
pub fn op_close_audio_decoder(state: &mut OpState, decoder_id: u32) {
    if decoder_id > 0 {
        state
            .borrow_mut::<HostState>()
            .streaming_decoders
            .remove(&decoder_id);
    }
}

#[op2]
#[serde]
#[allow(clippy::too_many_arguments)]
pub fn op_decode_audio_chunks(
    state: &mut OpState,
    decoder_id: u32,
    #[string] codec: String,
    sample_rate: u32,
    channels: u32,
    #[anybuffer] description: &[u8],
    #[anybuffer] chunks: &[u8],
    #[serde] frame_lengths: Vec<u32>,
) -> (bool, u32, u32, f64) {
    if chunks.is_empty() {
        return (false, 0, 0, 0.0);
    }
    if decoder_id > 0 {
        let host = state.borrow_mut::<HostState>();
        let Some(dec) = host.streaming_decoders.get_mut(&decoder_id) else {
            return (false, 0, 0, 0.0);
        };
        let decoded = match dec.decode_packets(chunks, &frame_lengths) {
            Ok(d) => d,
            Err(err) => {
                warn!(target: "rsmotion_js", "symphonia decode failed id={decoder_id}: {err}");
                return (false, 0, 0, 0.0);
            }
        };
        let out = (decoded.sample_rate, decoded.channels, decoded.duration_secs);
        host.last_pcm_buf = decoded.interleaved_f32_le;
        return (true, out.0, out.1, out.2);
    }
    let lower = codec.trim().to_ascii_lowercase();
    let single_mp3_dup = lower.contains("mp3") && frame_lengths.len() == 1;
    let (payload, format_hint): (std::borrow::Cow<'_, [u8]>, Option<&str>) =
        if lower.contains("mp3") {
            if single_mp3_dup {
                let mut doubled = Vec::with_capacity(chunks.len() * 2);
                doubled.extend_from_slice(chunks);
                doubled.extend_from_slice(chunks);
                (std::borrow::Cow::Owned(doubled), Some("mp3"))
            } else {
                (std::borrow::Cow::Borrowed(chunks), Some("mp3"))
            }
        } else if lower.starts_with("mp4a") || lower.contains("aac") {
            if frame_lengths.is_empty() {
                (std::borrow::Cow::Borrowed(chunks), Some("aac"))
            } else {
                let mut adts = Vec::with_capacity(chunks.len() + frame_lengths.len() * 7);
                let mut off = 0usize;
                for &len in &frame_lengths {
                    let take = len as usize;
                    let end = off.saturating_add(take).min(chunks.len());
                    if end > off {
                        adts.extend_from_slice(&wrap_aac_frame_adts(
                            &chunks[off..end],
                            sample_rate.max(8000),
                            channels.clamp(1, 7),
                        ));
                    }
                    off = end;
                }
                (std::borrow::Cow::Owned(adts), Some("aac"))
            }
        } else if lower.contains("flac") {
            if !description.is_empty() && !chunks.starts_with(b"fLaC") {
                let mut combined = Vec::with_capacity(description.len() + chunks.len());
                combined.extend_from_slice(description);
                combined.extend_from_slice(chunks);
                (std::borrow::Cow::Owned(combined), Some("flac"))
            } else {
                (std::borrow::Cow::Borrowed(chunks), Some("flac"))
            }
        } else {
            (std::borrow::Cow::Borrowed(chunks), None)
        };

    let sr_hint = (sample_rate > 0).then_some(sample_rate);
    let ch_hint = (channels > 0).then_some(channels);
    let decoded = match decode_audio_bytes(&payload, format_hint, sr_hint, ch_hint) {
        Ok(d) => d,
        Err(err) => {
            warn!(
                target: "rsmotion_js",
                "op_decode_audio_chunks failed codec={codec} payload_len={} sr={sample_rate} ch={channels} frames={}: {err:?}",
                payload.len(),
                frame_lengths.len()
            );
            return (false, 0, 0, 0.0);
        }
    };
    let mut pcm_bytes = decoded.interleaved_f32_le;
    let mut dur = decoded.duration_secs;
    if single_mp3_dup {
        let bpf = 4usize * (decoded.channels.max(1) as usize);
        let total_frames = pcm_bytes.len() / bpf;
        let half_frames = total_frames / 2;
        if half_frames > 0 {
            pcm_bytes.truncate(half_frames * bpf);
            if decoded.sample_rate > 0 {
                dur = (half_frames as f64) / f64::from(decoded.sample_rate);
            }
        }
    }
    let out = (decoded.sample_rate, decoded.channels, dur);
    let host = state.borrow_mut::<HostState>();
    host.last_pcm_buf = pcm_bytes;
    (true, out.0, out.1, out.2)
}

#[op2]
#[buffer]
pub fn op_take_decoded_audio_pcm(state: &mut OpState) -> Vec<u8> {
    let host = state.borrow_mut::<HostState>();
    std::mem::take(&mut host.last_pcm_buf)
}

#[op2(fast)]
pub fn op_console_log(#[string] level: String, #[string] msg: String) {
    match level.as_str() {
        "warn" => warn!(target: "rsmotion_js", "{msg}"),
        "error" => warn!(target: "rsmotion_js", "JS error log: {msg}"),
        _ => debug!(target: "rsmotion_js", "{msg}"),
    }
}

deno_core::extension!(
    rsmotion_host,
    ops = [
        op_probe_image,
        op_probe_media,
        op_register_font,
        op_register_font_bytes,
        op_read_text_asset,
        op_read_script_asset,
        op_expand_css_imports,
        op_canvas_measure_text,
        op_register_lottie,
        op_stat_asset,
        op_read_asset_slice,
        op_decode_audio_bytes,
        op_create_audio_decoder,
        op_close_audio_decoder,
        op_decode_audio_chunks,
        op_take_decoded_audio_pcm,
        op_console_log,
    ],
    options = {
        bundle_dir: PathBuf,
        offline: bool,
    },
    state = |state, options| {
        state.put(HostState::new(options.bundle_dir, options.offline));
    },
);
