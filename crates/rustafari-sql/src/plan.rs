//! Logical plan from SQL AST.

use rustafari_core::{ColumnType, Result, RustafariError, Value};
use sqlparser::ast::{
    Expr, Function, FunctionArgExpr, ObjectType, Query, SelectItem, SetExpr, Statement,
    TableFactor, Values,
};

/// Logical plan node (simplified relational algebra).
#[derive(Debug, Clone)]
pub enum PlanNode {
    /// Scan a table.
    Scan {
        table: String,
        namespace: Option<String>,
    },
    /// Filter rows by expression.
    Filter {
        input: Box<PlanNode>,
        predicate: FilterExpr,
    },
    /// Project columns.
    Project {
        input: Box<PlanNode>,
        columns: Vec<String>,
    },
    /// Global aggregation (no GROUP BY): SUM, COUNT, AVG, MIN, MAX.
    Aggregate {
        input: Box<PlanNode>,
        aggs: Vec<AggExpr>,
    },
    /// Limit rows.
    Limit {
        input: Box<PlanNode>,
        limit: u64,
        offset: u64,
    },
    /// Insert into table.
    Insert {
        table: String,
        namespace: Option<String>,
        columns: Vec<String>,
        values: Vec<Vec<Value>>,
    },
    /// Create table.
    CreateTable {
        table: String,
        namespace: Option<String>,
        columns: Vec<(String, ColumnType)>,
    },
    /// Drop table.
    DropTable {
        table: String,
        namespace: Option<String>,
    },
    /// Describe table (show columns/schema).
    DescribeTable {
        table: String,
        namespace: Option<String>,
    },
}

/// Simplified filter expression for execution.
#[derive(Debug, Clone)]
pub enum FilterExpr {
    Literal(bool),
    And(Box<FilterExpr>, Box<FilterExpr>),
    Or(Box<FilterExpr>, Box<FilterExpr>),
    Not(Box<FilterExpr>),
    Cmp { col: String, op: CmpOp, value: Value },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CmpOp {
    Eq,
    Ne,
    Gt,
    Ge,
    Lt,
    Le,
}

/// Aggregate expression: function name and column (or * for COUNT).
#[derive(Debug, Clone)]
pub enum AggExpr {
    Sum { column: String },
    CountStar,
    Count { column: String },
    Avg { column: String },
    Min { column: String },
    Max { column: String },
}

/// Root of a logical plan.
#[derive(Debug, Clone)]
pub struct LogicalPlan {
    pub root: PlanNode,
}

fn table_parts(name: &sqlparser::ast::ObjectName) -> (Option<String>, String) {
    let parts: Vec<String> = name.0.iter().map(|p| p.value.clone()).collect();
    if parts.len() >= 2 {
        (Some(parts[0].clone()), parts[1..].join("."))
    } else if parts.len() == 1 {
        (None, parts[0].clone())
    } else {
        (None, String::new())
    }
}

fn expr_to_value(expr: &Expr) -> Result<Value> {
    match expr {
        Expr::Value(v) => match v {
            sqlparser::ast::Value::Number(n, _) => {
                if n.contains('.') {
                    Ok(Value::Float64(n.parse().unwrap_or(0.0)))
                } else {
                    Ok(Value::Int64(n.parse().unwrap_or(0)))
                }
            }
            sqlparser::ast::Value::SingleQuotedString(s) => Ok(Value::String(s.clone())),
            sqlparser::ast::Value::Boolean(b) => Ok(Value::Bool(*b)),
            sqlparser::ast::Value::Null => Ok(Value::Null),
            sqlparser::ast::Value::UnQuotedString(s) => {
                if let Ok(n) = s.parse::<i64>() {
                    Ok(Value::Int64(n))
                } else if let Ok(f) = s.parse::<f64>() {
                    Ok(Value::Float64(f))
                } else {
                    Ok(Value::String(s.clone()))
                }
            }
            _ => Err(RustafariError::Parse("unsupported literal in VALUES".into())),
        },
        _ => Err(RustafariError::Parse("only literals supported in VALUES".into())),
    }
}

fn expr_to_filter(expr: &Expr) -> Result<FilterExpr> {
    match expr {
        Expr::BinaryOp { left, op, right } => match op {
            sqlparser::ast::BinaryOperator::And => {
                let l = expr_to_filter(left)?;
                let r = expr_to_filter(right)?;
                Ok(FilterExpr::And(Box::new(l), Box::new(r)))
            }
            sqlparser::ast::BinaryOperator::Or => {
                let l = expr_to_filter(left)?;
                let r = expr_to_filter(right)?;
                Ok(FilterExpr::Or(Box::new(l), Box::new(r)))
            }
            sqlparser::ast::BinaryOperator::Eq => {
                if let (Expr::Identifier(id), Expr::Value(v)) = (left.as_ref(), right.as_ref()) {
                    let value = expr_to_value(&Expr::Value(v.clone()))?;
                    return Ok(FilterExpr::Cmp {
                        col: id.value.clone(),
                        op: CmpOp::Eq,
                        value,
                    });
                }
                if let (Expr::Value(v), Expr::Identifier(id)) = (left.as_ref(), right.as_ref()) {
                    let value = expr_to_value(&Expr::Value(v.clone()))?;
                    return Ok(FilterExpr::Cmp {
                        col: id.value.clone(),
                        op: CmpOp::Eq,
                        value,
                    });
                }
                Err(RustafariError::Parse("comparison: expected column and literal".into()))
            }
            sqlparser::ast::BinaryOperator::NotEq => {
                if let (Expr::Identifier(id), Expr::Value(v)) = (left.as_ref(), right.as_ref()) {
                    let value = expr_to_value(&Expr::Value(v.clone()))?;
                    return Ok(FilterExpr::Cmp {
                        col: id.value.clone(),
                        op: CmpOp::Ne,
                        value,
                    });
                }
                Err(RustafariError::Parse("comparison: expected column and literal".into()))
            }
            sqlparser::ast::BinaryOperator::Gt
            | sqlparser::ast::BinaryOperator::Lt
            | sqlparser::ast::BinaryOperator::GtEq
            | sqlparser::ast::BinaryOperator::LtEq => {
                let (id, v) = if let Expr::Identifier(id) = left.as_ref() {
                    (id.value.clone(), expr_to_value(right)?)
                } else if let Expr::Identifier(id) = right.as_ref() {
                    (id.value.clone(), expr_to_value(left)?)
                } else {
                    return Err(RustafariError::Parse("comparison: expected column".into()));
                };
                let op = match op {
                    sqlparser::ast::BinaryOperator::Gt => CmpOp::Gt,
                    sqlparser::ast::BinaryOperator::Lt => CmpOp::Lt,
                    sqlparser::ast::BinaryOperator::GtEq => CmpOp::Ge,
                    sqlparser::ast::BinaryOperator::LtEq => CmpOp::Le,
                    _ => return Err(RustafariError::Parse("unsupported comparison".into())),
                };
                Ok(FilterExpr::Cmp { col: id, op, value: v })
            }
            _ => Err(RustafariError::Parse("unsupported binary op in WHERE".into())),
        },
        Expr::Value(sqlparser::ast::Value::Boolean(b)) => Ok(FilterExpr::Literal(*b)),
        Expr::Nested(inner) => expr_to_filter(inner),
        _ => Err(RustafariError::Parse("unsupported expression in WHERE".into())),
    }
}

fn query_to_plan(query: Query) -> Result<LogicalPlan> {
    let SetExpr::Select(sel) = *query.body else {
        return Err(RustafariError::Parse("only SELECT supported in query".into()));
    };
    let from = sel.from.first().ok_or_else(|| RustafariError::Parse("SELECT must have FROM".into()))?;
    let (ns, table) = match &from.relation {
        TableFactor::Table { name, .. } => table_parts(name),
        _ => return Err(RustafariError::Parse("only table scan supported".into())),
    };
    let mut root: PlanNode = PlanNode::Scan {
        table: table.clone(),
        namespace: ns.clone(),
    };
    if let Some(where_expr) = &sel.selection {
        root = PlanNode::Filter {
            input: Box::new(root),
            predicate: expr_to_filter(where_expr)?,
        };
    }
    // Check for aggregate-only projection (e.g. SELECT SUM(x), COUNT(*) FROM t).
    if let Some(aggs) = projection_as_aggs(&sel.projection) {
        root = PlanNode::Aggregate {
            input: Box::new(root),
            aggs,
        };
    } else {
        let columns: Vec<String> = sel
            .projection
            .iter()
            .filter_map(|p| {
                if let SelectItem::UnnamedExpr(Expr::Identifier(id)) = p {
                    Some(id.value.clone())
                } else {
                    None
                }
            })
            .collect();
        if !columns.is_empty() {
            root = PlanNode::Project {
                input: Box::new(root),
                columns,
            };
        }
    }
    let limit = query.limit.as_ref().map(parse_limit).unwrap_or(u64::MAX);
    let offset = query.offset.as_ref().map(|o| parse_limit(&o.value)).unwrap_or(0);
    if limit != u64::MAX || offset != 0 {
        root = PlanNode::Limit {
            input: Box::new(root),
            limit,
            offset,
        };
    }
    Ok(LogicalPlan { root })
}

fn parse_limit(expr: &Expr) -> u64 {
    if let Expr::Value(sqlparser::ast::Value::Number(n, _)) = expr {
        n.parse().unwrap_or(0)
    } else {
        0
    }
}

/// Parse a single aggregate expression from a function call (SUM, COUNT, AVG, MIN, MAX).
fn expr_to_agg(f: &Function) -> Result<AggExpr> {
    let name = f.name.0.first().map(|i| i.value.to_uppercase()).unwrap_or_default();
    let first_arg = f.args.first();
    let col_from_arg = |a: &sqlparser::ast::FunctionArg| -> Result<String> {
        match a {
            sqlparser::ast::FunctionArg::Unnamed(FunctionArgExpr::Expr(Expr::Identifier(id))) => {
                Ok(id.value.clone())
            }
            _ => Err(RustafariError::Parse("aggregate argument must be a column name".into())),
        }
    };
    match name.as_str() {
        "SUM" => Ok(AggExpr::Sum { column: col_from_arg(first_arg.ok_or_else(|| RustafariError::Parse("SUM requires one argument".into()))?)? }),
        "COUNT" => {
            if let Some(sqlparser::ast::FunctionArg::Unnamed(FunctionArgExpr::Wildcard)) = first_arg {
                Ok(AggExpr::CountStar)
            } else if let Some(a) = first_arg {
                Ok(AggExpr::Count { column: col_from_arg(a)? })
            } else {
                Err(RustafariError::Parse("COUNT requires * or column".into()))
            }
        }
        "AVG" => Ok(AggExpr::Avg { column: col_from_arg(first_arg.ok_or_else(|| RustafariError::Parse("AVG requires one argument".into()))?)? }),
        "MIN" => Ok(AggExpr::Min { column: col_from_arg(first_arg.ok_or_else(|| RustafariError::Parse("MIN requires one argument".into()))?)? }),
        "MAX" => Ok(AggExpr::Max { column: col_from_arg(first_arg.ok_or_else(|| RustafariError::Parse("MAX requires one argument".into()))?)? }),
        _ => Err(RustafariError::Parse(format!("unsupported aggregate: {}", name))),
    }
}

/// If the projection is only aggregate expressions, return them.
fn projection_as_aggs(projection: &[SelectItem]) -> Option<Vec<AggExpr>> {
    let mut aggs = Vec::with_capacity(projection.len());
    for p in projection {
        let expr = match p {
            SelectItem::UnnamedExpr(e) => e,
            _ => return None,
        };
        let agg = match expr {
            Expr::Function(f) => expr_to_agg(f).ok()?,
            _ => return None,
        };
        aggs.push(agg);
    }
    if aggs.is_empty() {
        return None;
    }
    Some(aggs)
}

fn values_from_ast(values: Values) -> Result<Vec<Vec<Value>>> {
    let rows: Vec<Vec<Value>> = values
        .rows
        .into_iter()
        .map(|row| {
            row.into_iter()
                .map(|e| expr_to_value(&e))
                .collect::<Result<Vec<_>>>()
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(rows)
}

pub fn ast_to_plan(stmt: Statement) -> Result<LogicalPlan> {
    match stmt {
        Statement::Query(q) => query_to_plan(*q),
        Statement::Insert {
            table_name,
            columns,
            source,
            ..
        } => {
            let (ns, table) = table_parts(&table_name);
            let columns: Vec<String> = columns
                .into_iter()
                .map(|c| c.to_string())
                .collect();
            let source_query = source.ok_or_else(|| RustafariError::Parse("INSERT must have source".into()))?;
            let values = match *source_query.body {
                SetExpr::Values(v) => values_from_ast(v)?,
                _ => return Err(RustafariError::Parse("INSERT only supports VALUES".into())),
            };
            Ok(LogicalPlan {
                root: PlanNode::Insert {
                    table,
                    namespace: ns,
                    columns,
                    values,
                },
            })
        }
        Statement::CreateTable {
            name,
            columns: cols,
            ..
        } => {
            let (ns, table) = table_parts(&name);
            let col_defs: Vec<(String, ColumnType)> = cols
                .into_iter()
                .map(|c| {
                    let name = c.name.to_string();
                    let ty = match c.data_type {
                        sqlparser::ast::DataType::BigInt(_) => ColumnType::BigInt,
                        sqlparser::ast::DataType::DoublePrecision => ColumnType::Double,
                        sqlparser::ast::DataType::Boolean => ColumnType::Boolean,
                        sqlparser::ast::DataType::Varchar(_) => ColumnType::Varchar(None),
                        sqlparser::ast::DataType::Text => ColumnType::Text,
                        sqlparser::ast::DataType::Timestamp(_, _) => ColumnType::Timestamp,
                        sqlparser::ast::DataType::Date => ColumnType::Date,
                        sqlparser::ast::DataType::Blob(_) => ColumnType::Blob,
                        _ => ColumnType::Text,
                    };
                    (name, ty)
                })
                .collect();
            Ok(LogicalPlan {
                root: PlanNode::CreateTable {
                    table,
                    namespace: ns,
                    columns: col_defs,
                },
            })
        }
        Statement::Drop {
            object_type,
            names,
            if_exists: _,
            ..
        } if object_type == ObjectType::Table => {
            let name = names.into_iter().next().ok_or_else(|| RustafariError::Parse("DROP TABLE requires a table name".into()))?;
            let (ns, table) = table_parts(&name);
            Ok(LogicalPlan {
                root: PlanNode::DropTable { table, namespace: ns },
            })
        }
        Statement::ExplainTable { table_name, .. } => {
            let (ns, table) = table_parts(&table_name);
            Ok(LogicalPlan {
                root: PlanNode::DescribeTable { table, namespace: ns },
            })
        }
        _ => Err(RustafariError::Parse("unsupported statement".into())),
    }
}
