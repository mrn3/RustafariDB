//! Row type (ordered list of values aligned with schema).

use crate::{Schema, Value};
use serde::{Deserialize, Serialize};

/// A row is a sequence of values aligned with a schema.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Row {
    pub values: Vec<Value>,
}

impl Row {
    pub fn new(values: Vec<Value>) -> Self {
        Self { values }
    }

    pub fn get(&self, index: usize) -> Option<&Value> {
        self.values.get(index)
    }

    pub fn get_by_name(&self, schema: &Schema, name: &str) -> Option<&Value> {
        schema.column_index(name).and_then(|i| self.get(i))
    }

    pub fn len(&self) -> usize {
        self.values.len()
    }

    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
}

impl From<Vec<Value>> for Row {
    fn from(values: Vec<Value>) -> Self {
        Self::new(values)
    }
}
