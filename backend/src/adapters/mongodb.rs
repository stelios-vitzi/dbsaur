//! Experimental, deliberately read-only document provider. No arbitrary commands or scripts.
use crate::{application::*, domain::*};
use async_trait::async_trait;
use mongodb::{
    Client,
    bson::{Bson, Document, doc},
    options::{ClientOptions, Credential, ServerAddress, Tls, TlsOptions},
};
use serde::Deserialize;
use serde_json::{Map, Value};
use std::time::Duration;

pub struct MongoAdapter;
const TIMEOUT: Duration = Duration::from_secs(10);
async fn bounded<T>(operation: impl std::future::Future<Output = AppResult<T>>) -> AppResult<T> {
    tokio::time::timeout(TIMEOUT, operation)
        .await
        .map_err(|_| {
            AppError(
                ErrorKind::Timeout,
                "MongoDB operation timed out. Narrow the filter or check the connection.",
            )
        })?
}
fn upstream(_: mongodb::error::Error) -> AppError {
    AppError(
        ErrorKind::Upstream,
        "MongoDB operation failed. Check connectivity, credentials, permissions, and query values.",
    )
}
fn client(input: &ConnectionInput) -> AppResult<Client> {
    let mut options = ClientOptions::builder()
        .hosts(vec![ServerAddress::Tcp {
            host: input.host.clone(),
            port: Some(input.port),
        }])
        .connect_timeout(TIMEOUT)
        .server_selection_timeout(TIMEOUT)
        .max_pool_size(2)
        .build();
    if !input.username.is_empty() {
        options.credential = Some(
            Credential::builder()
                .username(input.username.clone())
                .password(input.password.clone())
                .source(input.auth_source.clone().unwrap_or_else(|| "admin".into()))
                .build(),
        );
    } else if !input.password.is_empty() {
        return Err(invalid(
            "Enter a username when providing a MongoDB password.",
        ));
    }
    options.tls = Some(if input.ssl {
        Tls::Enabled(TlsOptions::default())
    } else {
        Tls::Disabled
    });
    Client::with_options(options).map_err(upstream)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FindQuery {
    collection: String,
    #[serde(default = "empty_object")]
    filter: Value,
    #[serde(default = "empty_object")]
    sort: Value,
    #[serde(default = "empty_object")]
    projection: Value,
}
fn empty_object() -> Value {
    Value::Object(Map::new())
}
fn document(value: Value) -> AppResult<Document> {
    fn check(value: &Value, depth: usize) -> bool {
        if depth > 32 {
            return false;
        }
        match value {
            Value::Object(map) => map.iter().all(|(key, value)| {
                !["$where", "$function", "$accumulator"].contains(&key.as_str())
                    && check(value, depth + 1)
            }),
            Value::Array(items) => items.iter().all(|value| check(value, depth + 1)),
            _ => true,
        }
    }
    if !value.is_object() || !check(&value, 0) {
        return Err(invalid(
            "Use JSON objects with at most 32 nesting levels. Server-side JavaScript is unavailable.",
        ));
    }
    match Bson::try_from(value).map_err(|_| invalid("Invalid MongoDB Extended JSON."))? {
        Bson::Document(document) => Ok(document),
        _ => Err(invalid("Expected a JSON object.")),
    }
}
fn parse(query: &SqlQuery) -> AppResult<(FindQuery, Document, Document, Document)> {
    query.bounds().map_err(invalid)?;
    let find: FindQuery = serde_json::from_str(&query.sql).map_err(|_| {
        invalid("Use a JSON object with collection, filter, sort, and projection fields.")
    })?;
    if find.collection.is_empty() || find.collection.len() > 128 || find.collection.contains('\0') {
        return Err(invalid("Choose a collection name of at most 128 bytes."));
    }
    let filter = document(find.filter.clone())?;
    let sort = document(find.sort.clone())?;
    if sort
        .values()
        .any(|value| !matches!(value, Bson::Int32(1 | -1) | Bson::Int64(1 | -1)))
    {
        return Err(invalid("Sort directions must be 1 or -1."));
    }
    let projection = document(find.projection.clone())?;
    if projection.values().any(|value| {
        !matches!(
            value,
            Bson::Int32(0 | 1) | Bson::Int64(0 | 1) | Bson::Boolean(_)
        )
    }) {
        return Err(invalid("Projection values must be 0, 1, false, or true."));
    }
    Ok((find, filter, sort, projection))
}

#[async_trait]
impl DatabaseAdapter for MongoAdapter {
    fn provider(&self) -> Provider {
        Provider {
            id: "mongodb",
            name: "MongoDB (experimental)",
        }
    }
    fn maintenance_database(&self) -> Option<&'static str> {
        Some("admin")
    }
    async fn test(&self, input: &ConnectionInput) -> AppResult<()> {
        bounded(async {
            client(input)?
                .database(&input.database)
                .run_command(doc! { "ping": 1 })
                .await
                .map_err(upstream)?;
            Ok(())
        })
        .await
    }
    async fn databases(&self, input: &ConnectionInput) -> AppResult<Vec<String>> {
        bounded(async {
            let mut names = client(input)?
                .list_database_names()
                .await
                .map_err(upstream)?;
            names.sort();
            Ok(names)
        })
        .await
    }
    async fn tables(&self, input: &ConnectionInput) -> AppResult<Vec<Table>> {
        bounded(async {
            let mut names = client(input)?
                .database(&input.database)
                .list_collection_names()
                .await
                .map_err(upstream)?;
            names.sort();
            Ok(names
                .into_iter()
                .map(|name| Table {
                    schema: input.database.clone(),
                    name,
                    kind: "COLLECTION".into(),
                    columns: vec![],
                    primary_keys: vec![],
                    key_constraints: vec![],
                    referenceable_columns: vec![],
                    foreign_keys: vec![],
                })
                .collect())
        })
        .await
    }
    async fn rows(&self, input: &ConnectionInput, query: &BrowseQuery) -> AppResult<TablePage> {
        query.bounds().map_err(invalid)?;
        if query.schema != input.database {
            return Err(invalid("Select the collection's database first."));
        }
        let sort = query
            .sort
            .as_ref()
            .map(|field| {
                let mut sort = Map::new();
                sort.insert(
                    field.clone(),
                    Value::from(if query.descending.unwrap_or(false) {
                        -1
                    } else {
                        1
                    }),
                );
                Value::Object(sort)
            })
            .unwrap_or_else(empty_object);
        self.query(
            input,
            &SqlQuery {
                sql: serde_json::json!({ "collection": query.table, "sort": sort }).to_string(),
                page: query.page,
                page_size: query.page_size,
            },
        )
        .await
    }
    async fn query(&self, input: &ConnectionInput, query: &SqlQuery) -> AppResult<TablePage> {
        bounded(async {
        let (find, filter, mut sort, projection) = parse(query)?;
        let (page, page_size) = query.bounds().map_err(invalid)?;
        let skip = u64::from(page) * u64::from(page_size);
        if skip > 100_000 {
            return Err(invalid(
                "MongoDB browsing is limited to the first 100,000 documents. Narrow the filter.",
            ));
        }
        if !sort.contains_key("_id") {
            sort.insert("_id", 1);
        }
        let client = client(input)?;
        let collection = client
            .database(&input.database)
            .collection::<Document>(&find.collection);
        let mut cursor = collection
            .find(filter)
            .sort(sort)
            .projection(projection)
            .skip(skip)
            .limit(i64::from(page_size) + 1)
            .batch_size(page_size.min(50))
            .max_time(TIMEOUT)
            .await
            .map_err(upstream)?;
        let mut rows = Vec::new();
        let mut bytes = 0;
        while cursor.advance().await.map_err(upstream)? {
            let document: Document = cursor.deserialize_current().map_err(upstream)?;
            let value = Bson::Document(document).into_canonical_extjson();
            bytes += value.to_string().len();
            if bytes > 8 * 1024 * 1024 {
                return Err(invalid(
                    "Document page exceeds 8 MiB. Reduce the page size or use a projection.",
                ));
            }
            if let Value::Object(document) = value {
                rows.push(document);
            }
        }
        let has_more = rows.len() > page_size as usize;
        rows.truncate(page_size as usize);
        Ok(TablePage {
            columns: vec![],
            rows,
            has_more,
            page,
            page_size,
            total_rows: None,
            editing: None,
        })
        }).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn query(sql: &str) -> SqlQuery {
        SqlQuery {
            sql: sql.into(),
            page: None,
            page_size: None,
        }
    }
    #[test]
    fn filters_preserve_bson_and_reject_scripts_and_commands() {
        let (_, filter, _, _) = parse(&query(r#"{"collection":"items","filter":{"_id":{"$oid":"507f1f77bcf86cd799439011"},"n":{"$numberLong":"9007199254740993"}}}"#)).unwrap();
        assert!(matches!(filter.get("_id"), Some(Bson::ObjectId(_))));
        assert_eq!(filter.get_i64("n").unwrap(), 9007199254740993);
        for value in [
            r#"{"collection":"items","filter":{"$or":[{"$where":"return true"}]}}"#,
            r#"{"collection":"items","filter":{"$expr":{"$function":{}}}}"#,
            r#"{"collection":"items","pipeline":[{"$out":"other"}]}"#,
            r#"{"collection":"items","sort":{"x":"desc"}}"#,
            r#"{"collection":"items","projection":{"x":{"$slice":1}}}"#,
            r#"{"collection":"items","filter":[]}"#,
        ] {
            assert!(parse(&query(value)).is_err(), "accepted {value}");
        }
    }
    #[test]
    fn canonical_documents_keep_exact_numbers_and_types() {
        let value = Bson::Document(doc! { "n": 9007199254740993_i64, "nested": { "ok": true } })
            .into_canonical_extjson();
        assert_eq!(value["n"]["$numberLong"], "9007199254740993");
        assert_eq!(value["nested"]["ok"], true);
    }
    #[tokio::test]
    async fn writes_are_unavailable_without_connecting() {
        let input = serde_json::from_value(serde_json::json!({"provider":"mongodb","host":"localhost","port":27017,"database":"test","username":"","password":"","ssl":false})).unwrap();
        assert!(
            MongoAdapter
                .mutate(
                    &input,
                    &Mutation::Execute {
                        sql: "dropDatabase".into()
                    }
                )
                .await
                .is_err()
        );
    }
}
