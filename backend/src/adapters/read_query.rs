use crate::application::{AppResult, invalid};
use sqlparser::{
    ast::{Query, Select, Statement, TableFactor, Visit, Visitor},
    dialect::Dialect,
    parser::Parser,
};
use std::ops::ControlFlow;

pub async fn bounded<T>(
    operation: impl std::future::Future<Output = AppResult<T>>,
) -> AppResult<T> {
    tokio::time::timeout(std::time::Duration::from_secs(15), operation)
        .await
        .map_err(|_| {
            crate::application::AppError(
                crate::application::ErrorKind::Timeout,
                "Database request timed out. Try a smaller query.",
            )
        })?
}

struct ReadOnly;
impl Visitor for ReadOnly {
    type Break = ();
    fn pre_visit_statement(&mut self, statement: &Statement) -> ControlFlow<()> {
        if matches!(statement, Statement::Query(_)) {
            ControlFlow::Continue(())
        } else {
            ControlFlow::Break(())
        }
    }
    fn pre_visit_table_factor(&mut self, table: &TableFactor) -> ControlFlow<()> {
        // Remote/pass-through table functions can contain SQL with side effects.
        if matches!(
            table,
            TableFactor::Table { args: None, .. }
                | TableFactor::Derived { .. }
                | TableFactor::NestedJoin { .. }
        ) {
            ControlFlow::Continue(())
        } else {
            ControlFlow::Break(())
        }
    }
    fn pre_visit_select(&mut self, select: &Select) -> ControlFlow<()> {
        if select.into.is_some() {
            ControlFlow::Break(())
        } else {
            ControlFlow::Continue(())
        }
    }
    fn pre_visit_query(&mut self, query: &Query) -> ControlFlow<()> {
        if !query.locks.is_empty()
            || query.for_clause.is_some()
            || query.settings.is_some()
            || query.format_clause.is_some()
            || !query.pipe_operators.is_empty()
        {
            ControlFlow::Break(())
        } else {
            ControlFlow::Continue(())
        }
    }
}

/// Execute the parsed representation, never the original text: executable comments
/// and dialect-specific batch delimiters cannot bypass the single-query boundary.
pub fn select(sql: &str, dialect: &dyn Dialect) -> AppResult<String> {
    if sql.contains("/*!") || sql.contains("/*M!") {
        return Err(invalid("Executable SQL comments are unsupported."));
    }
    let statements =
        Parser::parse_sql(dialect, sql).map_err(|_| invalid("Invalid SQL syntax in the query."))?;
    if statements.len() != 1 || statements.visit(&mut ReadOnly).is_break() {
        return Err(invalid(
            "Only a single read-only SELECT query is supported.",
        ));
    }
    super::sql::canonical(&statements[0], dialect)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlparser::dialect::{MsSqlDialect, MySqlDialect};
    #[test]
    fn rejects_writes_batches_and_select_into() {
        for dialect in [&MsSqlDialect {} as &dyn Dialect, &MySqlDialect {}] {
            for sql in [
                "DELETE FROM t",
                "SELECT 1; DROP TABLE t",
                "SELECT * INTO other FROM t",
                "SELECT * FROM t FOR UPDATE",
                "WITH x AS (DELETE FROM t RETURNING *) SELECT * FROM x",
                "SELECT NEXT VALUE FOR sequence_name",
                "EXEC('DELETE FROM t')",
                "SELECT * FROM OPENQUERY(remote_server, 'DELETE FROM t OUTPUT deleted.*')",
            ] {
                assert!(select(sql, dialect).is_err(), "{sql}");
            }
            assert!(select("SELECT 1 AS value", dialect).is_ok());
            assert!(select("SELECT '; DELETE' AS value -- comment", dialect).is_ok());
        }
    }
    #[test]
    fn executable_comments_are_removed() {
        assert!(
            select(
                "SELECT 1 AS value /*! INTO OUTFILE '/tmp/data' */",
                &MySqlDialect {}
            )
            .is_err()
        );
        assert_eq!(
            select("SELECT 1 AS value /* regular comment */", &MySqlDialect {}).unwrap(),
            "SELECT 1 AS value"
        );
    }
}
