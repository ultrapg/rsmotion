use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use sha2::{Digest, Sha256};
use url::Url;

use crate::error::{MediaError, Result};

/// Desktop Chrome User-Agent so Google Fonts CSS2 serves `.woff2` rules and `unicode-range` blocks.
pub const CHROME_WOFF2_UA: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";

const MAX_REMOTE_ASSET_BYTES: u64 = 64 * 1024 * 1024;
static TMP_SEQ: AtomicU64 = AtomicU64::new(0);

/// Returns the local persistent directory used for cached remote/inline assets.
pub fn cache_assets_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("RSMOTION_CACHE_DIR") {
        let trimmed = dir.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed).join("assets");
        }
    }
    if let Ok(home) = std::env::var("HOME") {
        let trimmed = home.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed).join(".cache/rsmotion/assets");
        }
    }
    std::env::temp_dir().join("rsmotion-cache/assets")
}

/// Returns `true` when `raw` points to a non-loopback remote HTTP(S) resource.
pub fn is_remote_http_url(raw: &str) -> bool {
    let trimmed = raw.trim();
    if trimmed.starts_with("//") {
        return true;
    }
    let lower = trimmed.to_ascii_lowercase();
    if lower.starts_with("https://") {
        return true;
    }
    if lower.starts_with("http://") {
        if let Ok(parsed) = Url::parse(trimmed) {
            let host = parsed.host_str().unwrap_or_default();
            return host != "127.0.0.1" && host != "localhost" && host != "::1";
        }
        return true;
    }
    false
}

fn normalize_remote_url(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.starts_with("//") {
        format!("https:{trimmed}")
    } else {
        trimmed.to_string()
    }
}

fn sha256_hex(parts: &[&[u8]]) -> String {
    let mut hasher = Sha256::new();
    for p in parts {
        hasher.update(p);
    }
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(64);
    for b in digest {
        hex.push_str(&format!("{b:02x}"));
    }
    hex
}

fn safe_extension_for_url(parsed: &Url) -> &'static str {
    let path = parsed.path().to_ascii_lowercase();
    if let Some((_, ext)) = path.rsplit_once('.') {
        match ext {
            "woff2" => return "woff2",
            "woff" => return "woff",
            "ttf" => return "ttf",
            "otf" => return "otf",
            "css" => return "css",
            "mp3" => return "mp3",
            "wav" => return "wav",
            "png" => return "png",
            "jpg" => return "jpg",
            "jpeg" => return "jpeg",
            "webp" => return "webp",
            "json" => return "json",
            "svg" => return "svg",
            "mp4" => return "mp4",
            "webm" => return "webm",
            "aac" => return "aac",
            "m4a" => return "m4a",
            "flac" => return "flac",
            "ogg" => return "ogg",
            _ => {}
        }
    }
    if path.ends_with("/css") || path.ends_with("/css2") {
        return "css";
    }
    "bin"
}

fn write_file_atomic(target: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let seq = TMP_SEQ.fetch_add(1, Ordering::Relaxed);
    let tmp_name = format!(
        ".tmp.{}.{seq}",
        target
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("asset")
    );
    let tmp_path = target.with_file_name(tmp_name);
    std::fs::write(&tmp_path, bytes)?;
    if let Err(err) = std::fs::rename(&tmp_path, target) {
        let _ = std::fs::remove_file(&tmp_path);
        if target.is_file() {
            return Ok(());
        }
        return Err(MediaError::Io(err));
    }
    Ok(())
}

/// Fetches a remote HTTP(S) asset (or returns the already-cached local copy) under `~/.cache/rsmotion/assets/`.
pub fn fetch_remote_asset(url: &str, offline: bool, user_agent: Option<&str>) -> Result<PathBuf> {
    let normalized = normalize_remote_url(url);
    let parsed = Url::parse(&normalized)
        .map_err(|e| MediaError::NotFound(format!("invalid remote URL {url}: {e}")))?;
    if parsed.scheme() != "http" && parsed.scheme() != "https" {
        return Err(MediaError::NotFound(format!(
            "unsupported remote URL scheme for {url}"
        )));
    }

    let ua_key = user_agent.unwrap_or("");
    let digest = sha256_hex(&[normalized.as_bytes(), b"\0", ua_key.as_bytes()]);
    let ext = safe_extension_for_url(&parsed);
    let cache_path = cache_assets_dir().join(format!("{digest}.{ext}"));

    if cache_path.is_file() {
        return Ok(cache_path);
    }

    if offline {
        return Err(MediaError::NotFound(format!(
            "offline mode enabled and remote asset is not cached: {normalized}"
        )));
    }

    let config = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(20)))
        .build();
    let agent: ureq::Agent = config.into();
    let ua_header = user_agent.unwrap_or("rsmotion/0.1");

    let mut resp = agent
        .get(&normalized)
        .header("User-Agent", ua_header)
        .call()
        .map_err(|e| {
            MediaError::DecodeFailed(format!("HTTP fetch failed for {normalized}: {e}"))
        })?;

    let bytes = resp
        .body_mut()
        .with_config()
        .limit(MAX_REMOTE_ASSET_BYTES)
        .read_to_vec()
        .map_err(|e| {
            MediaError::DecodeFailed(format!("failed reading HTTP body for {normalized}: {e}"))
        })?;

    write_file_atomic(&cache_path, &bytes)?;
    Ok(cache_path)
}

/// Resolves a local bundle asset first, falling back to cached HTTP(S) download when remote.
pub fn resolve_or_fetch_asset(bundle_dir: &Path, raw_src: &str, offline: bool) -> Option<PathBuf> {
    if let Some(local) = crate::resolve_media_path(bundle_dir, raw_src) {
        return Some(local);
    }
    if is_remote_http_url(raw_src) {
        return fetch_remote_asset(raw_src, offline, None).ok();
    }
    None
}

/// Persists inline binary data (such as `FontFace` buffers or Lottie JSON) by SHA-256 content hash.
pub fn cache_inline_bytes(prefix: &str, ext: &str, bytes: &[u8]) -> Result<PathBuf> {
    let digest = sha256_hex(&[bytes]);
    let clean_prefix: String = prefix
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
        .collect();
    let clean_ext: String = ext.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
    let pfx = if clean_prefix.is_empty() {
        "inline"
    } else {
        clean_prefix.as_str()
    };
    let ex = if clean_ext.is_empty() {
        "bin"
    } else {
        clean_ext.as_str()
    };
    let stable_path = cache_assets_dir().join(format!("{pfx}_{digest}.{ex}"));
    if stable_path.is_file() {
        return Ok(stable_path);
    }
    write_file_atomic(&stable_path, bytes)?;
    Ok(stable_path)
}

/// Decompresses `.woff2` (`wOF2`) or `.woff` (`wOFF`) font bytes to raw SFNT (TTF/OTF), or passes raw SFNT through.
pub fn decode_font_to_sfnt(bytes: &[u8]) -> Option<Vec<u8>> {
    if bytes.len() < 4 {
        return None;
    }
    if &bytes[..4] == b"wOF2" {
        wuff::decompress_woff2(bytes).ok()
    } else if &bytes[..4] == b"wOFF" {
        wuff::decompress_woff1(bytes).ok()
    } else {
        Some(bytes.to_vec())
    }
}
