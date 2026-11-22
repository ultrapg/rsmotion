use thiserror::Error;

/// Shared error type across `rsmotion` crates.
#[derive(Debug, Error)]
pub enum RsmotionError {
    #[error("invalid frame range `{0}`")]
    InvalidFrameRange(String),

    #[error("frame {frame} is out of bounds for duration {duration_in_frames}")]
    FrameOutOfBounds { frame: u32, duration_in_frames: u32 },

    #[error("invalid dimensions {width}x{height}: {reason}")]
    InvalidDimensions {
        width: u32,
        height: u32,
        reason: String,
    },

    #[error("invalid codec `{0}`")]
    InvalidCodec(String),

    #[error("invalid crf {crf} for codec {codec} (valid range {min}..={max})")]
    InvalidCrf {
        codec: String,
        crf: u8,
        min: u8,
        max: u8,
    },

    #[error("invalid every-nth-frame value `{0}` (must be >= 1)")]
    InvalidEveryNthFrame(u32),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, RsmotionError>;
