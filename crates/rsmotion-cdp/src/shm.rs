use std::fs::OpenOptions;
use std::path::Path;

use base64::Engine;
use memmap2::Mmap;
use serde_json::json;
use tracing::{debug, info};

use crate::client::CdpClient;
use crate::error::{CdpError, Result};

const ENCODER_PADDING: usize = 64;

/// POSIX shared-memory or file-backed raw-BGRA capture pool for one Chromium page.
pub struct ShmFramePool {
    slot_capacity: usize,
    width: u32,
    height: u32,
    mmap: Mmap,
    destroyed: bool,
}

impl ShmFramePool {
    /// Tries to create a `Page.remotionCreateFramePool` on the target page.
    ///
    /// Returns `Ok(Some(pool))` if Remotion's patched Chromium SHM capture is supported,
    /// or `Ok(None)` if standard Chrome does not implement the extension.
    pub async fn try_create(
        client: &CdpClient,
        session_id: &str,
        width: u32,
        height: u32,
    ) -> Result<Option<Self>> {
        let slot_capacity = (width as usize) * (height as usize) * 4 + ENCODER_PADDING;

        let res = match client
            .send_session(
                session_id,
                "Page.remotionCreateFramePool",
                json!({
                    "slotCount": 1,
                    "slotCapacity": slot_capacity,
                }),
            )
            .await
        {
            Ok(v) => v,
            Err(CdpError::CdpRemote { message, .. })
                if message.contains("wasn't found")
                    || message.contains("Method not found")
                    || message.contains("Raw frames") =>
            {
                debug!("Remotion SHM frame pool unavailable ({message}); will use PNG fallback");
                return Ok(None);
            }
            Err(e) => return Err(e),
        };

        let shm_name = res
            .get("sharedMemoryName")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                CdpError::SharedMemory(format!("missing sharedMemoryName in response: {res}"))
            })?;
        let backend = res
            .get("backend")
            .and_then(|v| v.as_str())
            .unwrap_or("posix-shm");

        let backing_path = if backend == "posix-shm" {
            let trimmed = shm_name.trim_start_matches('/');
            Path::new("/dev/shm").join(trimmed)
        } else {
            Path::new(shm_name).to_path_buf()
        };

        let file = OpenOptions::new()
            .read(true)
            .open(&backing_path)
            .map_err(|e| {
                CdpError::SharedMemory(format!(
                    "failed to open shared memory backing `{}`: {e}",
                    backing_path.display()
                ))
            })?;

        // SAFETY: The shared memory region is sized by Chromium to `slot_capacity` bytes and
        // reads only happen between `captureScreenshot` return and `remotionReleaseFrame`.
        let mmap = unsafe {
            Mmap::map(&file).map_err(|e| {
                CdpError::SharedMemory(format!("mmap failed for `{}`: {e}", backing_path.display()))
            })?
        };

        if mmap.len() < slot_capacity {
            return Err(CdpError::SharedMemory(format!(
                "mapped SHM length {} < expected slotCapacity {slot_capacity}",
                mmap.len()
            )));
        }

        info!(
            "Initialized Remotion SHM BGRA pool ({backend}: {})",
            backing_path.display()
        );

        Ok(Some(Self {
            slot_capacity,
            width,
            height,
            mmap,
            destroyed: false,
        }))
    }

    /// Captures one packed `width * height * 4` BGRA8888 frame via shared memory.
    pub async fn capture_bgra(
        &self,
        client: &CdpClient,
        target_id: &str,
        session_id: &str,
        frame_id: &str,
    ) -> Result<Vec<u8>> {
        client
            .send("Target.activateTarget", json!({ "targetId": target_id }))
            .await?;
        let capture_res = client
            .send_session(
                session_id,
                "Page.captureScreenshot",
                json!({
                    "format": "jpeg",
                    "clip": {
                        "x": 0,
                        "y": 0,
                        "width": self.width,
                        "height": self.height,
                        "scale": 1
                    },
                    "captureBeyondViewport": true,
                    "optimizeForSpeed": true,
                    "fromSurface": true,
                    "remotionFrameSlot": 0,
                    "remotionFrameId": frame_id,
                    "remotionFastViewport": true,
                }),
            )
            .await?;

        let copy_result = (|| -> Result<Vec<u8>> {
            let meta = capture_res.get("remotionFrame").ok_or_else(|| {
                CdpError::SharedMemory(format!(
                    "missing remotionFrame metadata in captureScreenshot result: {capture_res}"
                ))
            })?;

            let w = meta.get("width").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            let h = meta.get("height").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            let stride = meta.get("stride").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            let byte_length = meta.get("byteLength").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            let pix_fmt = meta
                .get("pixelFormat")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            if w != self.width || h != self.height || pix_fmt != "bgra" {
                return Err(CdpError::SharedMemory(format!(
                    "unexpected SHM frame metadata: {meta}"
                )));
            }
            if byte_length > self.slot_capacity || byte_length > self.mmap.len() {
                return Err(CdpError::SharedMemory(format!(
                    "byteLength {byte_length} exceeds mapped capacity {}",
                    self.mmap.len()
                )));
            }

            let row_bytes = (w as usize) * 4;
            if stride < row_bytes || stride * (h as usize) > self.mmap.len() {
                return Err(CdpError::SharedMemory(format!(
                    "invalid SHM stride {stride} for width {w}"
                )));
            }

            let total_bytes = row_bytes * (h as usize);
            let mut out = vec![0u8; total_bytes];
            if stride == row_bytes {
                out.copy_from_slice(&self.mmap[..total_bytes]);
            } else {
                for row in 0..(h as usize) {
                    let src_off = row * stride;
                    let dst_off = row * row_bytes;
                    out[dst_off..dst_off + row_bytes]
                        .copy_from_slice(&self.mmap[src_off..src_off + row_bytes]);
                }
            }
            Ok(out)
        })();

        // Always release slot back to Chromium even if metadata/copy failed.
        let _ = client
            .send_session(
                session_id,
                "Page.remotionReleaseFrame",
                json!({
                    "slot": 0,
                    "frameId": frame_id,
                }),
            )
            .await;

        copy_result
    }

    pub async fn destroy(mut self, client: &CdpClient, session_id: &str) {
        if !self.destroyed {
            self.destroyed = true;
            let _ = client
                .send_session(session_id, "Page.remotionDestroyFramePool", json!({}))
                .await;
        }
    }
}

/// Fallback capture via `Page.captureScreenshot` PNG + base64 decode -> BGRA8888.
pub async fn capture_png_fallback_bgra(
    client: &CdpClient,
    target_id: &str,
    session_id: &str,
    width: u32,
    height: u32,
) -> Result<Vec<u8>> {
    client
        .send("Target.activateTarget", json!({ "targetId": target_id }))
        .await?;
    let res = client
        .send_session(
            session_id,
            "Page.captureScreenshot",
            json!({
                "format": "png",
                "clip": {
                    "x": 0,
                    "y": 0,
                    "width": width,
                    "height": height,
                    "scale": 1
                },
                "captureBeyondViewport": true,
                "optimizeForSpeed": true,
                "fromSurface": true,
            }),
        )
        .await?;

    let b64 = res.get("data").and_then(|v| v.as_str()).ok_or_else(|| {
        CdpError::Protocol("missing base64 PNG data from captureScreenshot".into())
    })?;
    let png_bytes = base64::engine::general_purpose::STANDARD
        .decode(b64)
        .map_err(|e| CdpError::Protocol(format!("base64 decode failed: {e}")))?;
    let rgba_img =
        image::load_from_memory_with_format(&png_bytes, image::ImageFormat::Png)?.to_rgba8();

    let mut bgra = rgba_img.into_raw();
    for px in bgra.as_chunks_mut::<4>().0 {
        px.swap(0, 2);
    }
    Ok(bgra)
}
