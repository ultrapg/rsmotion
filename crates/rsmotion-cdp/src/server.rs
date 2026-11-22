use std::collections::{HashMap, VecDeque};
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use tiny_http::{Header, Method, Response, Server, StatusCode};
use url::Url;

use crate::error::{CdpError, Result};

const PROXY_CACHE_BUDGET_BYTES: usize = 48 * 1024 * 1024;
const BATCH_FRAMES: usize = 16;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ProxyCacheKey {
    path: PathBuf,
    frame_bucket: i64,
    transparent: bool,
}

struct CachedFrame {
    content_type: &'static str,
    bytes: Arc<Vec<u8>>,
}

struct FrameCache {
    map: HashMap<ProxyCacheKey, CachedFrame>,
    order: VecDeque<ProxyCacheKey>,
    total_bytes: usize,
    fps_cache: HashMap<PathBuf, f64>,
}

impl FrameCache {
    fn new() -> Self {
        Self {
            map: HashMap::new(),
            order: VecDeque::new(),
            total_bytes: 0,
            fps_cache: HashMap::new(),
        }
    }

    fn get(&self, key: &ProxyCacheKey) -> Option<CachedFrame> {
        self.map.get(key).map(|entry| CachedFrame {
            content_type: entry.content_type,
            bytes: Arc::clone(&entry.bytes),
        })
    }

    fn insert(&mut self, key: ProxyCacheKey, val: CachedFrame) {
        let size = val.bytes.len();
        if size > PROXY_CACHE_BUDGET_BYTES {
            return;
        }
        if let Some(old) = self.map.insert(key.clone(), val) {
            self.total_bytes = self.total_bytes.saturating_sub(old.bytes.len());
        } else {
            self.order.push_back(key);
        }
        self.total_bytes += size;

        while self.total_bytes > PROXY_CACHE_BUDGET_BYTES {
            let Some(oldest) = self.order.pop_front() else {
                break;
            };
            if let Some(removed) = self.map.remove(&oldest) {
                self.total_bytes = self.total_bytes.saturating_sub(removed.bytes.len());
            }
        }
    }
}

fn mime_for_path(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase())
        .as_deref()
    {
        Some("html" | "htm") => "text/html; charset=utf-8",
        Some("js" | "mjs" | "cjs") => "application/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("json" | "map") => "application/json; charset=utf-8",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        Some("gif") => "image/gif",
        Some("svg") => "image/svg+xml",
        Some("bmp") => "image/bmp",
        Some("ttf") => "font/ttf",
        Some("otf") => "font/otf",
        Some("woff") => "font/woff",
        Some("woff2") => "font/woff2",
        Some("mp4") => "video/mp4",
        Some("webm") => "video/webm",
        Some("wav") => "audio/wav",
        Some("mp3") => "audio/mpeg",
        Some("aac") => "audio/aac",
        _ => "application/octet-stream",
    }
}

fn probe_fps(path: &Path) -> f64 {
    let out = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=r_frame_rate",
            "-of",
            "csv=p=0",
        ])
        .arg(path)
        .output();

    if let Ok(o) = out {
        if o.status.success() {
            let text = String::from_utf8_lossy(&o.stdout);
            let s = text.trim();
            if let Some((num, den)) = s.split_once('/') {
                if let (Ok(n), Ok(d)) = (num.parse::<f64>(), den.parse::<f64>()) {
                    if d > 0.0 && n > 0.0 {
                        return n / d;
                    }
                }
            } else if let Ok(v) = s.parse::<f64>() {
                if v > 0.0 {
                    return v;
                }
            }
        }
    }
    30.0
}

fn split_bmp_stream(buf: &[u8]) -> Vec<Vec<u8>> {
    let mut frames = Vec::new();
    let mut offset = 0usize;
    while offset + 6 <= buf.len() {
        if buf[offset] != 0x42 || buf[offset + 1] != 0x4D {
            break;
        }
        let size = u32::from_le_bytes([
            buf[offset + 2],
            buf[offset + 3],
            buf[offset + 4],
            buf[offset + 5],
        ]) as usize;
        if size < 54 || offset + size > buf.len() {
            break;
        }
        frames.push(buf[offset..offset + size].to_vec());
        offset += size;
    }
    frames
}

fn percent_decode(input: &str) -> String {
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

/// Resolves a URL or asset `src` string against an optional local static directory.
pub fn resolve_local_asset(src: &str, serve_dir: Option<&Path>) -> Option<PathBuf> {
    if let Some(rest) = src.strip_prefix("file://") {
        let p = PathBuf::from(percent_decode(rest));
        return p.exists().then_some(p);
    }

    if let Ok(parsed) = Url::parse(src) {
        let host = parsed.host_str().unwrap_or_default();
        if (host == "127.0.0.1" || host == "localhost") && serve_dir.is_some() {
            let base = serve_dir?;
            let rel = percent_decode(parsed.path().trim_start_matches('/'));
            let cand = base.join(rel);
            if cand.exists() {
                return Some(cand);
            }
        }
        return None;
    }

    if let Some(base) = serve_dir {
        let cand = base.join(percent_decode(src.trim_start_matches('/')));
        if cand.exists() {
            return Some(cand);
        }
    }
    let direct = PathBuf::from(src);
    direct.exists().then_some(direct)
}

fn extract_proxy_frame(
    src: &str,
    time: f64,
    transparent: bool,
    serve_dir: Option<&Path>,
    cache: &Mutex<FrameCache>,
) -> std::result::Result<CachedFrame, String> {
    let local_path = resolve_local_asset(src, serve_dir)
        .ok_or_else(|| format!("cannot resolve local media source: {src}"))?;

    let fps = {
        let mut guard = cache.lock().unwrap();
        if let Some(&f) = guard.fps_cache.get(&local_path) {
            f
        } else {
            let f = probe_fps(&local_path);
            guard.fps_cache.insert(local_path.clone(), f);
            f
        }
    };

    let bucket = (time * fps).round() as i64;
    let key = ProxyCacheKey {
        path: local_path.clone(),
        frame_bucket: bucket,
        transparent,
    };

    if let Some(hit) = cache.lock().unwrap().get(&key) {
        return Ok(hit);
    }

    let time_str = format!("{:.6}", time.max(0.0));
    if transparent {
        let out = Command::new("ffmpeg")
            .args(["-hide_banner", "-loglevel", "error", "-ss", &time_str, "-i"])
            .arg(&local_path)
            .args(["-frames:v", "1", "-c:v", "png", "-f", "image2pipe", "-"])
            .output()
            .map_err(|e| format!("failed to spawn ffmpeg: {e}"))?;
        if !out.status.success() || out.stdout.is_empty() {
            return Err(format!(
                "ffmpeg png extract failed: {}",
                String::from_utf8_lossy(&out.stderr)
            ));
        }
        let frame = CachedFrame {
            content_type: "image/png",
            bytes: Arc::new(out.stdout),
        };
        cache.lock().unwrap().insert(
            key,
            CachedFrame {
                content_type: frame.content_type,
                bytes: Arc::clone(&frame.bytes),
            },
        );
        return Ok(frame);
    }

    let batch_str = BATCH_FRAMES.to_string();
    let out = Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-ss", &time_str, "-i"])
        .arg(&local_path)
        .args([
            "-frames:v",
            &batch_str,
            "-c:v",
            "bmp",
            "-f",
            "image2pipe",
            "-",
        ])
        .output()
        .map_err(|e| format!("failed to spawn ffmpeg: {e}"))?;

    if !out.status.success() || out.stdout.is_empty() {
        return Err(format!(
            "ffmpeg bmp batch extract failed: {}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }

    let bmp_frames = split_bmp_stream(&out.stdout);
    if bmp_frames.is_empty() {
        return Err("ffmpeg returned zero valid BMP frames".to_string());
    }

    let mut first_frame: Option<CachedFrame> = None;
    {
        let mut guard = cache.lock().unwrap();
        for (offset, bytes) in bmp_frames.into_iter().enumerate() {
            let entry = CachedFrame {
                content_type: "image/bmp",
                bytes: Arc::new(bytes),
            };
            if offset == 0 {
                first_frame = Some(CachedFrame {
                    content_type: entry.content_type,
                    bytes: Arc::clone(&entry.bytes),
                });
            }
            guard.insert(
                ProxyCacheKey {
                    path: local_path.clone(),
                    frame_bucket: bucket + (offset as i64),
                    transparent: false,
                },
                entry,
            );
        }
    }

    first_frame.ok_or_else(|| "missing first extracted frame".to_string())
}

fn cors_header() -> Header {
    Header::from_bytes("Access-Control-Allow-Origin", "*").unwrap()
}

fn handle_request(req: tiny_http::Request, serve_dir: Option<&Path>, cache: &Mutex<FrameCache>) {
    let url_str = req.url().to_string();
    let method = req.method().clone();

    if method == Method::Options {
        let mut resp = Response::empty(StatusCode(200));
        resp.add_header(cors_header());
        resp.add_header(
            Header::from_bytes("Access-Control-Allow-Private-Network", "true").unwrap(),
        );
        resp.add_header(
            Header::from_bytes("Access-Control-Allow-Methods", "GET, HEAD, OPTIONS").unwrap(),
        );
        let _ = req.respond(resp);
        return;
    }

    if url_str.starts_with("/proxy") {
        let parsed = match Url::parse(&format!("http://127.0.0.1{url_str}")) {
            Ok(u) => u,
            Err(e) => {
                let _ = req.respond(
                    Response::from_string(format!("invalid proxy URL: {e}"))
                        .with_status_code(StatusCode(400)),
                );
                return;
            }
        };

        let mut src = None;
        let mut time = 0.0_f64;
        let mut transparent = false;
        for (k, v) in parsed.query_pairs() {
            match k.as_ref() {
                "src" => src = Some(v.into_owned()),
                "time" => time = v.parse::<f64>().unwrap_or(0.0),
                "transparent" => transparent = v == "true",
                _ => {}
            }
        }

        let Some(src) = src else {
            let _ = req.respond(
                Response::from_string("missing src query param").with_status_code(StatusCode(400)),
            );
            return;
        };

        match extract_proxy_frame(&src, time, transparent, serve_dir, cache) {
            Ok(frame) => {
                let mut resp = Response::from_data(frame.bytes.as_slice().to_vec());
                resp.add_header(cors_header());
                resp.add_header(
                    Header::from_bytes("Cache-Control", "no-cache, no-store, must-revalidate")
                        .unwrap(),
                );
                resp.add_header(Header::from_bytes("Content-Type", frame.content_type).unwrap());
                let _ = req.respond(resp);
            }
            Err(err) => {
                let mut resp = Response::from_string(err).with_status_code(StatusCode(500));
                resp.add_header(cors_header());
                let _ = req.respond(resp);
            }
        }
        return;
    }

    let Some(base_dir) = serve_dir else {
        let _ = req.respond(Response::empty(StatusCode(404)));
        return;
    };

    let req_path = url_str.split('?').next().unwrap_or("/");
    let rel = percent_decode(req_path.trim_start_matches('/'));
    let mut candidate = if rel.is_empty() {
        base_dir.join("index.html")
    } else {
        base_dir.join(rel)
    };
    if candidate.is_dir() {
        candidate = candidate.join("index.html");
    }

    let Ok(canon_base) = std::fs::canonicalize(base_dir) else {
        let _ = req.respond(Response::empty(StatusCode(404)));
        return;
    };
    let Ok(canon_file) = std::fs::canonicalize(&candidate) else {
        let _ = req.respond(Response::empty(StatusCode(404)));
        return;
    };
    if !canon_file.starts_with(&canon_base) || !canon_file.is_file() {
        let _ = req.respond(Response::empty(StatusCode(403)));
        return;
    }

    let mime = mime_for_path(&canon_file);
    let Ok(mut file) = File::open(&canon_file) else {
        let _ = req.respond(Response::empty(StatusCode(404)));
        return;
    };
    let mut bytes = Vec::new();
    if file.read_to_end(&mut bytes).is_err() {
        let _ = req.respond(Response::empty(StatusCode(500)));
        return;
    }

    let range_header = req
        .headers()
        .iter()
        .find(|h| h.field.equiv("Range"))
        .map(|h| h.value.as_str().to_string());

    if let Some(range_val) = range_header {
        if let Some(spec) = range_val.strip_prefix("bytes=") {
            if let Some((start_s, end_s)) = spec.split_once('-') {
                let total = bytes.len();
                if total > 0 {
                    let start = start_s.trim().parse::<usize>().unwrap_or(0);
                    let end = if end_s.trim().is_empty() {
                        total - 1
                    } else {
                        end_s
                            .trim()
                            .parse::<usize>()
                            .unwrap_or(total - 1)
                            .min(total - 1)
                    };
                    if start <= end && start < total {
                        let slice = bytes[start..=end].to_vec();
                        let mut resp = Response::from_data(slice).with_status_code(StatusCode(206));
                        resp.add_header(cors_header());
                        resp.add_header(Header::from_bytes("Accept-Ranges", "bytes").unwrap());
                        resp.add_header(
                            Header::from_bytes(
                                "Content-Range",
                                format!("bytes {start}-{end}/{total}"),
                            )
                            .unwrap(),
                        );
                        resp.add_header(Header::from_bytes("Content-Type", mime).unwrap());
                        let _ = req.respond(resp);
                        return;
                    }
                }
            }
        }
    }

    let mut resp = Response::from_data(bytes);
    resp.add_header(cors_header());
    resp.add_header(Header::from_bytes("Accept-Ranges", "bytes").unwrap());
    resp.add_header(Header::from_bytes("Content-Type", mime).unwrap());
    let _ = req.respond(resp);
}

/// Embedded static bundle + `/proxy` video-frame extraction server.
pub struct EmbeddedServer {
    port: u16,
    serve_dir: Option<PathBuf>,
    server: Arc<Server>,
    shutdown: Arc<AtomicBool>,
    workers: Vec<JoinHandle<()>>,
}

impl EmbeddedServer {
    /// Starts an HTTP server on `127.0.0.1:0` serving optional static `serve_dir` and `/proxy`.
    pub fn start(serve_dir: Option<PathBuf>) -> Result<Self> {
        let server = Server::http("127.0.0.1:0")
            .map_err(|e| CdpError::Server(format!("failed to bind HTTP server: {e}")))?;
        let port = server
            .server_addr()
            .to_ip()
            .ok_or_else(|| CdpError::Server("missing socket port".to_string()))?
            .port();

        let server = Arc::new(server);
        let shutdown = Arc::new(AtomicBool::new(false));
        let cache = Arc::new(Mutex::new(FrameCache::new()));

        let mut workers = Vec::with_capacity(3);
        for idx in 0..3 {
            let srv = Arc::clone(&server);
            let flag = Arc::clone(&shutdown);
            let dir = serve_dir.clone();
            let c = Arc::clone(&cache);
            let handle = std::thread::Builder::new()
                .name(format!("rsmotion-http-{idx}"))
                .spawn(move || {
                    while !flag.load(Ordering::Relaxed) {
                        match srv.recv() {
                            Ok(req) => handle_request(req, dir.as_deref(), &c),
                            Err(_) => break,
                        }
                    }
                })?;
            workers.push(handle);
        }

        Ok(Self {
            port,
            serve_dir,
            server,
            shutdown,
            workers,
        })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn base_url(&self) -> String {
        format!("http://127.0.0.1:{}/index.html", self.port)
    }

    pub fn serve_dir(&self) -> Option<&Path> {
        self.serve_dir.as_deref()
    }
}

impl Drop for EmbeddedServer {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
        for _ in 0..self.workers.len() {
            self.server.unblock();
        }
        for w in self.workers.drain(..) {
            let _ = w.join();
        }
    }
}
