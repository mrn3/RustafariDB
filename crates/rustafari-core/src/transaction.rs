//! Transaction and isolation for OLTP.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Unique transaction identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
pub struct TransactionId(pub u64);

impl TransactionId {
    pub const fn none() -> Self {
        TransactionId(0)
    }
}

/// Isolation level (PostgreSQL-style).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IsolationLevel {
    ReadUncommitted,
    ReadCommitted,
    RepeatableRead,
    Serializable,
}

impl Default for IsolationLevel {
    fn default() -> Self {
        IsolationLevel::ReadCommitted
    }
}

/// State of a transaction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransactionState {
    Active,
    Committed,
    Aborted,
}

/// Transaction context (snapshot for MVCC).
#[derive(Debug, Clone)]
pub struct TransactionContext {
    pub id: TransactionId,
    pub snapshot_ts: DateTime<Utc>,
    pub isolation: IsolationLevel,
    pub state: TransactionState,
}

impl TransactionContext {
    pub fn new(id: TransactionId, isolation: IsolationLevel) -> Self {
        Self {
            id,
            snapshot_ts: Utc::now(),
            isolation,
            state: TransactionState::Active,
        }
    }
}
