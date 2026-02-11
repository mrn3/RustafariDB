//! # RustafariDB Core
//!
//! Core types, storage abstractions, and transaction management for a unified
//! OLTP, OLAP, and search database supporting SQL and MongoDB-style APIs.

pub mod error;
pub mod schema;
pub mod value;
pub mod row;
pub mod transaction;
pub mod catalog;

pub use error::{Result, RustafariError};
pub use schema::{Schema, Column, ColumnType};
pub use value::Value;
pub use row::Row;
pub use transaction::{TransactionId, IsolationLevel, TransactionState};
pub use catalog::{CatalogSnapshot, TableId, NamespaceId, Catalog, TableMeta};
pub use bytes::Bytes;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Database-wide result type.
pub type DbResult<T> = std::result::Result<T, error::RustafariError>;

/// Unique identifier for a database instance/session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(pub Uuid);

impl Default for SessionId {
    fn default() -> Self {
        SessionId(Uuid::new_v4())
    }
}
