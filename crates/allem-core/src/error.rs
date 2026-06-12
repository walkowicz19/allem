//! Library error type. Per project best practice (klayer `rust-dev`): libraries use
//! enum-based errors via `thiserror`; binaries wrap these with `anyhow`. No `unwrap()`.

use std::path::PathBuf;

/// Errors produced by the Allem engine.
#[derive(Debug, thiserror::Error)]
pub enum AllemError {
    #[error("i/o error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to parse {kind} manifest at {path}: {reason}")]
    ManifestParse {
        kind: String,
        path: PathBuf,
        reason: String,
    },

    #[error("no adapter registered for {what}")]
    NoAdapter { what: String },

    #[error("serialization error: {0}")]
    Serialize(#[from] serde_json::Error),

    #[error("configuration error: {0}")]
    Config(String),
}

/// Convenience result alias for the engine.
pub type Result<T> = std::result::Result<T, AllemError>;
