//! MongoDB-style update operations ($set, $unset, etc.).

use rustafari_core::Value;

#[derive(Debug, Clone)]
pub struct DocUpdate {
    pub ops: Vec<UpdateOp>,
}

#[derive(Debug, Clone)]
pub enum UpdateOp {
    Set { field: String, value: Value },
    Unset { field: String },
    Inc { field: String, delta: i64 },
}
