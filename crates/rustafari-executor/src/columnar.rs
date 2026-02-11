//! Columnar execution for OLAP (aggregations, vectorized scan over columnar chunks).

use arrow::array::{ArrayRef, Float64Builder, Int64Builder, StringBuilder};
use arrow::datatypes::{DataType, Field, Schema as ArrowSchema};
use arrow::record_batch::RecordBatch;
use rustafari_core::{Result, Value};
use rustafari_storage::{ColumnarChunk, ColumnChunk};
use std::sync::Arc;

/// Build Arrow RecordBatch from columnar chunk for analytics (e.g. SUM, AVG over columns).
pub fn chunk_to_record_batch(chunk: &ColumnarChunk) -> Result<RecordBatch> {
    let mut arrays: Vec<ArrayRef> = Vec::new();
    let mut fields: Vec<Field> = Vec::new();
    for col in &chunk.columns {
        let (arr, field) = column_to_arrow(col)?;
        arrays.push(arr);
        fields.push(field);
    }
    let schema = ArrowSchema::new(fields);
    RecordBatch::try_new(Arc::new(schema), arrays)
        .map_err(|e| rustafari_core::RustafariError::Execution(e.to_string()))
}

fn column_to_arrow(col: &ColumnChunk) -> Result<(ArrayRef, Field)> {
    let name = col.column_name.clone();
    if col.values.is_empty() {
        let field = Field::new(name, DataType::Int64, true);
        let arr: ArrayRef = Arc::new(arrow::array::Int64Array::from(vec![] as Vec<Option<i64>>));
        return Ok((arr, field));
    }
    let first = &col.values[0];
    let (arr, dtype) = match first {
        Value::Int64(_) => {
            let mut b = Int64Builder::new();
            for v in &col.values {
                match v {
                    Value::Int64(x) => b.append_value(*x),
                    Value::Null => b.append_null(),
                    _ => b.append_null(),
                }
            }
            (Arc::new(b.finish()) as ArrayRef, DataType::Int64)
        }
        Value::Float64(_) => {
            let mut b = Float64Builder::new();
            for v in &col.values {
                match v {
                    Value::Float64(x) => b.append_value(*x),
                    Value::Null => b.append_null(),
                    _ => b.append_null(),
                }
            }
            (Arc::new(b.finish()) as ArrayRef, DataType::Float64)
        }
        Value::String(_) => {
            let mut b = StringBuilder::new();
            for v in &col.values {
                match v {
                    Value::String(s) => b.append_value(s),
                    Value::Null => b.append_null(),
                    _ => b.append_null(),
                }
            }
            (Arc::new(b.finish()) as ArrayRef, DataType::Utf8)
        }
        _ => {
            let mut b = Int64Builder::new();
            for _ in &col.values {
                b.append_null();
            }
            (Arc::new(b.finish()) as ArrayRef, DataType::Int64)
        }
    };
    let field = Field::new(name, dtype, true);
    Ok((arr, field))
}
