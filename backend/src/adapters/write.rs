use crate::{application::*, domain::*};
use serde_json::{Map, Value};
use sqlparser::{
    ast::{ObjectType, Statement},
    dialect::Dialect,
    parser::Parser,
};
use std::collections::BTreeMap;

pub async fn bounded<T>(
    operation: impl std::future::Future<Output = AppResult<T>>,
) -> AppResult<T> {
    tokio::time::timeout(std::time::Duration::from_secs(15), operation).await.map_err(|_| AppError(ErrorKind::Timeout, "The change timed out. Refresh before retrying; the server may already have committed it."))?
}
pub fn identifier(name: &str, maximum: usize, quote: fn(&str) -> String) -> AppResult<String> {
    if name.is_empty() || name.len() > maximum || name.chars().any(char::is_control) {
        return Err(invalid(
            "Invalid object name. Check its length and control characters.",
        ));
    }
    Ok(quote(name))
}

pub struct TableKeyChange<'a> {
    pub provider: &'a str,
    pub schema: &'a str,
    pub table: &'a str,
    pub drop_primary_key: Option<&'a str>,
    pub drop_unique_keys: &'a [String],
    pub add_primary_key: &'a [String],
    pub add_unique_keys: &'a [Vec<String>],
}

pub fn alter_table_key_statements(
    change: TableKeyChange<'_>,
    maximum: usize,
    quote: fn(&str) -> String,
) -> AppResult<Vec<String>> {
    let TableKeyChange {
        provider,
        schema,
        table,
        drop_primary_key,
        drop_unique_keys,
        add_primary_key,
        add_unique_keys,
    } = change;
    if !matches!(provider, "postgres" | "mysql" | "mariadb" | "sqlserver") {
        return Err(invalid("This provider does not support key changes."));
    }
    if drop_primary_key.is_none()
        && drop_unique_keys.is_empty()
        && add_primary_key.is_empty()
        && add_unique_keys.is_empty()
    {
        return Err(invalid("Select a primary key or unique key change first."));
    }
    if add_primary_key.len() > 32 || add_unique_keys.len() > 32 {
        return Err(invalid(
            "A key change can contain at most 32 key definitions.",
        ));
    }
    let schema = identifier(schema, maximum, quote)?;
    let table = identifier(table, maximum, quote)?;
    let target = format!("{schema}.{table}");
    let mut drops = Vec::new();
    if let Some(name) = drop_primary_key {
        if provider == "mysql" || provider == "mariadb" {
            drops.push("DROP PRIMARY KEY".to_owned());
        } else {
            drops.push(format!(
                "DROP CONSTRAINT {}",
                identifier(name, maximum, quote)?
            ));
        }
    }
    for name in drop_unique_keys {
        drops.push(if provider == "mysql" || provider == "mariadb" {
            format!("DROP INDEX {}", identifier(name, maximum, quote)?)
        } else {
            format!("DROP CONSTRAINT {}", identifier(name, maximum, quote)?)
        });
    }
    let quoted_columns = |columns: &[String]| -> AppResult<String> {
        if columns.is_empty() || columns.len() > 32 {
            return Err(invalid("Each key must contain between 1 and 32 fields."));
        }
        let mut seen = std::collections::HashSet::new();
        let columns = columns
            .iter()
            .map(|column| {
                let unique_name = if provider == "postgres" {
                    column.clone()
                } else {
                    column.to_lowercase()
                };
                if !seen.insert(unique_name) {
                    return Err(invalid(
                        "A key cannot contain the same field more than once.",
                    ));
                }
                identifier(column, maximum, quote)
            })
            .collect::<AppResult<Vec<_>>>()?;
        Ok(columns.join(", "))
    };
    let mut additions = Vec::new();
    if !add_primary_key.is_empty() {
        additions.push(format!(
            "ADD CONSTRAINT {} PRIMARY KEY ({})",
            identifier(
                &format!("webdb_pk_{}", uuid::Uuid::new_v4().simple()),
                maximum,
                quote
            )?,
            quoted_columns(add_primary_key)?
        ));
    }
    for columns in add_unique_keys {
        additions.push(format!(
            "ADD CONSTRAINT {} UNIQUE ({})",
            identifier(
                &format!("webdb_uq_{}", uuid::Uuid::new_v4().simple()),
                maximum,
                quote
            )?,
            quoted_columns(columns)?
        ));
    }
    if provider == "sqlserver" {
        let mut statements = drops
            .into_iter()
            .map(|operation| format!("ALTER TABLE {target} {operation}"))
            .collect::<Vec<_>>();
        statements.extend(
            additions
                .into_iter()
                .map(|operation| format!("ALTER TABLE {target} {operation}")),
        );
        return Ok(statements);
    }
    if (provider == "mysql" || provider == "mariadb") && !add_primary_key.is_empty() {
        additions[0] = format!("ADD PRIMARY KEY ({})", quoted_columns(add_primary_key)?);
    }
    drops.extend(additions);
    Ok(vec![format!("ALTER TABLE {target} {}", drops.join(", "))])
}

pub fn validate_key_columns(
    requested: &[String],
    available: &[String],
    provider: &str,
) -> AppResult<()> {
    let equal = |left: &str, right: &str| {
        if provider == "postgres" {
            left == right
        } else {
            left.eq_ignore_ascii_case(right)
        }
    };
    for column in requested {
        if !available.iter().any(|candidate| equal(candidate, column)) {
            return Err(invalid(
                "A selected key field no longer exists. Refresh the table.",
            ));
        }
    }
    Ok(())
}

pub fn validate_key_drops(
    drop_primary_key: Option<&str>,
    drop_unique_keys: &[String],
    available: &[KeyConstraint],
) -> AppResult<()> {
    if let Some(name) = drop_primary_key
        && !available
            .iter()
            .any(|key| key.kind == "PRIMARY KEY" && key.name == name)
    {
        return Err(invalid(
            "The primary key changed. Refresh the table and try again.",
        ));
    }
    let mut seen = std::collections::HashSet::new();
    for name in drop_unique_keys {
        if !seen.insert(name)
            || !available
                .iter()
                .any(|key| key.kind == "UNIQUE" && key.name == *name)
        {
            return Err(invalid(
                "A unique key changed. Refresh the table and try again.",
            ));
        }
    }
    Ok(())
}

pub fn statement(sql: &str, dialect: &dyn Dialect) -> AppResult<String> {
    if sql.trim().is_empty() || sql.len() > 12_000 || sql.contains("/*!") || sql.contains("/*M!") {
        return Err(invalid(
            "Enter one SQL change of at most 12,000 bytes, without executable comments.",
        ));
    }
    let statements = Parser::parse_sql(dialect, sql)
        .map_err(|_| invalid("Check the SQL change syntax. Run one statement at a time."))?;
    if statements.len() != 1
        || !matches!(
            &statements[0],
            Statement::CreateTable(_)
                | Statement::AlterTable(_)
                | Statement::CreateIndex(_)
                | Statement::Insert(_)
                | Statement::Update(_)
                | Statement::Delete(_)
                | Statement::Truncate(_)
                | Statement::Drop {
                    object_type: ObjectType::Table | ObjectType::Index,
                    ..
                }
        )
    {
        return Err(invalid(
            "Use one CREATE/ALTER/DROP TABLE, CREATE/DROP INDEX, TRUNCATE, INSERT, UPDATE, or DELETE statement. Use the dedicated database actions for databases.",
        ));
    }
    super::sql::canonical(&statements[0], dialect)
}

pub fn values(columns: &[Column], values: &BTreeMap<String, Option<String>>) -> AppResult<()> {
    for (name, value) in values {
        let column = columns
            .iter()
            .find(|c| &c.name == name)
            .ok_or_else(|| invalid("Unknown column. Refresh the table."))?;
        if column.read_only {
            return Err(invalid(
                "Omit generated, identity, and unsupported fields; the database maintains them.",
            ));
        }
        if value.as_ref().is_some_and(|s| s.len() > 12_000) {
            return Err(invalid("Field values must be at most 12,000 bytes."));
        }
    }
    Ok(())
}

pub fn keys<'a>(
    columns: &'a [Column],
    original: &Map<String, Value>,
) -> AppResult<Vec<(&'a Column, String)>> {
    if original.len() != columns.len() || columns.iter().any(|c| !original.contains_key(&c.name)) {
        return Err(invalid(
            "The table structure changed. Refresh before editing.",
        ));
    }
    let keys = columns
        .iter()
        .filter(|c| c.primary_key)
        .map(|column| {
            let value = original[&column.name]
                .as_str()
                .ok_or_else(|| invalid("Invalid primary key snapshot. Refresh the table."))?;
            Ok((column, value.to_owned()))
        })
        .collect::<AppResult<Vec<_>>>()?;
    if keys.is_empty() {
        return Err(invalid(
            "Updating or deleting records requires a primary key for this provider.",
        ));
    }
    Ok(keys)
}
pub fn unchanged(
    current: Option<&Map<String, Value>>,
    original: &Map<String, Value>,
) -> AppResult<()> {
    if current != Some(original) {
        return Err(invalid(
            "The record changed or was deleted. Refresh before editing; no changes were saved.",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlparser::dialect::{MsSqlDialect, MySqlDialect};
    #[test]
    fn provider_schema_examples_preserve_quoted_names() {
        let mysql=statement("CREATE TABLE `db`.`t``x` (id bigint PRIMARY KEY, amount decimal(24,4), computed bigint GENERATED ALWAYS AS (id+1) STORED) ENGINE=InnoDB",&MySqlDialect {}).unwrap();
        assert!(mysql.contains("`t``x`"));
        let mssql = statement(
            "CREATE TABLE [dbo].[t]]x] (id bigint IDENTITY(1,1) PRIMARY KEY, note nvarchar(50))",
            &MsSqlDialect {},
        )
        .unwrap();
        assert!(mssql.contains("[t]]x]"));
        assert!(
            statement(
                "CREATE TABLE t (id int, computed AS (id+1))",
                &MsSqlDialect {}
            )
            .is_err()
        );
    }
    #[test]
    fn changes_cannot_control_transactions_or_drop_databases() {
        for dialect in [&MySqlDialect {} as &dyn Dialect, &MsSqlDialect {}] {
            for sql in [
                "COMMIT",
                "BEGIN TRANSACTION",
                "DROP DATABASE master",
                "CREATE DATABASE x",
                "SELECT 1",
                "DELETE FROM t; COMMIT",
                "EXEC('DROP TABLE t')",
                "CREATE PROCEDURE p AS DELETE FROM t",
            ] {
                assert!(statement(sql, dialect).is_err(), "{sql}");
            }
            for sql in [
                "CREATE TABLE t (id int PRIMARY KEY)",
                "ALTER TABLE t ADD note varchar(20)",
                "DROP TABLE t",
                "INSERT INTO t (id) VALUES (1)",
                "UPDATE t SET id=2",
                "DELETE FROM t",
            ] {
                assert!(statement(sql, dialect).is_ok(), "{sql}");
            }
        }
    }
    #[test]
    fn stale_rows_and_malformed_keys_are_rejected() {
        let original = serde_json::json!({"id":"1","value":"before"})
            .as_object()
            .unwrap()
            .clone();
        assert!(unchanged(None, &original).is_err());
        assert!(
            unchanged(
                serde_json::json!({"id":"1","value":"after"}).as_object(),
                &original
            )
            .is_err()
        );
        assert!(unchanged(Some(&original), &original).is_ok());
        let columns = vec![Column {
            name: "id".into(),
            data_type: "int".into(),
            nullable: false,
            primary_key: false,
            read_only: false,
            default_value: None,
            definition: None,
        }];
        assert!(keys(&columns, serde_json::json!({"id":"1"}).as_object().unwrap()).is_err());
    }

    #[test]
    fn key_change_statements_quote_identifiers_and_use_provider_syntax() {
        let quote_pg = |name: &str| format!("\"{}\"", name.replace('"', "\"\""));
        let quote_mysql = |name: &str| format!("`{}`", name.replace('`', "``"));
        let pg = alter_table_key_statements(
            TableKeyChange {
                provider: "postgres",
                schema: "odd schema",
                table: "t\"x",
                drop_primary_key: Some("old pk"),
                drop_unique_keys: &[],
                add_primary_key: &["a\"b".into(), "c".into()],
                add_unique_keys: &[vec!["c".into(), "d".into()]],
            },
            63,
            quote_pg,
        )
        .unwrap();
        assert!(pg[0].contains("DROP CONSTRAINT \"old pk\""));
        assert!(pg[0].contains("\"a\"\"b\", \"c\""));
        assert!(pg[0].contains("ADD CONSTRAINT \"webdb_uq_"));
        let mysql = alter_table_key_statements(
            TableKeyChange {
                provider: "mysql",
                schema: "db",
                table: "t",
                drop_primary_key: Some("PRIMARY"),
                drop_unique_keys: &["uq".into()],
                add_primary_key: &["id".into()],
                add_unique_keys: &[],
            },
            64,
            quote_mysql,
        )
        .unwrap();
        assert!(mysql[0].contains("DROP PRIMARY KEY, DROP INDEX `uq`, ADD PRIMARY KEY (`id`)"));
        let sqlserver = alter_table_key_statements(
            TableKeyChange {
                provider: "sqlserver",
                schema: "dbo",
                table: "t",
                drop_primary_key: Some("oldpk"),
                drop_unique_keys: &[],
                add_primary_key: &["id".into()],
                add_unique_keys: &[],
            },
            128,
            |name| format!("[{}]", name.replace(']', "]]")),
        )
        .unwrap();
        assert_eq!(sqlserver.len(), 2);
        assert!(sqlserver[0].contains("DROP CONSTRAINT [oldpk]"));
        assert!(sqlserver[1].contains("ADD CONSTRAINT [webdb_pk_"));
    }

    #[test]
    fn key_mutation_accepts_camel_case_contract_and_rejects_non_key_drops() {
        let mutation: Mutation = serde_json::from_value(serde_json::json!({
            "action": "alterTableKeys",
            "schema": "public",
            "table": "items",
            "dropPrimaryKey": "items_pkey",
            "dropUniqueKeys": ["items_name_key"],
            "addPrimaryKey": ["id", "tenant_id"],
            "addUniqueKeys": [["name", "tenant_id"]]
        }))
        .unwrap();
        assert!(matches!(mutation, Mutation::AlterTableKeys { .. }));

        let keys = vec![
            KeyConstraint {
                name: "items_pkey".into(),
                kind: "PRIMARY KEY".into(),
                columns: vec!["id".into()],
            },
            KeyConstraint {
                name: "items_name_key".into(),
                kind: "UNIQUE".into(),
                columns: vec!["name".into()],
            },
        ];
        assert!(validate_key_drops(Some("items_pkey"), &["items_name_key".into()], &keys).is_ok());
        assert!(validate_key_drops(Some("items_name_key"), &[], &keys).is_err());
        assert!(validate_key_drops(None, &["foreign_key".into()], &keys).is_err());
    }
}

pub async fn check_index(
    adapter: &dyn DatabaseAdapter,
    input: &ConnectionInput,
    mutation: &Mutation,
) -> AppResult<()> {
    if let Mutation::AlterIndex {
        schema,
        table,
        expected: Some(expected),
        ..
    } = mutation
    {
        let current = adapter.indexes(input, schema, table).await?;
        if !expected.editable || !current.iter().any(|i| i == expected) {
            return Err(invalid(
                "The index changed or cannot be edited here. Refresh before retrying.",
            ));
        }
    }
    Ok(())
}

pub fn index_statements(
    provider: &str,
    schema: &str,
    table: &str,
    expected: Option<&IndexInfo>,
    index: Option<&IndexInfo>,
    maximum: usize,
    quote: fn(&str) -> String,
) -> AppResult<Vec<String>> {
    if expected.is_none() && index.is_none() {
        return Err(invalid("Choose an index change."));
    }
    if expected.is_some_and(|i| !i.editable) {
        return Err(invalid(
            "This index must be managed with SQL or key constraints.",
        ));
    }
    let schema = identifier(schema, maximum, quote)?;
    let target = format!("{schema}.{}", identifier(table, maximum, quote)?);
    let mysql = matches!(provider, "mysql" | "mariadb");
    let mut statements = vec![];
    if let Some(old) = expected {
        let name = identifier(&old.name, maximum, quote)?;
        statements.push(if mysql {
            format!("DROP INDEX {name}")
        } else if provider == "postgres" {
            format!("DROP INDEX {schema}.{name}")
        } else {
            format!("DROP INDEX {name} ON {target}")
        });
    }
    if let Some(index) = index {
        if index.columns.is_empty() || index.columns.len() > 32 {
            return Err(invalid("Select between 1 and 32 index columns."));
        }
        let mut seen = std::collections::HashSet::new();
        let columns = index
            .columns
            .iter()
            .map(|c| {
                let key = if provider == "postgres" {
                    c.clone()
                } else {
                    c.to_lowercase()
                };
                if !seen.insert(key) {
                    return Err(invalid("Index columns must be distinct."));
                }
                identifier(c, maximum, quote)
            })
            .collect::<AppResult<Vec<_>>>()?
            .join(", ");
        let name = identifier(&index.name, maximum, quote)?;
        let unique = if index.unique { "UNIQUE " } else { "" };
        statements.push(if mysql {
            format!("ADD {unique}INDEX {name} ({columns})")
        } else {
            format!("CREATE {unique}INDEX {name} ON {target} ({columns})")
        });
    }
    if mysql {
        Ok(vec![format!(
            "ALTER TABLE {target} {}",
            statements.join(", ")
        )])
    } else {
        Ok(statements)
    }
}

#[cfg(test)]
mod index_tests {
    use super::*;

    fn index(name: &str, columns: &[&str]) -> IndexInfo {
        IndexInfo {
            name: name.into(),
            columns: columns.iter().map(|c| (*c).into()).collect(),
            unique: true,
            editable: true,
        }
    }

    #[test]
    fn mysql_replacement_is_one_atomic_alter_with_ordered_quoted_columns() {
        let old = index("old`index", &["a"]);
        let new = index("new`index", &["b`", "a"]);
        let plan = index_statements(
            "mysql",
            "db",
            "t",
            Some(&old),
            Some(&new),
            64,
            super::super::mysql::quote_identifier,
        )
        .unwrap();
        assert_eq!(
            plan,
            [
                "ALTER TABLE `db`.`t` DROP INDEX `old``index`, ADD UNIQUE INDEX `new``index` (`b```, `a`)"
            ]
        );
    }

    #[test]
    fn postgres_drop_is_schema_qualified_and_replacement_preserves_order() {
        let old = index("old", &["a"]);
        let new = index("new", &["b", "a"]);
        let plan = index_statements(
            "postgres",
            "s\"",
            "t",
            Some(&old),
            Some(&new),
            63,
            super::super::postgres::quote_identifier,
        )
        .unwrap();
        assert_eq!(
            plan,
            [
                "DROP INDEX \"s\"\"\".\"old\"",
                "CREATE UNIQUE INDEX \"new\" ON \"s\"\"\".\"t\" (\"b\", \"a\")"
            ]
        );
    }

    #[test]
    fn sql_server_drop_targets_the_table() {
        let old = index("i]", &["a"]);
        let plan = index_statements(
            "sqlserver",
            "dbo",
            "t]",
            Some(&old),
            None,
            128,
            super::super::sqlserver::quote_identifier,
        )
        .unwrap();
        assert_eq!(plan, ["DROP INDEX [i]]] ON [dbo].[t]]]"]);
    }

    #[test]
    fn invalid_and_protected_indexes_are_rejected() {
        let quote = super::super::mysql::quote_identifier;
        for columns in [vec![], vec!["a", "A"], vec!["a"; 33]] {
            assert!(
                index_statements(
                    "mysql",
                    "db",
                    "t",
                    None,
                    Some(&index("i", &columns)),
                    64,
                    quote
                )
                .is_err()
            );
        }
        let mut protected = index("pk", &["a"]);
        protected.editable = false;
        assert!(index_statements("mysql", "db", "t", Some(&protected), None, 64, quote).is_err());
        assert!(index_statements("mysql", "db", "t", None, None, 64, quote).is_err());
        assert!(
            index_statements(
                "mysql",
                "db",
                "t",
                None,
                Some(&index("bad\nname", &["a"])),
                64,
                quote
            )
            .is_err()
        );
    }

    #[test]
    fn generated_mysql_and_sql_server_index_sql_passes_the_dialect_parser() {
        for (provider, quote, dialect) in [
            (
                "mysql",
                super::super::mysql::quote_identifier as fn(&str) -> String,
                &sqlparser::dialect::MySqlDialect {} as &dyn Dialect,
            ),
            (
                "sqlserver",
                super::super::sqlserver::quote_identifier as fn(&str) -> String,
                &sqlparser::dialect::MsSqlDialect {} as &dyn Dialect,
            ),
        ] {
            let old = index("old", &["a"]);
            let new = index("new", &["b", "a"]);
            for sql in
                index_statements(provider, "s", "t", Some(&old), Some(&new), 64, quote).unwrap()
            {
                assert!(statement(&sql, dialect).is_ok(), "{sql}");
            }
        }
    }
}
