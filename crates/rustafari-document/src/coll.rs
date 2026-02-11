//! Document collection abstraction (MongoDB-style API surface).

use crate::filter::{matches_filter, DocFilter};
use rustafari_core::{Result, Row, Schema, TableId};
use rustafari_storage::{RowId, TableStore};
use std::sync::Arc;

/// Document collection backed by a table (row store + schema).
pub struct DocumentCollection {
    pub table_id: TableId,
    pub schema: Schema,
    store: Arc<TableStore>,
}

impl DocumentCollection {
    pub fn new(table_id: TableId, schema: Schema, store: Arc<TableStore>) -> Self {
        Self {
            table_id,
            schema,
            store,
        }
    }

    /// Insert one document (row). Column order must match schema.
    pub fn insert_one(&self, row: Row) -> Result<RowId> {
        self.store.insert(self.table_id, row)
    }

    /// Find rows matching filter. Returns (row_id, row) for each match.
    pub fn find(&self, filter: Option<&DocFilter>) -> Result<Vec<(RowId, Row)>> {
        let rows = self.store.scan(self.table_id)?;
        let mut out = Vec::new();
        for (rid, row) in rows {
            if let Some(f) = filter {
                let get_field = |name: &str| {
                    self.schema.column_index(name).and_then(|i| row.get(i).cloned())
                };
                if !matches_filter(f, &get_field) {
                    continue;
                }
            }
            out.push((rid, row));
        }
        Ok(out)
    }

    /// Find one row matching filter.
    pub fn find_one(&self, filter: Option<&DocFilter>) -> Result<Option<(RowId, Row)>> {
        Ok(self.find(filter)?.into_iter().next())
    }

    /// Delete rows matching filter. Returns count deleted.
    pub fn delete_many(&self, filter: Option<&DocFilter>) -> Result<u64> {
        let to_delete = self.find(filter)?;
        let mut n = 0;
        for (rid, _) in to_delete {
            if self.store.delete(self.table_id, rid)? {
                n += 1;
            }
        }
        Ok(n)
    }
}
