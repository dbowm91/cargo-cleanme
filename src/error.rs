use thiserror::Error;
#[derive(Debug, Error)]
pub enum AppError {
    #[error("configuration error: {0}")]
    Config(String),
    #[error("invalid scan root {path}: {reason}")]
    InvalidRoot { path: String, reason: String },
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("scanner failed: {0}")]
    Scan(String),
}
