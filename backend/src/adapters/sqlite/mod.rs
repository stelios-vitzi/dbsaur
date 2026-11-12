use crate::{application::*, domain::*};
use async_trait::async_trait;
use rusqlite::{Connection, OpenFlags, params, params_from_iter, types::ValueRef};
use serde_json::{Map, Value};
use sqlparser::dialect::SQLiteDialect;
use std::time::{Duration, Instant};

pub struct SqliteAdapter;

impl From<rusqlite::Error> for AppError {
    fn from(_: rusqlite::Error) -> Self {
        AppError(
            ErrorKind::Upstream,
            "SQLite operation failed. Check the file, permissions, SQL, and constraints; refresh before retrying.",
        )
    }
}

fn quote(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}
fn target(schema: &str, table: &str) -> AppResult<String> {
    if schema != "main" {
        return Err(invalid("Choose a table in the main SQLite database."));
    }
    Ok(format!(
        "\"main\".{}",
        super::write::identifier(table, 128, quote)?
    ))
}

async fn run<T: Send + 'static>(
    input: &ConnectionInput,
    write: bool,
    operation: impl FnOnce(&mut Connection) -> AppResult<T> + Send + 'static,
) -> AppResult<T> {
    input.validate().map_err(invalid)?;
    if write && input.read_only {
        return Err(invalid("This connection is read only."));
    }
    let path = input.database.clone();
    tokio::task::spawn_blocking(move || {
        // No CREATE or URI flag: connecting must never create a file, attach a
        // second database, or interpret URI parameters supplied as a path.
        let flags = if write {
            OpenFlags::SQLITE_OPEN_READ_WRITE
        } else {
            OpenFlags::SQLITE_OPEN_READ_ONLY
        };
        let mut connection =
            Connection::open_with_flags(path, flags | OpenFlags::SQLITE_OPEN_NO_MUTEX)?;
        connection.busy_timeout(Duration::from_secs(5))?;
        connection.pragma_update(None, "trusted_schema", false)?;
        connection.pragma_update(None, "foreign_keys", true)?;
        connection.pragma_update(None, "query_only", !write)?;
        connection.set_limit(
            rusqlite::limits::Limit::SQLITE_LIMIT_LENGTH,
            16 * 1024 * 1024,
        )?;
        connection.set_limit(rusqlite::limits::Limit::SQLITE_LIMIT_ATTACHED, 0)?;
        let deadline = Instant::now() + Duration::from_secs(10);
        connection.progress_handler(1000, Some(move || Instant::now() >= deadline))?;
        operation(&mut connection)
    })
    .await
    .map_err(|_| AppError(ErrorKind::Internal, "SQLite worker failed."))?
}

fn columns(connection: &Connection, table: &str) -> AppResult<Vec<Column>> {
    let mut statement = connection.prepare("SELECT name, type, \"notnull\", dflt_value, pk, hidden FROM pragma_table_xinfo(?1, 'main') WHERE hidden != 1 ORDER BY cid")?;
    let result = statement
        .query_map([table], |row| {
            let primary_key = row.get::<_, i64>(4)? > 0;
            Ok(Column {
                name: row.get(0)?,
                data_type: row.get(1)?,
                nullable: !row.get::<_, bool>(2)?,
                primary_key,
                read_only: row.get::<_, i64>(5)? != 0,
                default_value: row.get(3)?,
                definition: None,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    if result.is_empty() {
        return Err(invalid("Table not found. Refresh the database."));
    }
    Ok(result)
}

fn tables(connection: &Connection) -> AppResult<Vec<Table>> {
    let mut statement = connection.prepare("SELECT name, type FROM main.sqlite_schema WHERE type IN ('table','view') AND name NOT LIKE 'sqlite_%' ORDER BY name")?;
    let names = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    names.into_iter().map(|(name, kind)| {
        let columns = columns(connection, &name)?;
        let mut foreign_statement = connection.prepare("SELECT id, seq, \"from\", \"table\", \"to\" FROM pragma_foreign_key_list(?1, 'main')")?;
        let mut foreign_keys = foreign_statement.query_map([&name], |row| Ok(ForeignKey {
            name: format!("fk_{}_{}", name, row.get::<_, i64>(0)?),
            column: row.get(2)?, referenced_schema: "main".into(), referenced_table: row.get(3)?,
            referenced_column: row.get::<_, Option<String>>(4)?.unwrap_or_default(),
        }))?.collect::<Result<Vec<_>, _>>()?;
        // REFERENCES parent without a column list means the parent's primary key.
        for foreign in &mut foreign_keys {
            if foreign.referenced_column.is_empty() {
                let id: i64 = foreign.name.rsplit('_').next().and_then(|v| v.parse().ok()).unwrap_or(-1);
                let seq: u32 = connection.query_row("SELECT seq FROM pragma_foreign_key_list(?1, 'main') WHERE id = ?2 AND \"from\" = ?3", params![name, id, foreign.column], |r| r.get(0))?;
                let mut pk = connection.prepare("SELECT name FROM pragma_table_xinfo(?1, 'main') WHERE pk > 0 ORDER BY pk")?;
                let keys = pk.query_map([&foreign.referenced_table], |r| r.get::<_, String>(0))?.collect::<Result<Vec<_>, _>>()?;
                foreign.referenced_column = keys.get(seq as usize).cloned().unwrap_or_default();
            }
        }
        let primary_keys: Vec<_> = columns.iter().filter(|c| c.primary_key).map(|c| c.name.clone()).collect();
        let mut key_constraints = Vec::new();
        if !primary_keys.is_empty() {
            key_constraints.push(KeyConstraint { name: "PRIMARY".into(), kind: "PRIMARY KEY".into(), columns: primary_keys.clone() });
        }
        for index in indexes(connection, &name)? {
            if index.unique && !index.columns.is_empty() && connection.query_row("SELECT NOT partial AND NOT EXISTS (SELECT 1 FROM pragma_index_info(?1, 'main') WHERE name IS NULL) FROM pragma_index_list(?2, 'main') WHERE name = ?1", params![index.name, name], |r| r.get::<_, bool>(0))? {
                key_constraints.push(KeyConstraint { name: index.name, kind: "UNIQUE".into(), columns: index.columns });
            }
        }
        Ok(Table { schema: "main".into(), name, kind: if kind == "view" { "VIEW" } else { "BASE TABLE" }.into(),
            columns: columns.iter().map(|c| c.name.clone()).collect(), primary_keys,
            referenceable_columns: key_constraints.iter().filter(|k| k.columns.len() == 1).flat_map(|k| k.columns.clone()).collect(),
            key_constraints, foreign_keys })
    }).collect()
}

fn indexes(connection: &Connection, table: &str) -> AppResult<Vec<IndexInfo>> {
    let mut statement = connection
        .prepare("SELECT name, \"unique\", origin, partial FROM pragma_index_list(?1, 'main')")?;
    let rows = statement
        .query_map([table], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, bool>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, bool>(3)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    rows.into_iter()
        .map(|(name, unique, origin, partial)| {
            let mut statement = connection
                .prepare("SELECT name FROM pragma_index_info(?1, 'main') ORDER BY seqno")?;
            let columns = statement
                .query_map([&name], |r| r.get::<_, Option<String>>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            let advanced: bool = connection.query_row("SELECT EXISTS (SELECT 1 FROM pragma_index_xinfo(?1, 'main') WHERE key = 1 AND (desc != 0 OR coll != 'BINARY'))", [&name], |r| r.get(0))?;
            let editable = origin == "c" && !partial && !advanced && columns.iter().all(Option::is_some);
            Ok(IndexInfo {
                name,
                unique,
                editable,
                columns: columns.into_iter().flatten().collect(),
            })
        })
        .collect()
}

fn value(value: ValueRef<'_>) -> AppResult<Value> {
    Ok(match value {
        ValueRef::Null => Value::Null,
        ValueRef::Integer(n) => Value::String(n.to_string()),
        ValueRef::Real(n) => Value::String(n.to_string()),
        ValueRef::Text(text) => Value::String(
            std::str::from_utf8(text)
                .map_err(|_| invalid("SQLite text contains invalid UTF-8."))?
                .into(),
        ),
        ValueRef::Blob(bytes) => Value::String(bytes.iter().map(|b| format!("{b:02x}")).collect()),
    })
}

fn page(
    connection: &Connection,
    sql: &str,
    page: u32,
    size: u32,
    metadata: Option<Vec<Column>>,
) -> AppResult<TablePage> {
    let mut statement = connection.prepare(sql)?;
    if !statement.readonly() {
        return Err(invalid("Only read-only queries are supported here."));
    }
    let mut seen = std::collections::HashSet::new();
    let mut columns = statement
        .columns()
        .map(|column| {
            if !seen.insert(column.name().to_owned()) {
                return Err(invalid("Use unique column aliases."));
            }
            Ok(Column {
                name: column.name().into(),
                data_type: column.decl_type().unwrap_or("dynamic").into(),
                nullable: true,
                primary_key: false,
                read_only: true,
                default_value: None,
                definition: None,
            })
        })
        .collect::<AppResult<Vec<_>>>()?;
    let mut cursor = statement.query([])?;
    let mut rows = Vec::new();
    while let Some(row) = cursor.next()? {
        let mut record = Map::new();
        for (i, column) in columns.iter().enumerate() {
            record.insert(column.name.clone(), value(row.get_ref(i)?)?);
        }
        rows.push(record);
    }
    let has_more = rows.len() > size as usize;
    rows.truncate(size as usize);
    if let Some(metadata) = metadata {
        columns = metadata;
    }
    Ok(TablePage {
        columns,
        rows,
        has_more,
        page,
        page_size: size,
        total_rows: None,
        editing: None,
    })
}

fn bound_value(column: &Column, value: Option<&str>) -> AppResult<rusqlite::types::Value> {
    use rusqlite::types::Value as SqlValue;
    let Some(value) = value else {
        return Ok(SqlValue::Null);
    };
    let kind = column.data_type.to_uppercase();
    if kind.contains("BLOB") {
        if !value.len().is_multiple_of(2) || !value.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(invalid("Enter binary values as hexadecimal text."));
        }
        return Ok(SqlValue::Blob(
            (0..value.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&value[i..i + 2], 16).unwrap())
                .collect(),
        ));
    }
    Ok(SqlValue::Text(value.into()))
}

fn mutate(connection: &mut Connection, mutation: &Mutation) -> AppResult<MutationResult> {
    let tx = connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    if let Mutation::Insert { table, .. }
    | Mutation::Update { table, .. }
    | Mutation::Delete { table, .. } = mutation
    {
        let kind: String = tx.query_row(
            "SELECT type FROM main.sqlite_schema WHERE name = ?1",
            [table],
            |r| r.get(0),
        )?;
        if kind != "table" {
            return Err(invalid("Record editing is available for base tables only."));
        }
    }
    let affected_rows = match mutation {
        Mutation::AlterIndex {
            schema,
            table,
            expected,
            index,
        } => {
            target(schema, table)?;
            let available = columns(&tx, table)?
                .into_iter()
                .map(|c| c.name)
                .collect::<Vec<_>>();
            let current = indexes(&tx, table)?;
            if let Some(expected) = expected
                && current.iter().find(|i| i.name == expected.name) != Some(expected)
            {
                return Err(invalid("The index changed. Refresh before editing."));
            }
            if let Some(index) = index {
                super::write::validate_key_columns(&index.columns, &available, "sqlite")?;
                if current.iter().any(|i| {
                    i.name.eq_ignore_ascii_case(&index.name)
                        && !expected.as_ref().is_some_and(|old| old.name == i.name)
                }) {
                    return Err(invalid("An index with this name already exists."));
                }
            }
            for sql in super::write::index_statements(
                "sqlite",
                schema,
                table,
                expected.as_ref(),
                index.as_ref(),
                128,
                quote,
            )? {
                tx.execute(&sql, [])?;
            }
            0
        }
        Mutation::Execute { sql } => {
            let sql = super::write::statement(sql, &SQLiteDialect {})?;
            tx.execute(&sql, [])? as u64
        }
        Mutation::DropTable { schema, table } => {
            tx.execute(&format!("DROP TABLE {}", target(schema, table)?), [])?;
            0
        }
        Mutation::Insert {
            schema,
            table,
            values,
        } => {
            let target = target(schema, table)?;
            let columns = columns(&tx, table)?;
            super::write::values(&columns, values)?;
            let names = values
                .keys()
                .map(|n| quote(n))
                .collect::<Vec<_>>()
                .join(", ");
            let sql = if values.is_empty() {
                format!("INSERT INTO {target} DEFAULT VALUES")
            } else {
                format!(
                    "INSERT INTO {target} ({names}) VALUES ({})",
                    vec!["?"; values.len()].join(", ")
                )
            };
            let bindings = values
                .iter()
                .map(|(name, v)| {
                    bound_value(
                        columns.iter().find(|c| &c.name == name).unwrap(),
                        v.as_deref(),
                    )
                })
                .collect::<AppResult<Vec<_>>>()?;
            tx.execute(&sql, params_from_iter(bindings))? as u64
        }
        Mutation::Update {
            schema,
            table,
            original,
            ..
        }
        | Mutation::Delete {
            schema,
            table,
            original,
        } => {
            let target = target(schema, table)?;
            let columns = columns(&tx, table)?;
            let keys = super::write::keys(&columns, original)?;
            // SQLite permits multiple storage classes even in a primary key.
            // Try the exact representations and refuse ambiguous snapshots.
            let mut conditions = Vec::new();
            let mut bindings = Vec::new();
            for (column, text) in keys {
                use rusqlite::types::Value as SqlValue;
                let mut candidates = vec![SqlValue::Text(text.clone())];
                if let Ok(integer) = text.parse::<i64>() {
                    candidates.push(SqlValue::Integer(integer));
                }
                if let Ok(real) = text.parse::<f64>() {
                    candidates.push(SqlValue::Real(real));
                }
                if text.len().is_multiple_of(2) && text.bytes().all(|b| b.is_ascii_hexdigit()) {
                    candidates.push(SqlValue::Blob(
                        (0..text.len())
                            .step_by(2)
                            .map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap())
                            .collect(),
                    ));
                }
                conditions.push(format!(
                    "({})",
                    vec![format!("{} IS ?", quote(&column.name)); candidates.len()].join(" OR ")
                ));
                bindings.extend(candidates);
            }
            let condition = conditions.join(" AND ");
            let mut statement =
                tx.prepare(&format!("SELECT * FROM {target} WHERE {condition} LIMIT 2"))?;
            let names = statement
                .column_names()
                .map(str::to_owned)
                .collect::<Vec<_>>();
            let mut cursor = statement.query(params_from_iter(&bindings))?;
            let current = if let Some(row) = cursor.next()? {
                let mut record = Map::new();
                for (i, name) in names.iter().enumerate() {
                    record.insert(name.clone(), value(row.get_ref(i)?)?);
                }
                Some(record)
            } else {
                None
            };
            if cursor.next()?.is_some() {
                return Err(invalid(
                    "The primary key does not identify a unique record.",
                ));
            }
            super::write::unchanged(current.as_ref(), original)?;
            drop(cursor);
            drop(statement);
            let affected = if let Mutation::Update { values, .. } = mutation {
                if values.is_empty() {
                    return Err(invalid("Choose a field to update."));
                }
                super::write::values(&columns, values)?;
                let assignments = values
                    .keys()
                    .map(|n| format!("{} = ?", quote(n)))
                    .collect::<Vec<_>>()
                    .join(", ");
                let mut parameters = values
                    .iter()
                    .map(|(name, v)| {
                        bound_value(
                            columns.iter().find(|c| &c.name == name).unwrap(),
                            v.as_deref(),
                        )
                    })
                    .collect::<AppResult<Vec<_>>>()?;
                parameters.extend(bindings);
                tx.execute(
                    &format!("UPDATE {target} SET {assignments} WHERE {condition}"),
                    params_from_iter(parameters),
                )?
            } else {
                tx.execute(
                    &format!("DELETE FROM {target} WHERE {condition}"),
                    params_from_iter(bindings),
                )?
            };
            if affected != 1 {
                return Err(invalid("The record changed. Refresh before editing."));
            }
            affected as u64
        }
        _ => {
            return Err(invalid(
                "This SQLite schema action is unavailable. Use Database changes for supported SQLite DDL; key and type changes may require rebuilding the table. Create or delete database files outside DBsaur.",
            ));
        }
    };
    tx.commit()?;
    Ok(MutationResult { affected_rows })
}

#[async_trait]
impl DatabaseAdapter for SqliteAdapter {
    fn provider(&self) -> Provider {
        Provider {
            id: "sqlite",
            name: "SQLite",
        }
    }
    async fn test(&self, input: &ConnectionInput) -> AppResult<()> {
        run(input, false, |c| {
            c.query_row("PRAGMA main.schema_version", [], |_| Ok(()))?;
            Ok(())
        })
        .await
    }
    async fn tables(&self, input: &ConnectionInput) -> AppResult<Vec<Table>> {
        run(input, false, |c| tables(c)).await
    }
    async fn indexes(
        &self,
        input: &ConnectionInput,
        schema: &str,
        table: &str,
    ) -> AppResult<Vec<IndexInfo>> {
        target(schema, table)?;
        let table = table.to_owned();
        run(input, false, move |c| indexes(c, &table)).await
    }
    async fn rows(&self, input: &ConnectionInput, query: &BrowseQuery) -> AppResult<TablePage> {
        let (number, size) = query.bounds().map_err(invalid)?;
        let target = target(&query.schema, &query.table)?;
        let query = query.clone();
        let read_only = input.read_only;
        run(input, false, move |c| {
            let mut columns = columns(c, &query.table)?;
            let kind: String = c.query_row(
                "SELECT type FROM main.sqlite_schema WHERE name = ?1",
                [&query.table],
                |r| r.get(0),
            )?;
            for column in &mut columns {
                column.read_only |= read_only || kind != "table";
            }
            let order = if let Some(sort) = &query.sort {
                if !columns.iter().any(|c| &c.name == sort) {
                    return Err(invalid("Choose an existing sort column."));
                }
                format!(
                    " ORDER BY {} {}",
                    quote(sort),
                    if query.descending.unwrap_or(false) {
                        "DESC"
                    } else {
                        "ASC"
                    }
                )
            } else {
                let keys = columns
                    .iter()
                    .filter(|c| c.primary_key)
                    .map(|c| quote(&c.name))
                    .collect::<Vec<_>>();
                if keys.is_empty() {
                    String::new()
                } else {
                    format!(" ORDER BY {}", keys.join(", "))
                }
            };
            page(
                c,
                &format!(
                    "SELECT * FROM {target}{order} LIMIT {} OFFSET {}",
                    size + 1,
                    u64::from(number) * u64::from(size)
                ),
                number,
                size,
                Some(columns),
            )
        })
        .await
    }
    async fn query(&self, input: &ConnectionInput, query: &SqlQuery) -> AppResult<TablePage> {
        let (number, size) = query.bounds().map_err(invalid)?;
        let sql = super::read_query::select(query.statement(), &SQLiteDialect {})?;
        run(input, false, move |c| {
            // SQLite auto-renames duplicate names in subqueries; validate the
            // original projection before adding bounded pagination.
            let statement = c.prepare(&sql)?;
            let mut names = std::collections::HashSet::new();
            if statement
                .column_names()
                .any(|name| !names.insert(name.to_owned()))
            {
                return Err(invalid("Use unique column aliases."));
            }
            page(
                c,
                &format!(
                    "SELECT * FROM ({sql}) AS \"dbsaur_query\" LIMIT {} OFFSET {}",
                    size + 1,
                    u64::from(number) * u64::from(size)
                ),
                number,
                size,
                None,
            )
        })
        .await
    }
    async fn mutate(
        &self,
        input: &ConnectionInput,
        mutation: &Mutation,
    ) -> AppResult<MutationResult> {
        let mutation = mutation.clone();
        run(input, true, move |c| mutate(c, &mutation)).await
    }
}
