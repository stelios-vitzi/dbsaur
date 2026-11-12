pub(crate) mod write;
use crate::adapters::sqlx;
use crate::{application::*, domain::*};
use async_trait::async_trait;
use sqlx::{
    Column as _, ConnectOptions, Connection, Executor, MySqlConnection, Row, SqlSafeStr, Statement,
    TypeInfo,
    mysql::{MySqlConnectOptions, MySqlSslMode},
};
use std::{collections::BTreeMap, time::Duration};

pub struct MySqlAdapter;
// MariaDB shares the wire protocol and metadata adapter, with its own timeout setup.
pub(crate) async fn connect(input: &ConnectionInput) -> AppResult<MySqlConnection> {
    connect_mode(input, true).await
}
async fn connect_mode(input: &ConnectionInput, read_only: bool) -> AppResult<MySqlConnection> {
    let mut options = MySqlConnectOptions::new()
        .host(&input.host)
        .port(input.port)
        .username(&input.username)
        .password(&input.password)
        .ssl_mode(if input.ssl {
            MySqlSslMode::VerifyIdentity
        } else {
            MySqlSslMode::Disabled
        })
        .disable_statement_logging();
    if let Some(pem) = &input.tls_ca {
        options = options.ssl_ca_from_pem(pem.as_bytes().to_vec());
    }
    if let Some(pem) = &input.tls_cert {
        options = options.ssl_client_cert_from_pem(pem.as_bytes());
    }
    if let Some(pem) = &input.tls_key {
        options = options.ssl_client_key_from_pem(pem.as_bytes());
    }
    if !input.database.is_empty() {
        options = options.database(&input.database);
    }
    let mut connection = tokio::time::timeout(
        Duration::from_secs(10),
        MySqlConnection::connect_with(&options),
    )
    .await
    .map_err(|_| AppError(ErrorKind::Timeout, "Database connection timed out."))??;
    sqlx::query("SET SESSION autocommit = 1")
        .execute(&mut connection)
        .await?;
    // Each operation opens its own connection; browse connections enforce read-only mode.
    sqlx::query(
        "SET SESSION sql_mode = CONCAT_WS(',', @@SESSION.sql_mode, 'NO_BACKSLASH_ESCAPES')",
    )
    .execute(&mut connection)
    .await?;
    sqlx::query("SET SESSION lock_wait_timeout = 2")
        .execute(&mut connection)
        .await?;
    if read_only {
        sqlx::query("SET SESSION TRANSACTION READ ONLY")
            .execute(&mut connection)
            .await?;
    } else {
        sqlx::query("SET SESSION TRANSACTION READ WRITE")
            .execute(&mut connection)
            .await?;
        sqlx::query(
            "SET SESSION sql_mode = CONCAT_WS(',', @@SESSION.sql_mode, 'STRICT_ALL_TABLES')",
        )
        .execute(&mut connection)
        .await?;
    }
    sqlx::query("SET SESSION innodb_lock_wait_timeout = 2")
        .execute(&mut connection)
        .await?;
    if input.provider == "mariadb" {
        sqlx::query("SET SESSION max_statement_time = 10")
            .execute(&mut connection)
            .await?;
    } else {
        sqlx::query("SET SESSION MAX_EXECUTION_TIME = 10000")
            .execute(&mut connection)
            .await?;
    }
    Ok(connection)
}
pub fn quote_identifier(name: &str) -> String {
    format!("`{}`", name.replace('`', "``"))
}

pub(crate) async fn databases(input: &ConnectionInput) -> AppResult<Vec<String>> {
    let mut connection = connect(input).await?;
    Ok(sqlx::query_scalar("SELECT SCHEMA_NAME FROM information_schema.SCHEMATA WHERE SCHEMA_NAME NOT IN ('information_schema', 'performance_schema', 'mysql', 'sys') ORDER BY SCHEMA_NAME").fetch_all(&mut connection).await?)
}
pub(crate) async fn tables(input: &ConnectionInput) -> AppResult<Vec<Table>> {
    let mut connection = connect(input).await?;
    let records = sqlx::query("SELECT t.TABLE_SCHEMA, t.TABLE_NAME, t.TABLE_TYPE, c.COLUMN_NAME FROM information_schema.TABLES t LEFT JOIN information_schema.COLUMNS c ON c.TABLE_SCHEMA=t.TABLE_SCHEMA AND c.TABLE_NAME=t.TABLE_NAME WHERE t.TABLE_SCHEMA=DATABASE() ORDER BY t.TABLE_SCHEMA,t.TABLE_NAME,c.ORDINAL_POSITION").fetch_all(&mut connection).await?;
    let mut tables = BTreeMap::<(String, String), Table>::new();
    for record in records {
        let schema: String = record.try_get("TABLE_SCHEMA")?;
        let name: String = record.try_get("TABLE_NAME")?;
        let table = tables
            .entry((schema.clone(), name.clone()))
            .or_insert(Table {
                schema,
                name,
                kind: record.try_get("TABLE_TYPE")?,
                columns: vec![],
                primary_keys: vec![],
                key_constraints: vec![],
                referenceable_columns: vec![],
                foreign_keys: vec![],
            });
        if let Some(name) = record.try_get::<Option<String>, _>("COLUMN_NAME")? {
            table.columns.push(name);
        }
    }
    let key_constraints = sqlx::query("SELECT tc.TABLE_SCHEMA,tc.TABLE_NAME,tc.CONSTRAINT_NAME,tc.CONSTRAINT_TYPE,kcu.COLUMN_NAME FROM information_schema.TABLE_CONSTRAINTS tc JOIN information_schema.KEY_COLUMN_USAGE kcu ON kcu.CONSTRAINT_SCHEMA=tc.CONSTRAINT_SCHEMA AND kcu.TABLE_SCHEMA=tc.TABLE_SCHEMA AND kcu.TABLE_NAME=tc.TABLE_NAME AND kcu.CONSTRAINT_NAME=tc.CONSTRAINT_NAME WHERE tc.TABLE_SCHEMA=DATABASE() AND tc.CONSTRAINT_TYPE IN ('PRIMARY KEY','UNIQUE') ORDER BY tc.TABLE_NAME,tc.CONSTRAINT_NAME,kcu.ORDINAL_POSITION")
        .fetch_all(&mut connection).await?;
    for key in key_constraints {
        let schema: String = key.try_get("TABLE_SCHEMA")?;
        let table_name: String = key.try_get("TABLE_NAME")?;
        if let Some(table) = tables.get_mut(&(schema, table_name)) {
            let kind: String = key.try_get("CONSTRAINT_TYPE")?;
            let name: String = key.try_get("CONSTRAINT_NAME")?;
            let column: String = key.try_get("COLUMN_NAME")?;
            if kind == "PRIMARY KEY" {
                table.primary_keys.push(column.clone());
            }
            if let Some(constraint) = table
                .key_constraints
                .iter_mut()
                .find(|constraint| constraint.name == name)
            {
                constraint.columns.push(column);
            } else {
                table.key_constraints.push(KeyConstraint {
                    name,
                    kind,
                    columns: vec![column],
                });
            }
        }
    }
    let referenceable = sqlx::query("SELECT TABLE_SCHEMA,TABLE_NAME,MIN(COLUMN_NAME) AS COLUMN_NAME FROM information_schema.STATISTICS WHERE TABLE_SCHEMA=DATABASE() AND NON_UNIQUE=0 GROUP BY TABLE_SCHEMA,TABLE_NAME,INDEX_NAME HAVING COUNT(*)=1 AND COUNT(COLUMN_NAME)=1 AND MIN(SUB_PART) IS NULL")
        .fetch_all(&mut connection).await?;
    for row in referenceable {
        let schema: String = row.try_get("TABLE_SCHEMA")?;
        let name: String = row.try_get("TABLE_NAME")?;
        let column: String = row.try_get("COLUMN_NAME")?;
        if let Some(table) = tables.get_mut(&(schema, name))
            && !table.referenceable_columns.contains(&column)
        {
            table.referenceable_columns.push(column);
        }
    }
    // MariaDB and older/configured MySQL InnoDB instances also accept a leading
    // indexed column. New MySQL installations can require standard unique keys.
    let restriction = sqlx::query("SHOW VARIABLES LIKE 'restrict_fk_on_non_standard_key'")
        .fetch_optional(&mut connection)
        .await?;
    let allow_nonstandard = restriction
        .as_ref()
        .map(|row| row.try_get::<String, _>("Value"))
        .transpose()?
        .is_none_or(|value| value.eq_ignore_ascii_case("OFF") || value == "0");
    if allow_nonstandard {
        let indexed = sqlx::query("SELECT DISTINCT s.TABLE_SCHEMA,s.TABLE_NAME,s.COLUMN_NAME FROM information_schema.STATISTICS s JOIN information_schema.TABLES t ON t.TABLE_SCHEMA=s.TABLE_SCHEMA AND t.TABLE_NAME=s.TABLE_NAME WHERE s.TABLE_SCHEMA=DATABASE() AND t.ENGINE='InnoDB' AND s.INDEX_TYPE='BTREE' AND s.SEQ_IN_INDEX=1 AND s.SUB_PART IS NULL AND s.COLUMN_NAME IS NOT NULL")
            .fetch_all(&mut connection).await?;
        for row in indexed {
            let schema: String = row.try_get("TABLE_SCHEMA")?;
            let name: String = row.try_get("TABLE_NAME")?;
            let column: String = row.try_get("COLUMN_NAME")?;
            if let Some(table) = tables.get_mut(&(schema, name))
                && !table.referenceable_columns.contains(&column)
            {
                table.referenceable_columns.push(column);
            }
        }
    }
    let relationships = sqlx::query("SELECT CONSTRAINT_NAME,TABLE_SCHEMA,TABLE_NAME,COLUMN_NAME,REFERENCED_TABLE_SCHEMA,REFERENCED_TABLE_NAME,REFERENCED_COLUMN_NAME FROM information_schema.KEY_COLUMN_USAGE WHERE TABLE_SCHEMA=DATABASE() AND REFERENCED_TABLE_NAME IS NOT NULL ORDER BY TABLE_NAME,CONSTRAINT_NAME,ORDINAL_POSITION")
        .fetch_all(&mut connection).await?;
    for relation in relationships {
        let schema: String = relation.try_get("TABLE_SCHEMA")?;
        let table_name: String = relation.try_get("TABLE_NAME")?;
        if let Some(table) = tables.get_mut(&(schema, table_name)) {
            table.foreign_keys.push(ForeignKey {
                name: relation.try_get("CONSTRAINT_NAME")?,
                column: relation.try_get("COLUMN_NAME")?,
                referenced_schema: relation.try_get("REFERENCED_TABLE_SCHEMA")?,
                referenced_table: relation.try_get("REFERENCED_TABLE_NAME")?,
                referenced_column: relation.try_get("REFERENCED_COLUMN_NAME")?,
            });
        }
    }
    Ok(tables.into_values().collect())
}
fn projection(columns: &[Column]) -> String {
    columns
        .iter()
        .map(|column| {
            let name = quote_identifier(&column.name);
            if [
                "BINARY",
                "BLOB",
                "BIT",
                "GEOMETRY",
                "POINT",
                "LINESTRING",
                "POLYGON",
            ]
            .iter()
            .any(|t| column.data_type.to_uppercase().contains(t))
            {
                format!("HEX(source.{name}) AS {name}")
            } else {
                format!("CAST(source.{name} AS CHAR) AS {name}")
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}
async fn page(
    connection: &mut MySqlConnection,
    source: &str,
    columns: Vec<Column>,
    order: &str,
    page: u32,
    page_size: u32,
) -> AppResult<TablePage> {
    let sql = format!(
        "SELECT {} FROM {source} AS source{order} LIMIT ? OFFSET ?",
        projection(&columns)
    );
    let mut tx = connection.begin().await?;
    let records = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(u64::from(page_size) + 1)
        .bind(u64::from(page) * u64::from(page_size))
        .fetch_all(&mut *tx)
        .await?;
    let has_more = records.len() > page_size as usize;
    let rows = records
        .into_iter()
        .take(page_size as usize)
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
                .collect::<Result<_, sqlx::Error>>()
        })
        .collect::<Result<Vec<_>, _>>()?;
    let count_sql = format!("SELECT COUNT(*) FROM {source} AS dbsaur_count");
    let total: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(count_sql.as_str()))
        .fetch_one(&mut *tx)
        .await?;
    tx.rollback().await?;
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
pub(crate) async fn rows(input: &ConnectionInput, query: &BrowseQuery) -> AppResult<TablePage> {
    let (number, size) = query.bounds().map_err(invalid)?;
    // MySQL schemas are databases. Do not let single-database browsing escape its scope.
    if query.schema != input.database {
        return Err(invalid("Choose a table in the active database."));
    }
    let mut connection = connect(input).await?;
    let columns = metadata(
        &mut connection,
        &query.schema,
        &query.table,
        &input.provider,
    )
    .await?;
    let mut order = columns
        .iter()
        .filter(|c| c.primary_key)
        .map(|c| format!("source.{}", quote_identifier(&c.name)))
        .collect::<Vec<_>>();
    if let Some(sort) = &query.sort {
        if !columns.iter().any(|c| &c.name == sort) {
            return Err(invalid("Choose an existing column to sort."));
        }
        order.insert(
            0,
            format!(
                "source.{} {}",
                quote_identifier(sort),
                if query.descending.unwrap_or(false) {
                    "DESC"
                } else {
                    "ASC"
                }
            ),
        );
    }
    let order = if order.is_empty() {
        String::new()
    } else {
        format!(" ORDER BY {}", order.join(", "))
    };
    page(
        &mut connection,
        &format!(
            "{}.{}",
            quote_identifier(&query.schema),
            quote_identifier(&query.table)
        ),
        columns,
        &order,
        number,
        size,
    )
    .await
}
pub(crate) async fn query(input: &ConnectionInput, query: &SqlQuery) -> AppResult<TablePage> {
    let (number, size) = query.bounds().map_err(invalid)?;
    let mut sql = super::read_query::select(&query.sql, &sqlparser::dialect::MySqlDialect {})?;
    let mut connection = connect(input).await?;
    let mut edit_plan = None;
    if let Some(candidate) =
        super::query_editing::candidate(&sql, &sqlparser::dialect::MySqlDialect {}, false)
    {
        let (schema, table) = if candidate.name.len() == 2 {
            (candidate.name[0].clone(), candidate.name[1].clone())
        } else {
            (input.database.clone(), candidate.name[0].clone())
        };
        let target = sqlx::query("SELECT TABLE_SCHEMA,TABLE_NAME FROM information_schema.TABLES WHERE TABLE_SCHEMA=? AND TABLE_NAME=? AND TABLE_TYPE='BASE TABLE'").bind(&schema).bind(&table).fetch_optional(&mut connection).await?;
        if let Some(target) = target {
            let schema: String = target.try_get("TABLE_SCHEMA")?;
            let table: String = target.try_get("TABLE_NAME")?;
            let base = metadata(&mut connection, &schema, &table, &input.provider).await?;
            edit_plan = super::query_editing::prepare(
                candidate,
                schema,
                table,
                base,
                &sqlparser::dialect::MySqlDialect {},
                &input.provider,
            );
        }
    }
    if let Some(plan) = &edit_plan {
        sql = plan.sql.clone();
    }
    let source = format!("({sql})");
    let describe = format!("SELECT * FROM {source} AS source");
    let statement = connection
        .prepare(sqlx::AssertSqlSafe(describe.as_str()).into_sql_str())
        .await?;
    let mut names = std::collections::HashSet::new();
    let mut columns = Vec::new();
    for column in statement.columns() {
        if !names.insert(column.name().to_owned()) {
            return Err(invalid("Use unique column aliases for query results."));
        }
        columns.push(Column {
            name: column.name().into(),
            data_type: column.type_info().name().into(),
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
    let mut result = page(&mut connection, &source, columns, "", number, size).await?;
    if let Some(plan) = edit_plan {
        super::query_editing::finish(&mut result, plan)?;
    }
    Ok(result)
}
#[async_trait]
impl DatabaseAdapter for MySqlAdapter {
    async fn indexes(
        &self,
        input: &ConnectionInput,
        schema: &str,
        table: &str,
    ) -> AppResult<Vec<IndexInfo>> {
        super::read_query::bounded(indexes(input, schema, table)).await
    }

    async fn mutate(
        &self,
        input: &ConnectionInput,
        mutation: &Mutation,
    ) -> AppResult<MutationResult> {
        super::write::bounded(write::mutate(input, mutation)).await
    }
    fn maintenance_database(&self) -> Option<&'static str> {
        Some("")
    }
    fn provider(&self) -> Provider {
        Provider {
            id: "mysql",
            name: "MySQL",
        }
    }
    async fn test(&self, input: &ConnectionInput) -> AppResult<()> {
        super::read_query::bounded(async {
            let mut c = connect(input).await?;
            sqlx::query("SELECT 1").execute(&mut c).await?;
            Ok(())
        })
        .await
    }
    async fn databases(&self, input: &ConnectionInput) -> AppResult<Vec<String>> {
        super::read_query::bounded(databases(input)).await
    }
    async fn tables(&self, input: &ConnectionInput) -> AppResult<Vec<Table>> {
        super::read_query::bounded(tables(input)).await
    }
    async fn rows(&self, input: &ConnectionInput, q: &BrowseQuery) -> AppResult<TablePage> {
        super::read_query::bounded(rows(input, q)).await
    }
    async fn query(&self, input: &ConnectionInput, q: &SqlQuery) -> AppResult<TablePage> {
        super::read_query::bounded(query(input, q)).await
    }
}
async fn metadata(
    connection: &mut MySqlConnection,
    schema: &str,
    table: &str,
    provider: &str,
) -> AppResult<Vec<Column>> {
    let metadata = sqlx::query("SELECT COLUMN_NAME, COLUMN_TYPE, IS_NULLABLE, COLUMN_DEFAULT, COLUMN_KEY, EXTRA, CHARACTER_SET_NAME, COLLATION_NAME, COLUMN_COMMENT FROM information_schema.COLUMNS WHERE TABLE_SCHEMA=? AND TABLE_NAME=? ORDER BY ORDINAL_POSITION").bind(schema).bind(table).fetch_all(connection).await?;
    if metadata.is_empty() {
        return Err(AppError(
            ErrorKind::NotFound,
            "Table not found or not accessible.",
        ));
    }
    let columns = metadata
        .into_iter()
        .map(|row| {
            Ok(Column {
                name: row.try_get("COLUMN_NAME")?,
                data_type: row.try_get("COLUMN_TYPE")?,
                nullable: row.try_get::<String, _>("IS_NULLABLE")? == "YES",
                primary_key: row.try_get::<String, _>("COLUMN_KEY")? == "PRI",
                read_only: (row.try_get::<String, _>("EXTRA")?.contains("VIRTUAL")
                    || row.try_get::<String, _>("EXTRA")?.contains("STORED"))
                    || row
                        .try_get::<String, _>("EXTRA")?
                        .contains("auto_increment")
                    || row
                        .try_get::<String, _>("COLUMN_TYPE")?
                        .to_lowercase()
                        .split('(')
                        .next()
                        .is_some_and(|typ| {
                            matches!(
                                typ,
                                "geometry"
                                    | "point"
                                    | "linestring"
                                    | "polygon"
                                    | "multipoint"
                                    | "multilinestring"
                                    | "multipolygon"
                                    | "geometrycollection"
                            )
                        }),
                default_value: row.try_get("COLUMN_DEFAULT")?,
                definition: Some(ColumnDefinition {
                    data_type: row.try_get("COLUMN_TYPE")?,
                    nullable: row.try_get::<String, _>("IS_NULLABLE")? == "YES",
                    default_expression: row.try_get::<Option<String>, _>("COLUMN_DEFAULT")?.map(
                        |value| {
                            if provider == "mariadb"
                                || row
                                    .try_get::<String, _>("EXTRA")
                                    .unwrap_or_default()
                                    .contains("DEFAULT_GENERATED")
                                || value.to_ascii_lowercase().starts_with("current_timestamp")
                            {
                                value
                            } else {
                                super::column_change::mysql_literal(&value)
                            }
                        },
                    ),
                    character_set: row.try_get("CHARACTER_SET_NAME")?,
                    collation: row.try_get("COLLATION_NAME")?,
                    comment: row.try_get("COLUMN_COMMENT")?,
                    extra: row.try_get("EXTRA")?,
                    default_constraint: None,
                }),
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()?;

    Ok(columns)
}

pub(crate) async fn indexes(
    input: &ConnectionInput,
    schema: &str,
    table: &str,
) -> AppResult<Vec<IndexInfo>> {
    let mut c = connect(input).await?;
    let visibility = if input.provider == "mariadb" {
        "IGNORED='NO'"
    } else {
        "IS_VISIBLE='YES'"
    };
    let sql = format!(
        "SELECT INDEX_NAME,COLUMN_NAME,NON_UNIQUE,INDEX_TYPE,SUB_PART,COLLATION,INDEX_COMMENT,({visibility}) AS visible FROM information_schema.STATISTICS WHERE TABLE_SCHEMA=? AND TABLE_NAME=? ORDER BY INDEX_NAME,SEQ_IN_INDEX"
    );
    let rows = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(schema)
        .bind(table)
        .fetch_all(&mut c)
        .await?;
    let mut indexes = BTreeMap::<String, IndexInfo>::new();
    for r in rows {
        let name: String = r.try_get("INDEX_NAME")?;
        let column: Option<String> = r.try_get("COLUMN_NAME")?;
        let editable = r.try_get::<i64, _>("visible")? == 1
            && name != "PRIMARY"
            && column.is_some()
            && r.try_get::<String, _>("INDEX_TYPE")? == "BTREE"
            && r.try_get::<Option<i64>, _>("SUB_PART")?.is_none()
            && r.try_get::<Option<String>, _>("COLLATION")?.as_deref() == Some("A")
            && r.try_get::<String, _>("INDEX_COMMENT")?.is_empty();
        let unique = r.try_get::<i64, _>("NON_UNIQUE")? == 0;
        let index = indexes.entry(name.clone()).or_insert(IndexInfo {
            name,
            columns: vec![],
            unique,
            editable,
        });
        index.editable &= editable;
        index
            .columns
            .push(column.unwrap_or_else(|| "(expression)".into()));
    }
    Ok(indexes.into_values().collect())
}

#[cfg(test)]
mod tests {
    #[test]
    fn identifiers_are_quoted() {
        assert_eq!(
            super::quote_identifier("x`; DROP TABLE t;--"),
            "`x``; DROP TABLE t;--`"
        );
    }
}
