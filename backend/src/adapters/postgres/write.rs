use super::{connect, quote_identifier};
use crate::adapters::sqlx;
use crate::{application::*, domain::*};
use sqlx::{Connection, Row};

fn identifier(name: &str) -> AppResult<String> {
    if name.is_empty() || name.len() > 63 || name.chars().any(char::is_control) {
        return Err(invalid(
            "Names must contain 1–63 bytes and no control characters.",
        ));
    }
    Ok(quote_identifier(name))
}

fn write_message(code: Option<&str>) -> &'static str {
    match code {
        Some("23503") => {
            "Foreign key violation: existing rows contain values with no matching referenced row, or a referenced row is still in use."
        }
        Some("23505") => "Unique constraint: that key or value already exists.",
        Some("23502") => "Required field: a NOT NULL column needs a value or a default.",
        Some("23514") => "Check constraint: a value does not satisfy the table's rules.",
        Some("23P01") => "Exclusion constraint: this value conflicts with an existing record.",
        Some("42501") => {
            "Permission denied. Ask your database administrator for the required write privileges."
        }
        Some("2BP01") => {
            "Other objects depend on this object. Remove those dependencies explicitly first."
        }
        Some("55006") => "The database is in use. Disconnect its clients before dropping it.",
        Some("42830") => "Foreign key target must be backed by a PRIMARY KEY or UNIQUE constraint.",
        Some("42804") => {
            "Foreign key field types are incompatible. Use matching data types for the source and referenced fields."
        }
        Some("42601") => {
            "The database rejected the generated SQL syntax. Check the selected fields and provider compatibility."
        }
        Some("42809") => "The selected foreign key target is not a referenceable table or field.",
        Some("42710") => "A constraint with that name already exists.",
        Some("42P04" | "42P07") => "An object with that name already exists.",
        Some("42P01" | "42703" | "3D000") => {
            "The database, table, or column no longer exists. Refresh the explorer."
        }
        Some("57014" | "55P03" | "40P01" | "40001") => {
            "The change timed out or conflicted with another transaction. Refresh and try again."
        }
        Some(code) if code.starts_with("22") => {
            "Invalid value for the column type. Check its format, range, and length."
        }
        _ => {
            "The database rejected the change. Check SQL syntax, generated columns, constraints, and permissions."
        }
    }
}

fn write_error(error: sqlx::Error) -> AppError {
    if let sqlx::Error::Database(db) = &error {
        return invalid(write_message(db.code().as_deref()));
    }
    error.into()
}

pub(super) async fn mutate(
    input: &ConnectionInput,
    mutation: &Mutation,
) -> AppResult<MutationResult> {
    // Database DDL cannot run inside a transaction. Never force-disconnect clients.
    if let Mutation::CreateDatabase { name } | Mutation::DropDatabase { name } = mutation {
        if !input.all_databases {
            return Err(invalid(
                "Connect in instance mode to create or drop databases.",
            ));
        }
        let quoted = identifier(name)?;
        let create = matches!(mutation, Mutation::CreateDatabase { .. });
        if !create
            && (name == &input.database
                || matches!(name.as_str(), "postgres" | "template0" | "template1"))
        {
            return Err(invalid(
                "Switch to another database first. System databases cannot be dropped here.",
            ));
        }
        let mut connection = connect(input).await?;
        sqlx::query("SET statement_timeout = '10s'")
            .execute(&mut connection)
            .await?;
        sqlx::query("SET lock_timeout = '2s'")
            .execute(&mut connection)
            .await?;
        let sql = format!(
            "{} DATABASE {quoted}",
            if create { "CREATE" } else { "DROP" }
        );
        sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .execute(&mut connection)
            .await
            .map_err(write_error)?;
        return Ok(MutationResult { affected_rows: 0 });
    }
    let mut connection = connect(input).await?;
    let mut tx = connection.begin().await?;
    sqlx::query("SET LOCAL statement_timeout = '10s'")
        .execute(&mut *tx)
        .await?;
    sqlx::query("SET LOCAL lock_timeout = '2s'")
        .execute(&mut *tx)
        .await?;
    let affected_rows = match mutation {
        Mutation::AlterColumn {
            schema,
            table,
            column,
            expected,
            change,
        } => {
            let target = format!("{}.{}", identifier(schema)?, identifier(table)?);
            let lock = format!("LOCK TABLE {target} IN ACCESS EXCLUSIVE MODE");
            sqlx::query(sqlx::AssertSqlSafe(lock.as_str()))
                .execute(&mut *tx)
                .await
                .map_err(write_error)?;
            let current = super::schema_column(&mut tx, schema, table, column)
                .await
                .map_err(|_| {
                    invalid("Column metadata could not be loaded. Refresh the explorer.")
                })?;
            let plan = super::super::column_change::plan(
                "postgres", schema, table, column, &current, expected, change,
            )?;
            for statement in plan.statements {
                sqlx::query(sqlx::AssertSqlSafe(statement.as_str()))
                    .execute(&mut *tx)
                    .await
                    .map_err(write_error)?;
            }
            0
        }
        Mutation::Execute { sql } => {
            if sql.trim().is_empty() || sql.len() > 12_000 {
                return Err(invalid("Enter one SQL statement of at most 12,000 bytes."));
            }
            // The extended query protocol permits one statement only. Restrict command
            // families so transaction control cannot bypass this transaction boundary.
            let command = sql
                .split_whitespace()
                .next()
                .unwrap_or("")
                .to_ascii_uppercase();
            if !matches!(
                command.as_str(),
                "CREATE" | "ALTER" | "DROP" | "TRUNCATE" | "INSERT" | "UPDATE" | "DELETE"
            ) {
                return Err(invalid(
                    "Use CREATE, ALTER, DROP, TRUNCATE, INSERT, UPDATE, or DELETE. Run SELECT in the read-only query editor.",
                ));
            }
            sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
                .execute(&mut *tx)
                .await
                .map_err(write_error)?
                .rows_affected()
        }
        Mutation::DropTable { schema, table } => {
            let sql = format!(
                "DROP TABLE {}.{} RESTRICT",
                identifier(schema)?,
                identifier(table)?
            );
            sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
                .execute(&mut *tx)
                .await
                .map_err(write_error)?
                .rows_affected()
        }
        Mutation::AlterTableKeys {
            schema,
            table,
            drop_primary_key,
            drop_unique_keys,
            add_primary_key,
            add_unique_keys,
        } => {
            let qualified = format!("{}.{}", identifier(schema)?, identifier(table)?);
            let lock = format!("LOCK TABLE {qualified} IN ACCESS EXCLUSIVE MODE");
            sqlx::query(sqlx::AssertSqlSafe(lock.as_str()))
                .execute(&mut *tx)
                .await
                .map_err(write_error)?;
            let rows = sqlx::query("SELECT c.column_name FROM information_schema.columns c JOIN information_schema.tables t ON t.table_schema=c.table_schema AND t.table_name=c.table_name WHERE c.table_schema=$1 AND c.table_name=$2 AND t.table_type='BASE TABLE' ORDER BY c.ordinal_position")
                .bind(schema).bind(table).fetch_all(&mut *tx).await.map_err(write_error)?;
            let available = rows
                .iter()
                .map(|row| row.get::<String, _>("column_name"))
                .collect::<Vec<_>>();
            if available.is_empty() {
                return Err(invalid(
                    "Choose an accessible base table. Views remain read only.",
                ));
            }
            let key_rows = sqlx::query("SELECT constraint_name,constraint_type FROM information_schema.table_constraints WHERE table_schema=$1 AND table_name=$2 AND constraint_type IN ('PRIMARY KEY','UNIQUE')")
                .bind(schema).bind(table).fetch_all(&mut *tx).await.map_err(write_error)?;
            let current_keys = key_rows
                .iter()
                .map(|row| KeyConstraint {
                    name: row.get("constraint_name"),
                    kind: row.get("constraint_type"),
                    columns: vec![],
                })
                .collect::<Vec<_>>();
            super::super::write::validate_key_drops(
                drop_primary_key.as_deref(),
                drop_unique_keys,
                &current_keys,
            )?;
            let mut requested = add_primary_key.clone();
            requested.extend(add_unique_keys.iter().flatten().cloned());
            super::super::write::validate_key_columns(&requested, &available, "postgres")?;
            let statements = super::super::write::alter_table_key_statements(
                super::super::write::TableKeyChange {
                    provider: "postgres",
                    schema,
                    table,
                    drop_primary_key: drop_primary_key.as_deref(),
                    drop_unique_keys,
                    add_primary_key,
                    add_unique_keys,
                },
                63,
                quote_identifier,
            )?;
            for statement in statements {
                sqlx::query(sqlx::AssertSqlSafe(statement.as_str()))
                    .execute(&mut *tx)
                    .await
                    .map_err(write_error)?;
            }
            0
        }
        Mutation::Insert { schema, table, .. }
        | Mutation::Update { schema, table, .. }
        | Mutation::Delete { schema, table, .. } => {
            let qualified = format!("{}.{}", identifier(schema)?, identifier(table)?);
            // Lock the relation before reading metadata so concurrent DDL cannot change
            // the meaning of a column between validation and execution.
            let lock = format!("LOCK TABLE {qualified} IN ROW EXCLUSIVE MODE");
            sqlx::query(sqlx::AssertSqlSafe(lock.as_str()))
                .execute(&mut *tx)
                .await
                .map_err(write_error)?;
            let metadata = sqlx::query("SELECT a.attname, pg_catalog.format_type(a.atttypid, NULL) AS cast_type, a.attgenerated::text AS generated, a.attidentity::text AS identity, EXISTS(SELECT 1 FROM pg_catalog.pg_index i WHERE i.indrelid=c.oid AND i.indisprimary AND a.attnum=ANY(i.indkey)) AS primary_key, (SELECT CASE WHEN udt_name IN ('geometry', 'geography') THEN udt_name ELSE data_type END FROM information_schema.columns WHERE table_schema=n.nspname AND table_name=c.relname AND column_name=a.attname) AS data_type FROM pg_catalog.pg_attribute a JOIN pg_catalog.pg_class c ON c.oid=a.attrelid JOIN pg_catalog.pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname=$1 AND c.relname=$2 AND c.relkind IN ('r','p') AND a.attnum>0 AND NOT a.attisdropped ORDER BY a.attnum")
                .bind(schema).bind(table).fetch_all(&mut *tx).await.map_err(write_error)?;
            if metadata.is_empty() {
                return Err(invalid("Choose a base table with accessible columns."));
            }
            let values = match mutation {
                Mutation::Insert { values, .. } | Mutation::Update { values, .. } => Some(values),
                _ => None,
            };
            if let Some(values) = values {
                for name in values.keys() {
                    let column = metadata
                        .iter()
                        .find(|r| r.get::<String, _>("attname") == *name)
                        .ok_or_else(|| invalid("Unknown column. Refresh the table."))?;
                    if !column.get::<String, _>("generated").is_empty()
                        || column.get::<String, _>("identity") == "a"
                    {
                        return Err(invalid(
                            "Generated columns and ALWAYS identity columns are maintained by the database. Omit them.",
                        ));
                    }
                }
            }
            let mut location = None;
            if let Mutation::Update { original, .. } | Mutation::Delete { original, .. } = mutation
            {
                if original.len() != metadata.len()
                    || metadata
                        .iter()
                        .any(|r| !original.contains_key(&r.get::<String, _>("attname")))
                {
                    return Err(invalid(
                        "The table structure changed. Refresh before editing.",
                    ));
                }
                let projection = metadata
                    .iter()
                    .map(|r| {
                        let name = quote_identifier(&r.get::<String, _>("attname"));
                        let typ: String = r.get("data_type");
                        if matches!(
                            typ.as_str(),
                            "bigint"
                                | "numeric"
                                | "decimal"
                                | "json"
                                | "jsonb"
                                | "ARRAY"
                                | "geometry"
                                | "geography"
                        ) {
                            format!("source.{name}::text AS {name}")
                        } else {
                            format!("source.{name}")
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                // Use native primary-key equality for index lookup, while still
                // comparing the complete snapshot to detect lost updates.
                let mut predicates = vec![format!(
                    "(SELECT to_jsonb(record) FROM (SELECT {projection}) record) = $1"
                )];
                let mut keys = Vec::new();
                for column in metadata.iter().filter(|r| r.get::<bool, _>("primary_key")) {
                    let name: String = column.get("attname");
                    let value = &original[&name];
                    if value.is_null() || value.is_object() || value.is_array() {
                        continue;
                    }
                    keys.push(
                        value
                            .as_str()
                            .map(str::to_owned)
                            .unwrap_or_else(|| value.to_string()),
                    );
                    predicates.push(format!(
                        "source.{} = ${}::text::{}",
                        quote_identifier(&name),
                        keys.len() + 1,
                        column.get::<String, _>("cast_type")
                    ));
                }
                let sql = format!(
                    "SELECT source.tableoid::text AS relation, source.ctid::text AS position FROM {qualified} AS source WHERE {} LIMIT 2 FOR UPDATE OF source",
                    predicates.join(" AND ")
                );
                let mut select = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
                    .bind(serde_json::Value::Object(original.clone()));
                for key in keys {
                    select = select.bind(key);
                }
                let matches = select.fetch_all(&mut *tx).await.map_err(write_error)?;
                if matches.len() != 1 {
                    return Err(invalid(
                        "The record changed, was deleted, or is not unique. Refresh before editing; no changes were saved.",
                    ));
                }
                location = Some((
                    matches[0].get::<String, _>("relation"),
                    matches[0].get::<String, _>("position"),
                ));
            }
            let mut params: Vec<Option<String>> = Vec::new();
            let sql = match mutation {
                Mutation::Insert { values, .. } | Mutation::Update { values, .. } => {
                    let mut names = Vec::new();
                    let mut casts = Vec::new();
                    for (name, value) in values {
                        let column = metadata
                            .iter()
                            .find(|r| r.get::<String, _>("attname") == *name)
                            .unwrap();
                        names.push(quote_identifier(name));
                        params.push(value.clone());
                        // format_type is PostgreSQL-generated, identifier-quoted type SQL.
                        casts.push(format!(
                            "${}::text::{}",
                            params.len(),
                            column.get::<String, _>("cast_type")
                        ));
                    }
                    if matches!(mutation, Mutation::Insert { .. }) {
                        if names.is_empty() {
                            format!("INSERT INTO {qualified} DEFAULT VALUES")
                        } else {
                            format!(
                                "INSERT INTO {qualified} ({}) VALUES ({})",
                                names.join(", "),
                                casts.join(", ")
                            )
                        }
                    } else {
                        if names.is_empty() {
                            return Err(invalid("Change at least one field."));
                        }
                        let assignments = names
                            .iter()
                            .zip(casts)
                            .map(|(name, cast)| format!("{name} = {cast}"))
                            .collect::<Vec<_>>()
                            .join(", ");
                        format!("UPDATE {qualified} SET {assignments}")
                    }
                }
                _ => format!("DELETE FROM {qualified}"),
            };
            let sql = if let Some((relation, position)) = location {
                params.push(Some(relation));
                params.push(Some(position));
                format!(
                    "{sql} WHERE tableoid = ${}::text::oid AND ctid = ${}::text::tid",
                    params.len() - 1,
                    params.len()
                )
            } else {
                sql
            };
            let mut query = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()));
            for param in params {
                query = query.bind(param);
            }
            let count = query
                .execute(&mut *tx)
                .await
                .map_err(write_error)?
                .rows_affected();
            if count != 1 {
                return Err(invalid(
                    "The change did not affect exactly one record. No changes were saved.",
                ));
            }
            count
        }
        _ => unreachable!(),
    };
    tx.commit().await.map_err(write_error)?;
    Ok(MutationResult { affected_rows })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn foreign_key_errors_explain_what_needs_to_change() {
        assert!(write_message(Some("42830")).contains("PRIMARY KEY or UNIQUE"));
        assert!(write_message(Some("42804")).contains("types are incompatible"));
        assert!(write_message(Some("23503")).contains("existing rows"));
        assert!(write_message(Some("unknown")).contains("database rejected the change"));
    }

    #[test]
    fn identifiers_are_validated_and_quoted() {
        assert_eq!(identifier("a\";--").unwrap(), "\"a\"\";--\"");
        for name in ["", "a\0b", &"x".repeat(64)] {
            assert!(identifier(name).is_err());
        }
    }
}
