use super::{connect_mode, metadata, projection, quote_identifier};
use crate::adapters::sqlx;
use crate::{application::*, domain::*};
use sqlx::{Connection, MySqlConnection, Row};

fn identifier(name: &str) -> AppResult<String> {
    super::super::write::identifier(name, 64, quote_identifier)
}
fn write_message(number: Option<u16>) -> &'static str {
    match number {
        Some(1062) => "Unique constraint: that key or value already exists.",
        Some(1451 | 1217) => {
            "Foreign key violation: another row still references the record being changed."
        }
        Some(1452 | 1216) => {
            "Foreign key violation: existing rows contain values with no matching referenced row."
        }
        Some(1822) => {
            "Foreign key target must have an index, typically a PRIMARY KEY or UNIQUE constraint."
        }
        Some(3780) => {
            "Foreign key field types are incompatible. Use matching data types, sizes, and collations."
        }
        Some(1215 | 1005) => {
            "Could not create the foreign key. Check that both fields have compatible types and indexes, and that both tables use a supported storage engine."
        }
        Some(1826) => "A foreign key constraint with that name already exists.",
        Some(1072) => "The selected foreign key field does not exist.",
        Some(1048 | 1364) => "Required field: provide a non-NULL value or a database default.",
        Some(3819 | 4025) => "Check constraint: a value does not satisfy the table rules.",
        Some(1205 | 1213) => {
            "The change timed out or conflicted with another transaction. Refresh and try again."
        }
        Some(1044 | 1045 | 1142 | 1143) => {
            "Permission denied. Ask for the required write privileges."
        }
        Some(1264 | 1265 | 1292 | 1366 | 1406) => {
            "Invalid value for the column type. Check its format, range, and length."
        }
        _ => {
            "The database rejected the change. Check SQL syntax, generated columns, constraints, and permissions."
        }
    }
}
fn write_error(error: sqlx::Error) -> AppError {
    if let sqlx::Error::Database(db) = &error {
        let number = db
            .try_downcast_ref::<sqlx::mysql::MySqlDatabaseError>()
            .map(|error| error.number());
        return invalid(write_message(number));
    }
    invalid("Database change failed. Check the connection before retrying.")
}
fn binary(column: &Column) -> bool {
    let typ = column.data_type.to_lowercase();
    typ.contains("binary") || typ.contains("blob")
}
fn parameter(column: &Column, value: &Option<String>) -> AppResult<&'static str> {
    let bit = column.data_type.to_lowercase().starts_with("bit(");
    if (binary(column) || bit)
        && value.as_ref().is_some_and(|s| {
            (binary(column) && s.len() % 2 != 0)
                || (bit && (s.is_empty() || s.len() > 16))
                || !s.bytes().all(|b| b.is_ascii_hexdigit())
        })
    {
        return Err(invalid(
            "Use hexadecimal text without a 0x prefix. Binary values need complete byte pairs; BIT values need 1-16 hex digits.",
        ));
    }
    Ok(if binary(column) {
        "UNHEX(?)"
    } else if bit {
        "CAST(CONV(?, 16, 10) AS UNSIGNED)"
    } else {
        "?"
    })
}
// Avoid MySQL's floating-point coercion when comparing a text-bound key
// against an integer or DECIMAL column (especially values above 2^53).
fn key_parameter(column: &Column, value: &Option<String>) -> AppResult<String> {
    let typ = column.data_type.to_lowercase();
    let base = typ.split(['(', ' ']).next().unwrap_or("");
    if matches!(
        base,
        "tinyint" | "smallint" | "mediumint" | "int" | "integer" | "bigint"
    ) {
        return Ok(if typ.contains("unsigned") {
            "CAST(? AS UNSIGNED)"
        } else {
            "CAST(? AS SIGNED)"
        }
        .into());
    }
    if matches!(base, "decimal" | "numeric") {
        let numbers = typ
            .split_once('(')
            .and_then(|(_, rest)| rest.split_once(')'))
            .map(|(numbers, _)| numbers)
            .ok_or_else(|| invalid("Unsupported numeric key type."))?;
        let parts = numbers
            .split(',')
            .map(str::parse::<u8>)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| invalid("Unsupported numeric key type."))?;
        if parts.len() != 2
            || parts[0] == 0
            || parts[0] > 65
            || parts[1] > 30
            || parts[1] > parts[0]
        {
            return Err(invalid("Unsupported numeric key type."));
        }
        return Ok(format!("CAST(? AS DECIMAL({},{}))", parts[0], parts[1]));
    }
    Ok(parameter(column, value)?.into())
}
async fn execute(connection: &mut MySqlConnection, sql: &str) -> AppResult<u64> {
    Ok(sqlx::query(sqlx::AssertSqlSafe(sql))
        .execute(connection)
        .await
        .map_err(write_error)?
        .rows_affected())
}
pub(crate) async fn mutate(
    input: &ConnectionInput,
    mutation: &Mutation,
) -> AppResult<MutationResult> {
    super::super::write::check_index(&super::MySqlAdapter, input, mutation).await?;
    if let Mutation::AlterColumn {
        schema,
        table,
        column,
        expected,
        change,
    } = mutation
    {
        if schema != &input.database {
            return Err(invalid("Choose a table in the active database."));
        }
        let mut connection = connect_mode(input, false).await?;
        let base_table: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM information_schema.TABLES WHERE TABLE_SCHEMA=? AND TABLE_NAME=? AND TABLE_TYPE='BASE TABLE'").bind(schema).bind(table).fetch_one(&mut connection).await.map_err(write_error)?;
        if base_table != 1 {
            return Err(invalid(
                "Choose an accessible base table. Views remain read only.",
            ));
        }
        let columns = metadata(&mut connection, schema, table, &input.provider)
            .await
            .map_err(|_| invalid("Column metadata could not be loaded. Refresh the explorer."))?;
        let current = columns
            .iter()
            .find(|item| &item.name == column)
            .and_then(|item| item.definition.as_ref())
            .ok_or_else(|| invalid("The column no longer exists. Refresh the explorer."))?;
        let plan = super::super::column_change::plan(
            &input.provider,
            schema,
            table,
            column,
            current,
            expected,
            change,
        )?;
        for statement in plan.statements {
            execute(&mut connection, &statement).await?;
        }
        return Ok(MutationResult { affected_rows: 0 });
    }
    if let Mutation::CreateDatabase { name } | Mutation::DropDatabase { name } = mutation {
        if !input.all_databases {
            return Err(invalid(
                "Connect in instance mode to create or drop databases.",
            ));
        }
        let quoted = identifier(name)?;
        if matches!(mutation, Mutation::DropDatabase { .. })
            && (name.eq_ignore_ascii_case(&input.database)
                || ["mysql", "information_schema", "performance_schema", "sys"]
                    .iter()
                    .any(|s| name.eq_ignore_ascii_case(s)))
        {
            return Err(invalid(
                "Switch to another database first. System databases cannot be dropped here.",
            ));
        }
        let mut connection = connect_mode(input, false).await?;
        let sql = format!(
            "{} DATABASE {quoted}",
            if matches!(mutation, Mutation::CreateDatabase { .. }) {
                "CREATE"
            } else {
                "DROP"
            }
        );
        execute(&mut connection, &sql).await?;
        return Ok(MutationResult { affected_rows: 0 });
    }
    if let Mutation::AlterIndex {
        schema,
        table,
        expected,
        index,
    } = mutation
    {
        let statements = super::super::write::index_statements(
            &input.provider,
            schema,
            table,
            expected.as_ref(),
            index.as_ref(),
            64,
            quote_identifier,
        )?;
        let mut connection = connect_mode(input, false).await?;
        for sql in statements {
            execute(&mut connection, &sql).await?;
        }
        return Ok(MutationResult { affected_rows: 0 });
    }
    if let Mutation::Execute { sql } = mutation {
        let sql = super::super::write::statement(sql, &sqlparser::dialect::MySqlDialect {})?;
        // MySQL/MariaDB DDL implicitly commits. Execute exactly once, without
        // claiming that a surrounding transaction could roll it back.
        let mut connection = connect_mode(input, false).await?;
        return Ok(MutationResult {
            affected_rows: execute(&mut connection, &sql).await?,
        });
    }
    if let Mutation::AlterTableKeys {
        schema,
        table,
        drop_primary_key,
        drop_unique_keys,
        add_primary_key,
        add_unique_keys,
    } = mutation
    {
        if schema != &input.database {
            return Err(invalid("Choose a table in the active database."));
        }
        let mut connection = connect_mode(input, false).await?;
        let rows = sqlx::query("SELECT c.COLUMN_NAME FROM information_schema.COLUMNS c JOIN information_schema.TABLES t ON t.TABLE_SCHEMA=c.TABLE_SCHEMA AND t.TABLE_NAME=c.TABLE_NAME WHERE c.TABLE_SCHEMA=? AND c.TABLE_NAME=? AND t.TABLE_TYPE='BASE TABLE' ORDER BY c.ORDINAL_POSITION")
            .bind(schema).bind(table).fetch_all(&mut connection).await.map_err(write_error)?;
        let available = rows
            .iter()
            .map(|row| row.get::<String, _>("COLUMN_NAME"))
            .collect::<Vec<_>>();
        if available.is_empty() {
            return Err(invalid(
                "Choose an accessible base table. Views remain read only.",
            ));
        }
        let key_rows = sqlx::query("SELECT CONSTRAINT_NAME,CONSTRAINT_TYPE FROM information_schema.TABLE_CONSTRAINTS WHERE TABLE_SCHEMA=? AND TABLE_NAME=? AND CONSTRAINT_TYPE IN ('PRIMARY KEY','UNIQUE')")
            .bind(schema).bind(table).fetch_all(&mut connection).await.map_err(write_error)?;
        let current_keys = key_rows
            .iter()
            .map(|row| {
                Ok(KeyConstraint {
                    name: row.try_get("CONSTRAINT_NAME").map_err(write_error)?,
                    kind: row.try_get("CONSTRAINT_TYPE").map_err(write_error)?,
                    columns: vec![],
                })
            })
            .collect::<AppResult<Vec<_>>>()?;
        super::super::write::validate_key_drops(
            drop_primary_key.as_deref(),
            drop_unique_keys,
            &current_keys,
        )?;
        let mut requested = add_primary_key.clone();
        requested.extend(add_unique_keys.iter().flatten().cloned());
        super::super::write::validate_key_columns(&requested, &available, &input.provider)?;
        let statements = super::super::write::alter_table_key_statements(
            super::super::write::TableKeyChange {
                provider: &input.provider,
                schema,
                table,
                drop_primary_key: drop_primary_key.as_deref(),
                drop_unique_keys,
                add_primary_key,
                add_unique_keys,
            },
            64,
            quote_identifier,
        )?;
        let statement = statements
            .first()
            .ok_or_else(|| invalid("No key changes were generated."))?;
        return Ok(MutationResult {
            affected_rows: execute(&mut connection, statement).await?,
        });
    }
    let (schema, table) = match mutation {
        Mutation::Insert { schema, table, .. }
        | Mutation::Update { schema, table, .. }
        | Mutation::Delete { schema, table, .. }
        | Mutation::DropTable { schema, table } => (schema, table),
        _ => unreachable!(),
    };
    if schema != &input.database {
        return Err(invalid("Choose a table in the active database."));
    }
    let qualified = format!("{}.{}", identifier(schema)?, identifier(table)?);
    let mut connection = connect_mode(input, false).await?;
    if matches!(mutation, Mutation::DropTable { .. }) {
        execute(&mut connection, &format!("DROP TABLE {qualified}")).await?;
        return Ok(MutationResult { affected_rows: 0 });
    }
    let mut tx = connection.begin().await.map_err(write_error)?;
    // Hold a metadata lock through the transaction before inspecting columns/engine.
    execute(&mut tx, &format!("SELECT * FROM {qualified} LIMIT 0")).await?;
    let engine: Option<String> = sqlx::query_scalar("SELECT ENGINE FROM information_schema.TABLES WHERE TABLE_SCHEMA=? AND TABLE_NAME=? AND TABLE_TYPE='BASE TABLE'").bind(schema).bind(table).fetch_optional(&mut *tx).await.map_err(write_error)?.flatten();
    if !engine.is_some_and(|engine| engine.eq_ignore_ascii_case("InnoDB")) {
        return Err(invalid(
            "Record editing requires an InnoDB base table so failed changes can roll back.",
        ));
    }
    let columns = metadata(&mut tx, schema, table, &input.provider).await?;
    let mut params: Vec<Option<String>> = Vec::new();
    let mut where_sql = String::new();
    if let Mutation::Update { original, .. } | Mutation::Delete { original, .. } = mutation {
        let keys = super::super::write::keys(&columns, original)?;
        let mut predicates = Vec::new();
        for (column, value) in &keys {
            let value = Some(value.clone());
            predicates.push(format!(
                "source.{} <=> {}",
                quote_identifier(&column.name),
                key_parameter(column, &value)?
            ));
            params.push(value);
        }
        where_sql = format!(" WHERE {}", predicates.join(" AND "));
        let sql = format!(
            "SELECT {} FROM {qualified} AS source{where_sql} LIMIT 2 FOR UPDATE",
            projection(&columns)
        );
        let mut query = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()));
        for param in &params {
            query = query.bind(param);
        }
        let records = query.fetch_all(&mut *tx).await.map_err(write_error)?;
        let current = records
            .first()
            .map(|row| {
                columns
                    .iter()
                    .enumerate()
                    .map(|(i, c)| {
                        Ok((
                            c.name.clone(),
                            row.try_get::<Option<String>, _>(i)?
                                .map_or(serde_json::Value::Null, serde_json::Value::String),
                        ))
                    })
                    .collect::<Result<serde_json::Map<_, _>, sqlx::Error>>()
            })
            .transpose()?;
        if records.len() != 1 {
            return Err(invalid(
                "The record changed or was deleted. Refresh before editing.",
            ));
        }
        super::super::write::unchanged(current.as_ref(), original)?;
    }
    let mut assignments = Vec::new();
    let mut names = Vec::new();
    let mut expressions = Vec::new();
    let mut value_params = Vec::new();
    if let Mutation::Insert { values, .. } | Mutation::Update { values, .. } = mutation {
        super::super::write::values(&columns, values)?;
        for (name, value) in values {
            let column = columns.iter().find(|c| &c.name == name).unwrap();
            let expression = parameter(column, value)?;
            names.push(quote_identifier(name));
            expressions.push(expression);
            assignments.push(format!("{} = {expression}", quote_identifier(name)));
            value_params.push(value.clone());
        }
    }
    let sql = match mutation {
        Mutation::Insert { .. } => format!(
            "INSERT INTO {qualified} ({}) VALUES ({})",
            names.join(", "),
            expressions.join(", ")
        ),
        Mutation::Update { .. } => {
            if assignments.is_empty() {
                return Err(invalid("Change at least one field."));
            }
            format!(
                "UPDATE {qualified} AS source SET {}{where_sql}",
                assignments.join(", ")
            )
        }
        _ => format!("DELETE source FROM {qualified} AS source{where_sql}"),
    };
    value_params.extend(params);
    let mut query = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()));
    for param in value_params {
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
    tx.commit().await.map_err(write_error)?;
    Ok(MutationResult {
        affected_rows: count,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn foreign_key_errors_explain_what_needs_to_change() {
        assert!(write_message(Some(1822)).contains("PRIMARY KEY or UNIQUE"));
        assert!(write_message(Some(3780)).contains("types are incompatible"));
        assert!(write_message(Some(1452)).contains("existing rows"));
        assert!(write_message(Some(65535)).contains("database rejected the change"));
    }

    #[test]
    fn numeric_keys_keep_the_native_precision() {
        let mut c = Column {
            name: "id".into(),
            data_type: "bigint unsigned".into(),
            nullable: false,
            primary_key: true,
            read_only: false,
            default_value: None,
            definition: None,
        };
        assert_eq!(
            key_parameter(&c, &Some("18446744073709551615".into())).unwrap(),
            "CAST(? AS UNSIGNED)"
        );
        c.data_type = "bigint".into();
        assert_eq!(
            key_parameter(&c, &Some("9007199254740993".into())).unwrap(),
            "CAST(? AS SIGNED)"
        );
        c.data_type = "decimal(65,30)".into();
        assert_eq!(
            key_parameter(&c, &Some("1.1".into())).unwrap(),
            "CAST(? AS DECIMAL(65,30))"
        );
        c.data_type = "decimal(65,30); DELETE FROM t".into();
        // The cast uses validated numeric components, never the source type SQL.
        assert_eq!(
            key_parameter(&c, &Some("1.1".into())).unwrap(),
            "CAST(? AS DECIMAL(65,30))"
        );
        c.data_type = "decimal(65,30; DELETE FROM t)".into();
        assert!(key_parameter(&c, &None).is_err());
    }
    #[test]
    fn binary_values_cannot_be_interpreted_as_sql_or_silently_corrupted() {
        let column = Column {
            name: "value".into(),
            data_type: "varbinary(30)".into(),
            nullable: true,
            primary_key: false,
            read_only: false,
            default_value: None,
            definition: None,
        };
        assert_eq!(
            parameter(&column, &Some("00aaff".into())).unwrap(),
            "UNHEX(?)"
        );
        for value in ["0x00", "a", "zz", "'; DROP TABLE t"] {
            assert!(parameter(&column, &Some(value.into())).is_err());
        }
        assert!(parameter(&column, &None).is_ok());
    }
}
