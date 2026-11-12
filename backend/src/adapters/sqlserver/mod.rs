mod write;
use crate::{application::*, domain::*};
use async_trait::async_trait;
use std::{collections::BTreeMap, time::Duration};
use tiberius::{AuthMethod, Client, Config, EncryptionLevel, Row};
use tokio::net::TcpStream;
use tokio_util::compat::{Compat, TokioAsyncWriteCompatExt};

type SqlClient = Client<Compat<TcpStream>>;
pub struct SqlServerAdapter;
fn driver_error(_: tiberius::error::Error) -> AppError {
    AppError(
        ErrorKind::Upstream,
        "SQL Server request failed. Check connection details, TLS, permissions, SQL syntax, and unique column aliases.",
    )
}
async fn connect(input: &ConnectionInput) -> AppResult<SqlClient> {
    connect_mode(input, true).await
}
async fn connect_mode(input: &ConnectionInput, read_only: bool) -> AppResult<SqlClient> {
    let mut config = Config::new();
    config.host(&input.host);
    config.port(input.port);
    config.database(&input.database);
    config.authentication(AuthMethod::sql_server(&input.username, &input.password));
    config.application_name("dbsaur");
    config.encryption(if input.ssl {
        EncryptionLevel::Required
    } else {
        EncryptionLevel::NotSupported
    });
    config.command_timeout(Some(Duration::from_secs(10)));
    // ApplicationIntent is a routing hint; SQL validation below is still required.
    config.readonly(read_only);
    let mut client = tokio::time::timeout(Duration::from_secs(10), async {
        let tcp = TcpStream::connect(config.get_addr()).await.map_err(|_| {
            AppError(
                ErrorKind::Upstream,
                "Cannot reach SQL Server. Check the host and TCP port.",
            )
        })?;
        tcp.set_nodelay(true).map_err(|_| {
            AppError(
                ErrorKind::Upstream,
                "Cannot configure SQL Server connection.",
            )
        })?;
        Client::connect(config, tcp.compat_write())
            .await
            .map_err(driver_error)
    })
    .await
    .map_err(|_| AppError(ErrorKind::Timeout, "SQL Server connection timed out."))??;
    client
        .simple_query("SET IMPLICIT_TRANSACTIONS OFF; SET LOCK_TIMEOUT 2000; SET NOCOUNT ON;")
        .await
        .map_err(driver_error)?
        .into_results()
        .await
        .map_err(driver_error)?;
    Ok(client)
}
pub fn quote_identifier(name: &str) -> String {
    format!("[{}]", name.replace(']', "]]"))
}
fn text(row: &Row, name: &str) -> AppResult<String> {
    row.try_get::<&str, _>(name)
        .map_err(driver_error)?
        .map(str::to_owned)
        .ok_or_else(|| invalid("Database metadata is unavailable."))
}
async fn metadata(client: &mut SqlClient, schema: &str, table: &str) -> AppResult<Vec<Column>> {
    let rows = client.query("SELECT c.COLUMN_NAME,c.DATA_TYPE,c.IS_NULLABLE,c.COLUMN_DEFAULT,c.COLLATION_NAME,CASE WHEN c.DATA_TYPE IN ('varchar','nvarchar','char','nchar','binary','varbinary') THEN c.DATA_TYPE+'('+CASE WHEN c.CHARACTER_MAXIMUM_LENGTH=-1 THEN 'max' ELSE CONVERT(varchar(10),c.CHARACTER_MAXIMUM_LENGTH) END+')' WHEN c.DATA_TYPE IN ('decimal','numeric') THEN c.DATA_TYPE+'('+CONVERT(varchar(10),c.NUMERIC_PRECISION)+','+CONVERT(varchar(10),c.NUMERIC_SCALE)+')' WHEN c.DATA_TYPE IN ('time','datetime2','datetimeoffset') THEN c.DATA_TYPE+'('+CONVERT(varchar(10),c.DATETIME_PRECISION)+')' ELSE c.DATA_TYPE END AS full_type,(SELECT dc.name FROM sys.default_constraints dc JOIN sys.columns sc ON sc.object_id=dc.parent_object_id AND sc.column_id=dc.parent_column_id WHERE dc.parent_object_id=OBJECT_ID(QUOTENAME(c.TABLE_SCHEMA)+'.'+QUOTENAME(c.TABLE_NAME)) AND sc.name=c.COLUMN_NAME) AS default_constraint,CAST(CASE WHEN COALESCE(COLUMNPROPERTY(OBJECT_ID(QUOTENAME(c.TABLE_SCHEMA)+'.'+QUOTENAME(c.TABLE_NAME)),c.COLUMN_NAME,'IsIdentity'),0)=1 OR COALESCE(COLUMNPROPERTY(OBJECT_ID(QUOTENAME(c.TABLE_SCHEMA)+'.'+QUOTENAME(c.TABLE_NAME)),c.COLUMN_NAME,'IsComputed'),0)=1 OR COALESCE(COLUMNPROPERTY(OBJECT_ID(QUOTENAME(c.TABLE_SCHEMA)+'.'+QUOTENAME(c.TABLE_NAME)),c.COLUMN_NAME,'GeneratedAlwaysType'),0)<>0 OR c.DATA_TYPE IN ('timestamp','rowversion','geometry','geography','hierarchyid','sql_variant') THEN 1 ELSE 0 END AS bit) AS read_only,CAST(CASE WHEN EXISTS (SELECT 1 FROM INFORMATION_SCHEMA.TABLE_CONSTRAINTS tc JOIN INFORMATION_SCHEMA.KEY_COLUMN_USAGE k ON tc.CONSTRAINT_CATALOG=k.CONSTRAINT_CATALOG AND tc.CONSTRAINT_SCHEMA=k.CONSTRAINT_SCHEMA AND tc.CONSTRAINT_NAME=k.CONSTRAINT_NAME WHERE tc.CONSTRAINT_TYPE='PRIMARY KEY' AND tc.TABLE_SCHEMA=c.TABLE_SCHEMA AND tc.TABLE_NAME=c.TABLE_NAME AND k.COLUMN_NAME=c.COLUMN_NAME) THEN 1 ELSE 0 END AS bit) AS primary_key FROM INFORMATION_SCHEMA.COLUMNS c WHERE c.TABLE_SCHEMA=@P1 AND c.TABLE_NAME=@P2 ORDER BY c.ORDINAL_POSITION", &[&schema,&table]).await.map_err(driver_error)?.into_first_result().await.map_err(driver_error)?;
    rows.iter()
        .map(|row| {
            Ok(Column {
                name: text(row, "COLUMN_NAME")?,
                data_type: text(row, "DATA_TYPE")?,
                nullable: text(row, "IS_NULLABLE")? == "YES",
                primary_key: row
                    .try_get::<bool, _>("primary_key")
                    .map_err(driver_error)?
                    .unwrap_or(false),
                read_only: row
                    .try_get::<bool, _>("read_only")
                    .map_err(driver_error)?
                    .unwrap_or(true),
                default_value: row
                    .try_get::<&str, _>("COLUMN_DEFAULT")
                    .map_err(driver_error)?
                    .map(str::to_owned),
                definition: Some(ColumnDefinition {
                    data_type: text(row, "full_type")?,
                    nullable: text(row, "IS_NULLABLE")? == "YES",
                    default_expression: row
                        .try_get::<&str, _>("COLUMN_DEFAULT")
                        .map_err(driver_error)?
                        .map(str::to_owned),
                    character_set: None,
                    collation: row
                        .try_get::<&str, _>("COLLATION_NAME")
                        .map_err(driver_error)?
                        .map(str::to_owned),
                    comment: String::new(),
                    extra: if row
                        .try_get::<bool, _>("read_only")
                        .map_err(driver_error)?
                        .unwrap_or(true)
                    {
                        "generated".into()
                    } else {
                        String::new()
                    },
                    default_constraint: row
                        .try_get::<&str, _>("default_constraint")
                        .map_err(driver_error)?
                        .map(str::to_owned),
                }),
            })
        })
        .collect()
}
fn projection(columns: &[Column]) -> String {
    columns
        .iter()
        .map(|column| {
            let name = quote_identifier(&column.name);
            let binary = ["binary", "image", "timestamp", "rowversion"]
                .iter()
                .any(|t| column.data_type.contains(t));
            let style = if binary || matches!(column.data_type.as_str(), "money" | "smallmoney") {
                2
            } else if column.data_type.starts_with("float") || column.data_type.starts_with("real")
            {
                3
            } else {
                126
            };
            let expression = if binary {
                format!("CONVERT(varbinary(max), source.{name})")
            } else {
                format!("source.{name}")
            };
            format!("CONVERT(nvarchar(max), {expression}, {style}) AS {name}")
        })
        .collect::<Vec<_>>()
        .join(", ")
}
async fn page(
    client: &mut SqlClient,
    source: &str,
    columns: Vec<Column>,
    order: &str,
    number: u32,
    size: u32,
) -> AppResult<TablePage> {
    let sql = format!(
        "SELECT {} FROM {source} AS source ORDER BY {order} OFFSET @P1 ROWS FETCH NEXT @P2 ROWS ONLY",
        projection(&columns)
    );
    let offset = i64::from(number) * i64::from(size);
    let limit = i64::from(size) + 1;
    let records = client
        .query(sql, &[&offset, &limit])
        .await
        .map_err(driver_error)?
        .into_first_result()
        .await
        .map_err(driver_error)?;
    let has_more = records.len() > size as usize;
    let rows = records
        .into_iter()
        .take(size as usize)
        .map(|row| {
            columns
                .iter()
                .enumerate()
                .map(|(index, column)| {
                    Ok((
                        column.name.clone(),
                        row.try_get::<&str, _>(index)
                            .map_err(driver_error)?
                            .map_or(serde_json::Value::Null, |s| s.into()),
                    ))
                })
                .collect::<AppResult<_>>()
        })
        .collect::<AppResult<_>>()?;
    let count_sql = format!("SELECT COUNT_BIG(*) AS total FROM {source} AS dbsaur_count");
    let count = client
        .query(count_sql, &[])
        .await
        .map_err(driver_error)?
        .into_first_result()
        .await
        .map_err(driver_error)?;
    let total = count
        .first()
        .and_then(|row| row.get::<i64, _>("total"))
        .ok_or_else(|| invalid("The result count is unavailable."))?;
    Ok(TablePage {
        columns,
        rows,
        has_more,

        total_rows: Some(total.to_string()),

        editing: None,
        page: number,
        page_size: size,
    })
}
#[async_trait]
impl DatabaseAdapter for SqlServerAdapter {
    async fn indexes(
        &self,
        input: &ConnectionInput,
        schema: &str,
        table: &str,
    ) -> AppResult<Vec<IndexInfo>> {
        super::read_query::bounded(async {
            let mut c = connect(input).await?;
            let rows = c.query("SELECT i.name AS name, col.name AS column_name, i.is_unique, CAST(CASE WHEN i.type=2 AND i.is_primary_key=0 AND i.is_unique_constraint=0 AND i.has_filter=0 AND i.is_disabled=0 AND i.is_hypothetical=0 AND i.fill_factor=0 AND i.is_padded=0 AND i.ignore_dup_key=0 AND i.allow_row_locks=1 AND i.allow_page_locks=1 AND EXISTS(SELECT 1 FROM sys.filegroups fg WHERE fg.data_space_id=i.data_space_id AND fg.is_default=1) AND NOT EXISTS(SELECT 1 FROM sys.index_columns x WHERE x.object_id=i.object_id AND x.index_id=i.index_id AND (x.is_included_column=1 OR x.is_descending_key=1)) AND NOT EXISTS(SELECT 1 FROM sys.partitions p WHERE p.object_id=i.object_id AND p.index_id=i.index_id AND p.data_compression<>0) THEN 1 ELSE 0 END AS bit) AS editable FROM sys.indexes i JOIN sys.tables t ON t.object_id=i.object_id JOIN sys.schemas s ON s.schema_id=t.schema_id JOIN sys.index_columns ic ON ic.object_id=i.object_id AND ic.index_id=i.index_id JOIN sys.columns col ON col.object_id=ic.object_id AND col.column_id=ic.column_id WHERE s.name=@P1 AND t.name=@P2 AND i.index_id>0 ORDER BY i.name,ic.key_ordinal,ic.index_column_id", &[&schema,&table]).await.map_err(driver_error)?.into_first_result().await.map_err(driver_error)?;
            let mut indexes = BTreeMap::<String, IndexInfo>::new();
            for r in rows {
                let name = text(&r,"name")?;
                let editable = r.try_get::<bool,_>("editable").map_err(driver_error)?.unwrap_or(false);
                let unique = r.try_get::<bool,_>("is_unique").map_err(driver_error)?.unwrap_or(false);
                indexes.entry(name.clone()).or_insert(IndexInfo { name, columns: vec![], unique, editable }).columns.push(text(&r,"column_name")?);
            }
            Ok(indexes.into_values().collect())
        }).await
    }

    async fn mutate(
        &self,
        input: &ConnectionInput,
        mutation: &Mutation,
    ) -> AppResult<MutationResult> {
        super::write::bounded(write::mutate(input, mutation)).await
    }
    fn maintenance_database(&self) -> Option<&'static str> {
        Some("master")
    }
    fn provider(&self) -> Provider {
        Provider {
            id: "sqlserver",
            name: "SQL Server",
        }
    }
    async fn test(&self, input: &ConnectionInput) -> AppResult<()> {
        let mut c = connect(input).await?;
        c.query("SELECT 1", &[])
            .await
            .map_err(driver_error)?
            .into_results()
            .await
            .map_err(driver_error)?;
        Ok(())
    }
    async fn databases(&self, input: &ConnectionInput) -> AppResult<Vec<String>> {
        let mut c = connect(input).await?;
        let rows=c.query("SELECT name FROM sys.databases WHERE state=0 AND HAS_DBACCESS(name)=1 AND name NOT IN ('master','model','msdb','tempdb') ORDER BY name",&[]).await.map_err(driver_error)?.into_first_result().await.map_err(driver_error)?;
        rows.iter().map(|row| text(row, "name")).collect()
    }
    async fn tables(&self, input: &ConnectionInput) -> AppResult<Vec<Table>> {
        let mut c = connect(input).await?;
        let records=c.query("SELECT t.TABLE_SCHEMA,t.TABLE_NAME,t.TABLE_TYPE,c.COLUMN_NAME FROM INFORMATION_SCHEMA.TABLES t LEFT JOIN INFORMATION_SCHEMA.COLUMNS c ON c.TABLE_SCHEMA=t.TABLE_SCHEMA AND c.TABLE_NAME=t.TABLE_NAME ORDER BY t.TABLE_SCHEMA,t.TABLE_NAME,c.ORDINAL_POSITION",&[]).await.map_err(driver_error)?.into_first_result().await.map_err(driver_error)?;
        let mut tables = BTreeMap::<(String, String), Table>::new();
        for row in &records {
            let schema = text(row, "TABLE_SCHEMA")?;
            let name = text(row, "TABLE_NAME")?;
            let table = tables
                .entry((schema.clone(), name.clone()))
                .or_insert(Table {
                    schema,
                    name,
                    kind: text(row, "TABLE_TYPE")?,
                    columns: vec![],
                    primary_keys: vec![],
                    key_constraints: vec![],
                    referenceable_columns: vec![],
                    foreign_keys: vec![],
                });
            if let Some(name) = row
                .try_get::<&str, _>("COLUMN_NAME")
                .map_err(driver_error)?
            {
                table.columns.push(name.into());
            }
        }
        let key_constraints = c.query("SELECT s.name AS table_schema,t.name AS table_name,kc.name AS constraint_name,CASE kc.type WHEN 'PK' THEN 'PRIMARY KEY' ELSE 'UNIQUE' END AS constraint_type,col.name AS column_name FROM sys.key_constraints kc JOIN sys.tables t ON t.object_id=kc.parent_object_id JOIN sys.schemas s ON s.schema_id=t.schema_id JOIN sys.index_columns ic ON ic.object_id=kc.parent_object_id AND ic.index_id=kc.unique_index_id AND ic.key_ordinal>0 JOIN sys.columns col ON col.object_id=t.object_id AND col.column_id=ic.column_id WHERE kc.type IN ('PK','UQ') ORDER BY s.name,t.name,kc.name,ic.key_ordinal",&[]).await.map_err(driver_error)?.into_first_result().await.map_err(driver_error)?;
        for row in &key_constraints {
            let schema = text(row, "table_schema")?;
            let table_name = text(row, "table_name")?;
            if let Some(table) = tables.get_mut(&(schema, table_name)) {
                let kind = text(row, "constraint_type")?;
                let name = text(row, "constraint_name")?;
                let column = text(row, "column_name")?;
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
        let referenceable = c.query("SELECT s.name AS table_schema,t.name AS table_name,MIN(col.name) AS column_name FROM sys.indexes i JOIN sys.tables t ON t.object_id=i.object_id JOIN sys.schemas s ON s.schema_id=t.schema_id JOIN sys.index_columns ic ON ic.object_id=i.object_id AND ic.index_id=i.index_id AND ic.key_ordinal>0 JOIN sys.columns col ON col.object_id=t.object_id AND col.column_id=ic.column_id WHERE i.is_unique=1 AND i.is_disabled=0 AND i.is_hypothetical=0 AND i.has_filter=0 GROUP BY s.name,t.name,i.index_id HAVING COUNT(*)=1", &[]).await.map_err(driver_error)?.into_first_result().await.map_err(driver_error)?;
        for row in &referenceable {
            let schema = text(row, "table_schema")?;
            let name = text(row, "table_name")?;
            let column = text(row, "column_name")?;
            if let Some(table) = tables.get_mut(&(schema, name))
                && !table.referenceable_columns.contains(&column)
            {
                table.referenceable_columns.push(column);
            }
        }
        let relationships = c.query("SELECT fk.name AS constraint_name,ss.name AS source_schema,st.name AS source_table,sc.name AS source_column,rs.name AS referenced_schema,rt.name AS referenced_table,rc.name AS referenced_column FROM sys.foreign_key_columns fkc JOIN sys.foreign_keys fk ON fk.object_id=fkc.constraint_object_id JOIN sys.tables st ON st.object_id=fkc.parent_object_id JOIN sys.schemas ss ON ss.schema_id=st.schema_id JOIN sys.columns sc ON sc.object_id=st.object_id AND sc.column_id=fkc.parent_column_id JOIN sys.tables rt ON rt.object_id=fkc.referenced_object_id JOIN sys.schemas rs ON rs.schema_id=rt.schema_id JOIN sys.columns rc ON rc.object_id=rt.object_id AND rc.column_id=fkc.referenced_column_id ORDER BY ss.name,st.name,fk.name,fkc.constraint_column_id",&[]).await.map_err(driver_error)?.into_first_result().await.map_err(driver_error)?;
        for row in &relationships {
            let schema = text(row, "source_schema")?;
            let table_name = text(row, "source_table")?;
            if let Some(table) = tables.get_mut(&(schema, table_name)) {
                table.foreign_keys.push(ForeignKey {
                    name: text(row, "constraint_name")?,
                    column: text(row, "source_column")?,
                    referenced_schema: text(row, "referenced_schema")?,
                    referenced_table: text(row, "referenced_table")?,
                    referenced_column: text(row, "referenced_column")?,
                });
            }
        }
        Ok(tables.into_values().collect())
    }
    async fn rows(&self, input: &ConnectionInput, query: &BrowseQuery) -> AppResult<TablePage> {
        let (number, size) = query.bounds().map_err(invalid)?;
        let mut c = connect(input).await?;
        let columns = metadata(&mut c, &query.schema, &query.table).await?;
        if columns.is_empty() {
            return Err(AppError(
                ErrorKind::NotFound,
                "Table not found or not accessible.",
            ));
        }
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
            "(SELECT NULL)".into()
        } else {
            order.join(", ")
        };
        page(
            &mut c,
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
    async fn query(&self, input: &ConnectionInput, query: &SqlQuery) -> AppResult<TablePage> {
        let (number, size) = query.bounds().map_err(invalid)?;
        let mut sql = super::read_query::select(&query.sql, &sqlparser::dialect::MsSqlDialect {})?;
        let mut c = connect(input).await?;
        let mut edit_plan = None;
        if let Some(candidate) =
            super::query_editing::candidate(&sql, &sqlparser::dialect::MsSqlDialect {}, false)
        {
            let target_name = candidate
                .name
                .iter()
                .map(|name| quote_identifier(name))
                .collect::<Vec<_>>()
                .join(".");
            let target = c.query("SELECT s.name AS schema_name,t.name AS table_name FROM sys.tables t JOIN sys.schemas s ON s.schema_id=t.schema_id WHERE t.object_id=OBJECT_ID(@P1)",&[&target_name]).await.map_err(driver_error)?.into_first_result().await.map_err(driver_error)?;
            if let Some(target) = target.first() {
                let schema = text(target, "schema_name")?;
                let table = text(target, "table_name")?;
                let base = metadata(&mut c, &schema, &table).await?;
                edit_plan = super::query_editing::prepare(
                    candidate,
                    schema,
                    table,
                    base,
                    &sqlparser::dialect::MsSqlDialect {},
                    "sqlserver",
                );
            }
        }
        if let Some(plan) = &edit_plan {
            sql = plan.sql.clone();
        }
        let source = format!("({sql})");
        let describe = format!("SELECT * FROM {source} AS source");
        // Describing the derived SELECT validates it with SQL Server without executing it.
        let records=c.query("SELECT name,system_type_name,is_nullable,error_number FROM sys.dm_exec_describe_first_result_set(@P1,NULL,0) WHERE is_hidden=0 OR error_number IS NOT NULL ORDER BY column_ordinal",&[&describe]).await.map_err(driver_error)?.into_first_result().await.map_err(driver_error)?;
        let mut names = std::collections::HashSet::new();
        let mut columns = Vec::new();
        for row in &records {
            if row
                .try_get::<i32, _>("error_number")
                .map_err(driver_error)?
                .is_some()
            {
                return Err(invalid(
                    "Check SQL Server SELECT syntax and aliases. Use TOP or OFFSET with an inner ORDER BY; CTEs are not supported here.",
                ));
            }
            let name = text(row, "name")?;
            if !names.insert(name.clone()) {
                return Err(invalid("Use unique column aliases for query results."));
            }
            columns.push(Column {
                name,
                data_type: text(row, "system_type_name")?,
                nullable: row
                    .try_get::<bool, _>("is_nullable")
                    .map_err(driver_error)?
                    .unwrap_or(true),
                primary_key: false,
                read_only: true,
                default_value: None,
                definition: None,
            });
        }
        if columns.is_empty() {
            return Err(invalid("Run a SELECT query that returns named columns."));
        }
        let mut result = page(&mut c, &source, columns, "(SELECT NULL)", number, size).await?;
        if let Some(plan) = edit_plan {
            super::query_editing::finish(&mut result, plan)?;
        }
        Ok(result)
    }
}
#[cfg(test)]
mod tests {
    #[test]
    fn identifiers_are_quoted() {
        assert_eq!(
            super::quote_identifier("x]; DROP TABLE t;--"),
            "[x]]; DROP TABLE t;--]"
        );
    }
}
