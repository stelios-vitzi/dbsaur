use dbsaur::adapters::sqlx;
use dbsaur::{
    adapters::postgres::{PostgresAdapter, quote_identifier},
    application::DatabaseAdapter,
    domain::{BrowseQuery, ConnectionInput, SqlQuery},
};
use sqlx::{
    Connection, Executor, PgConnection,
    postgres::{PgConnectOptions, PgSslMode},
};

// Creates and deletes only a randomly named schema in an explicitly supplied test DB.
#[tokio::test]
#[ignore = "requires WEBDB_TEST_CONNECTION_JSON for a disposable PostgreSQL database"]
async fn postgres_browsing_is_exact_quoted_and_read_only() {
    let input: ConnectionInput = serde_json::from_str(
        &std::env::var("WEBDB_TEST_CONNECTION_JSON").expect("set disposable test connection JSON"),
    )
    .expect("valid connection JSON");
    assert_eq!(input.provider, "postgres");
    let options = PgConnectOptions::new()
        .host(&input.host)
        .port(input.port)
        .database(&input.database)
        .username(&input.username)
        .password(&input.password)
        .ssl_mode(if input.ssl {
            PgSslMode::VerifyFull
        } else {
            PgSslMode::Disable
        });
    let mut setup = PgConnection::connect_with(&options).await.unwrap();
    let schema = format!("test_{}", uuid::Uuid::new_v4().simple());
    let qualified = format!(
        "{}.{}",
        quote_identifier(&schema),
        quote_identifier("odd\"table")
    );
    setup.execute(sqlx::AssertSqlSafe(format!("CREATE SCHEMA {}; CREATE TABLE {qualified} (id bigint PRIMARY KEY, amount numeric(20,4), note text); INSERT INTO {qualified} VALUES (9007199254740993, 12345678901234.1234, NULL), (9007199254740994, 2.5000, 'hello');", quote_identifier(&schema)).as_str())).await.unwrap();
    let adapter = PostgresAdapter;
    adapter.test(&input).await.unwrap();
    let settings = adapter.settings(&input).await.unwrap();
    assert_eq!(settings.database, input.database);
    assert_eq!(settings.provider, "postgres");
    assert_eq!(settings.properties["datname"], input.database);
    assert!(settings.settings.iter().any(|s| s["name"] == "work_mem"));
    assert!(
        settings
            .settings
            .iter()
            .all(|s| s.get("sourcefile").is_none())
    );
    let tables = adapter.tables(&input).await.unwrap();
    assert!(
        tables
            .iter()
            .any(|t| t.schema == schema && t.name == "odd\"table")
    );
    let mut query = BrowseQuery {
        schema: schema.clone(),
        table: "odd\"table".into(),
        page_size: Some(1),
        ..Default::default()
    };
    let first = adapter.rows(&input, &query).await.unwrap();
    assert_eq!(first.rows[0]["id"], "9007199254740993");
    assert_eq!(first.rows[0]["amount"], "12345678901234.1234");
    assert!(first.rows[0]["note"].is_null());
    assert!(first.has_more);
    assert!(first.columns[0].primary_key);
    let result = adapter.query(&input, &SqlQuery {
        sql: format!("WITH selected AS (SELECT * FROM {qualified}) SELECT * FROM selected ORDER BY id;"),
        page: Some(0), page_size: Some(1),
    }).await.unwrap();
    assert_eq!(result.rows[0]["id"], "9007199254740993");
    assert_eq!(result.rows[0]["amount"], "12345678901234.1234");
    assert!(result.has_more);
    let empty = adapter
        .query(
            &input,
            &SqlQuery {
                sql: format!("SELECT * FROM {qualified} WHERE false"),
                page: None,
                page_size: None,
            },
        )
        .await
        .unwrap();
    assert!(empty.rows.is_empty());
    assert_eq!(empty.columns.len(), 3);
    for sql in [
        format!("DELETE FROM {qualified}"),
        format!("SELECT * FROM {qualified}; DELETE FROM {qualified}"),
        format!("WITH removed AS (DELETE FROM {qualified} RETURNING *) SELECT * FROM removed"),
        "SELECT 1 AS duplicate, 2 AS duplicate".into(),
    ] {
        assert!(
            adapter
                .query(
                    &input,
                    &SqlQuery {
                        sql,
                        page: None,
                        page_size: None
                    }
                )
                .await
                .is_err()
        );
    }
    query.sort = Some("amount".into());
    query.descending = Some(true);
    let numeric_sorted = adapter.rows(&input, &query).await.unwrap();
    assert_eq!(
        numeric_sorted.rows[0]["amount"], "12345678901234.1234",
        "numeric sorting must use the native column, not its serialized text alias"
    );
    query.sort = None;
    query.descending = None;
    query.page = Some(1);
    assert!(!adapter.rows(&input, &query).await.unwrap().has_more);
    query.sort = Some("id; DROP SCHEMA public;--".into());
    assert!(adapter.rows(&input, &query).await.is_err());
    let namespace = quote_identifier(&schema);
    setup.execute(sqlx::AssertSqlSafe(format!("CREATE FUNCTION {namespace}.attempt_write() RETURNS bigint LANGUAGE plpgsql AS $$ BEGIN INSERT INTO {qualified} (id) VALUES (1); RETURN 1; END $$; CREATE VIEW {namespace}.writes AS SELECT {namespace}.attempt_write() AS id;").as_str())).await.unwrap();
    query.table = "writes".into();
    query.sort = None;
    query.page = None;
    assert!(
        adapter.rows(&input, &query).await.is_err(),
        "read-only transaction must reject writes through a view"
    );
    assert!(
        adapter
            .query(
                &input,
                &SqlQuery {
                    sql: format!("SELECT {namespace}.attempt_write() AS value"),
                    page: None,
                    page_size: None,
                }
            )
            .await
            .is_err(),
        "query execution must reject writes through a function"
    );
    setup
        .execute(sqlx::AssertSqlSafe(
            format!("DROP SCHEMA {namespace} CASCADE").as_str(),
        ))
        .await
        .unwrap();
}
