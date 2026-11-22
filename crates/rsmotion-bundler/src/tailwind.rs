use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::UNIX_EPOCH;

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use tracing::warn;

fn find_tailwindcss_bin(remotion_root: &Path) -> Option<PathBuf> {
    let mut cur = Some(remotion_root);
    while let Some(dir) = cur {
        let bin = dir.join("node_modules/.bin/tailwindcss");
        if bin.is_file() {
            return Some(bin);
        }
        cur = dir.parent();
    }
    None
}

fn remotion_config_enables_tailwind(remotion_root: &Path) -> bool {
    for name in [
        "remotion.config.ts",
        "remotion.config.mts",
        "remotion.config.js",
        "remotion.config.mjs",
    ] {
        let p = remotion_root.join(name);
        if let Ok(content) = fs::read_to_string(&p) {
            if content.contains("enableTailwind") {
                return true;
            }
        }
    }
    false
}

fn css_uses_tailwind(raw_css: &str, remotion_root: &Path, flag: bool) -> bool {
    if flag || remotion_config_enables_tailwind(remotion_root) {
        return true;
    }
    raw_css.contains("@import \"tailwindcss\"")
        || raw_css.contains("@import 'tailwindcss'")
        || raw_css.contains("@tailwind ")
        || raw_css.contains("@tailwind\t")
}

fn hash_tree_state(dir: &Path, hasher: &mut Sha256, depth: usize) {
    if depth > 6 {
        return;
    }
    let Ok(rd) = fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = rd.filter_map(|e| e.ok()).collect();
    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        let name = entry.file_name();
        let s = name.to_string_lossy();
        if matches!(
            s.as_ref(),
            "node_modules" | ".git" | "dist" | "build" | "target"
        ) {
            continue;
        }
        let path = entry.path();
        let Ok(meta) = fs::metadata(&path) else {
            continue;
        };
        if meta.is_dir() {
            hash_tree_state(&path, hasher, depth + 1);
        } else if meta.is_file() {
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            if matches!(
                ext.as_str(),
                "ts" | "tsx" | "js" | "jsx" | "mjs" | "css" | "html"
            ) {
                hasher.update(path.to_string_lossy().as_bytes());
                let mtime = meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map(|d| d.as_nanos())
                    .unwrap_or(0);
                hasher.update(mtime.to_le_bytes());
                hasher.update(meta.len().to_le_bytes());
            }
        }
    }
}

fn tailwind_cache_dir() -> PathBuf {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"));
    home.join(".cache/rsmotion/tailwind")
}

fn run_tailwindcss_cli(
    tw_bin: &Path,
    css_path: &Path,
    remotion_root: &Path,
    raw_css: &str,
) -> Result<String> {
    let mut hasher = Sha256::new();
    hasher.update(b"tw-v1:");
    hasher.update(css_path.to_string_lossy().as_bytes());
    hasher.update(raw_css.as_bytes());
    let src_dir = remotion_root.join("src");
    if src_dir.is_dir() {
        hash_tree_state(&src_dir, &mut hasher, 0);
    } else {
        hash_tree_state(remotion_root, &mut hasher, 0);
    }
    let hex: String = hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();

    let cache_dir = tailwind_cache_dir();
    let _ = fs::create_dir_all(&cache_dir);
    let cached_css = cache_dir.join(format!("{hex}.css"));
    if cached_css.is_file() {
        if let Ok(hit) = fs::read_to_string(&cached_css) {
            return Ok(hit);
        }
    }

    let tmp_out = cache_dir.join(format!("{hex}.{}.tmp.css", std::process::id()));
    let out = Command::new(tw_bin)
        .current_dir(remotion_root)
        .arg("-i")
        .arg(css_path)
        .arg("-o")
        .arg(&tmp_out)
        .output()
        .with_context(|| format!("invoking tailwindcss CLI {}", tw_bin.display()))?;

    if !out.status.success() {
        let _ = fs::remove_file(&tmp_out);
        anyhow::bail!(
            "tailwindcss CLI failed (exit {:?}): {}",
            out.status.code(),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    let compiled = fs::read_to_string(&tmp_out)
        .with_context(|| format!("reading compiled tailwind CSS {}", tmp_out.display()))?;
    let _ = fs::rename(&tmp_out, &cached_css);
    Ok(compiled)
}

fn parse_local_import_spec(line: &str) -> Option<(String, &str)> {
    let trimmed = line.trim();
    if !trimmed.starts_with("@import") {
        return None;
    }
    let rest = trimmed["@import".len()..].trim_start();
    let (spec, tail) = if let Some(inner) = rest.strip_prefix("url(") {
        let end = inner.find(')')?;
        (
            inner[..end]
                .trim()
                .trim_matches(|c| c == '"' || c == '\'')
                .to_string(),
            inner[end + 1..].trim(),
        )
    } else {
        let q = rest.chars().next().filter(|c| *c == '"' || *c == '\'')?;
        let after = &rest[1..];
        let end = after.find(q)?;
        (after[..end].to_string(), after[end + 1..].trim())
    };

    if spec.starts_with("http://")
        || spec.starts_with("https://")
        || spec.starts_with("//")
        || spec.starts_with("data:")
    {
        return None;
    }
    if tail.starts_with(';') || tail.is_empty() {
        Some((spec, ""))
    } else {
        Some((spec, tail))
    }
}

fn inline_local_css_imports(css_path: &Path, visited: &mut HashSet<PathBuf>) -> Result<String> {
    let canon = fs::canonicalize(css_path).unwrap_or_else(|_| css_path.to_path_buf());
    if !visited.insert(canon.clone()) {
        return Ok(String::new());
    }
    let raw = fs::read_to_string(&canon)
        .with_context(|| format!("reading CSS file {}", canon.display()))?;
    let base_dir = canon.parent().unwrap_or_else(|| Path::new("."));

    let mut out = String::with_capacity(raw.len());
    for line in raw.lines() {
        if let Some((spec, _)) = parse_local_import_spec(line) {
            if spec.starts_with('.') {
                let candidate = base_dir.join(&spec);
                if candidate.is_file() {
                    out.push_str(&inline_local_css_imports(&candidate, visited)?);
                    out.push('\n');
                    continue;
                }
            }
        }
        out.push_str(line);
        out.push('\n');
    }
    Ok(out)
}

/// Processes a CSS file (optional Tailwind CLI compilation + local relative `@import` inlining).
pub fn process_css_file(
    css_path: &Path,
    remotion_root: &Path,
    enable_tailwind: bool,
) -> Result<(String, bool)> {
    let raw = fs::read_to_string(css_path)
        .with_context(|| format!("reading CSS file {}", css_path.display()))?;
    if css_uses_tailwind(&raw, remotion_root, enable_tailwind) {
        if let Some(tw_bin) = find_tailwindcss_bin(remotion_root) {
            let compiled = run_tailwindcss_cli(&tw_bin, css_path, remotion_root, &raw)?;
            return Ok((compiled, true));
        }
        warn!(
            "Tailwind CSS requested for {} but `node_modules/.bin/tailwindcss` was not found; falling back to plain CSS.",
            css_path.display()
        );
    }

    let mut visited = HashSet::new();
    let inlined = inline_local_css_imports(css_path, &mut visited)?;
    Ok((inlined, false))
}

fn extract_css_module_classes(css: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    let bytes = css.as_bytes();
    let mut i = 0usize;
    let mut brace_depth = 0usize;

    while i < bytes.len() {
        match bytes[i] {
            b'/' if i + 1 < bytes.len() && bytes[i + 1] == b'*' => {
                i += 2;
                while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                    i += 1;
                }
                i = (i + 2).min(bytes.len());
            }
            b'{' => {
                brace_depth += 1;
                i += 1;
            }
            b'}' => {
                brace_depth = brace_depth.saturating_sub(1);
                i += 1;
            }
            b'.' if brace_depth == 0 => {
                let start = i + 1;
                let mut end = start;
                while end < bytes.len()
                    && (bytes[end].is_ascii_alphanumeric()
                        || bytes[end] == b'_'
                        || bytes[end] == b'-')
                {
                    end += 1;
                }
                if end > start && !bytes[start].is_ascii_digit() {
                    let cls = &css[start..end];
                    if seen.insert(cls.to_string()) {
                        out.push(cls.to_string());
                    }
                }
                i = end;
            }
            _ => i += 1,
        }
    }
    out
}

/// Wraps CSS text in an idempotent runtime `<style>` injector ES module.
pub fn css_to_style_loader_js(
    css: &str,
    source_label: &str,
    is_css_module: bool,
) -> Result<String> {
    let css_json = serde_json::to_string(css)?;
    let key_json = serde_json::to_string(source_label)?;
    let mut js = format!(
        r#"const __rsmotion_css = {css_json};
const __rsmotion_css_key = {key_json};
if (typeof document !== "undefined") {{
  const __set = (globalThis.__rsmotion_css_injected ||= new Set());
  if (!__set.has(__rsmotion_css_key)) {{
    __set.add(__rsmotion_css_key);
    const __st = document.createElement("style");
    __st.setAttribute("data-rsmotion-css", __rsmotion_css_key);
    __st.appendChild(document.createTextNode(__rsmotion_css));
    const __target = document.head || document.getElementsByTagName("head")[0] || document.documentElement;
    if (__target && typeof __target.appendChild === "function") {{
      __target.appendChild(__st);
    }}
  }}
}}
"#
    );

    if is_css_module {
        let mut map = serde_json::Map::new();
        for cls in extract_css_module_classes(css) {
            map.insert(cls.clone(), serde_json::Value::String(cls));
        }
        let obj_json = serde_json::to_string(&serde_json::Value::Object(map))?;
        js.push_str(&format!("export default {obj_json};\n"));
    } else {
        js.push_str("export default __rsmotion_css;\n");
    }
    Ok(js)
}
