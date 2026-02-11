//! B-tree index for range and point lookups (OLTP).

use rustafari_core::{Result, RustafariError, Value};
use rustafari_storage::RowId;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::RwLock;

/// Key in the B-tree: column value(s) serialized for ordering.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct IndexKey(Vec<u8>);

impl IndexKey {
    pub fn from_value(v: &Value) -> Result<Self> {
        let bytes = match v {
            Value::Null => return Err(RustafariError::Index("null not indexable".into())),
            Value::Bool(b) => vec![if *b { 1 } else { 0 }],
            Value::Int64(i) => i.to_be_bytes().to_vec(),
            Value::Float64(f) => f.to_bits().to_be_bytes().to_vec(),
            Value::String(s) => s.as_bytes().to_vec(),
            Value::Timestamp(ts) => ts.timestamp_millis().to_be_bytes().to_vec(),
            Value::Date(d) => d.and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp_millis().to_be_bytes().to_vec(),
            _ => return Err(RustafariError::Index("type not indexable in btree".into())),
        };
        Ok(IndexKey(bytes))
    }
}

/// B-tree index: key -> list of row IDs (supporting duplicates).
#[derive(Debug, Default)]
pub struct BTreeIndex {
    /// Table and column name for identification.
    pub table_column: Option<(rustafari_core::TableId, String)>,
    map: RwLock<BTreeMap<IndexKey, Vec<RowId>>>,
}

impl BTreeIndex {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&self, key: IndexKey, row_id: RowId) -> Result<()> {
        self.map
            .write()
            .map_err(|_| RustafariError::Index("lock poisoned".into()))?
            .entry(key)
            .or_default()
            .push(row_id);
        Ok(())
    }

    pub fn delete(&self, key: &IndexKey, row_id: RowId) -> Result<bool> {
        let mut g = self.map
            .write()
            .map_err(|_| RustafariError::Index("lock poisoned".into()))?;
        if let Some(ids) = g.get_mut(key) {
            if let Some(pos) = ids.iter().position(|&r| r == row_id) {
                ids.remove(pos);
                if ids.is_empty() {
                    g.remove(key);
                }
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Point lookup: exact key.
    pub fn get(&self, key: &IndexKey) -> Result<Vec<RowId>> {
        Ok(self.map
            .read()
            .map_err(|_| RustafariError::Index("lock poisoned".into()))?
            .get(key)
            .cloned()
            .unwrap_or_default())
    }

    /// Range scan: keys in [start, end) (inclusive start, exclusive end if end given).
    pub fn range(&self, start: &IndexKey, end: Option<&IndexKey>) -> Result<Vec<RowId>> {
        let g = self.map
            .read()
            .map_err(|_| RustafariError::Index("lock poisoned".into()))?;
        let start_owned = start.clone();
        let end_owned = end.cloned().unwrap_or_else(|| IndexKey(vec![0xff; 256]));
        let range = start_owned..=end_owned;
        let mut out = Vec::new();
        for (_, ids) in g.range(range) {
            out.extend(ids.iter().copied());
        }
        Ok(out)
    }
}
