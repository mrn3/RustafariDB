//! In-memory row store (table_id -> rows). Production would use pages + B-tree.

use parking_lot::RwLock;
use rustafari_core::{Result, Row, TableId};
use crate::RowStore;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};

/// Stable row identifier within a table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RowId(pub u64);

/// In-memory table storage: map table_id -> (row_id -> row).
#[derive(Default)]
pub struct TableStore {
    next_row_id: AtomicU64,
    data: RwLock<BTreeMap<(TableId, RowId), Row>>,
}

impl TableStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&self, table_id: TableId, row: Row) -> Result<RowId> {
        let id = RowId(self.next_row_id.fetch_add(1, Ordering::SeqCst));
        self.data.write().insert((table_id, id), row);
        Ok(id)
    }

    pub fn get(&self, table_id: TableId, row_id: RowId) -> Result<Option<Row>> {
        Ok(self.data.read().get(&(table_id, row_id)).cloned())
    }

    pub fn delete(&self, table_id: TableId, row_id: RowId) -> Result<bool> {
        Ok(self.data.write().remove(&(table_id, row_id)).is_some())
    }

    pub fn update(&self, table_id: TableId, row_id: RowId, row: Row) -> Result<bool> {
        let mut g = self.data.write();
        if g.contains_key(&(table_id, row_id)) {
            g.insert((table_id, row_id), row);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub fn scan(
        &self,
        table_id: TableId,
    ) -> Result<Vec<(RowId, Row)>> {
        let guard = self.data.read();
        Ok(guard
            .range((table_id, RowId(0))..(TableId(table_id.0 + 1), RowId(0)))
            .map(|(&(_, rid), row)| (rid, row.clone()))
            .collect())
    }

    /// Remove all rows for a table (e.g. when dropping the table).
    pub fn drop_table(&self, table_id: TableId) -> Result<()> {
        let mut guard = self.data.write();
        let keys: Vec<_> = guard
            .range((table_id, RowId(0))..(TableId(table_id.0 + 1), RowId(0)))
            .map(|(k, _)| *k)
            .collect();
        for k in keys {
            guard.remove(&k);
        }
        Ok(())
    }
}

impl RowStore for TableStore {
    fn insert(&self, table_id: TableId, row: Row) -> Result<RowId> {
        TableStore::insert(self, table_id, row)
    }
    fn scan(&self, table_id: TableId) -> Result<Vec<(RowId, Row)>> {
        TableStore::scan(self, table_id)
    }
    fn get(&self, table_id: TableId, row_id: RowId) -> Result<Option<Row>> {
        TableStore::get(self, table_id, row_id)
    }
    fn delete(&self, table_id: TableId, row_id: RowId) -> Result<bool> {
        TableStore::delete(self, table_id, row_id)
    }
    fn update(&self, table_id: TableId, row_id: RowId, row: Row) -> Result<bool> {
        TableStore::update(self, table_id, row_id, row)
    }
}
