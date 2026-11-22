use thiserror::Error;

#[derive(Debug, Error)]
pub enum LayoutError {
    #[error("taffy layout error: {0}")]
    Taffy(#[from] taffy::TaffyError),

    #[error("invalid scene tree root index {0}")]
    InvalidRoot(usize),
}

pub type Result<T> = std::result::Result<T, LayoutError>;
