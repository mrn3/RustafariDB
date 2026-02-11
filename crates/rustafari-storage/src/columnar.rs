//! Columnar storage for OLAP (column chunks, real-time analytics).
//! Supports fast SUM/COUNT/AVG over billions of rows via chunked column scans.

use rustafari_core::{Result, TableId, Value};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Default rows per columnar chunk. Tuned for cache-friendly scan and incremental flush.
pub const COLUMNAR_CHUNK_SIZE: usize = 100_000;

/// A chunk of one column (vector of values). Enables vectorized execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnChunk {
    pub column_name: String,
    pub values: Vec<Value>,
}

impl ColumnChunk {
    /// Vectorized sum for Int64 column (skips nulls). Used for fast OLAP SUM(col).
    pub fn sum_i64(&self) -> Option<i64> {
        Self::sum_i64_slice(&self.values)
    }

    /// Vectorized sum for Float64 column (skips nulls).
    pub fn sum_f64(&self) -> Option<f64> {
        Self::sum_f64_slice(&self.values)
    }

    /// Count non-null values in this column.
    pub fn count_non_null(&self) -> usize {
        Self::count_non_null_slice(&self.values)
    }

    /// Min/Max for Int64 (skips nulls).
    pub fn min_max_i64(&self) -> Option<(i64, i64)> {
        Self::min_max_i64_slice(&self.values)
    }

    /// Min/Max for Float64 (skips nulls).
    pub fn min_max_f64(&self) -> Option<(f64, f64)> {
        Self::min_max_f64_slice(&self.values)
    }

    /// Sum Int64 over a slice (for use when scanning by column name).
    pub fn sum_i64_slice(values: &[Value]) -> Option<i64> {
        let mut sum = 0i64;
        let mut any = false;
        for v in values {
            if let Value::Int64(x) = v {
                sum = sum.saturating_add(*x);
                any = true;
            }
        }
        if any { Some(sum) } else { None }
    }

    pub fn sum_f64_slice(values: &[Value]) -> Option<f64> {
        let mut sum = 0.0;
        let mut any = false;
        for v in values {
            if let Value::Float64(x) = v {
                sum += x;
                any = true;
            }
        }
        if any { Some(sum) } else { None }
    }

    pub fn count_non_null_slice(values: &[Value]) -> usize {
        values.iter().filter(|v| !matches!(v, Value::Null)).count()
    }

    pub fn min_max_i64_slice(values: &[Value]) -> Option<(i64, i64)> {
        let mut min = i64::MAX;
        let mut max = i64::MIN;
        let mut any = false;
        for v in values {
            if let Value::Int64(x) = v {
                min = min.min(*x);
                max = max.max(*x);
                any = true;
            }
        }
        if any { Some((min, max)) } else { None }
    }

    pub fn min_max_f64_slice(values: &[Value]) -> Option<(f64, f64)> {
        let mut min = f64::INFINITY;
        let mut max = f64::NEG_INFINITY;
        let mut any = false;
        for v in values {
            if let Value::Float64(x) = v {
                min = min.min(*x);
                max = max.max(*x);
                any = true;
            }
        }
        if any { Some((min, max)) } else { None }
    }
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
