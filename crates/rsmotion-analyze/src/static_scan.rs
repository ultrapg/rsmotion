use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Bundle-level static feature hint discovered before running JS evaluation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StaticSignal {
    pub feature: String,
    pub source: String,
    pub detail: String,
}

/// Result of statically inspecting a local bundle directory (`index.html` and JS bundles).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StaticBundleReport {
    pub is_remote_url: bool,
    pub bundle_dir: Option<PathBuf>,
    pub script_files: Vec<PathBuf>,
    pub external_stylesheets: Vec<String>,
    pub signals: Vec<StaticSignal>,
}

/// Resolves `serve_url` to a local bundle directory when it is not an HTTP(S) URL.
pub fn resolve_local_bundle_dir(serve_url: &str) -> Option<PathBuf> {
    if serve_url.starts_with("http://") || serve_url.starts_with("https://") {
        return None;
    }
    let path = PathBuf::from(serve_url);
    if !path.exists() {
        return None;
    }
    if path.is_file() {
        Some(
            path.parent()
                .unwrap_or_else(|| Path::new("."))
                .to_path_buf(),
        )
    } else {
        Some(path)
    }
}

fn extract_attr(attrs_raw: &str, name: &str) -> Option<String> {
    let lower = attrs_raw.to_ascii_lowercase();
    let needle = format!("{name}=");
    let pos = lower.find(&needle)?;
    let rest = attrs_raw[pos + needle.len()..].trim_start();
    if let Some(q) = rest.chars().next().filter(|c| *c == '"' || *c == '\'') {
        let after = &rest[1..];
        let end = after.find(q)?;
        Some(after[..end].to_string())
    } else {
        let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
        Some(rest[..end].to_string())
    }
}

fn collect_html_tags(html: &str, tag: &str) -> Vec<String> {
    let lower = html.to_ascii_lowercase();
    let open = format!("<{tag}");
    let mut out = Vec::new();
    let mut cursor = 0usize;
    while let Some(rel) = lower[cursor..].find(&open) {
        let start = cursor + rel + open.len();
        let Some(end_rel) = lower[start..].find('>') else {
            break;
        };
        out.push(html[start..start + end_rel].to_string());
        cursor = start + end_rel + 1;
    }
    out
}

/// Scans `index.html` and local JS files for bundle-level hints and stylesheets.
pub fn scan_bundle_static(serve_url: &str) -> StaticBundleReport {
    if serve_url.starts_with("http://") || serve_url.starts_with("https://") {
        return StaticBundleReport {
            is_remote_url: true,
            bundle_dir: None,
            script_files: Vec::new(),
            external_stylesheets: Vec::new(),
            signals: vec![StaticSignal {
                feature: "remote-serve-url".to_string(),
                source: serve_url.to_string(),
                detail: "HTTP serveUrl requires Tier 2 browser host".to_string(),
            }],
        };
    }

    let Some(bundle_dir) = resolve_local_bundle_dir(serve_url) else {
        return StaticBundleReport {
            is_remote_url: false,
            bundle_dir: None,
            script_files: Vec::new(),
            external_stylesheets: Vec::new(),
            signals: vec![StaticSignal {
                feature: "missing-bundle".to_string(),
                source: serve_url.to_string(),
                detail: "local bundle path does not exist".to_string(),
            }],
        };
    };

    let index_path = bundle_dir.join("index.html");
    let Ok(html) = std::fs::read_to_string(&index_path) else {
        return StaticBundleReport {
            is_remote_url: false,
            bundle_dir: Some(bundle_dir),
            script_files: Vec::new(),
            external_stylesheets: Vec::new(),
            signals: vec![StaticSignal {
                feature: "missing-index-html".to_string(),
                source: index_path.display().to_string(),
                detail: "index.html missing or unreadable".to_string(),
            }],
        };
    };

    let mut external_stylesheets = Vec::new();
    let mut script_files = Vec::new();
    let mut signals = Vec::new();
    let mut seen: HashSet<(String, String)> = HashSet::new();

    let mut push_signal = |feature: &str, source: &str, detail: &str| {
        if seen.insert((feature.to_string(), source.to_string())) {
            signals.push(StaticSignal {
                feature: feature.to_string(),
                source: source.to_string(),
                detail: detail.to_string(),
            });
        }
    };

    for attrs in collect_html_tags(&html, "link") {
        if let Some(rel) = extract_attr(&attrs, "rel") {
            if rel.eq_ignore_ascii_case("stylesheet") {
                let href = extract_attr(&attrs, "href").unwrap_or_default();
                external_stylesheets.push(href.clone());
                push_signal(
                    "external-stylesheet",
                    "index.html",
                    &format!("<link rel=\"stylesheet\" href=\"{href}\">"),
                );
            }
        }
    }

    for attrs in collect_html_tags(&html, "script") {
        if let Some(src) = extract_attr(&attrs, "src") {
            let clean = src.split(['?', '#']).next().unwrap_or(&src);
            let clean = clean
                .strip_prefix("./")
                .unwrap_or(clean)
                .trim_start_matches('/');
            let cand = bundle_dir.join(clean);
            if cand.is_file() {
                script_files.push(cand);
            }
        }
    }

    const PATTERNS: &[(&str, &[&str], &str)] = &[
        (
            "webgl-three",
            &["WebGLRenderer", "@remotion/three"],
            "Three.js / WebGL reference detected in bundle",
        ),
        (
            "lottie",
            &["lottie-web", "@remotion/lottie", "bodymovin"],
            "Lottie player reference detected in bundle",
        ),
        (
            "backdrop-filter",
            &["backdropFilter", "backdrop-filter"],
            "CSS backdrop-filter style referenced in bundle",
        ),
        (
            "mix-blend-mode",
            &["mixBlendMode", "mix-blend-mode"],
            "CSS mix-blend-mode style referenced in bundle",
        ),
        (
            "canvas-context",
            &[
                ".getContext(\"2d\")",
                ".getContext('2d')",
                ".getContext(\"webgl",
                ".getContext('webgl",
            ],
            "HTML5 <canvas> context call detected in bundle",
        ),
        (
            "foreignObject",
            &["foreignObject"],
            "SVG <foreignObject> reference detected in bundle",
        ),
    ];

    for script_path in &script_files {
        let Ok(content) = std::fs::read_to_string(script_path) else {
            continue;
        };
        let src_name = script_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("bundle.js");
        for &(feature, needles, detail) in PATTERNS {
            if needles.iter().any(|needle| content.contains(needle)) {
                push_signal(feature, src_name, detail);
            }
        }
    }

    StaticBundleReport {
        is_remote_url: false,
        bundle_dir: Some(bundle_dir),
        script_files,
        external_stylesheets,
        signals,
    }
}
