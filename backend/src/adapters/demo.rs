use crate::{application::*, domain::*};
use async_trait::async_trait;
use serde_json::json;

pub struct DemoAdapter;
type DemoDataset = (Vec<Column>, Vec<serde_json::Map<String, serde_json::Value>>);
fn dataset(table: &str) -> Option<DemoDataset> {
    let fields: Vec<(&str, &str, bool)> = match table {
        "customers" => vec![
            ("id", "bigint", false),
            ("name", "text", false),
            ("email", "text", false),
            ("plan", "text", false),
            ("active", "boolean", false),
            ("created_at", "timestamp with time zone", false),
        ],
        "orders" => vec![
            ("id", "bigint", false),
            ("customer_id", "bigint", false),
            ("amount", "numeric", false),
            ("status", "text", false),
        ],
        "products" => vec![
            ("id", "bigint", false),
            ("name", "text", false),
            ("price", "numeric", false),
            ("description", "text", true),
        ],
        _ => return None,
    };
    let columns = fields
        .into_iter()
        .map(|(name, data_type, nullable)| Column {
            name: name.into(),
            data_type: data_type.into(),
            nullable,
            primary_key: name == "id",
            read_only: true,
            default_value: None,
            definition: None,
        })
        .collect();
    let names = [
        "Amelia Brooks",
        "Oliver Chen",
        "Sofia Martinez",
        "Noah Williams",
        "Isabella Rossi",
        "Liam Anderson",
        "Mia Thompson",
        "Ethan Park",
        "Charlotte Wilson",
        "Lucas Meyer",
        "Ava Robinson",
        "Leo Nakamura",
    ];
    let rows = (1..=124).map(|i| {
        let name = names[(i - 1) % names.len()];
        let value = match table {
            "customers" => json!({"id": i.to_string(), "name": name, "email": format!("{}{}@example.com", name.split_whitespace().next().unwrap_or("user").to_lowercase(), i), "plan": (["Pro", "Team", "Free"][(i-1)%3]), "active": i%7 != 0, "created_at": format!("2026-09-{:02}T09:30:00Z", (i-1)%28+1)}),
            "orders" => json!({"id": i.to_string(), "customer_id": ((i-1)%12+1).to_string(), "amount": format!("{}.00", i*19), "status": (["completed", "pending", "refunded"][(i-1)%3])}),
            _ => json!({"id": i.to_string(), "name": format!("{} {}", ["Notebook", "Desk lamp", "Studio headphones"][(i-1)%3], i), "price": "49.00", "description": if i%3 == 0 { None } else { Some("A carefully considered everyday essential.") }}),
        };
        value.as_object().cloned().unwrap_or_default()
    }).collect();
    Some((columns, rows))
}
#[async_trait]
impl DatabaseAdapter for DemoAdapter {
    fn provider(&self) -> Provider {
        Provider {
            id: "demo",
            name: "Sample workspace",
        }
    }
    async fn test(&self, _: &ConnectionInput) -> AppResult<()> {
        Ok(())
    }
    async fn query(&self, _: &ConnectionInput, query: &SqlQuery) -> AppResult<TablePage> {
        let (page, page_size) = query.bounds().map_err(invalid)?;
        let sql = sample_sql(query.statement())?;
        let connection = sample_database()
            .map_err(|_| AppError(ErrorKind::Internal, "The sample workspace is unavailable."))?;
        let interrupt = connection.get_interrupt_handle();
        let task = tokio::task::spawn_blocking(move || {
            run_sample_query(&connection, &sql, page, page_size)
        });
        // Interrupt runaway queries (for example recursive CTEs) instead of leaking a
        // blocked worker thread after the request deadline.
        let guard = tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(10)).await;
            interrupt.interrupt();
        });
        let result = task
            .await
            .map_err(|_| AppError(ErrorKind::Internal, "The sample query failed."));
        guard.abort();
        result?
    }
    async fn tables(&self, _: &ConnectionInput) -> AppResult<Vec<Table>> {
        Ok(["customers", "orders", "products"]
            .into_iter()
            .map(|name| Table {
                schema: "public".into(),
                name: name.into(),
                kind: "BASE TABLE".into(),
                columns: dataset(name)
                    .map(|(columns, _)| columns.into_iter().map(|column| column.name).collect())
                    .unwrap_or_default(),
                primary_keys: vec!["id".into()],
                referenceable_columns: vec!["id".into()],
                key_constraints: vec![KeyConstraint {
                    name: format!("{name}_pkey"),
                    kind: "PRIMARY KEY".into(),
                    columns: vec!["id".into()],
                }],
                foreign_keys: if name == "orders" {
                    vec![ForeignKey {
                        name: "orders_customer_id_fkey".into(),
                        column: "customer_id".into(),
                        referenced_schema: "public".into(),
                        referenced_table: "customers".into(),
                        referenced_column: "id".into(),
                    }]
                } else {
                    vec![]
                },
            })
            .collect())
    }
    async fn rows(&self, _: &ConnectionInput, query: &BrowseQuery) -> AppResult<TablePage> {
        let (page, page_size) = query.bounds().map_err(invalid)?;
        if query.schema != "public" {
            return Err(invalid("Unknown sample schema."));
        }
        let (columns, mut rows) =
            dataset(&query.table).ok_or_else(|| invalid("Unknown sample table."))?;
        if let Some(sort) = &query.sort {
            if !columns.iter().any(|c| &c.name == sort) {
                return Err(invalid("Choose an existing column to sort."));
            }
            rows.sort_by(|a, b| {
                let a = a[sort].to_string();
                let b = b[sort].to_string();
                let order = match (
                    a.trim_matches('"').parse::<f64>(),
                    b.trim_matches('"').parse::<f64>(),
                ) {
                    (Ok(a), Ok(b)) => a.total_cmp(&b),
                    _ => a.cmp(&b),
                };
                if query.descending.unwrap_or(false) {
                    order.reverse()
                } else {
                    order
                }
            });
        }
        let offset = (page * page_size) as usize;
        let total = rows.len();
        let has_more = rows.len() > offset + page_size as usize;
        Ok(TablePage {
            columns,
            rows: rows
                .into_iter()
                .skip(offset)
                .take(page_size as usize)
                .collect(),
            has_more,

            total_rows: Some(total.to_string()),

            editing: None,
            page,
            page_size,
        })
    }
}

const SAMPLE_TABLES: [&str; 3] = ["customers", "orders", "products"];

/// Validate as a single read-only PostgreSQL SELECT, then translate the few
/// PostgreSQL spellings SQLite lacks so sample queries behave like PostgreSQL.
fn sample_sql(sql: &str) -> AppResult<String> {
    use sqlparser::{
        ast::{
            CastKind, Expr, Function, FunctionArg, FunctionArgExpr, FunctionArgumentList,
            FunctionArguments, Ident, ObjectName, Value, VisitMut, VisitorMut,
        },
        dialect::PostgreSqlDialect,
        parser::Parser,
    };
    use std::ops::ControlFlow;

    fn lower(expr: &mut Expr) -> Box<Expr> {
        let expr = std::mem::replace(expr, Expr::value(Value::Null));
        Box::new(Expr::Function(Function {
            name: ObjectName::from(vec![Ident::new("lower")]),
            uses_odbc_syntax: false,
            parameters: FunctionArguments::None,
            args: FunctionArguments::List(FunctionArgumentList {
                duplicate_treatment: None,
                args: vec![FunctionArg::Unnamed(FunctionArgExpr::Expr(expr))],
                clauses: vec![],
            }),
            filter: None,
            null_treatment: None,
            over: None,
            within_group: vec![],
        }))
    }
    struct Sqlite;
    impl VisitorMut for Sqlite {
        type Break = ();
        fn post_visit_expr(&mut self, expr: &mut Expr) -> ControlFlow<()> {
            match expr {
                Expr::ILike {
                    negated,
                    any: false,
                    expr: value,
                    pattern,
                    escape_char,
                } => {
                    *expr = Expr::Like {
                        negated: *negated,
                        any: false,
                        expr: lower(value),
                        pattern: lower(pattern),
                        escape_char: escape_char.take(),
                    };
                }
                Expr::Cast { kind, .. } if *kind == CastKind::DoubleColon => *kind = CastKind::Cast,
                _ => {}
            }
            ControlFlow::Continue(())
        }
    }

    let dialect = PostgreSqlDialect {};
    let canonical = super::read_query::select(sql, &dialect)?;
    let mut statements = Parser::parse_sql(&dialect, &canonical)
        .map_err(|_| invalid("Run one supported SELECT query with unique column aliases."))?;
    let _ = statements.visit(&mut Sqlite);
    Ok(statements.remove(0).to_string())
}

fn sample_database() -> rusqlite::Result<rusqlite::Connection> {
    use rusqlite::types::Value;
    let connection = rusqlite::Connection::open_in_memory()?;
    // A `public` database lets both `customers` and `public.customers` resolve.
    // PostgreSQL LIKE is case-sensitive; ILIKE is rewritten to lower(...) LIKE lower(...).
    connection
        .execute_batch("ATTACH DATABASE ':memory:' AS public; PRAGMA case_sensitive_like = ON;")?;
    for table in SAMPLE_TABLES {
        let Some((columns, rows)) = dataset(table) else {
            continue;
        };
        let definitions = columns
            .iter()
            .map(|c| {
                format!(
                    "\"{}\" {}{}{}",
                    c.name,
                    c.data_type,
                    if c.nullable { "" } else { " NOT NULL" },
                    if c.primary_key { " PRIMARY KEY" } else { "" }
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        connection.execute_batch(&format!("CREATE TABLE public.\"{table}\" ({definitions})"))?;
        let placeholders = vec!["?"; columns.len()].join(", ");
        let mut insert = connection.prepare(&format!(
            "INSERT INTO public.\"{table}\" VALUES ({placeholders})"
        ))?;
        for row in rows {
            insert.execute(rusqlite::params_from_iter(columns.iter().map(
                |c| match &row[&c.name] {
                    serde_json::Value::Null => Value::Null,
                    serde_json::Value::Bool(b) => Value::Integer(i64::from(*b)),
                    serde_json::Value::String(s) => Value::Text(s.clone()),
                    other => Value::Text(other.to_string()),
                },
            )))?;
        }
    }
    connection.execute_batch("PRAGMA query_only = ON;")?;
    Ok(connection)
}

fn sample_error(error: rusqlite::Error) -> AppError {
    let message = match &error {
        rusqlite::Error::SqliteFailure(failure, message) => {
            if failure.code == rusqlite::ErrorCode::OperationInterrupted {
                return AppError(
                    ErrorKind::Timeout,
                    "Database request timed out. Try a smaller query.",
                );
            }
            message.clone().unwrap_or_default()
        }
        _ => String::new(),
    };
    invalid(if message.starts_with("no such function") {
        "This function is not available in the sample workspace. Connect PostgreSQL for full SQL."
    } else if message.starts_with("no such table") {
        "Unknown sample table. Query public.customers, public.orders, or public.products."
    } else if message.starts_with("no such column") || message.contains("ambiguous column") {
        "Check the column names. Run one SELECT query at a time."
    } else {
        "Check the SQL syntax, table names, and column aliases. Run one SELECT query at a time."
    })
}

fn run_sample_query(
    connection: &rusqlite::Connection,
    sql: &str,
    page: u32,
    page_size: u32,
) -> AppResult<TablePage> {
    use rusqlite::types::ValueRef;
    // SQLite silently renames duplicate subquery columns, so check the query itself.
    let mut names = std::collections::HashSet::new();
    for name in connection
        .prepare(sql)
        .map_err(sample_error)?
        .column_names()
    {
        if !names.insert(name.to_owned()) {
            return Err(invalid("Use unique column aliases for query results."));
        }
    }
    let mut statement = connection
        .prepare(&format!("SELECT * FROM (\n{sql}\n) LIMIT ?1 OFFSET ?2"))
        .map_err(sample_error)?;
    let mut columns = Vec::new();
    for column in statement.columns() {
        columns.push(Column {
            name: column.name().to_owned(),
            data_type: column.decl_type().unwrap_or("").to_lowercase(),
            nullable: true,
            primary_key: false,
            read_only: true,
            default_value: None,
            definition: None,
        });
    }
    if columns.is_empty() {
        return Err(invalid("Run a SELECT query that returns columns."));
    }
    let mut records = statement
        .query(rusqlite::params![
            i64::from(page_size) + 1,
            i64::from(page) * i64::from(page_size)
        ])
        .map_err(sample_error)?;
    let mut rows = Vec::new();
    while let Some(record) = records.next().map_err(sample_error)? {
        let mut row = serde_json::Map::new();
        for (index, column) in columns.iter().enumerate() {
            let value = match record.get_ref(index).map_err(sample_error)? {
                ValueRef::Null => serde_json::Value::Null,
                ValueRef::Integer(i) if column.data_type == "boolean" => (i != 0).into(),
                // Match the PostgreSQL adapter: bigint and numeric are exact strings.
                ValueRef::Integer(i) if column.data_type == "numeric" => format!("{i}.00").into(),
                ValueRef::Real(f) if column.data_type == "numeric" => format!("{f:.2}").into(),
                ValueRef::Integer(i) if column.data_type == "bigint" => i.to_string().into(),
                ValueRef::Integer(i) => i.into(),
                ValueRef::Real(f) => serde_json::Number::from_f64(f)
                    .map(serde_json::Value::Number)
                    .unwrap_or(serde_json::Value::Null),
                ValueRef::Text(t) => String::from_utf8_lossy(t).into_owned().into(),
                ValueRef::Blob(_) => serde_json::Value::Null,
            };
            row.insert(column.name.clone(), value);
        }
        rows.push(row);
    }
    drop(records);
    let has_more = rows.len() > page_size as usize;
    rows.truncate(page_size as usize);
    let total: i64 = connection
        .query_row(&format!("SELECT COUNT(*) FROM (\n{sql}\n)"), [], |r| {
            r.get(0)
        })
        .map_err(sample_error)?;
    Ok(TablePage {
        columns,
        rows,
        has_more,
        total_rows: Some(total.to_string()),
        editing: None,
        page,
        page_size,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn run(sql: &str) -> AppResult<TablePage> {
        let input: ConnectionInput = serde_json::from_str(
            r#"{"provider":"demo","host":"","port":0,"database":"","username":"","password":"","ssl":false}"#,
        )
        .unwrap();
        let query = SqlQuery {
            sql: sql.into(),
            page: None,
            page_size: Some(1000),
        };
        DemoAdapter.query(&input, &query).await
    }

    #[tokio::test]
    async fn sample_queries_support_postgres_filters() {
        let all = run("SELECT * FROM public.customers").await.unwrap();
        assert_eq!(all.rows.len(), 124);
        assert_eq!(all.rows[0]["id"], "1");
        assert_eq!(all.rows[0]["active"], true);
        let ilike = run("SELECT id, name FROM customers WHERE name ILIKE 'amelia%';")
            .await
            .unwrap();
        assert!(!ilike.rows.is_empty());
        assert!(ilike.rows.iter().all(|r| r["name"] == "Amelia Brooks"));
        let like = run("SELECT * FROM customers WHERE name LIKE 'amelia%'").await;
        assert!(like.unwrap().rows.is_empty());
        let not_ilike = run("SELECT * FROM customers WHERE name NOT ILIKE '%A%'").await;
        let not_ilike = not_ilike.unwrap();
        assert!(!not_ilike.rows.is_empty());
        assert!(not_ilike.rows.iter().all(|r| r["name"] == "Oliver Chen"));
        let described = run("SELECT * FROM products WHERE description IS NOT NULL")
            .await
            .unwrap();
        let missing = run("SELECT * FROM products WHERE description IS NULL")
            .await
            .unwrap();
        assert!(!missing.rows.is_empty());
        assert_eq!(described.rows.len() + missing.rows.len(), 124);
        let orders = run("SELECT o.id, o.amount, c.name FROM orders o JOIN customers c ON c.id = o.customer_id WHERE o.status IN ('completed', 'pending') AND o.amount::numeric > 100 ORDER BY o.amount DESC LIMIT 3")
            .await
            .unwrap();
        assert_eq!(orders.rows.len(), 3);
        assert_eq!(orders.rows[0]["amount"], "2356.00");
        let grouped = run("SELECT status, COUNT(*) AS total FROM orders GROUP BY status HAVING COUNT(*) > 1 ORDER BY status")
            .await
            .unwrap();
        assert_eq!(grouped.rows.len(), 3);
        let active = run("SELECT * FROM customers WHERE active IS TRUE").await;
        assert!(active.unwrap().rows.len() < 124);
    }

    #[tokio::test]
    async fn sample_queries_stay_read_only_and_sanitized() {
        for sql in [
            "DELETE FROM customers",
            "SELECT 1; DROP TABLE customers",
            "ATTACH DATABASE 'x.db' AS x",
            "SELECT * FROM missing_table",
            "SELECT nope FROM customers",
            "SELECT id, id FROM customers",
        ] {
            assert!(run(sql).await.is_err(), "{sql}");
        }
    }
}
