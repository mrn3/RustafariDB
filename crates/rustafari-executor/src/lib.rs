//! # RustafariDB Executor
//!
//! Executes logical plans: SQL (scan, filter, project, limit, insert, create table)
//! and document operations. Columnar execution for analytics can be extended here.

pub mod sql_executor;
pub mod columnar;

pub use sql_executor::SqlExecutor;

use rustafari_core::{Catalog, ColumnType, Result, Row, Schema, TableMeta, Value};
use rustafari_index::{BTreeIndex, IndexKey};
use rustafari_sql::{AggExpr, CmpOp, FilterExpr, LogicalPlan, PlanNode};
use rustafari_storage::{ColumnarChunk, ColumnChunk, ColumnarStore, RowId, TableStore, COLUMNAR_CHUNK_SIZE};
use std::collections::HashMap;
use std::sync::Arc;

/// Index lookup derived from a predicate on an indexed column (e.g. id).
#[derive(Debug)]
enum IdLookup {
    Point(Value),
    RangeInclusive(Value, Value),
}

/// Database session state: catalog + row store + columnar store + indexes.
pub struct SessionState {
    pub catalog: Arc<parking_lot::RwLock<Catalog>>,
    pub row_store: Arc<TableStore>,
    pub columnar_store: Arc<ColumnarStore>,
    /// (table_id, column_name) -> B-tree index (e.g. primary key on id).
    index_store: Arc<parking_lot::RwLock<HashMap<(rustafari_core::TableId, String), Arc<BTreeIndex>>>>,
    /// Rows buffered for columnar flush (table_id -> rows). Flushed when len >= COLUMNAR_CHUNK_SIZE.
    columnar_buffer: Arc<parking_lot::RwLock<HashMap<rustafari_core::TableId, Vec<Row>>>>,
}

impl SessionState {
    pub fn new() -> Self {
        Self {
            catalog: Arc::new(parking_lot::RwLock::new(Catalog::new())),
            row_store: Arc::new(TableStore::new()),
            columnar_store: Arc::new(ColumnarStore::new()),
            index_store: Arc::new(parking_lot::RwLock::new(HashMap::new())),
            columnar_buffer: Arc::new(parking_lot::RwLock::new(HashMap::new())),
        }
    }

    /// Flush buffered rows for a table into a columnar chunk (if any). Call before columnar reads.
    fn flush_columnar_buffer(&self, table_id: rustafari_core::TableId, schema: &Schema) -> Result<()> {
        let mut buf = self.columnar_buffer.write();
        let rows = match buf.get_mut(&table_id) {
            Some(r) if r.len() >= 1 => std::mem::take(r),
            _ => return Ok(()),
        };
        drop(buf);
        if rows.is_empty() {
            return Ok(());
        }
        let columns: Vec<ColumnChunk> = schema
            .columns
            .iter()
            .map(|col| {
                let idx = schema.column_index(&col.name).unwrap_or(0);
                let values: Vec<Value> = rows
                    .iter()
                    .map(|r| r.get(idx).cloned().unwrap_or(Value::Null))
                    .collect();
                ColumnChunk {
                    column_name: col.name.clone(),
                    values,
                }
            })
            .collect();
        let chunk = ColumnarChunk::new(self.columnar_store.next_chunk_id(), columns);
        self.columnar_store.insert_chunk(table_id, chunk)?;
        Ok(())
    }

    /// Push a row to the columnar buffer and flush if we reached COLUMNAR_CHUNK_SIZE.
    fn push_columnar_row(&self, table_id: rustafari_core::TableId, row: Row, schema: &Schema) -> Result<()> {
        let mut buf = self.columnar_buffer.write();
        let entry = buf.entry(table_id).or_default();
        entry.push(row);
        if entry.len() < COLUMNAR_CHUNK_SIZE {
            return Ok(());
        }
        let rows = std::mem::take(entry);
        drop(buf);
        let columns: Vec<ColumnChunk> = schema
            .columns
            .iter()
            .map(|col| {
                let idx = schema.column_index(&col.name).unwrap_or(0);
                let values: Vec<Value> = rows
                    .iter()
                    .map(|r| r.get(idx).cloned().unwrap_or(Value::Null))
                    .collect();
                ColumnChunk {
                    column_name: col.name.clone(),
                    values,
                }
            })
            .collect();
        let chunk = ColumnarChunk::new(self.columnar_store.next_chunk_id(), columns);
        self.columnar_store.insert_chunk(table_id, chunk)?;
        Ok(())
    }

    fn get_index(&self, table_id: rustafari_core::TableId, column: &str) -> Option<Arc<BTreeIndex>> {
        self.index_store
            .read()
            .get(&(table_id, column.to_string()))
            .cloned()
    }

    pub fn table_meta(&self, namespace: Option<&str>, table: &str) -> Result<Option<TableMeta>> {
        let ns = namespace.unwrap_or("public");
        let catalog = self.catalog.read();
        Ok(catalog.get_table(ns, table).cloned())
    }

    /// List all namespace (database) names. For use with SHOW DATABASES.
    pub fn list_databases(&self) -> Vec<String> {
        self.catalog.read().list_namespaces()
    }

    /// List table names in a namespace. Default namespace is "public". For use with SHOW TABLES.
    pub fn list_tables(&self, namespace: Option<&str>) -> Vec<String> {
        let ns = namespace.unwrap_or("public");
        let catalog = self.catalog.read();
        let mut names: Vec<String> = catalog.list_tables(ns).into_iter().map(|m| m.name.clone()).collect();
        names.sort();
        names
    }
}

impl Default for SessionState {
    fn default() -> Self {
        Self::new()
    }
}

/// Execute a logical plan and return result rows (for SELECT) or row count (for INSERT/CREATE).
pub fn execute_plan(state: &SessionState, plan: &LogicalPlan) -> Result<ExecutionResult> {
    match &plan.root {
        PlanNode::Scan { table, namespace } => {
            let meta = state.table_meta(namespace.as_deref(), table)?
                .ok_or_else(|| rustafari_core::RustafariError::TableNotFound(format!("{}.{}", namespace.as_deref().unwrap_or("public"), table)))?;
            let rows = state.row_store.scan(meta.id)?;
            Ok(ExecutionResult::Rows(
                rows.into_iter().map(|(_, r)| r).collect(),
            ))
        }
        PlanNode::Filter { input, predicate } => {
            let meta = scan_meta(state, input)?;
            // Use index when predicate is id = v or id >= v1 AND id <= v2 and we have an index on id.
            let rows = if let (PlanNode::Scan { table: _, namespace: _ }, Some(lookup)) = (
                input.as_ref(),
                predicate_to_id_lookup(predicate, "id"),
            ) {
                if let Some(index) = state.get_index(meta.id, "id") {
                    let row_ids: Vec<RowId> = match lookup {
                        IdLookup::Point(v) => {
                            let key = IndexKey::from_value(&v)?;
                            index.get(&key)?
                        }
                        IdLookup::RangeInclusive(v1, v2) => {
                            let start = IndexKey::from_value(&v1)?;
                            let end = IndexKey::from_value(&v2)?;
                            index.range(&start, Some(&end))?
                        }
                    };
                    let mut out = Vec::with_capacity(row_ids.len());
                    for rid in row_ids {
                        if let Some(row) = state.row_store.get(meta.id, rid)? {
                            out.push(row);
                        }
                    }
                    out
                } else {
                    let sub = execute_plan(state, &LogicalPlan { root: *input.clone() })?;
                    match sub {
                        ExecutionResult::Rows(r) => r
                            .into_iter()
                            .filter(|row| eval_predicate(row, predicate, &meta.schema))
                            .collect(),
                        _ => return Err(rustafari_core::RustafariError::Execution("filter expects rows".into())),
                    }
                }
            } else {
                let sub = execute_plan(state, &LogicalPlan { root: *input.clone() })?;
                let rows = match sub {
                    ExecutionResult::Rows(r) => r,
                    _ => return Err(rustafari_core::RustafariError::Execution("filter expects rows".into())),
                };
                rows.into_iter()
                    .filter(|row| eval_predicate(row, predicate, &meta.schema))
                    .collect()
            };
            Ok(ExecutionResult::Rows(rows))
        }
        PlanNode::Project { input, columns } => {
            let sub = execute_plan(state, &LogicalPlan { root: *input.clone() })?;
            let rows = match sub {
                ExecutionResult::Rows(r) => r,
                _ => return Err(rustafari_core::RustafariError::Execution("project expects rows".into())),
            };
            let meta = scan_meta(state, input)?;
            let projected: Vec<Row> = rows
                .into_iter()
                .map(|row| {
                    let values: Vec<Value> = columns
                        .iter()
                        .filter_map(|c| meta.schema.column_index(c).and_then(|i| row.get(i).cloned()))
                        .collect();
                    Row::new(values)
                })
                .collect();
            Ok(ExecutionResult::Rows(projected))
        }
        PlanNode::Limit { input, limit, offset } => {
            let sub = execute_plan(state, &LogicalPlan { root: *input.clone() })?;
            let rows = match sub {
                ExecutionResult::Rows(r) => r,
                _ => return Err(rustafari_core::RustafariError::Execution("limit expects rows".into())),
            };
            let skip = (*offset as usize).min(rows.len());
            let take = (*limit as usize).min(rows.len().saturating_sub(skip));
            let limited: Vec<Row> = rows.into_iter().skip(skip).take(take).collect();
            Ok(ExecutionResult::Rows(limited))
        }
        PlanNode::Aggregate { input, aggs } => {
            let meta = scan_meta(state, input)?;
            state.flush_columnar_buffer(meta.id, &meta.schema)?;
            let chunks = state.columnar_store.chunks(meta.id)?;
            let schema = &meta.schema;
            let mut result_values: Vec<Value> = Vec::with_capacity(aggs.len());
            for agg in aggs {
                let v = if chunks.is_empty() {
                    aggregate_over_rows(state, &meta, input, agg)?
                } else {
                    aggregate_over_chunks(&chunks, schema, agg)?
                };
                result_values.push(v);
            }
            Ok(ExecutionResult::Rows(vec![Row::new(result_values)]))
        }
        PlanNode::Insert { table, namespace, columns: _, values } => {
            let ns = namespace.as_deref().unwrap_or("public");
            let meta = state.table_meta(Some(ns), table)?
                .ok_or_else(|| rustafari_core::RustafariError::TableNotFound(format!("{}.{}", ns, table)))?;
            let id_col = meta.schema.column_index("id");
            let mut count = 0u64;
            for row_vals in values {
                let row = Row::new(row_vals.clone());
                let row_id = state.row_store.insert(meta.id, row.clone())?;
                if let (Some(id_idx), Some(index)) = (id_col, state.get_index(meta.id, "id")) {
                    if let Some(id_val) = row_vals.get(id_idx) {
                        let _ = index.insert(IndexKey::from_value(id_val)?, row_id);
                    }
                }
                state.push_columnar_row(meta.id, row, &meta.schema)?;
                count += 1;
            }
            Ok(ExecutionResult::RowsAffected(count))
        }
        PlanNode::CreateTable { table, namespace, columns } => {
            let ns = namespace.as_deref().unwrap_or("public");
            let schema = Schema::new(
                columns
                    .iter()
                    .map(|(name, ty)| rustafari_core::Column::new(name.clone(), ty.clone(), true))
                    .collect(),
            );
            let meta = state.catalog.write().create_table(ns, table.clone(), schema, None);
            if meta.schema.column_index("id").is_some() {
                state
                    .index_store
                    .write()
                    .insert((meta.id, "id".to_string()), Arc::new(BTreeIndex::new()));
            }
            Ok(ExecutionResult::RowsAffected(0))
        }
        PlanNode::DropTable { table, namespace } => {
            let ns = namespace.as_deref().unwrap_or("public");
            let meta = state.table_meta(Some(ns), table)?
                .ok_or_else(|| rustafari_core::RustafariError::TableNotFound(format!("{}.{}", ns, table)))?
                .clone();
            state.row_store.drop_table(meta.id)?;
            state.columnar_store.drop_table(meta.id)?;
            state.index_store.write().retain(|(tid, _), _| *tid != meta.id);
            state.catalog.write().drop_table(ns, table);
            Ok(ExecutionResult::RowsAffected(0))
        }
        PlanNode::DescribeTable { table, namespace } => {
            let ns = namespace.as_deref().unwrap_or("public");
            let meta = state.table_meta(Some(ns), table)?
                .ok_or_else(|| rustafari_core::RustafariError::TableNotFound(format!("{}.{}", ns, table)))?;
            let rows: Vec<Row> = meta
                .schema
                .columns
                .iter()
                .map(|col| {
                    Row::new(vec![
                        Value::String(col.name.clone()),
                        Value::String(column_type_name(&col.data_type)),
                        Value::String(if col.nullable { "YES" } else { "NO" }.to_string()),
                    ])
                })
                .collect();
            Ok(ExecutionResult::Rows(rows))
        }
    }
}

/// If the predicate is a point or range lookup on the given column, return it for index use.
fn predicate_to_id_lookup(pred: &FilterExpr, column: &str) -> Option<IdLookup> {
    match pred {
        FilterExpr::Cmp { col, op: CmpOp::Eq, value } if col == column => Some(IdLookup::Point(value.clone())),
        FilterExpr::And(a, b) => {
            let range = match (a.as_ref(), b.as_ref()) {
                (FilterExpr::Cmp { col: c1, op: CmpOp::Ge, value: v1 }, FilterExpr::Cmp { col: c2, op: CmpOp::Le, value: v2 })
                    if c1 == column && c2 == column => Some((v1.clone(), v2.clone())),
                (FilterExpr::Cmp { col: c1, op: CmpOp::Le, value: v1 }, FilterExpr::Cmp { col: c2, op: CmpOp::Ge, value: v2 })
                    if c1 == column && c2 == column => Some((v2.clone(), v1.clone())),
                _ => None,
            };
            range.map(|(v1, v2)| IdLookup::RangeInclusive(v1, v2))
        }
        _ => None,
    }
}

fn scan_meta(state: &SessionState, node: &PlanNode) -> Result<TableMeta> {
    let (table, namespace) = match node {
        PlanNode::Scan { table, namespace } => (table.clone(), namespace.clone()),
        PlanNode::Filter { input, .. }
        | PlanNode::Project { input, .. }
        | PlanNode::Limit { input, .. }
        | PlanNode::Aggregate { input, .. } => return scan_meta(state, input),
        _ => return Err(rustafari_core::RustafariError::Execution("expected scan under filter/project/aggregate".into())),
    };
    let ns = namespace.as_deref().unwrap_or("public");
    state
        .table_meta(Some(ns), &table)?
        .ok_or_else(|| rustafari_core::RustafariError::TableNotFound(format!("{}.{}", ns, table)))
}

fn eval_predicate(row: &Row, pred: &FilterExpr, schema: &Schema) -> bool {
    match pred {
        FilterExpr::Literal(b) => *b,
        FilterExpr::Cmp { col, op, value } => {
            let Some(v) = schema.column_index(col).and_then(|i| row.get(i)) else { return false };
            match op {
                CmpOp::Eq => v == value,
                CmpOp::Ne => v != value,
                CmpOp::Gt => compare(v, value).map(|c| c == std::cmp::Ordering::Greater).unwrap_or(false),
                CmpOp::Ge => compare(v, value).map(|c| c != std::cmp::Ordering::Less).unwrap_or(false),
                CmpOp::Lt => compare(v, value).map(|c| c == std::cmp::Ordering::Less).unwrap_or(false),
                CmpOp::Le => compare(v, value).map(|c| c != std::cmp::Ordering::Greater).unwrap_or(false),
            }
        }
        FilterExpr::And(a, b) => eval_predicate(row, a, schema) && eval_predicate(row, b, schema),
        FilterExpr::Or(a, b) => eval_predicate(row, a, schema) || eval_predicate(row, b, schema),
        FilterExpr::Not(n) => !eval_predicate(row, n, schema),
    }
}

fn column_type_name(ty: &ColumnType) -> String {
    match ty {
        ColumnType::BigInt => "bigint".into(),
        ColumnType::Double => "double".into(),
        ColumnType::Varchar(None) => "varchar".into(),
        ColumnType::Varchar(Some(n)) => format!("varchar({})", n),
        ColumnType::Text => "text".into(),
        ColumnType::Boolean => "boolean".into(),
        ColumnType::Timestamp => "timestamp".into(),
        ColumnType::Date => "date".into(),
        ColumnType::Blob => "blob".into(),
        ColumnType::Document => "document".into(),
        ColumnType::Array(inner) => format!("array({})", column_type_name(inner)),
        ColumnType::Vector(dim) => format!("vector({})", dim),
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

/// Compute one aggregate over columnar chunks (vectorized path).
fn aggregate_over_chunks(
    chunks: &[ColumnarChunk],
    _schema: &Schema,
    agg: &AggExpr,
) -> Result<Value> {
    use rustafari_storage::ColumnChunk;
    match agg {
        AggExpr::CountStar => {
            let n: usize = chunks.iter().map(|c| c.row_count).sum();
            Ok(Value::Int64(n as i64))
        }
        AggExpr::Sum { column } => {
            let mut sum_i = 0i64;
            let mut sum_f = 0.0f64;
            let mut has_i = false;
            let mut has_f = false;
            for ch in chunks {
                if let Some(slice) = ch.column(column) {
                    if let Some(s) = ColumnChunk::sum_i64_slice(slice) {
                        sum_i = sum_i.saturating_add(s);
                        has_i = true;
                    }
                    if let Some(s) = ColumnChunk::sum_f64_slice(slice) {
                        sum_f += s;
                        has_f = true;
                    }
                }
            }
            if has_f {
                Ok(Value::Float64(sum_f))
            } else if has_i {
                Ok(Value::Int64(sum_i))
            } else {
                Ok(Value::Null)
            }
        }
        AggExpr::Count { column } => {
            let mut n = 0usize;
            for ch in chunks {
                if let Some(slice) = ch.column(column) {
                    n += ColumnChunk::count_non_null_slice(slice);
                }
            }
            Ok(Value::Int64(n as i64))
        }
        AggExpr::Avg { column } => {
            let mut sum_i = 0i64;
            let mut sum_f = 0.0f64;
            let mut count = 0usize;
            for ch in chunks {
                if let Some(slice) = ch.column(column) {
                    if let Some(s) = ColumnChunk::sum_i64_slice(slice) {
                        sum_i = sum_i.saturating_add(s);
                    }
                    if let Some(s) = ColumnChunk::sum_f64_slice(slice) {
                        sum_f += s;
                    }
                    count += ColumnChunk::count_non_null_slice(slice);
                }
            }
            if count == 0 {
                Ok(Value::Null)
            } else {
                Ok(Value::Float64((sum_i as f64 + sum_f) / count as f64))
            }
        }
        AggExpr::Min { column } => {
            let mut min_i = i64::MAX;
            let mut min_f = f64::INFINITY;
            let mut has_i = false;
            let mut has_f = false;
            for ch in chunks {
                if let Some(slice) = ch.column(column) {
                    if let Some((mi, _)) = ColumnChunk::min_max_i64_slice(slice) {
                        min_i = min_i.min(mi);
                        has_i = true;
                    }
                    if let Some((mi, _)) = ColumnChunk::min_max_f64_slice(slice) {
                        min_f = min_f.min(mi);
                        has_f = true;
                    }
                }
            }
            if has_f {
                Ok(Value::Float64(min_f))
            } else if has_i && min_i != i64::MAX {
                Ok(Value::Int64(min_i))
            } else {
                Ok(Value::Null)
            }
        }
        AggExpr::Max { column } => {
            let mut max_i = i64::MIN;
            let mut max_f = f64::NEG_INFINITY;
            let mut has_i = false;
            let mut has_f = false;
            for ch in chunks {
                if let Some(slice) = ch.column(column) {
                    if let Some((_, ma)) = ColumnChunk::min_max_i64_slice(slice) {
                        max_i = max_i.max(ma);
                        has_i = true;
                    }
                    if let Some((_, ma)) = ColumnChunk::min_max_f64_slice(slice) {
                        max_f = max_f.max(ma);
                        has_f = true;
                    }
                }
            }
            if has_f {
                Ok(Value::Float64(max_f))
            } else if has_i && max_i != i64::MIN {
                Ok(Value::Int64(max_i))
            } else {
                Ok(Value::Null)
            }
        }
    }
}

fn aggregate_over_rows(
    state: &SessionState,
    meta: &TableMeta,
    input: &PlanNode,
    agg: &AggExpr,
) -> Result<Value> {
    let sub = execute_plan(state, &LogicalPlan { root: input.clone() })?;
    let rows = match sub {
        ExecutionResult::Rows(r) => r,
        _ => return Err(rustafari_core::RustafariError::Execution("aggregate input must yield rows".into())),
    };
    let schema = &meta.schema;
    match agg {
        AggExpr::CountStar => Ok(Value::Int64(rows.len() as i64)),
        AggExpr::Sum { column } => {
            let idx = schema.column_index(column).ok_or_else(|| {
                rustafari_core::RustafariError::Execution(format!("column not found: {}", column))
            })?;
            let mut sum_i = 0i64;
            let mut sum_f = 0.0f64;
            let mut has_i = false;
            let mut has_f = false;
            for r in &rows {
                if let Some(v) = r.get(idx) {
                    match v {
                        Value::Int64(x) => {
                            sum_i = sum_i.saturating_add(*x);
                            has_i = true;
                        }
                        Value::Float64(x) => {
                            sum_f += x;
                            has_f = true;
                        }
                        _ => {}
                    }
                }
            }
            if has_f {
                Ok(Value::Float64(sum_f + sum_i as f64))
            } else if has_i {
                Ok(Value::Int64(sum_i))
            } else {
                Ok(Value::Null)
            }
        }
        AggExpr::Count { column } => {
            let idx = schema.column_index(column).ok_or_else(|| {
                rustafari_core::RustafariError::Execution(format!("column not found: {}", column))
            })?;
            let n = rows.iter().filter(|r| r.get(idx).map(|v| !v.is_null()).unwrap_or(false)).count();
            Ok(Value::Int64(n as i64))
        }
        AggExpr::Avg { column } => {
            let idx = schema.column_index(column).ok_or_else(|| {
                rustafari_core::RustafariError::Execution(format!("column not found: {}", column))
            })?;
            let mut sum = 0.0f64;
            let mut count = 0usize;
            for r in &rows {
                if let Some(v) = r.get(idx) {
                    match v {
                        Value::Int64(x) => {
                            sum += *x as f64;
                            count += 1;
                        }
                        Value::Float64(x) => {
                            sum += *x;
                            count += 1;
                        }
                        _ => {}
                    }
                }
            }
            if count == 0 {
                Ok(Value::Null)
            } else {
                Ok(Value::Float64(sum / count as f64))
            }
        }
        AggExpr::Min { column } => {
            let idx = schema.column_index(column).ok_or_else(|| {
                rustafari_core::RustafariError::Execution(format!("column not found: {}", column))
            })?;
            let mut min_i = i64::MAX;
            let mut min_f = f64::INFINITY;
            let mut has_i = false;
            let mut has_f = false;
            for r in &rows {
                if let Some(v) = r.get(idx) {
                    match v {
                        Value::Int64(x) => {
                            min_i = min_i.min(*x);
                            has_i = true;
                        }
                        Value::Float64(x) => {
                            min_f = min_f.min(*x);
                            has_f = true;
                        }
                        _ => {}
                    }
                }
            }
            if has_f {
                Ok(Value::Float64(min_f))
            } else if has_i {
                Ok(Value::Int64(min_i))
            } else {
                Ok(Value::Null)
            }
        }
        AggExpr::Max { column } => {
            let idx = schema.column_index(column).ok_or_else(|| {
                rustafari_core::RustafariError::Execution(format!("column not found: {}", column))
            })?;
            let mut max_i = i64::MIN;
            let mut max_f = f64::NEG_INFINITY;
            let mut has_i = false;
            let mut has_f = false;
            for r in &rows {
                if let Some(v) = r.get(idx) {
                    match v {
                        Value::Int64(x) => {
                            max_i = max_i.max(*x);
                            has_i = true;
                        }
                        Value::Float64(x) => {
                            max_f = max_f.max(*x);
                            has_f = true;
                        }
                        _ => {}
                    }
                }
            }
            if has_f {
                Ok(Value::Float64(max_f))
            } else if has_i {
                Ok(Value::Int64(max_i))
            } else {
                Ok(Value::Null)
            }
        }
    }
}

#[derive(Debug)]
pub enum ExecutionResult {
    Rows(Vec<Row>),
    RowsAffected(u64),
}
