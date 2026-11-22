use std::fs;
use std::path::Path;
use std::time::UNIX_EPOCH;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Metadata entry for a file inside `public/`, matching Remotion `window.remotion_staticFiles`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StaticFileEntry {
    pub name: String,
    pub last_modified: u64,
    pub size_in_bytes: u64,
    pub src: String,
}

fn walk_public_dir(root: &Path, dir: &Path, out: &mut Vec<StaticFileEntry>) -> Result<()> {
    let mut entries: Vec<_> = fs::read_dir(dir)
        .with_context(|| format!("reading public dir {}", dir.display()))?
        .filter_map(|e| e.ok())
        .collect();
    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        let path = entry.path();
        let meta =
            fs::metadata(&path).with_context(|| format!("stat public asset {}", path.display()))?;
        if meta.is_dir() {
            walk_public_dir(root, &path, out)?;
        } else if meta.is_file() {
            let rel = path.strip_prefix(root).unwrap_or(&path);
            let posix = rel
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/");
            let last_modified = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);
            out.push(StaticFileEntry {
                src: format!("./public/{posix}"),
                name: posix,
                last_modified,
                size_in_bytes: meta.len(),
            });
        }
    }
    Ok(())
}

/// Recursively lists all static files under `public_dir` in deterministic sorted order.
pub fn collect_static_files(public_dir: Option<&Path>) -> Result<(Vec<StaticFileEntry>, bool)> {
    let Some(dir) = public_dir else {
        return Ok((Vec::new(), false));
    };
    if !dir.is_dir() {
        return Ok((Vec::new(), false));
    }
    let mut files = Vec::new();
    walk_public_dir(dir, dir, &mut files)?;
    files.sort_by(|a, b| a.name.cmp(&b.name));
    Ok((files, true))
}

fn remove_existing_path(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(meta) => {
            if meta.file_type().is_symlink() || meta.is_file() {
                fs::remove_file(path)
                    .with_context(|| format!("removing stale file {}", path.display()))?;
            } else if meta.is_dir() {
                fs::remove_dir_all(path)
                    .with_context(|| format!("removing stale directory {}", path.display()))?;
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e).with_context(|| format!("inspecting {}", path.display())),
    }
    Ok(())
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst).with_context(|| format!("creating {}", dst.display()))?;
    for entry in fs::read_dir(src).with_context(|| format!("reading {}", src.display()))? {
        let entry = entry?;
        let sp = entry.path();
        let dp = dst.join(entry.file_name());
        let meta = fs::metadata(&sp)?;
        if meta.is_dir() {
            copy_dir_recursive(&sp, &dp)?;
        } else if meta.is_file() {
            fs::copy(&sp, &dp).with_context(|| {
                format!("copying public file {} -> {}", sp.display(), dp.display())
            })?;
        }
    }
    Ok(())
}

/// Mirrors `public_dir` into `<out_dir>/public` (symlinked on Unix when requested, else copied).
pub fn mirror_public_dir(public_dir: Option<&Path>, out_dir: &Path, symlink: bool) -> Result<()> {
    let dst_public = out_dir.join("public");
    remove_existing_path(&dst_public)?;

    let Some(src_dir) = public_dir else {
        return Ok(());
    };
    if !src_dir.is_dir() {
        return Ok(());
    }

    let abs_src = fs::canonicalize(src_dir).unwrap_or_else(|_| src_dir.to_path_buf());
    #[cfg(unix)]
    if symlink {
        std::os::unix::fs::symlink(&abs_src, &dst_public).with_context(|| {
            format!(
                "symlinking public dir {} -> {}",
                abs_src.display(),
                dst_public.display()
            )
        })?;
        return Ok(());
    }

    copy_dir_recursive(&abs_src, &dst_public)
}

/// Emits a minimal headless `index.html` compatible with the Remotion runtime protocol.
pub fn render_index_html(
    project_name: &str,
    cwd: &Path,
    static_files: &[StaticFileEntry],
    has_public: bool,
) -> Result<String> {
    let project_json = serde_json::to_string(project_name)?;
    let cwd_json = serde_json::to_string(&cwd.to_string_lossy().into_owned())?;
    let static_json = serde_json::to_string(static_files)?;
    let public_exists_expr = if has_public {
        r#"new URL("./public", window.location.href).pathname"#
    } else {
        "null"
    };

    Ok(format!(
        r#"<!DOCTYPE html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>rsmotion Bundle</title>
    <style>html, body {{ margin: 0; padding: 0; }}</style>
  </head>
  <body>
    <script>
      window.remotion_staticBase = new URL("./public", window.location.href).pathname;
      window.remotion_publicPath = "./";
      window.remotion_publicFolderExists = {public_exists_expr};
      window.remotion_staticFiles = {static_json}.map((file) => ({{
        ...file,
        src: new URL(file.src, window.location.href).pathname,
      }}));
      window.remotion_sampleRate = window.remotion_sampleRate ?? 48000;
      window.remotion_audioEnabled ??= true;
      window.remotion_videoEnabled ??= true;
      window.remotion_projectName = {project_json};
      window.remotion_cwd = {cwd_json};
      window.siteVersion = '12';
      window.remotion_version = '4.0.545';
    </script>
    <div id="video-container"></div>
    <script src="./bundle.js"></script>
  </body>
</html>
"#
    ))
}
