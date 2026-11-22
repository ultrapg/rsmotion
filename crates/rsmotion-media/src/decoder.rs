use std::collections::{HashMap, VecDeque};
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::Arc;

use tracing::debug;

use crate::error::{MediaError, Result};
use crate::probe::{probe_media, MediaMetadata};
use crate::uri::resolve_media_path;

const DEFAULT_CACHE_BUDGET_BYTES: usize = 64 * 1024 * 1024;
const MAX_FORWARD_SKIP_FRAMES: i64 = 12;

/// Single decoded video frame stored as tightly packed RGBA8888 bytes.
#[derive(Debug, Clone)]
pub struct DecodedVideoFrame {
    pub width: u32,
    pub height: u32,
    pub frame_index: i64,
    pub rgba: Arc<Vec<u8>>,
}

struct ActiveVideoStream {
    child: Child,
    stdout: BufReader<ChildStdout>,
    width: u32,
    height: u32,
    next_frame_idx: i64,
    last_frame: Option<DecodedVideoFrame>,
    eof: bool,
}

impl Drop for ActiveVideoStream {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl ActiveVideoStream {
    fn spawn(path: &Path, meta: &MediaMetadata, start_idx: i64, transparent: bool) -> Result<Self> {
        let mut cmd = Command::new("ffmpeg");
        cmd.args(["-hide_banner", "-loglevel", "error"]);

        if transparent && meta.codec_name.as_deref() == Some("vp9") {
            cmd.args(["-c:v", "libvpx-vp9"]);
        }

        if start_idx > 0 {
            let seek_secs = (start_idx as f64) / meta.fps;
            cmd.args(["-ss", &format!("{seek_secs:.6}")]);
        }

        cmd.arg("-i").arg(path).args([
            "-an", "-sn", "-dn", "-f", "rawvideo", "-pix_fmt", "rgba", "pipe:1",
        ]);
        cmd.stdin(Stdio::null());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::null());

        let mut child = cmd.spawn()?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| MediaError::DecodeFailed("missing ffmpeg stdout pipe".into()))?;

        debug!(
            "spawned persistent video stream for {} at frame {}",
            path.display(),
            start_idx
        );

        Ok(Self {
            child,
            stdout: BufReader::with_capacity(256 * 1024, stdout),
            width: meta.width,
            height: meta.height,
            next_frame_idx: start_idx,
            last_frame: None,
            eof: false,
        })
    }

    fn read_next_frame(&mut self) -> Result<Option<DecodedVideoFrame>> {
        if self.eof {
            return Ok(None);
        }
        let byte_len = (self.width as usize) * (self.height as usize) * 4;
        let mut buf = vec![0u8; byte_len];
        match self.stdout.read_exact(&mut buf) {
            Ok(()) => {
                let frame = DecodedVideoFrame {
                    width: self.width,
                    height: self.height,
                    frame_index: self.next_frame_idx,
                    rgba: Arc::new(buf),
                };
                self.next_frame_idx += 1;
                self.last_frame = Some(frame.clone());
                Ok(Some(frame))
            }
            Err(err) if err.kind() == std::io::ErrorKind::UnexpectedEof => {
                self.eof = true;
                Ok(None)
            }
            Err(err) => Err(MediaError::Io(err)),
        }
    }
}

/// Manages persistent sequential FFmpeg video streams and a bounded RGBA frame cache.
pub struct VideoDecoderPool {
    bundle_dir: PathBuf,
    meta_cache: HashMap<PathBuf, MediaMetadata>,
    src_path_cache: HashMap<String, Option<PathBuf>>,
    streams: HashMap<(PathBuf, bool), ActiveVideoStream>,
    frame_cache: HashMap<(PathBuf, i64, bool), DecodedVideoFrame>,
    cache_order: VecDeque<(PathBuf, i64, bool)>,
    cache_bytes: usize,
    cache_budget_bytes: usize,
}

impl VideoDecoderPool {
    pub fn new(bundle_dir: &Path) -> Self {
        Self {
            bundle_dir: bundle_dir.to_path_buf(),
            meta_cache: HashMap::new(),
            src_path_cache: HashMap::new(),
            streams: HashMap::new(),
            frame_cache: HashMap::new(),
            cache_order: VecDeque::new(),
            cache_bytes: 0,
            cache_budget_bytes: DEFAULT_CACHE_BUDGET_BYTES,
        }
    }

    /// Resolves `raw_src` against `bundle_dir`, memoizing lookups.
    pub fn resolve_path(&mut self, raw_src: &str) -> Option<PathBuf> {
        if let Some(cached) = self.src_path_cache.get(raw_src) {
            return cached.clone();
        }
        let resolved = resolve_media_path(&self.bundle_dir, raw_src);
        self.src_path_cache
            .insert(raw_src.to_string(), resolved.clone());
        resolved
    }

    /// Probes and caches metadata for `raw_src`.
    pub fn probe(&mut self, raw_src: &str) -> Result<MediaMetadata> {
        let path = self
            .resolve_path(raw_src)
            .ok_or_else(|| MediaError::NotFound(raw_src.to_string()))?;
        if let Some(meta) = self.meta_cache.get(&path) {
            return Ok(meta.clone());
        }
        let meta = probe_media(&path)?;
        self.meta_cache.insert(path, meta.clone());
        Ok(meta)
    }

    /// Returns cached `(width, height)` for `raw_src` if previously probed.
    pub fn cached_dimensions(&self, raw_src: &str) -> Option<(u32, u32)> {
        let path = self.src_path_cache.get(raw_src)?.as_ref()?;
        let meta = self.meta_cache.get(path)?;
        meta.has_video.then_some((meta.width, meta.height))
    }

    fn insert_cache(&mut self, key: (PathBuf, i64, bool), frame: DecodedVideoFrame) {
        let size = frame.rgba.len();
        if size > self.cache_budget_bytes {
            return;
        }
        if let Some(prev) = self.frame_cache.insert(key.clone(), frame) {
            self.cache_bytes = self.cache_bytes.saturating_sub(prev.rgba.len());
        } else {
            self.cache_order.push_back(key);
        }
        self.cache_bytes += size;

        while self.cache_bytes > self.cache_budget_bytes {
            let Some(oldest) = self.cache_order.pop_front() else {
                break;
            };
            if let Some(removed) = self.frame_cache.remove(&oldest) {
                self.cache_bytes = self.cache_bytes.saturating_sub(removed.rgba.len());
            }
        }
    }

    /// Decodes (or fetches from cache / sequential pipe) the frame at `time_secs`.
    pub fn decode_frame(
        &mut self,
        raw_src: &str,
        time_secs: f64,
        transparent: bool,
    ) -> Result<DecodedVideoFrame> {
        let path = self
            .resolve_path(raw_src)
            .ok_or_else(|| MediaError::NotFound(raw_src.to_string()))?;
        let meta = if let Some(m) = self.meta_cache.get(&path) {
            m.clone()
        } else {
            let m = probe_media(&path)?;
            self.meta_cache.insert(path.clone(), m.clone());
            m
        };

        if !meta.has_video || meta.width == 0 || meta.height == 0 {
            return Err(MediaError::DecodeFailed(format!(
                "source has no video stream: {}",
                path.display()
            )));
        }

        let raw_idx = ((time_secs.max(0.0) + 1e-4) * meta.fps).floor() as i64;
        let max_idx = meta
            .nb_frames
            .filter(|&n| n > 0)
            .map(|n| (n - 1) as i64)
            .or_else(|| {
                (meta.duration_secs > 0.0)
                    .then(|| ((meta.duration_secs * meta.fps).floor() as i64 - 1).max(0))
            });
        let target_idx = match max_idx {
            Some(mx) => raw_idx.clamp(0, mx),
            None => raw_idx.max(0),
        };

        let cache_key = (path.clone(), target_idx, transparent);
        if let Some(hit) = self.frame_cache.get(&cache_key) {
            return Ok(hit.clone());
        }

        let stream_key = (path.clone(), transparent);
        let can_reuse = self.streams.get(&stream_key).is_some_and(|st| {
            !st.eof
                && target_idx >= st.next_frame_idx
                && (target_idx - st.next_frame_idx) <= MAX_FORWARD_SKIP_FRAMES
        });

        if !can_reuse {
            self.streams.remove(&stream_key);
            let stream = ActiveVideoStream::spawn(&path, &meta, target_idx, transparent)?;
            self.streams.insert(stream_key.clone(), stream);
        }

        let mut buffered: Vec<DecodedVideoFrame> = Vec::new();
        let result = {
            let stream = self
                .streams
                .get_mut(&stream_key)
                .ok_or_else(|| MediaError::DecodeFailed("active stream vanished".into()))?;
            let mut found: Option<DecodedVideoFrame> = None;
            while stream.next_frame_idx <= target_idx {
                match stream.read_next_frame()? {
                    Some(df) => {
                        let done = df.frame_index == target_idx;
                        if done {
                            found = Some(df.clone());
                        }
                        buffered.push(df);
                        if done {
                            break;
                        }
                    }
                    None => {
                        found = stream.last_frame.clone();
                        break;
                    }
                }
            }
            found
        };

        for df in buffered {
            let k = (path.clone(), df.frame_index, transparent);
            self.insert_cache(k, df);
        }

        result.ok_or_else(|| {
            MediaError::DecodeFailed(format!(
                "failed to decode frame {} of {}",
                target_idx,
                path.display()
            ))
        })
    }
}
