use thiserror::Error;

/// Errors raised while analyzing a Remotion bundle or selecting a render tier.
#[derive(Debug, Error)]
pub enum AnalyzeError {
    #[error("composition not found: {0}")]
    CompositionNotFound(String),

    #[error("invalid bundle at {0}")]
    BundleInvalid(String),

    #[error("Tier 1 JS host error: {0}")]
    JsHost(#[from] rsmotion_js::JsHostError),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, AnalyzeError>;
