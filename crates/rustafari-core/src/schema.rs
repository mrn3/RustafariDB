//! Schema and column types.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Column definition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Column {
    pub name: String,
    pub data_type: ColumnType,
    pub nullable: bool,
}

/// Supported column types (SQL + document-friendly).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ColumnType {
    /// 64-bit signed integer.
    BigInt,
    /// 64-bit float.
    Double,
    /// Variable-length string.
    Varchar(Option<u32>),
    /// UTF-8 text (unbounded for full-text).
    Text,
    /// Boolean.
    Boolean,
    /// Timestamp with timezone.
    Timestamp,
    /// Date only.
    Date,
    /// Binary blob.
    Blob,
    /// Nested document (JSON).
    Document,
    /// Array of values.
    Array(Box<ColumnType>),
    /// Vector for similarity search (dimension).
    Vector(usize),
}

impl Column {
    pub fn new(name: impl Into<String>, data_type: ColumnType, nullable: bool) -> Self {
        Self {
            name: name.into(),
            data_type,
            nullable,
        }
    }
}

/// Table/view schema.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Schema {
    pub columns: Vec<Column>,
    /// Column name -> index for fast lookup.
    #[serde(skip)]
    name_to_index: HashMap<String, usize>,
}

impl Schema {
    pub fn new(columns: Vec<Column>) -> Self {
        let name_to_index = columns
            .iter()
            .enumerate()
            .map(|(i, c)| (c.name.clone(), i))
            .collect();
        Self {
            columns,
            name_to_index,
        }
    }

    pub fn column_index(&self, name: &str) -> Option<usize> {
        self.name_to_index.get(name).copied()
    }

    pub fn column_by_name(&self, name: &str) -> Option<&Column> {
        self.column_index(name).map(|i| &self.columns[i])
    }

    /// Rebuild name_to_index from columns (e.g. after deserializing, since name_to_index is skipped).
    pub fn rebuild_name_to_index(&mut self) {
        self.name_to_index = self
            .columns
            .iter()
            .enumerate()
            .map(|(i, c)| (c.name.clone(), i))
            .collect();
    }
}

impl Default for Schema {
    fn default() -> Self {
        Self {
            columns: Vec::new(),
            name_to_index: HashMap::new(),
        }
    }
}
