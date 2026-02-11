//! # RustafariDB Storage
//!
//! Page-based storage, WAL, and buffer pool for OLTP and columnar chunks for OLAP.

pub mod page;
pub mod wal;
pub mod table_store;
pub mod columnar;

pub use page::{PageId, Page, PAGE_SIZE};
pub use wal::{WalRecord, WalWriter};
pub use table_store::{TableStore, RowId};
pub use columnar::{ColumnChunk, ColumnarChunk, ColumnarStore};

use rustafari_core::{Row, TableId};

/// Trait for row-oriented table storage (OLTP).
pub trait RowStore: Send + Sync {
    fn insert(&self, table_id: TableId, row: Row) -> rustafari_core::Result<RowId>;
    fn scan(&self, table_id: TableId) -> rustafari_core::Result<Vec<(RowId, Row)>>;
    fn get(&self, table_id: TableId, row_id: RowId) -> rustafari_core::Result<Option<Row>>;
    fn delete(&self, table_id: TableId, row_id: RowId) -> rustafari_core::Result<bool>;
    fn update(&self, table_id: TableId, row_id: RowId, row: Row) -> rustafari_core::Result<bool>;
}
