//! SQL execution entry point.

use crate::{ExecutionResult, SessionState};
use rustafari_core::Result;

/// SQL executor: parse + plan + execute.
pub struct SqlExecutor;

impl SqlExecutor {
    /// Execute SQL string and return result.
    pub fn execute(state: &SessionState, sql: &str) -> Result<ExecutionResult> {
        let plan = rustafari_sql::sql_to_plan(sql)?;
        crate::execute_plan(state, &plan)
    }
}
