//! Columnar storage for OLAP (column chunks, real-time analytics).

use rustafari_core::{Result, TableId, Value};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A chunk of one column (vector of values). Enables vectorized execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnChunk {
    pub column_name: String,
    pub values: Vec<Value>,
}

/// A columnar chunk is a set of column chunks with the same row count.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnarChunk {
    pub chunk_id: u64,
    pub columns: Vec<ColumnChunk>,
    pub row_count: usize,
}

impl ColumnarChunk {
    pub fn new(chunk_id: u64, columns: Vec<ColumnChunk>) -> Self {
        let row_count = columns.first().map(|c| c.values.len()).unwrap_or(0);
        Self {
            chunk_id,
            columns,
            row_count,
        }
    }

    pub fn column(&self, name: &str) -> Option<&[Value]> {
        self.columns
            .iter()
            .find(|c| c.column_name == name)
            .map(|c| c.values.as_slice())
    }
}

/// In-memory columnar store (table_id -> list of chunks). Real-time analytics over recent data.
#[derive(Default)]
pub struct ColumnarStore {
    next_chunk_id: std::sync::atomic::AtomicU64,
    chunks: parking_lot::RwLock<HashMap<rustafari_core::TableId, Vec<ColumnarChunk>>>,
}

impl ColumnarStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert_chunk(
        &self,
        table_id: TableId,
        chunk: ColumnarChunk,
    ) -> Result<()> {
        self.chunks
            .write()
            .entry(table_id)
            .or_default()
            .push(chunk);
        Ok(())
    }

    pub fn chunks(
        &self,
        table_id: TableId,
    ) -> Result<Vec<ColumnarChunk>> {
        Ok(self
            .chunks
            .read()
            .get(&table_id)
            .cloned()
            .unwrap_or_default())
    }

    pub fn next_chunk_id(&self) -> u64 {
        self.next_chunk_id
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
    }

    /// Remove all chunks for a table (e.g. when dropping the table).
    pub fn drop_table(&self, table_id: TableId) -> Result<()> {
        self.chunks.write().remove(&table_id);
        Ok(())
    }
}
