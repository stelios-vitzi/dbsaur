use super::{SqlClient, connect_mode, driver_error, metadata, projection, quote_identifier, text};
use crate::{application::*, domain::*};
use tiberius::Query;

fn identifier(name: &str) -> AppResult<String> {
    super::super::write::identifier(name, 128, quote_identifier)
}
fn write_message(code: Option<u32>) -> &'static str {
    match code {
        Some(2601 | 2627) => "Unique constraint: that key or value already exists.",
        Some(547) => {
            "Foreign key or CHECK constraint violation: existing values conflict with the requested change or referenced data."
        }
        Some(1776) => "Foreign key target must be backed by a PRIMARY KEY or UNIQUE constraint.",
        Some(1770) => "The selected referenced field does not exist.",
        Some(1778) => {
            "Foreign key field types are incompatible. Use matching data types for the source and referenced fields."
        }
        Some(515) => "Required field: provide a non-NULL value or a database default.",
        Some(245 | 248 | 8114 | 8115 | 8152 | 2628 | 241 | 242) => {
            "Invalid value for the column type. Check its format, range, and length."
        }
        Some(229 | 230 | 262) => "Permission denied. Ask for the required write privileges.",
        Some(1205 | 1222) => {
            "The change timed out or conflicted with another transaction. Refresh and try again."
        }
        _ => {
            "SQL Server rejected the change. Check SQL syntax, generated columns, constraints, permissions, and the connection before retrying."
        }
    }
}
fn write_error(error: tiberius::error::Error) -> AppError {
    invalid(write_message(error.code()))
}
async fn command(client: &mut SqlClient, sql: &str) -> AppResult<()> {
    client
        .simple_query(sql)
        .await
        .map_err(write_error)?
        .into_results()
        .await
        .map_err(write_error)?;
    Ok(())
}
fn parameter(column: &Column, value: &Option<String>, index: usize) -> AppResult<String> {
    let binary = column.data_type.contains("binary") || column.data_type == "image";
    if binary
        && value
            .as_ref()
            .is_some_and(|s| s.len() % 2 != 0 || !s.bytes().all(|b| b.is_ascii_hexdigit()))
    {
        return Err(invalid(
            "Binary values use an even number of hexadecimal digits without a 0x prefix.",
        ));
    }
    Ok(if binary {
        format!("CONVERT(varbinary(max), @P{index}, 2)")
    } else {
        format!("@P{index}")
    })
}
async fn counted(
    client: &mut SqlClient,
    sql: String,
    params: Vec<Option<String>>,
) -> AppResult<u64> {
    // SELECT @@ROWCOUNT reports the target statement rather than summing trigger counts.
    let mut query = Query::new(format!(
        "{sql}; SELECT CONVERT(bigint, @@ROWCOUNT) AS webdb_affected"
    ));
    for param in params {
        query.bind(param);
    }
    let mut results = query
        .query(client)
        .await
        .map_err(write_error)?
        .into_results()
        .await
        .map_err(write_error)?;
    let count = results
        .pop()
        .and_then(|r| r.into_iter().next())
        .ok_or_else(|| invalid("The database did not return a change count."))?
        .try_get::<i64, _>("webdb_affected")
        .map_err(driver_error)?
        .ok_or_else(|| invalid("The database did not return a change count."))?;
    u64::try_from(count).map_err(|_| invalid("Invalid database change count."))
}
pub(super) async fn mutate(
    input: &ConnectionInput,
    mutation: &Mutation,
) -> AppResult<MutationResult> {
    if let Mutation::CreateDatabase { name } | Mutation::DropDatabase { name } = mutation {
        if !input.all_databases {
            return Err(invalid(
                "Connect in instance mode to create or drop databases.",
            ));
        }
        let quoted = identifier(name)?;
        if matches!(mutation, Mutation::DropDatabase { .. })
            && (name.eq_ignore_ascii_case(&input.database)
                || ["master", "model", "msdb", "tempdb", "distribution"]
                    .iter()
                    .any(|s| name.eq_ignore_ascii_case(s)))
        {
            return Err(invalid(
                "Switch to another database first. System databases cannot be dropped here.",
            ));
        }
        let mut client = connect_mode(input, false).await?;
        command(
            &mut client,
            &format!(
                "{} DATABASE {quoted}",
                if matches!(mutation, Mutation::CreateDatabase { .. }) {
                    "CREATE"
                } else {
                    "DROP"
                }
            ),
        )
        .await?;
        return Ok(MutationResult { affected_rows: 0 });
    }
    // Validate user-authored SQL before opening a write connection.
    let statement = if let Mutation::Execute { sql } = mutation {
        Some(super::super::write::statement(
            sql,
            &sqlparser::dialect::MsSqlDialect {},
        )?)
    } else {
        None
    };
    let mut client = connect_mode(input, false).await?;
    command(
        &mut client,
        "SET ANSI_WARNINGS ON; SET ARITHABORT ON; SET XACT_ABORT ON; BEGIN TRANSACTION",
    )
    .await?;
    let result = apply(&mut client, mutation, statement).await;
    match result {
        Ok(affected_rows) => {
            command(&mut client, "COMMIT TRANSACTION").await?;
            Ok(MutationResult { affected_rows })
        }
        Err(error) => {
            let _ = command(&mut client, "IF @@TRANCOUNT > 0 ROLLBACK TRANSACTION").await;
            Err(error)
        }
    }
}
async fn apply(
    client: &mut SqlClient,
    mutation: &Mutation,
    statement: Option<String>,
) -> AppResult<u64> {
    if let Mutation::AlterColumn {
        schema,
        table,
        column,
        expected,
        change,
    } = mutation
    {
        let target = format!("{}.{}", identifier(schema)?, identifier(table)?);
        command(
            client,
            &format!("SELECT TOP (1) 1 AS dbsaur_lock FROM {target} WITH (TABLOCKX,HOLDLOCK)"),
        )
        .await?;
        let base_table = client.query("SELECT t.name FROM sys.tables t JOIN sys.schemas s ON s.schema_id=t.schema_id WHERE s.name=@P1 AND t.name=@P2", &[&schema.as_str(),&table.as_str()]).await.map_err(write_error)?.into_first_result().await.map_err(write_error)?;
        if base_table.len() != 1 {
            return Err(invalid(
                "Choose an accessible base table. Views remain read only.",
            ));
        }
        let columns = metadata(client, schema, table).await?;
        let current = columns
            .iter()
            .find(|item| &item.name == column)
            .and_then(|item| item.definition.as_ref())
            .ok_or_else(|| invalid("The column no longer exists. Refresh the explorer."))?;
        let plan = super::super::column_change::plan(
            "sqlserver",
            schema,
            table,
            column,
            current,
            expected,
            change,
        )?;
        for statement in plan.statements {
            command(client, &statement).await?;
        }
        if let Some(name) = plan.rename {
            let object = format!("{target}.{}", identifier(column)?);
            client
                .execute(
                    "EXEC sys.sp_rename @objname=@P1, @newname=@P2, @objtype=N'COLUMN'",
                    &[&object.as_str(), &name.as_str()],
                )
                .await
                .map_err(write_error)?;
        }
        return Ok(0);
    }
    if let Some(sql) = statement {
        return counted(client, sql, vec![]).await;
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
        let rows = client.query("SELECT t.TABLE_TYPE,c.COLUMN_NAME FROM INFORMATION_SCHEMA.TABLES t JOIN INFORMATION_SCHEMA.COLUMNS c ON c.TABLE_SCHEMA=t.TABLE_SCHEMA AND c.TABLE_NAME=t.TABLE_NAME WHERE t.TABLE_SCHEMA=@P1 AND t.TABLE_NAME=@P2 ORDER BY c.ORDINAL_POSITION", &[&schema.as_str(), &table.as_str()]).await.map_err(write_error)?.into_first_result().await.map_err(write_error)?;
        if rows.is_empty()
            || rows
                .iter()
                .any(|row| text(row, "TABLE_TYPE").ok() != Some("BASE TABLE".into()))
        {
            return Err(invalid(
                "Choose an accessible base table. Views remain read only.",
            ));
        }
        let available = rows
            .iter()
            .map(|row| text(row, "COLUMN_NAME"))
            .collect::<AppResult<Vec<_>>>()?;
        let key_rows = client.query("SELECT kc.name AS constraint_name,CASE kc.type WHEN 'PK' THEN 'PRIMARY KEY' ELSE 'UNIQUE' END AS constraint_type FROM sys.key_constraints kc JOIN sys.tables t ON t.object_id=kc.parent_object_id JOIN sys.schemas s ON s.schema_id=t.schema_id WHERE s.name=@P1 AND t.name=@P2 AND kc.type IN ('PK','UQ')", &[&schema.as_str(), &table.as_str()]).await.map_err(write_error)?.into_first_result().await.map_err(write_error)?;
        let current_keys = key_rows
            .iter()
            .map(|row| {
                Ok(KeyConstraint {
                    name: text(row, "constraint_name")?,
                    kind: text(row, "constraint_type")?,
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
        super::super::write::validate_key_columns(&requested, &available, "sqlserver")?;
        let statements = super::super::write::alter_table_key_statements(
            super::super::write::TableKeyChange {
                provider: "sqlserver",
                schema,
                table,
                drop_primary_key: drop_primary_key.as_deref(),
                drop_unique_keys,
                add_primary_key,
                add_unique_keys,
            },
            128,
            quote_identifier,
        )?;
        for statement in statements {
            command(client, &statement).await?;
        }
        return Ok(0);
    }
    let (schema, table) = match mutation {
        Mutation::Insert { schema, table, .. }
        | Mutation::Update { schema, table, .. }
        | Mutation::Delete { schema, table, .. }
        | Mutation::DropTable { schema, table } => (schema, table),
        _ => unreachable!(),
    };
    let qualified = format!("{}.{}", identifier(schema)?, identifier(table)?);
    if matches!(mutation, Mutation::DropTable { .. }) {
        command(client, &format!("DROP TABLE {qualified}")).await?;
        return Ok(0);
    }
    // A short exclusive table lock protects both the column layout and record
    // snapshot, including empty tables, until commit/rollback.
    command(
        client,
        &format!("SELECT TOP (1) 1 AS webdb_lock FROM {qualified} WITH (TABLOCKX, HOLDLOCK)"),
    )
    .await?;
    let base=client.query("SELECT TABLE_TYPE FROM INFORMATION_SCHEMA.TABLES WHERE TABLE_SCHEMA=@P1 AND TABLE_NAME=@P2",&[&schema.as_str(),&table.as_str()]).await.map_err(write_error)?.into_first_result().await.map_err(write_error)?;
    if base.first().and_then(|r| r.get::<&str, _>(0)) != Some("BASE TABLE") {
        return Err(invalid(
            "Choose an accessible base table. Views remain read only.",
        ));
    }
    let columns = metadata(client, schema, table).await?;
    let mut params: Vec<Option<String>> = Vec::new();
    let mut predicates = Vec::new();
    if let Mutation::Update { original, .. } | Mutation::Delete { original, .. } = mutation {
        for (column, value) in super::super::write::keys(&columns, original)? {
            params.push(Some(value));
            predicates.push(format!(
                "source.{} = {}",
                quote_identifier(&column.name),
                parameter(column, params.last().unwrap(), params.len())?
            ));
        }
        let sql = format!(
            "SELECT TOP (2) {} FROM {qualified} AS source WHERE {}",
            projection(&columns),
            predicates.join(" AND ")
        );
        let mut query = Query::new(sql);
        for param in &params {
            query.bind(param.clone());
        }
        let records = query
            .query(client)
            .await
            .map_err(write_error)?
            .into_first_result()
            .await
            .map_err(write_error)?;
        if records.len() != 1 {
            return Err(invalid(
                "The record changed or was deleted. Refresh before editing.",
            ));
        }
        let current = columns
            .iter()
            .enumerate()
            .map(|(i, column)| {
                Ok((
                    column.name.clone(),
                    records[0]
                        .try_get::<&str, _>(i)
                        .map_err(driver_error)?
                        .map_or(serde_json::Value::Null, |s| s.into()),
                ))
            })
            .collect::<AppResult<serde_json::Map<_, _>>>()?;
        super::super::write::unchanged(Some(&current), original)?;
    }
    let mut names = Vec::new();
    let mut expressions = Vec::new();
    let mut assignments = Vec::new();
    if let Mutation::Insert { values, .. } | Mutation::Update { values, .. } = mutation {
        super::super::write::values(&columns, values)?;
        for (name, value) in values {
            let column = columns.iter().find(|c| &c.name == name).unwrap();
            params.push(value.clone());
            let expression = parameter(column, value, params.len())?;
            names.push(quote_identifier(name));
            assignments.push(format!("{} = {expression}", quote_identifier(name)));
            expressions.push(expression);
        }
    }
    let sql = match mutation {
        Mutation::Insert { .. } if names.is_empty() => {
            format!("INSERT INTO {qualified} DEFAULT VALUES")
        }
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
                "UPDATE source SET {} FROM {qualified} AS source WHERE {}",
                assignments.join(", "),
                predicates.join(" AND ")
            )
        }
        _ => format!(
            "DELETE source FROM {qualified} AS source WHERE {}",
            predicates.join(" AND ")
        ),
    };
    let count = counted(client, sql, params).await?;
    if count != 1 {
        return Err(invalid(
            "The change did not affect exactly one record. No changes were saved.",
        ));
    }
    Ok(count)
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn foreign_key_errors_explain_what_needs_to_change() {
        assert!(write_message(Some(1776)).contains("PRIMARY KEY or UNIQUE"));
        assert!(write_message(Some(1778)).contains("types are incompatible"));
        assert!(write_message(Some(547)).contains("constraint violation"));
        assert!(write_message(Some(99999)).contains("SQL Server rejected the change"));
    }

    #[test]
    fn binary_parameters_validate_hex_and_bind_values() {
        let c = Column {
            name: "blob".into(),
            data_type: "varbinary".into(),
            nullable: true,
            primary_key: false,
            read_only: false,
            default_value: None,
            definition: None,
        };
        assert_eq!(
            parameter(&c, &Some("00aaff".into()), 2).unwrap(),
            "CONVERT(varbinary(max), @P2, 2)"
        );
        for s in ["0x00", "a", "zz", "'; DELETE FROM t"] {
            assert!(parameter(&c, &Some(s.into()), 1).is_err());
        }
    }
}
