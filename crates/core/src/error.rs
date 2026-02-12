use thiserror::Error;

#[derive(Debug, Error)]
pub enum OtellError {
    #[error("invalid argument: {0}")]
    InvalidArgument(String),

    #[error("configuration error: {0}")]
    Config(String),

    #[error("parse error: {0}")]
    Parse(String),

    #[error("storage error: {0}")]
    Store(String),

    #[error("ingest error: {0}")]
    Ingest(String),

    #[error("io error: {0}")]
    Io(String),

    #[error("internal error: {0}")]
    Internal(String),
}

pub type Result<T> = std::result::Result<T, OtellError>;
