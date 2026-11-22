use std::path::{Path, PathBuf};

use url::Url;

/// Internal URI prefix emitted by the Tier 1 V8 snapshot for `<OffthreadVideo>` and `<Video>`.
pub const VIDEO_FRAME_URI_PREFIX: &str = "rsmotion-video://frame";

/// Parsed Tier 1 video frame reference.
#[derive(Debug, Clone, PartialEq)]
pub struct VideoFrameRequest {
    pub src: String,
    pub time_secs: f64,
    pub transparent: bool,
}

/// Formats an internal Tier 1 video frame URI.
pub fn format_video_frame_uri(src: &str, time_secs: f64, transparent: bool) -> String {
    let mut url = Url::parse(VIDEO_FRAME_URI_PREFIX).expect("valid static video frame URI");
    url.query_pairs_mut()
        .append_pair("src", src)
        .append_pair("time", &format!("{:.6}", time_secs.max(0.0)))
        .append_pair("transparent", if transparent { "true" } else { "false" });
    url.to_string()
}

/// Parses an internal Tier 1 `rsmotion-video://frame?...` URI.
pub fn parse_video_frame_uri(uri: &str) -> Option<VideoFrameRequest> {
    if !uri.starts_with("rsmotion-video://") {
        return None;
    }
    let parsed = Url::parse(uri).ok()?;
    if parsed.scheme() != "rsmotion-video" {
        return None;
    }

    let mut src: Option<String> = None;
    let mut time_secs = 0.0_f64;
    let mut transparent = false;

    for (k, v) in parsed.query_pairs() {
        match k.as_ref() {
            "src" => src = Some(v.into_owned()),
            "time" => {
                if let Ok(t) = v.parse::<f64>() {
                    if t.is_finite() {
                        time_secs = t.max(0.0);
                    }
                }
            }
            "transparent" => transparent = v == "true",
            _ => {}
        }
    }

    let src = src.filter(|s| !s.is_empty())?;
    Some(VideoFrameRequest {
        src,
        time_secs,
        transparent,
    })
}

/// Percent-decodes URL path segments cleanly.
pub fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(h), Some(l)) = (
                (bytes[i + 1] as char).to_digit(16),
                (bytes[i + 2] as char).to_digit(16),
            ) {
                out.push(((h << 4) | l) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Resolves a media `src` string (local path, `file://`, or localhost bundle URL) against `bundle_dir`.
pub fn resolve_media_path(bundle_dir: &Path, raw_src: &str) -> Option<PathBuf> {
    let trimmed = raw_src.trim();
    if trimmed.is_empty()
        || trimmed.starts_with("data:")
        || trimmed.starts_with("blob:")
        || trimmed.starts_with("https://")
    {
        return None;
    }

    let without_origin = if trimmed.starts_with("http://") {
        let parsed = Url::parse(trimmed).ok()?;
        let host = parsed.host_str().unwrap_or_default();
        if host != "127.0.0.1" && host != "localhost" {
            return None;
        }
        percent_decode(parsed.path())
    } else if let Some(rest) = trimmed.strip_prefix("file://") {
        percent_decode(rest)
    } else {
        percent_decode(trimmed)
    };

    let clean = without_origin
        .split(['?', '#'])
        .next()
        .unwrap_or("")
        .trim_start_matches('/');
    let clean = clean.strip_prefix("./").unwrap_or(clean);

    let canonical_bundle = std::fs::canonicalize(bundle_dir).ok()?;

    let mut candidates = Vec::with_capacity(4);
    if !clean.is_empty() {
        candidates.push(bundle_dir.join(clean));
        candidates.push(bundle_dir.join("public").join(clean));
    }
    let raw_path = Path::new(&without_origin);
    if raw_path.is_absolute() && raw_path.is_file() {
        candidates.push(raw_path.to_path_buf());
    }

    for cand in candidates {
        if let Ok(canon) = std::fs::canonicalize(&cand) {
            if (canon.starts_with(&canonical_bundle) || raw_path.is_absolute()) && canon.is_file() {
                return Some(canon);
            }
        }
    }
    None
}
