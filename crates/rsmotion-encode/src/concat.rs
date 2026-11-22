use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::error::{EncodeError, Result};

/// Concatenates rendered video chunks losslessly using FFmpeg's concat demuxer.
pub fn concat_chunks(chunk_paths: &[PathBuf], output: &Path) -> Result<()> {
    if chunk_paths.is_empty() {
        return Err(EncodeError::FfmpegFailed {
            code: -1,
            stderr: "concat_chunks called with empty chunk list".to_string(),
        });
    }

    if let Some(parent) = output.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }

    let list_path = std::env::temp_dir().join(format!(
        "rsmotion-concat-{}-{}.txt",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));

    {
        let mut f = std::fs::File::create(&list_path)?;
        for chunk in chunk_paths {
            let abs = std::fs::canonicalize(chunk).unwrap_or_else(|_| chunk.clone());
            let escaped = abs.to_string_lossy().replace('\'', "'\\''");
            writeln!(f, "file '{escaped}'")?;
        }
    }

    let out = Command::new("ffmpeg")
        .args([
            "-y",
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "concat",
            "-safe",
            "0",
            "-i",
        ])
        .arg(&list_path)
        .args(["-c", "copy", "-movflags", "+faststart"])
        .arg(output)
        .output();

    let _ = std::fs::remove_file(&list_path);
    let out = out?;

    if !out.status.success() {
        return Err(EncodeError::FfmpegFailed {
            code: out.status.code().unwrap_or(-1),
            stderr: String::from_utf8_lossy(&out.stderr).trim().to_string(),
        });
    }

    Ok(())
}
