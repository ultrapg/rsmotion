use thiserror::Error;

#[derive(Debug, Error)]
pub enum EncodeError {
    #[error("core validation error: {0}")]
    Core(#[from] rsmotion_core::RsmotionError),

    #[error("requested hardware encoder `{0}` is unavailable: {1}")]
    EncoderUnavailable(String, String),

    #[error("invalid frame {index}: expected {expected_bytes} bytes, got {actual_bytes}")]
    InvalidFrameBuffer {
        index: u32,
        expected_bytes: usize,
        actual_bytes: usize,
    },

    #[error("encoder worker channel disconnected unexpectedly")]
    ChannelClosed,

    #[error("ffmpeg exited with status {code}: {stderr}")]
    FfmpegFailed { code: i32, stderr: String },

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("image/png encoding error: {0}")]
    Image(String),
}

pub type Result<T> = std::result::Result<T, EncodeError>;
