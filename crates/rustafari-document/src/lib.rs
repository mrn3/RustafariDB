//! # RustafariDB Document
//!
//! MongoDB-style document API: collections, find/findOne, insert, update, delete,
//! and a query filter DSL compatible with MongoDB syntax.

pub mod filter;
pub mod update;
pub mod coll;

pub use filter::{DocFilter, FilterOp};
pub use update::{DocUpdate, UpdateOp};
pub use coll::DocumentCollection;

use rustafari_core::{Result, Row, Value};
use serde_json::Value as JsonValue;
use std::collections::BTreeMap;

/// Convert BTreeMap<String, Value> to/from JSON for MongoDB-style docs.
pub fn doc_to_row(doc: &BTreeMap<String, JsonValue>) -> Result<Row> {
    let values: Vec<Value> = doc
        .values()
        .map(json_to_value)
        .collect::<Result<Vec<_>>>()?;
    Ok(Row::new(values))
}

fn json_to_value(j: &JsonValue) -> Result<Value> {
    Ok(match j {
        JsonValue::Null => Value::Null,
        JsonValue::Bool(b) => Value::Bool(*b),
        JsonValue::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::Int64(i)
            } else if let Some(f) = n.as_f64() {
                Value::Float64(f)
            } else {
                Value::Float64(n.as_f64().unwrap_or(0.0))
            }
        }
        JsonValue::String(s) => Value::String(s.clone()),
        JsonValue::Array(arr) => {
            Value::Array(arr.iter().map(json_to_value).collect::<Result<Vec<_>>>()?)
        }
        JsonValue::Object(obj) => {
            let map: BTreeMap<String, Value> = obj
                .iter()
                .map(|(k, v)| Ok((k.clone(), json_to_value(v)?)))
                .collect::<Result<_>>()?;
            Value::Document(map)
        }
    })
}

/// MongoDB-style filter: parses from JSON like `{ "name": "alice", "age": { "$gte": 21 } }`.
pub fn parse_filter(json: &str) -> Result<DocFilter> {
    let v: JsonValue = serde_json::from_str(json).map_err(|e| rustafari_core::RustafariError::Parse(e.to_string()))?;
    let obj = v.as_object().ok_or_else(|| rustafari_core::RustafariError::Parse("filter must be object".into()))?;
    filter::filter_from_json(obj)
}
