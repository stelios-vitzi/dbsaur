use crate::{application::*, domain::*};
use sqlparser::{
    dialect::{Dialect, MsSqlDialect, MySqlDialect, PostgreSqlDialect},
    parser::Parser,
    tokenizer::Token,
};

// Match NO_BACKSLASH_ESCAPES on write connections when rendering defaults.
#[derive(Debug)]
struct ColumnMySqlDialect;
impl Dialect for ColumnMySqlDialect {
    fn dialect(&self) -> std::any::TypeId {
        std::any::TypeId::of::<MySqlDialect>()
    }
    fn is_identifier_start(&self, ch: char) -> bool {
        MySqlDialect {}.is_identifier_start(ch)
    }
    fn is_identifier_part(&self, ch: char) -> bool {
        MySqlDialect {}.is_identifier_part(ch)
    }
    fn is_delimited_identifier_start(&self, ch: char) -> bool {
        ch == '`'
    }
    fn identifier_quote_style(&self, _: &str) -> Option<char> {
        Some('`')
    }
    fn supports_string_literal_backslash_escape(&self) -> bool {
        false
    }
    fn supports_string_literal_concatenation(&self) -> bool {
        true
    }
    fn ignores_wildcard_escapes(&self) -> bool {
        true
    }
}

pub(super) struct ColumnPlan {
    pub statements: Vec<String>,
    pub rename: Option<String>,
}

fn fragment(value: &str, dialect: &dyn Dialect, data_type: bool) -> AppResult<String> {
    if value.is_empty() || value.len() > 4096 || value.contains('\0') {
        return Err(invalid(
            "Enter a column type or default expression of at most 4096 bytes.",
        ));
    }
    let mut parser = Parser::new(dialect)
        .try_with_sql(value)
        .map_err(|_| invalid("Check the column type or default expression."))?;
    let result = if data_type {
        let value = parser
            .parse_data_type()
            .map_err(|_| invalid("Invalid column type."))?;
        super::sql::canonical_data_type(&value, dialect)?
    } else {
        let value = parser
            .parse_expr()
            .map_err(|_| invalid("Invalid default expression."))?;
        super::sql::canonical_expression(&value, dialect)?
    };
    if parser.peek_token().token != Token::EOF {
        return Err(invalid(
            "Use one data type or one default expression, without extra SQL.",
        ));
    }
    Ok(result)
}

fn postgres_collation(value: &str) -> AppResult<String> {
    let mut parser = Parser::new(&PostgreSqlDialect {})
        .try_with_sql(value)
        .map_err(|_| invalid("Invalid collation name."))?;
    let name = parser
        .parse_object_name(false)
        .map_err(|_| invalid("Invalid collation name."))?;
    if parser.peek_token().token != Token::EOF || name.0.is_empty() || name.0.len() > 2 {
        return Err(invalid(
            "Use a collation name, optionally qualified by its schema.",
        ));
    }
    name.0
        .iter()
        .map(|part| {
            let sqlparser::ast::ObjectNamePart::Identifier(ident) = part else {
                return Err(invalid("Invalid collation name."));
            };
            super::write::identifier(&ident.value, 63, super::postgres::quote_identifier)
        })
        .collect::<AppResult<Vec<_>>>()
        .map(|parts| parts.join("."))
}

pub(super) fn plan(
    provider: &str,
    schema: &str,
    table: &str,
    column: &str,
    current: &ColumnDefinition,
    expected: &ColumnDefinition,
    change: &ColumnChange,
) -> AppResult<ColumnPlan> {
    if current != expected {
        return Err(invalid(
            "The column definition changed. Reopen the editor before saving.",
        ));
    }
    let (maximum, quote, dialect): (usize, fn(&str) -> String, &dyn Dialect) = match provider {
        "postgres" => (63, super::postgres::quote_identifier, &PostgreSqlDialect {}),
        "mysql" | "mariadb" => (64, super::mysql::quote_identifier, &ColumnMySqlDialect),
        "sqlserver" => (128, super::sqlserver::quote_identifier, &MsSqlDialect {}),
        _ => return Err(invalid("Column editing is unavailable for this provider.")),
    };
    let ident = |name: &str| super::write::identifier(name, maximum, quote);
    let target = format!("{}.{}", ident(schema)?, ident(table)?);
    let name = ident(column)?;
    let mut statements = vec![];
    let mut native_type_statements = vec![];
    let mut rename = None;
    match change {
        ColumnChange::Rename { name: next } => {
            let next_quoted = ident(next)?;
            if next == column {
                return Err(invalid("Enter a different column name."));
            }
            if provider == "sqlserver" {
                rename = Some(next.clone());
            } else {
                statements.push(format!(
                    "ALTER TABLE {target} RENAME COLUMN {name} TO {next_quoted}"
                ));
            }
        }
        ColumnChange::Drop => statements.push(format!(
            "ALTER TABLE {target} DROP COLUMN {name}{}",
            if provider == "postgres" {
                " RESTRICT"
            } else {
                ""
            }
        )),
        ColumnChange::Definition {
            data_type,
            nullable,
            default_expression,
            character_set,
            collation,
        } => {
            if !current.extra.is_empty() && !current.extra.eq_ignore_ascii_case("DEFAULT_GENERATED")
            {
                return Err(invalid(
                    "Generated, identity, auto-updated, and other special column definitions cannot be replaced by this editor. Rename and deletion remain available.",
                ));
            }
            if provider != "mysql"
                && provider != "mariadb"
                && character_set != &current.character_set
            {
                return Err(invalid(
                    "This provider controls character encoding through its database or collation, not a column character set.",
                ));
            }
            let typ = fragment(data_type.trim(), dialect, true)?;
            let default = default_expression
                .as_deref()
                .map(|value| fragment(value.trim(), dialect, false))
                .transpose()?;
            let type_changed = data_type.trim() != current.data_type
                || collation != &current.collation
                || character_set != &current.character_set;
            let default_changed = default_expression != &current.default_expression;
            let nullability = if *nullable { "NULL" } else { "NOT NULL" };
            if provider == "mysql" || provider == "mariadb" {
                if !type_changed && !default_changed && *nullable == current.nullable {
                    return Err(invalid("No column changes to save."));
                }
                let charset = character_set
                    .as_deref()
                    .map(|value| ident(value).map(|name| format!(" CHARACTER SET {name}")))
                    .transpose()?
                    .unwrap_or_default();
                let collate = collation
                    .as_deref()
                    .map(|value| ident(value).map(|name| format!(" COLLATE {name}")))
                    .transpose()?
                    .unwrap_or_default();
                let default = default
                    .map(|value| format!(" DEFAULT ({value})"))
                    .unwrap_or_default();
                let comment = mysql_literal(&current.comment);
                statements.push(format!("ALTER TABLE {target} MODIFY COLUMN {name} {typ}{charset}{collate} {nullability}{default} COMMENT {comment}"));
            } else {
                // Remove and restore defaults around type changes, retaining their exact expressions.
                if type_changed || default_changed {
                    if provider == "postgres" {
                        statements.push(format!(
                            "ALTER TABLE {target} ALTER COLUMN {name} DROP DEFAULT"
                        ));
                    } else if let Some(constraint) = &current.default_constraint {
                        statements.push(format!(
                            "ALTER TABLE {target} DROP CONSTRAINT {}",
                            ident(constraint)?
                        ));
                    }
                }
                if type_changed || (provider == "sqlserver" && *nullable != current.nullable) {
                    let collate = collation
                        .as_deref()
                        .map(|value| {
                            (if provider == "postgres" {
                                postgres_collation(value)
                            } else {
                                ident(value)
                            })
                            .map(|name| format!(" COLLATE {name}"))
                        })
                        .transpose()?
                        .unwrap_or_else(|| {
                            if provider == "sqlserver"
                                && current.collation.is_some()
                                && ["CHAR", "VARCHAR", "NCHAR", "NVARCHAR", "TEXT", "NTEXT"]
                                    .iter()
                                    .any(|prefix| typ.to_ascii_uppercase().starts_with(prefix))
                            {
                                " COLLATE DATABASE_DEFAULT".into()
                            } else {
                                String::new()
                            }
                        });
                    if provider == "postgres" && !collate.is_empty() {
                        native_type_statements.push(statements.len());
                    }
                    statements.push(if provider == "postgres" {
                        format!("ALTER TABLE {target} ALTER COLUMN {name} TYPE {typ}{collate}")
                    } else {
                        format!(
                            "ALTER TABLE {target} ALTER COLUMN {name} {typ}{collate} {nullability}"
                        )
                    });
                }
                if provider == "postgres" && *nullable != current.nullable {
                    statements.push(format!(
                        "ALTER TABLE {target} ALTER COLUMN {name} {} NOT NULL",
                        if *nullable { "DROP" } else { "SET" }
                    ));
                }
                if (type_changed || default_changed)
                    && let Some(default) = default
                {
                    statements.push(if provider == "postgres" {
                        format!("ALTER TABLE {target} ALTER COLUMN {name} SET DEFAULT {default}")
                    } else {
                        format!(
                            "ALTER TABLE {target} ADD CONSTRAINT {} DEFAULT ({default}) FOR {name}",
                            ident(&format!("dbsaur_df_{}", uuid::Uuid::new_v4().simple()))?
                        )
                    });
                }
            }
        }
    }
    if statements.is_empty() && rename.is_none() {
        return Err(invalid("No column changes to save."));
    }
    // sqlparser does not support SQL Server's native ALTER COLUMN grammar or
    // PostgreSQL's COLLATE clause in ALTER COLUMN TYPE.
    // These statements contain only fixed syntax, quoted identifiers and separately
    // parsed, losslessly rendered types/expressions. Arbitrary Execute SQL still
    // uses the unchanged statement gate.
    let statements = statements
        .iter()
        .enumerate()
        .map(|(index, sql)| {
            if provider == "sqlserver" || native_type_statements.contains(&index) {
                Ok(sql.clone())
            } else {
                super::write::statement(sql, dialect)
            }
        })
        .collect::<AppResult<Vec<_>>>()?;
    Ok(ColumnPlan { statements, rename })
}

pub(super) fn mysql_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn column() -> ColumnDefinition {
        ColumnDefinition {
            data_type: "varchar(80)".into(),
            nullable: true,
            default_expression: Some("'untitled'".into()),
            character_set: None,
            collation: None,
            comment: String::new(),
            extra: String::new(),
            default_constraint: None,
        }
    }
    #[test]
    fn column_changes_are_quoted_bounded_and_snapshot_checked() {
        let current = column();
        for provider in ["postgres", "mysql", "mariadb", "sqlserver"] {
            let renamed = plan(
                provider,
                "s",
                "t",
                "c",
                &current,
                &current,
                &ColumnChange::Rename {
                    name: "a]; DROP TABLE t;--".into(),
                },
            )
            .unwrap();
            assert!(renamed.rename.is_some() || renamed.statements.len() == 1);
            let mut stale = column();
            stale.nullable = false;
            assert!(
                plan(
                    provider,
                    "s",
                    "t",
                    "c",
                    &current,
                    &stale,
                    &ColumnChange::Drop
                )
                .is_err()
            );
        }
    }
    #[test]
    fn definitions_reject_sql_injection_and_preserve_provider_semantics() {
        let current = column();
        for provider in ["postgres", "mysql", "mariadb", "sqlserver"] {
            for bad_type in [
                "integer; DROP TABLE t",
                "integer NOT NULL",
                "integer, injected text",
            ] {
                assert!(
                    plan(
                        provider,
                        "s",
                        "t",
                        "c",
                        &current,
                        &current,
                        &ColumnChange::Definition {
                            data_type: bad_type.into(),
                            nullable: false,
                            default_expression: None,
                            character_set: None,
                            collation: None
                        }
                    )
                    .is_err()
                );
            }
            let result = plan(
                provider,
                "s",
                "t",
                "c",
                &current,
                &current,
                &ColumnChange::Definition {
                    data_type: "varchar(120)".into(),
                    nullable: false,
                    default_expression: Some("'new value'".into()),
                    character_set: None,
                    collation: None,
                },
            )
            .unwrap();
            assert!(
                result
                    .statements
                    .iter()
                    .all(|sql| sql.starts_with("ALTER TABLE"))
            );
        }
        assert!(fragment("1; DROP TABLE t", &PostgreSqlDialect {}, false).is_err());
    }
    #[test]
    fn literal_backslashes_and_bracketed_expression_names_are_preserved() {
        let literal = mysql_literal("C:\\records\\O'Reilly");
        assert_eq!(
            fragment(&literal, &ColumnMySqlDialect, false).unwrap(),
            literal
        );
        let expression = "[name]]; DROP TABLE t;--]";
        assert_eq!(
            fragment(expression, &MsSqlDialect {}, false).unwrap(),
            expression
        );
        let current = column();
        let change = ColumnChange::Definition {
            data_type: "varchar(120)".into(),
            nullable: true,
            default_expression: Some(literal.clone()),
            character_set: None,
            collation: None,
        };
        let result = plan("mysql", "s", "t", "c", &current, &current, &change).unwrap();
        assert!(result.statements[0].contains(&literal));
    }
}
