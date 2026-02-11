//! MongoDB-style query filter DSL.

use rustafari_core::{Result, RustafariError, Value};
use std::collections::BTreeMap;
use serde_json::Map;
use serde_json::Value as JsonValue;

/// Top-level filter: AND of conditions (or single condition).
#[derive(Debug, Clone)]
pub struct DocFilter {
    pub conditions: Vec<FilterCondition>,
}

#[derive(Debug, Clone)]
pub enum FilterCondition {
    /// Field op value (e.g. age >= 21)
    Cmp {
        field: String,
        op: FilterOp,
        value: Value,
    },
    /// $and / $or of sub-filters
    And(Vec<DocFilter>),
    Or(Vec<DocFilter>),
    Not(Box<DocFilter>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterOp {
    Eq,
    Ne,
    Gt,
    Gte,
    Lt,
    Lte,
    In,
    Nin,
}

pub fn filter_from_json(obj: &Map<String, JsonValue>) -> Result<DocFilter> {
    let mut conditions = Vec::new();
    for (k, v) in obj {
        if k.starts_with('$') {
            match k.as_str() {
                "$and" => {
                    let arr = v.as_array().ok_or_else(|| RustafariError::Parse("$and must be array".into()))?;
                    let sub: Vec<DocFilter> = arr
                        .iter()
                        .map(|e| {
                            let o = e.as_object().ok_or_else(|| RustafariError::Parse("$and element must be object".into()))?;
                            filter_from_json(o)
                        })
                        .collect::<Result<_>>()?;
                    conditions.push(FilterCondition::And(sub));
                }
                "$or" => {
                    let arr = v.as_array().ok_or_else(|| RustafariError::Parse("$or must be array".into()))?;
                    let sub: Vec<DocFilter> = arr
                        .iter()
                        .map(|e| {
                            let o = e.as_object().ok_or_else(|| RustafariError::Parse("$or element must be object".into()))?;
                            filter_from_json(o)
                        })
                        .collect::<Result<_>>()?;
                    conditions.push(FilterCondition::Or(sub));
                }
                "$not" => {
                    let o = v.as_object().ok_or_else(|| RustafariError::Parse("$not must be object".into()))?;
                    conditions.push(FilterCondition::Not(Box::new(filter_from_json(o)?)));
                }
                _ => return Err(RustafariError::Parse(format!("unknown operator {}", k))),
            }
        } else {
            // field: value or field: { $op: value }
            let (op, value) = if let Some(sub) = v.as_object() {
                if let Some(inner) = sub.get("$eq") {
                    (FilterOp::Eq, json_to_value(inner)?)
                } else if let Some(inner) = sub.get("$ne") {
                    (FilterOp::Ne, json_to_value(inner)?)
                } else if let Some(inner) = sub.get("$gt") {
                    (FilterOp::Gt, json_to_value(inner)?)
                } else if let Some(inner) = sub.get("$gte") {
                    (FilterOp::Gte, json_to_value(inner)?)
                } else if let Some(inner) = sub.get("$lt") {
                    (FilterOp::Lt, json_to_value(inner)?)
                } else if let Some(inner) = sub.get("$lte") {
                    (FilterOp::Lte, json_to_value(inner)?)
                } else if let Some(inner) = sub.get("$in") {
                    (FilterOp::In, json_to_value(inner)?)
                } else if let Some(inner) = sub.get("$nin") {
                    (FilterOp::Nin, json_to_value(inner)?)
                } else {
                    return Err(RustafariError::Parse("unknown operator in field".into()));
                }
            } else {
                (FilterOp::Eq, json_to_value(v)?)
            };
            conditions.push(FilterCondition::Cmp {
                field: k.clone(),
                op,
                value,
            });
        }
    }
    Ok(DocFilter { conditions })
}

fn json_to_value(j: &JsonValue) -> Result<Value> {
    Ok(match j {
        JsonValue::Null => Value::Null,
        JsonValue::Bool(b) => Value::Bool(*b),
        JsonValue::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::Int64(i)
            } else {
                Value::Float64(n.as_f64().unwrap_or(0.0))
            }
        }
        JsonValue::String(s) => Value::String(s.clone()),
        JsonValue::Array(arr) => Value::Array(arr.iter().map(json_to_value).collect::<Result<Vec<_>>>()?),
        JsonValue::Object(obj) => {
            let map: BTreeMap<String, Value> = obj
                .iter()
                .map(|(k, v)| Ok((k.clone(), json_to_value(v)?)))
                .collect::<Result<_>>()?;
            Value::Document(map)
        }
    })
}

/// Evaluate filter against a row (given schema column names and row values by index).
pub fn matches_filter(filter: &DocFilter, get_field: &dyn Fn(&str) -> Option<Value>) -> bool {
    for c in &filter.conditions {
        match c {
            FilterCondition::Cmp { field, op, value } => {
                let Some(fv) = get_field(field) else { return false };
                if !cmp_match(*op, &fv, value) {
                    return false;
                }
            }
            FilterCondition::And(sub) => {
                for f in sub {
                    if !matches_filter(f, get_field) {
                        return false;
                    }
                }
            }
            FilterCondition::Or(sub) => {
                if !sub.iter().any(|f| matches_filter(f, get_field)) {
                    return false;
                }
            }
            FilterCondition::Not(sub) => {
                if matches_filter(sub, get_field) {
                    return false;
                }
            }
        }
    }
    true
}

fn cmp_match(op: FilterOp, field_val: &Value, cond: &Value) -> bool {
    use FilterOp::*;
    match op {
        Eq => field_val == cond,
        Ne => field_val != cond,
        Gt => compare(field_val, cond).map(|c| c == std::cmp::Ordering::Greater).unwrap_or(false),
        Gte => compare(field_val, cond).map(|c| c != std::cmp::Ordering::Less).unwrap_or(false),
        Lt => compare(field_val, cond).map(|c| c == std::cmp::Ordering::Less).unwrap_or(false),
        Lte => compare(field_val, cond).map(|c| c != std::cmp::Ordering::Greater).unwrap_or(false),
        In => {
            if let Value::Array(arr) = cond {
                arr.iter().any(|v| v == field_val)
            } else {
                false
            }
        }
        Nin => {
            if let Value::Array(arr) = cond {
                !arr.iter().any(|v| v == field_val)
            } else {
                true
            }
        }
    }
}

fn compare(a: &Value, b: &Value) -> Option<std::cmp::Ordering> {
    match (a, b) {
        (Value::Int64(x), Value::Int64(y)) => Some(x.cmp(y)),
        (Value::Float64(x), Value::Float64(y)) => x.partial_cmp(y),
        (Value::String(x), Value::String(y)) => Some(x.cmp(y)),
        _ => None,
    }
}
