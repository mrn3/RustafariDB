//! Error types for RustafariDB.

use thiserror::Error;

/// Database errors.
#[derive(Error, Debug)]
pub enum RustafariError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Transaction conflict: {0}")]
    TransactionConflict(String),

    #[error("Transaction not found: {0}")]
    TransactionNotFound(String),

    #[error("Table not found: {0}")]
    TableNotFound(String),

    #[error("Namespace not found: {0}")]
    NamespaceNotFound(String),

    #[error("Schema error: {0}")]
    Schema(String),

    #[error("Parse error: {0}")]
    Parse(String),

    #[error("Execution error: {0}")]
    Execution(String),

    #[error("Storage error: {0}")]
    Storage(String),

    #[error("Index error: {0}")]
    Index(String),

    #[error("Lake error: {0}")]
    Lake(String),

    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, RustafariError>;
