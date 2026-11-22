use thiserror::Error;

#[derive(Debug, Error)]
pub enum CdpError {
    #[error("no Chrome / Chromium executable found (searched explicit path, env, local .remotion, system paths)")]
    BrowserNotFound,

    #[error("failed to launch browser `{0}`: {1}")]
    BrowserLaunch(String, String),

    #[error("timed out waiting for Chrome DevTools WebSocket URL")]
    DevToolsTimeout,

    #[error("CDP WebSocket/protocol error: {0}")]
    Protocol(String),

    #[error("CDP command `{method}` timed out after {timeout_ms} ms")]
    CommandTimeout { method: String, timeout_ms: u64 },

    #[error("CDP command `{method}` returned error: {message}")]
    CdpRemote { method: String, message: String },

    #[error("Remotion render error in page: {0}")]
    RenderCancelled(String),

    #[error(
        "Remotion render timed out after {timeout_ms} ms (open delayRender handles: {handles})"
    )]
    RenderTimeout { timeout_ms: u64, handles: String },

    #[error("composition `{0}` not found in bundle")]
    CompositionNotFound(String),

    #[error("shared-memory frame capture error: {0}")]
    SharedMemory(String),

    #[error("HTTP/proxy server error: {0}")]
    Server(String),

    #[error("encode error: {0}")]
    Encode(#[from] rsmotion_encode::EncodeError),

    #[error("core error: {0}")]
    Core(#[from] rsmotion_core::RsmotionError),

    #[error("image decode error: {0}")]
    Image(#[from] image::ImageError),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, CdpError>;
