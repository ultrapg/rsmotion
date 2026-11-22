pub mod html;
pub mod rolldown_build;
pub mod tailwind;

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Instant, UNIX_EPOCH};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const HASH_MARKER_FILE: &str = ".rsmotion-bundle-hash";
const SOURCE_EXTENSIONS: &[&str] = &["ts", "tsx", "js", "jsx", "mjs", "cjs", "mts", "cts"];

/// Configuration for native Rust `rolldown` bundling of a Remotion source entrypoint.
#[derive(Debug, Clone)]
pub struct BundleOptions {
    pub entry_point: PathBuf,
    pub out_dir: Option<PathBuf>,
    pub public_dir: Option<PathBuf>,
    pub enable_tailwind: bool,
    pub use_cache: bool,
    pub symlink_public_dir: bool,
}

/// Summary report returned after bundling (or hitting the incremental cache).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleReport {
    pub out_dir: PathBuf,
    pub entry_point: PathBuf,
    pub bundle_js_bytes: u64,
    pub static_files_count: usize,
    pub tailwind_enabled: bool,
    pub cache_hit: bool,
    pub wall_ms: f64,
}

/// Returns true when `path_or_url` points to a source file (`.ts`/`.tsx`/`.js`/...) or project directory
/// needing bundling rather than an HTTP URL or an already built bundle directory (`index.html`).
pub fn is_source_entry(path_or_url: &str) -> bool {
    if path_or_url.starts_with("http://") || path_or_url.starts_with("https://") {
        return false;
    }
    let p = Path::new(path_or_url);
    if p.is_file() {
        let ext = p
            .extension()
            .and_then(|e| e.to_str())
            .map(|s| s.to_ascii_lowercase());
        return matches!(ext.as_deref(), Some(e) if SOURCE_EXTENSIONS.contains(&e));
    }
    if p.is_dir() && !p.join("index.html").is_file() {
        for cand in [
            "src/index.ts",
            "src/index.tsx",
            "src/index.js",
            "src/index.jsx",
            "index.ts",
            "index.tsx",
            "index.js",
            "index.jsx",
        ] {
            if p.join(cand).is_file() {
                return true;
            }
        }
    }
    false
}

/// Resolves a source file or Remotion project directory to a canonical `.ts`/`.tsx`/`.js`/`.jsx` entrypoint.
pub fn resolve_entry_file(path: &Path) -> Result<PathBuf> {
    if path.is_file() {
        return fs::canonicalize(path)
            .with_context(|| format!("canonicalizing entrypoint {}", path.display()));
    }
    if path.is_dir() {
        for cand in [
            "src/index.ts",
            "src/index.tsx",
            "src/index.js",
            "src/index.jsx",
            "index.ts",
            "index.tsx",
            "index.js",
            "index.jsx",
        ] {
            let p = path.join(cand);
            if p.is_file() {
                return fs::canonicalize(&p)
                    .with_context(|| format!("canonicalizing entrypoint {}", p.display()));
            }
        }
        bail!(
            "No Remotion entrypoint found in directory {} (checked src/index.{{ts,tsx,js,jsx}} and index.{{ts,tsx,js,jsx}})",
            path.display()
        );
    }
    bail!("Entrypoint does not exist: {}", path.display());
}

fn infer_remotion_root(entry_file: &Path) -> PathBuf {
    let mut cur = entry_file.parent();
    while let Some(dir) = cur {
        if dir.join("package.json").is_file() {
            return dir.to_path_buf();
        }
        cur = dir.parent();
    }
    entry_file
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf()
}

fn read_project_name(remotion_root: &Path) -> String {
    let pkg_path = remotion_root.join("package.json");
    if let Ok(raw) = fs::read_to_string(&pkg_path) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw) {
            if let Some(name) = v.get("name").and_then(|n| n.as_str()) {
                let clean = name.rsplit('/').next().unwrap_or(name).trim();
                if !clean.is_empty() {
                    return clean.to_string();
                }
            }
        }
    }
    remotion_root
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("remotion-video")
        .to_string()
}

fn hash_dir_inputs(dir: &Path, hasher: &mut Sha256, depth: usize) {
    if depth > 7 {
        return;
    }
    let Ok(rd) = fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = rd.filter_map(|e| e.ok()).collect();
    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        let fname = entry.file_name();
        let s = fname.to_string_lossy();
        if matches!(
            s.as_ref(),
            "node_modules" | ".git" | "build" | "dist" | "target" | ".remotion"
        ) {
            continue;
        }
        let path = entry.path();
        let Ok(meta) = fs::metadata(&path) else {
            continue;
        };
        if meta.is_dir() {
            hash_dir_inputs(&path, hasher, depth + 1);
        } else if meta.is_file() {
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

fn compute_bundle_input_hash(
    entry_file: &Path,
    remotion_root: &Path,
    public_dir: Option<&Path>,
    enable_tailwind: bool,
) -> String {
    let mut h = Sha256::new();
    h.update(b"rsmotion-bundler-v2:");
    h.update(entry_file.to_string_lossy().as_bytes());
    h.update(remotion_root.to_string_lossy().as_bytes());
    h.update([u8::from(enable_tailwind)]);
    if let Some(pd) = public_dir {
        h.update(pd.to_string_lossy().as_bytes());
    }
    for f in [
        "package.json",
        "package-lock.json",
        "tsconfig.json",
        "remotion.config.ts",
        "remotion.config.js",
        "remotion.config.mjs",
    ] {
        let p = remotion_root.join(f);
        if let Ok(bytes) = fs::read(&p) {
            h.update(f.as_bytes());
            h.update(bytes);
        }
    }
    let src_dir = remotion_root.join("src");
    if src_dir.is_dir() {
        hash_dir_inputs(&src_dir, &mut h, 0);
    } else if let Some(parent) = entry_file.parent() {
        hash_dir_inputs(parent, &mut h, 0);
    }
    if let Some(pd) = public_dir.filter(|p| p.is_dir()) {
        hash_dir_inputs(pd, &mut h, 0);
    }
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

fn default_cache_dir(hash: &str) -> PathBuf {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"));
    home.join(".cache/rsmotion/bundles").join(hash)
}

fn try_cache_hit(
    target_dir: &Path,
    expected_hash: &str,
    entry_file: &Path,
    static_files_count: usize,
    tailwind_hint: bool,
    started: Instant,
) -> Option<BundleReport> {
    let bundle_js = target_dir.join("bundle.js");
    let index_html = target_dir.join("index.html");
    let marker = target_dir.join(HASH_MARKER_FILE);
    if !bundle_js.is_file() || !index_html.is_file() {
        return None;
    }
    let stored = fs::read_to_string(&marker).ok()?;
    if stored.trim() != expected_hash {
        return None;
    }
    let bytes = fs::metadata(&bundle_js).ok()?.len();
    Some(BundleReport {
        out_dir: target_dir.to_path_buf(),
        entry_point: entry_file.to_path_buf(),
        bundle_js_bytes: bytes,
        static_files_count,
        tailwind_enabled: tailwind_hint,
        cache_hit: true,
        wall_ms: started.elapsed().as_secs_f64() * 1000.0,
    })
}

/// Bundles a Remotion TypeScript/JavaScript entrypoint using native Rust Rolldown.
pub async fn bundle_async(options: BundleOptions) -> Result<BundleReport> {
    let started = Instant::now();
    let entry_file = resolve_entry_file(&options.entry_point)?;
    let remotion_root = infer_remotion_root(&entry_file);
    let project_name = read_project_name(&remotion_root);

    let resolved_public = options.public_dir.clone().or_else(|| {
        let cand = remotion_root.join("public");
        cand.is_dir().then_some(cand)
    });
    let (static_files, has_public) = html::collect_static_files(resolved_public.as_deref())?;
    let input_hash = compute_bundle_input_hash(
        &entry_file,
        &remotion_root,
        resolved_public.as_deref(),
        options.enable_tailwind,
    );

    let out_dir = match options.out_dir {
        Some(dir) => {
            if dir.is_absolute() {
                dir
            } else {
                std::env::current_dir()?.join(dir)
            }
        }
        None => default_cache_dir(&input_hash),
    };

    if options.use_cache {
        if let Some(hit) = try_cache_hit(
            &out_dir,
            &input_hash,
            &entry_file,
            static_files.len(),
            options.enable_tailwind,
            started,
        ) {
            return Ok(hit);
        }
    }

    let (bundle_js_bytes, used_tw) = rolldown_build::run_rolldown_bundle(
        &entry_file,
        &remotion_root,
        &out_dir,
        options.enable_tailwind,
    )
    .await?;

    html::mirror_public_dir(
        resolved_public.as_deref(),
        &out_dir,
        options.symlink_public_dir,
    )?;
    let index_html =
        html::render_index_html(&project_name, &remotion_root, &static_files, has_public)?;
    fs::write(out_dir.join("index.html"), index_html)
        .with_context(|| format!("writing {}", out_dir.join("index.html").display()))?;
    fs::write(out_dir.join(HASH_MARKER_FILE), format!("{input_hash}\n"))?;

    Ok(BundleReport {
        out_dir,
        entry_point: entry_file,
        bundle_js_bytes,
        static_files_count: static_files.len(),
        tailwind_enabled: used_tw || options.enable_tailwind,
        cache_hit: false,
        wall_ms: started.elapsed().as_secs_f64() * 1000.0,
    })
}

/// Synchronous wrapper around [`bundle_async`] safe from both non-Tokio and multi-threaded Tokio contexts.
pub fn bundle(options: BundleOptions) -> Result<BundleReport> {
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        return tokio::task::block_in_place(|| handle.block_on(bundle_async(options)));
    }
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .context("building Tokio runtime for rsmotion-bundler")?;
    rt.block_on(bundle_async(options))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_source_entries_vs_built_bundles() {
        let fixtures_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join("tests/fixtures");
        assert!(is_source_entry(
            &fixtures_dir.join("src/index.ts").to_string_lossy()
        ));
        assert!(!is_source_entry(
            &fixtures_dir.join("build").to_string_lossy()
        ));
        assert!(!is_source_entry("http://localhost:3000/index.html"));
    }

    #[test]
    fn renders_compatible_index_html() {
        let html = html::render_index_html(
            "demo",
            Path::new("/work/demo"),
            &[html::StaticFileEntry {
                name: "sample.png".into(),
                last_modified: 123,
                size_in_bytes: 456,
                src: "./public/sample.png".into(),
            }],
            true,
        )
        .unwrap();
        assert!(html.contains("window.siteVersion = '12'"));
        assert!(html.contains("window.remotion_version = '4.0.545'"));
        assert!(html.contains("window.remotion_staticFiles"));
        assert!(html.contains("./public/sample.png"));
        assert!(html.contains("<script src=\"./bundle.js\"></script>"));
    }

    #[test]
    fn bundles_fixtures_and_hits_incremental_cache() {
        let fixtures_entry = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join("tests/fixtures/src/index.ts");
        let out_dir =
            std::env::temp_dir().join(format!("rsmotion-bundler-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&out_dir);

        let cold = bundle(BundleOptions {
            entry_point: fixtures_entry.clone(),
            out_dir: Some(out_dir.clone()),
            public_dir: None,
            enable_tailwind: false,
            use_cache: false,
            symlink_public_dir: false,
        })
        .expect("cold Rolldown bundle should succeed");

        assert!(!cold.cache_hit);
        assert!(
            cold.bundle_js_bytes > 50_000,
            "bytes={}",
            cold.bundle_js_bytes
        );
        assert!(out_dir.join("bundle.js").is_file());
        assert!(out_dir.join("index.html").is_file());
        assert!(out_dir.join("public/sample-audio.wav").is_file());

        let warm = bundle(BundleOptions {
            entry_point: fixtures_entry,
            out_dir: Some(out_dir.clone()),
            public_dir: None,
            enable_tailwind: false,
            use_cache: true,
            symlink_public_dir: false,
        })
        .expect("warm Rolldown cache check should succeed");

        let _ = fs::remove_dir_all(&out_dir);
        let max_cold_ms = if cfg!(debug_assertions) {
            15_000.0
        } else {
            2_000.0
        };
        assert!(
            cold.wall_ms < max_cold_ms,
            "cold bundle took {} ms (limit {} ms)",
            cold.wall_ms,
            max_cold_ms
        );
        assert!(warm.cache_hit);
        assert!(warm.wall_ms < 100.0, "warm hit took {} ms", warm.wall_ms);
    }
}
