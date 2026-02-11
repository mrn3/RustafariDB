//! SQL dialect (PostgreSQL-like).

use sqlparser::dialect::Dialect;

/// RustafariDB SQL dialect: PostgreSQL-compatible with common extensions.
#[derive(Debug)]
pub struct RustafariDialect;

impl Dialect for RustafariDialect {
    fn is_identifier_start(&self, ch: char) -> bool {
        sqlparser::dialect::GenericDialect.is_identifier_start(ch)
    }

    fn is_identifier_part(&self, ch: char) -> bool {
        sqlparser::dialect::GenericDialect.is_identifier_part(ch)
    }

    fn supports_filter_during_aggregation(&self) -> bool {
        true
    }

    fn supports_group_by_expr(&self) -> bool {
        true
    }
}
