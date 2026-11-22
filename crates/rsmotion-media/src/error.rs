use thiserror::Error;

/// Errors raised while probing, resolving, or decoding Tier 1 media streams.
#[derive(Debug, Error)]
pub enum MediaError {
    #[error("media source not found: {0}")]
    NotFound(String),

    #[error("ffprobe failed: {0}")]
    ProbeFailed(String),

    #[error("video decode failed: {0}")]
    DecodeFailed(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, MediaError>;
