mod settings;
mod write;
use crate::adapters::sqlx;
use crate::{application::*, domain::*};
use async_trait::async_trait;
use sqlx::{
    Column as SqlxColumn, ConnectOptions, Connection, Executor, PgConnection, Row, SqlSafeStr,
    Statement, TypeInfo,
    postgres::{PgConnectOptions, PgSslMode},
};
use std::{str::FromStr, time::Duration};

pub struct PostgresAdapter;

pub(super) fn sensitive_setting(name: &str) -> bool {
    [
        "password",
        "secret",
        "token",
        "passphrase",
        "conninfo",
        "command",
    ]
    .iter()
    .any(|part| name.to_lowercase().contains(part))
}

fn redact_setting(mut setting: serde_json::Value) -> serde_json::Value {
    let name = setting["name"].as_str().unwrap_or_default().to_lowercase();
    // Connection strings and shell commands can embed credentials, even when
    // PostgreSQL grants this user permission to inspect their values.
    if sensitive_setting(&name) {
        for field in ["setting", "boot_val", "reset_val", "databaseValue"] {
            setting[field] = "[redacted]".into();
        }
    }
    setting
}

#[cfg(test)]
mod settings_tests {
    #[test]
    fn credentials_are_redacted_in_every_value() {
        for name in [
            "primary_conninfo",
            "ssl_passphrase_command",
            "extension.api_token",
        ] {
            let result = super::redact_setting(serde_json::json!({
                "name": name, "setting": "private", "boot_val": "private", "reset_val": "private"
            }));
            assert!(!result.to_string().contains("private"));
            assert_eq!(result["name"], name);
        }
        let ordinary = serde_json::json!({"name": "work_mem", "setting": "4096"});
        assert_eq!(super::redact_setting(ordinary.clone()), ordinary);
    }
}

impl From<sqlx::Error> for AppError {
    fn from(error: sqlx::Error) -> Self {
        // The infrastructure boundary replaces driver errors with safe messages.
        let message = match &error {
            sqlx::Error::Database(e) if e.code().as_deref() == Some("28P01") => {
                "Authentication failed. Check your username and password."
            }
            sqlx::Error::Database(e) if e.code().as_deref() == Some("42501") => {
                "This database user does not have permission to read that table."
            }
            sqlx::Error::Database(e) if e.code().as_deref() == Some("57014") => {
                "The query timed out. Try a smaller page."
            }
            _ => {
                "Database request failed. Check connection details, TLS settings, and read permissions."
            }
        };
        Self(ErrorKind::Upstream, message)
    }
}

pub fn quote_identifier(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

fn query_error(error: sqlx::Error) -> AppError {
    match &error {
        sqlx::Error::Database(e) => match e.code().as_deref() {
            Some("42601" | "42P01" | "42703" | "42702" | "42P18" | "42P02") => invalid(
                "Check the SQL syntax, table names, and column aliases. Run one SELECT query at a time.",
            ),
            Some("25006" | "0A000") => invalid("Only read-only SELECT queries are supported."),
            _ => error.into(),
        },
        _ => error.into(),
    }
}

pub(super) async fn connect(input: &ConnectionInput) -> AppResult<PgConnection> {
    let mut options = PgConnectOptions::from_str("postgresql://")?
        .host(&input.host)
        .port(input.port)
        .database(if input.database.is_empty() && input.all_databases {
            "postgres"
        } else {
            &input.database
        })
        .username(&input.username)
        .password(&input.password)
        .ssl_mode(if input.ssl {
            PgSslMode::VerifyFull
        } else {
            PgSslMode::Disable
        })
        .application_name("dbsaur")
        .disable_statement_logging();
    if let Some(pem) = &input.tls_ca {
        options = options.ssl_root_cert_from_pem(pem.as_bytes().to_vec());
    }
    if let Some(pem) = &input.tls_cert {
        options = options.ssl_client_cert_from_pem(pem.as_bytes());
    }
    if let Some(pem) = &input.tls_key {
        options = options.ssl_client_key_from_pem(pem.as_bytes());
    }
    tokio::time::timeout(
        Duration::from_secs(10),
        PgConnection::connect_with(&options),
    )
    .await
    .map_err(|_| {
        AppError(
            ErrorKind::Timeout,
            "Connection timed out. Check the host and port.",
        )
    })?
    .map_err(Into::into)
}

async fn begin(connection: &mut PgConnection) -> AppResult<sqlx::Transaction<'_, sqlx::Postgres>> {
    let mut tx = connection.begin().await?;
    sqlx::query("SET TRANSACTION READ ONLY")
        .execute(&mut *tx)
        .await?;
    sqlx::query("SET LOCAL statement_timeout = '10s'")
        .execute(&mut *tx)
        .await?;
    sqlx::query("SET LOCAL lock_timeout = '2s'")
        .execute(&mut *tx)
        .await?;
    Ok(tx)
}

#[async_trait]
impl DatabaseAdapter for PostgresAdapter {
    async fn geometry(
        &self,
        input: &ConnectionInput,
        request: &GeometryRequest,
    ) -> AppResult<GeometryPreview> {
        request.validate().map_err(invalid)?;
        let mut connection = connect(input).await?;
        let mut tx = begin(&mut connection).await?;
        // The extension can live outside public. Qualify every PostGIS function/type.
        let schema: Option<String> = sqlx::query_scalar("SELECT n.nspname::text FROM pg_catalog.pg_extension e JOIN pg_catalog.pg_namespace n ON n.oid=e.extnamespace WHERE e.extname='postgis'")
            .fetch_optional(&mut *tx).await?;
        let schema = quote_identifier(
            &schema.ok_or_else(|| invalid("PostGIS is not installed in this database."))?,
        );
        let sql = format!(
            "SELECT {schema}.ST_SRID(g) AS srid, {schema}.ST_IsEmpty(g) AS empty, CASE WHEN {schema}.ST_SRID(g) > 0 AND NOT {schema}.ST_IsEmpty(g) THEN {schema}.ST_AsGeoJSON({schema}.ST_Transform({schema}.ST_CurveToLine(g), 4326), 9, 0)::jsonb ELSE NULL END AS geojson FROM (SELECT $1::text::{schema}.geometry AS g) value"
        );
        let row = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(&request.value).fetch_one(&mut *tx).await
            .map_err(|_| invalid("This geometry could not be mapped. Check its validity and coordinate reference system; a 2D map supports points, lines, polygons, and collections."))?;
        let preview = GeometryPreview {
            srid: row.get("srid"),
            empty: row.get("empty"),
            geojson: row.get("geojson"),
        };
        tx.rollback().await?;
        Ok(preview)
    }
    fn maintenance_database(&self) -> Option<&'static str> {
        Some("postgres")
    }
    async fn update_settings(
        &self,
        input: &ConnectionInput,
        request: &UpdateDatabaseSettings,
    ) -> AppResult<()> {
        settings::update(input, request).await
    }
    async fn settings(&self, input: &ConnectionInput) -> AppResult<DatabaseSettings> {
        let mut connection = connect(input).await?;
        let mut tx = connection.begin().await?;
        sqlx::query("SET TRANSACTION READ ONLY")
            .execute(&mut *tx)
            .await?;
        // Read the effective configuration before applying our own timeout overrides.
        let settings: Vec<serde_json::Value> = tokio::time::timeout(Duration::from_secs(10), sqlx::query_scalar(
            "SELECT (to_jsonb(s) - 'sourcefile' - 'sourceline') || jsonb_build_object('databaseValue', (SELECT substr(config, strpos(config, '=') + 1) FROM pg_db_role_setting dr CROSS JOIN LATERAL unnest(dr.setconfig) config WHERE dr.setdatabase = (SELECT oid FROM pg_database WHERE datname = current_database()) AND dr.setrole = 0 AND split_part(config, '=', 1) = s.name)) FROM pg_settings s ORDER BY category, name"
        ).fetch_all(&mut *tx)).await.map_err(|_| AppError(ErrorKind::Timeout, "Reading database settings timed out. Try again."))??;
        sqlx::query("SET LOCAL statement_timeout = '10s'")
            .execute(&mut *tx)
            .await?;
        sqlx::query("SET LOCAL lock_timeout = '2s'")
            .execute(&mut *tx)
            .await?;
        let properties: serde_json::Value = sqlx::query_scalar(
            "SELECT to_jsonb(d) || jsonb_build_object('owner', pg_get_userbyid(datdba), 'encoding', pg_encoding_to_char(encoding), 'tablespace', (SELECT spcname FROM pg_tablespace WHERE oid = d.dattablespace)) FROM pg_database d WHERE datname = current_database()"
        ).fetch_one(&mut *tx).await?;
        let (can_edit, superuser) = settings::permissions(&mut tx).await?;
        tx.rollback().await?;
        Ok(DatabaseSettings {
            provider: input.provider.clone(),
            database: input.database.clone(),
            properties: properties.as_object().cloned().unwrap_or_default(),
            settings: settings
                .into_iter()
                .map(|mut setting| {
                    setting["databaseScoped"] = settings::editable(
                        setting["name"].as_str().unwrap_or_default(),
                        setting["context"].as_str().unwrap_or_default(),
                        true,
                    )
                    .into();
                    setting["editable"] = (can_edit
                        && settings::editable(
                            setting["name"].as_str().unwrap_or_default(),
                            setting["context"].as_str().unwrap_or_default(),
                            superuser,
                        ))
                    .into();
                    redact_setting(setting)
                })
                .collect(),
            can_edit,
        })
    }
    async fn mutate(
        &self,
        input: &ConnectionInput,
        mutation: &Mutation,
    ) -> AppResult<MutationResult> {
        write::mutate(input, mutation).await
    }
    async fn databases(&self, input: &ConnectionInput) -> AppResult<Vec<String>> {
        let mut connection = connect(input).await?;
        let mut tx = begin(&mut connection).await?;
        let names = sqlx::query_scalar("SELECT datname::text FROM pg_database WHERE NOT datistemplate AND datallowconn AND has_database_privilege(datname, 'CONNECT') ORDER BY datname")
            .fetch_all(&mut *tx).await?;
        tx.commit().await?;
        Ok(names)
    }
    fn provider(&self) -> Provider {
        Provider {
            id: "postgres",
            name: "PostgreSQL",
        }
    }
    async fn test(&self, input: &ConnectionInput) -> AppResult<()> {
        let mut connection = connect(input).await?;
        let mut tx = begin(&mut connection).await?;
        sqlx::query("SELECT 1").execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }
    async fn query(&self, input: &ConnectionInput, query: &SqlQuery) -> AppResult<TablePage> {
        let (page, page_size) = query.bounds().map_err(invalid)?;
        let mut connection = connect(input).await?;
        let mut tx = begin(&mut connection).await?;
        let mut edit_plan = None;
        if let Some(candidate) = super::query_editing::candidate(
            query.statement(),
            &sqlparser::dialect::PostgreSqlDialect {},
            true,
        ) {
            let source = candidate
                .name
                .iter()
                .map(|name| quote_identifier(name))
                .collect::<Vec<_>>()
                .join(".");
            let target = sqlx::query("SELECT n.nspname AS schema,c.relname AS name FROM pg_catalog.pg_class c JOIN pg_catalog.pg_namespace n ON n.oid=c.relnamespace WHERE c.oid=to_regclass($1) AND c.relkind IN ('r','p')")
                .bind(source).fetch_optional(&mut *tx).await?;
            if let Some(target) = target {
                let schema: String = target.get("schema");
                let table: String = target.get("name");
                let base = metadata(&mut tx, &schema, &table).await?;
                edit_plan = super::query_editing::prepare(
                    candidate,
                    schema,
                    table,
                    base,
                    &sqlparser::dialect::PostgreSqlDialect {},
                    "postgres",
                );
            }
        }
        let sql_source = edit_plan
            .as_ref()
            .map(|plan| plan.sql.as_str())
            .unwrap_or_else(|| query.statement());
        // Preparing a derived-table SELECT uses PostgreSQL's parser and single-statement
        // extended protocol. Commands cannot run here; execution remains READ ONLY.
        let source = format!("SELECT * FROM (\n{}\n) AS webdb_query", sql_source);
        let statement = (&mut *tx)
            .prepare(sqlx::AssertSqlSafe(source.as_str()).into_sql_str())
            .await
            .map_err(query_error)?;
        let mut names = std::collections::HashSet::new();
        let mut columns = Vec::new();
        let mut projection = Vec::new();
        for column in statement.columns() {
            if !names.insert(column.name().to_owned()) {
                return Err(invalid("Use unique column aliases for query results."));
            }
            let name = quote_identifier(column.name());
            let data_type = column.type_info().name();
            projection.push(
                if matches!(
                    data_type,
                    "INT8" | "NUMERIC" | "JSON" | "JSONB" | "geometry" | "geography"
                ) || data_type.ends_with("[]")
                {
                    format!("webdb_query.{name}::text AS {name}")
                } else {
                    format!("webdb_query.{name}")
                },
            );
            columns.push(Column {
                name: column.name().to_owned(),
                data_type: data_type.to_lowercase(),
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
        let sql = format!(
            "SELECT to_jsonb(record) AS value FROM (SELECT {} FROM (\n{}\n) AS webdb_query LIMIT $1 OFFSET $2) AS record",
            projection.join(", "),
            sql_source
        );
        // SQL is intentionally user-authored. Generated identifiers are quoted and
        // pagination values are bound; the read-only transaction is the write barrier.
        let records = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(i64::from(page_size) + 1)
            .bind(i64::from(page) * i64::from(page_size))
            .fetch_all(&mut *tx)
            .await
            .map_err(query_error)?;
        let has_more = records.len() > page_size as usize;
        let rows = records
            .into_iter()
            .take(page_size as usize)
            .map(|r| {
                let value: serde_json::Value = r.get("value");
                value.as_object().cloned().unwrap_or_default()
            })
            .collect();
        let count_sql = format!("SELECT COUNT(*) AS total FROM (\n{sql_source}\n) AS dbsaur_count");
        let total: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(count_sql.as_str()))
            .fetch_one(&mut *tx)
            .await
            .map_err(query_error)?;
        tx.rollback().await?;
        let mut result = TablePage {
            columns,
            rows,
            has_more,

            total_rows: Some(total.to_string()),

            editing: None,
            page,
            page_size,
        };
        if let Some(plan) = edit_plan {
            super::query_editing::finish(&mut result, plan)?;
        }
        Ok(result)
    }
    async fn tables(&self, input: &ConnectionInput) -> AppResult<Vec<Table>> {
        let mut connection = connect(input).await?;
        let mut tx = begin(&mut connection).await?;
        let rows = sqlx::query("SELECT t.table_schema, t.table_name, t.table_type, ARRAY(SELECT c.column_name::text FROM information_schema.columns c WHERE c.table_schema=t.table_schema AND c.table_name=t.table_name ORDER BY c.ordinal_position) AS columns FROM information_schema.tables t WHERE t.table_schema NOT IN ('pg_catalog', 'information_schema') ORDER BY t.table_schema, t.table_name")
            .fetch_all(&mut *tx).await?;
        let mut tables: Vec<Table> = rows
            .iter()
            .map(|r| Table {
                schema: r.get("table_schema"),
                name: r.get("table_name"),
                kind: r.get("table_type"),
                columns: r.get("columns"),
                primary_keys: vec![],
                key_constraints: vec![],
                referenceable_columns: vec![],
                foreign_keys: vec![],
            })
            .collect();
        let key_constraints = sqlx::query("SELECT tc.table_schema,tc.table_name,tc.constraint_name,tc.constraint_type,kcu.column_name FROM information_schema.table_constraints tc JOIN information_schema.key_column_usage kcu ON tc.constraint_catalog=kcu.constraint_catalog AND tc.constraint_schema=kcu.constraint_schema AND tc.constraint_name=kcu.constraint_name AND tc.table_schema=kcu.table_schema AND tc.table_name=kcu.table_name WHERE tc.constraint_type IN ('PRIMARY KEY','UNIQUE') AND tc.table_schema NOT IN ('pg_catalog','information_schema') ORDER BY tc.table_schema,tc.table_name,tc.constraint_name,kcu.ordinal_position")
            .fetch_all(&mut *tx).await?;
        for key in key_constraints {
            let schema: String = key.get("table_schema");
            let table_name: String = key.get("table_name");
            let name: String = key.get("constraint_name");
            let kind: String = key.get("constraint_type");
            let column: String = key.get("column_name");
            if let Some(table) = tables
                .iter_mut()
                .find(|table| table.schema == schema && table.name == table_name)
            {
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
        // Include standalone unique indexes, not just named table constraints.
        let referenceable = sqlx::query("SELECT n.nspname AS table_schema,c.relname AS table_name,a.attname AS column_name FROM pg_catalog.pg_index i JOIN pg_catalog.pg_class c ON c.oid=i.indrelid JOIN pg_catalog.pg_namespace n ON n.oid=c.relnamespace JOIN pg_catalog.pg_attribute a ON a.attrelid=c.oid AND a.attnum=i.indkey[0] WHERE i.indisunique AND i.indisvalid AND i.indimmediate AND i.indpred IS NULL AND i.indexprs IS NULL AND i.indnkeyatts=1 AND n.nspname NOT IN ('pg_catalog','information_schema')")
            .fetch_all(&mut *tx).await?;
        for row in referenceable {
            let schema: String = row.get("table_schema");
            let name: String = row.get("table_name");
            let column: String = row.get("column_name");
            if let Some(table) = tables
                .iter_mut()
                .find(|table| table.schema == schema && table.name == name)
                && !table.referenceable_columns.contains(&column)
            {
                table.referenceable_columns.push(column);
            }
        }
        let relationships = sqlx::query("SELECT con.conname AS constraint_name,source_ns.nspname AS table_schema,source_table.relname AS table_name,source_column.attname AS column_name,target_ns.nspname AS referenced_schema,target_table.relname AS referenced_table,target_column.attname AS referenced_column FROM pg_catalog.pg_constraint con JOIN pg_catalog.pg_class source_table ON source_table.oid=con.conrelid JOIN pg_catalog.pg_namespace source_ns ON source_ns.oid=source_table.relnamespace JOIN pg_catalog.pg_class target_table ON target_table.oid=con.confrelid JOIN pg_catalog.pg_namespace target_ns ON target_ns.oid=target_table.relnamespace JOIN LATERAL unnest(con.conkey) WITH ORDINALITY AS source_key(attnum,ord) ON true JOIN LATERAL unnest(con.confkey) WITH ORDINALITY AS target_key(attnum,ord) ON target_key.ord=source_key.ord JOIN pg_catalog.pg_attribute source_column ON source_column.attrelid=source_table.oid AND source_column.attnum=source_key.attnum JOIN pg_catalog.pg_attribute target_column ON target_column.attrelid=target_table.oid AND target_column.attnum=target_key.attnum WHERE con.contype='f' ORDER BY source_ns.nspname,source_table.relname,con.conname,source_key.ord")
            .fetch_all(&mut *tx).await?;
        for relation in relationships {
            let schema: String = relation.get("table_schema");
            let table_name: String = relation.get("table_name");
            if let Some(table) = tables
                .iter_mut()
                .find(|table| table.schema == schema && table.name == table_name)
            {
                table.foreign_keys.push(ForeignKey {
                    name: relation.get("constraint_name"),
                    column: relation.get("column_name"),
                    referenced_schema: relation.get("referenced_schema"),
                    referenced_table: relation.get("referenced_table"),
                    referenced_column: relation.get("referenced_column"),
                });
            }
        }
        tx.commit().await?;
        Ok(tables)
    }
    async fn rows(&self, input: &ConnectionInput, query: &BrowseQuery) -> AppResult<TablePage> {
        let (page, page_size) = query.bounds().map_err(invalid)?;
        let mut connection = connect(input).await?;
        let mut tx = begin(&mut connection).await?;
        let columns = metadata(&mut tx, &query.schema, &query.table).await?;
        if let Some(sort) = &query.sort
            && !columns.iter().any(|c| &c.name == sort)
        {
            return Err(invalid("Choose an existing column to sort."));
        }
        let mut order: Vec<String> = columns
            .iter()
            .filter(|c| c.primary_key)
            .map(|c| format!("source.{}", quote_identifier(&c.name)))
            .collect();
        if let Some(sort) = &query.sort {
            order.insert(
                0,
                format!(
                    "source.{} {} NULLS LAST",
                    quote_identifier(sort),
                    if query.descending.unwrap_or(false) {
                        "DESC"
                    } else {
                        "ASC"
                    }
                ),
            );
        }
        // Physical identifiers break ties, including across partitions, for unchanged data.
        let is_table: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM information_schema.tables WHERE table_schema=$1 AND table_name=$2 AND table_type='BASE TABLE')").bind(&query.schema).bind(&query.table).fetch_one(&mut *tx).await?;
        if is_table {
            order.push("source.tableoid".into());
            order.push("source.ctid".into());
        }
        let order_by = if order.is_empty() {
            String::new()
        } else {
            format!(" ORDER BY {}", order.join(", "))
        };
        // Preserve exact 64-bit integers and decimals across JavaScript's number boundary.
        let projection = columns
            .iter()
            .map(|c| {
                let name = quote_identifier(&c.name);
                if matches!(
                    c.data_type.as_str(),
                    "bigint"
                        | "numeric"
                        | "decimal"
                        | "ARRAY"
                        | "json"
                        | "jsonb"
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
        let sql = format!(
            "SELECT to_jsonb(record) AS value FROM (SELECT {projection} FROM {}.{} AS source{order_by} LIMIT $1 OFFSET $2) record",
            quote_identifier(&query.schema),
            quote_identifier(&query.table)
        );
        // Dynamic fragments are only quoted identifiers from validated metadata;
        // page size and offset remain bind parameters.
        let records = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(i64::from(page_size) + 1)
            .bind(i64::from(page) * i64::from(page_size))
            .fetch_all(&mut *tx)
            .await?;
        let has_more = records.len() > page_size as usize;
        let rows = records
            .into_iter()
            .take(page_size as usize)
            .map(|r| {
                let value: serde_json::Value = r.get("value");
                value.as_object().cloned().unwrap_or_default()
            })
            .collect();
        let count_sql = format!(
            "SELECT COUNT(*) FROM {}.{}",
            quote_identifier(&query.schema),
            quote_identifier(&query.table)
        );
        let total: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(count_sql.as_str()))
            .fetch_one(&mut *tx)
            .await?;
        tx.commit().await?;
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
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn identifier_escaping() {
        assert_eq!(
            quote_identifier("a\"; DROP TABLE users;--"),
            "\"a\"\"; DROP TABLE users;--\""
        );
    }
}

fn column_definition(row: &sqlx::postgres::PgRow) -> ColumnDefinition {
    ColumnDefinition {
        data_type: row.get("full_type"),
        nullable: row.get::<String, _>("is_nullable") == "YES",
        default_expression: row.get("column_default"),
        character_set: row.get("character_set"),
        collation: row.get("collation_name"),
        comment: String::new(),
        extra: row.get("schema_extra"),
        default_constraint: None,
    }
}
pub(super) async fn schema_column(
    connection: &mut sqlx::PgConnection,
    schema: &str,
    table: &str,
    column: &str,
) -> AppResult<ColumnDefinition> {
    let row = sqlx::query("SELECT pg_catalog.format_type(a.atttypid,a.atttypmod) AS full_type,c.is_nullable,c.column_default,CASE WHEN c.collation_name IS NULL THEN NULL ELSE quote_ident(c.collation_schema)||'.'||quote_ident(c.collation_name) END AS collation_name,current_setting('server_encoding') AS character_set,CASE WHEN c.is_generated='ALWAYS' OR c.is_identity='YES' THEN 'generated' ELSE '' END AS schema_extra FROM information_schema.columns c JOIN pg_catalog.pg_class t ON t.relname=c.table_name JOIN pg_catalog.pg_namespace n ON n.oid=t.relnamespace AND n.nspname=c.table_schema JOIN pg_catalog.pg_attribute a ON a.attrelid=t.oid AND a.attname=c.column_name AND a.attnum>0 AND NOT a.attisdropped WHERE c.table_schema=$1 AND c.table_name=$2 AND c.column_name=$3 AND t.relkind IN ('r','p')")
        .bind(schema).bind(table).bind(column).fetch_optional(connection).await?
        .ok_or_else(|| invalid("Choose an existing column in an accessible base table."))?;
    Ok(column_definition(&row))
}

async fn metadata(
    connection: &mut PgConnection,
    schema: &str,
    table: &str,
) -> AppResult<Vec<Column>> {
    let metadata = sqlx::query(r#"SELECT c.column_name, CASE WHEN c.udt_name IN ('geometry', 'geography') THEN c.udt_name ELSE c.data_type END AS data_type, c.is_nullable, c.column_default,
          (SELECT pg_catalog.format_type(a.atttypid,a.atttypmod) FROM pg_catalog.pg_attribute a JOIN pg_catalog.pg_class t ON t.oid=a.attrelid JOIN pg_catalog.pg_namespace n ON n.oid=t.relnamespace WHERE n.nspname=c.table_schema AND t.relname=c.table_name AND a.attname=c.column_name AND a.attnum>0 AND NOT a.attisdropped) AS full_type,
          CASE WHEN c.collation_name IS NULL THEN NULL ELSE quote_ident(c.collation_schema)||'.'||quote_ident(c.collation_name) END AS collation_name,current_setting('server_encoding') AS character_set,
          CASE WHEN c.is_generated='ALWAYS' OR c.is_identity='YES' THEN 'generated' ELSE '' END AS schema_extra,
          (c.is_generated='ALWAYS' OR COALESCE(c.identity_generation='ALWAYS', false)) AS read_only,
          EXISTS (SELECT 1 FROM information_schema.table_constraints tc JOIN information_schema.key_column_usage k
          ON tc.constraint_catalog=k.constraint_catalog AND tc.constraint_schema=k.constraint_schema AND tc.constraint_name=k.constraint_name
          WHERE tc.constraint_type='PRIMARY KEY' AND tc.table_schema=c.table_schema AND tc.table_name=c.table_name AND k.table_schema=c.table_schema AND k.table_name=c.table_name AND k.column_name=c.column_name) AS primary_key
          FROM information_schema.columns c WHERE c.table_schema=$1 AND c.table_name=$2 ORDER BY c.ordinal_position"#)
            .bind(schema).bind(table).fetch_all(connection).await?;
    if metadata.is_empty() {
        return Err(AppError(
            ErrorKind::NotFound,
            "Table not found or not accessible.",
        ));
    }
    let columns: Vec<Column> = metadata
        .iter()
        .map(|r| Column {
            name: r.get("column_name"),
            data_type: r.get("data_type"),
            nullable: r.get::<String, _>("is_nullable") == "YES",
            primary_key: r.get("primary_key"),
            read_only: r.get("read_only"),
            default_value: r.get("column_default"),
            definition: Some(column_definition(r)),
        })
        .collect();
    Ok(columns)
}
