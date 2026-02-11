//! # RustafariDB SQL
//!
//! SQL parser and logical plan (ANSI SQL style, PostgreSQL-compatible dialect).

pub mod plan;
pub mod dialect;

pub use plan::{CmpOp, FilterExpr, LogicalPlan, PlanNode};
pub use dialect::RustafariDialect;

use rustafari_core::{Result, RustafariError};
use sqlparser::ast::Statement;
use sqlparser::parser::Parser;

/// Parse SQL string into AST statements.
pub fn parse_sql(sql: &str) -> Result<Vec<Statement>> {
    let dialect = RustafariDialect;
    Parser::parse_sql(&dialect, sql).map_err(|e| RustafariError::Parse(e.to_string()))
}

/// Convert SQL AST to logical plan (simplified: one statement -> one plan).
pub fn sql_to_plan(sql: &str) -> Result<LogicalPlan> {
    let stmts = parse_sql(sql)?;
    let stmt = stmts
        .into_iter()
        .next()
        .ok_or_else(|| RustafariError::Parse("empty statement".into()))?;
    plan::ast_to_plan(stmt)
}
