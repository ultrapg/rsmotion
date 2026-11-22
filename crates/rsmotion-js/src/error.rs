use thiserror::Error;

#[derive(Debug, Error)]
pub enum JsHostError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("bundle parse error: {0}")]
    BundleParse(String),

    #[error("V8 execution error: {0}")]
    V8(String),

    #[error("render cancelled in JS: {0}")]
    RenderCancelled(String),

    #[error("render timed out after {timeout_ms}ms (pending delayRender: {handles})")]
    RenderTimeout { timeout_ms: u64, handles: String },

    #[error("composition `{0}` not found in bundle")]
    CompositionNotFound(String),
}

pub type Result<T> = std::result::Result<T, JsHostError>;
