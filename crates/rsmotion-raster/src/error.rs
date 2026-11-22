use thiserror::Error;

#[derive(Debug, Error)]
pub enum RasterError {
    #[error("encode error: {0}")]
    Encode(#[from] rsmotion_encode::EncodeError),

    #[error("image error: {0}")]
    Image(#[from] image::ImageError),

    #[error("dimension mismatch: golden is {gw}x{gh}, actual is {aw}x{ah}")]
    DimensionMismatch { gw: u32, gh: u32, aw: u32, ah: u32 },

    #[error("invalid pixel buffer length: expected {expected}, got {actual}")]
    InvalidBufferLength { expected: usize, actual: usize },

    #[error("GPU EGL initialization failed: {0}")]
    GpuUnavailable(String),

    #[error("Skia surface/context creation failed: {0}")]
    SkiaFailed(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, RasterError>;
