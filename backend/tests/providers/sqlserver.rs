#[tokio::test]
#[ignore = "Requires WEBDB_TEST_SQLSERVER_CONNECTION_JSON naming a disposable SQL Server database"]
async fn browse() {
    super::super::browse(&dbsaur::adapters::sqlserver::SqlServerAdapter, "WEBDB_TEST_SQLSERVER_CONNECTION_JSON", "sqlserver", "SELECT CAST(9007199254740993 AS decimal(20,0)) AS exact_value, CAST(NULL AS nvarchar(20)) AS nullable_value").await;
}

#[tokio::test]
#[ignore = "Requires WEBDB_TEST_SQLSERVER_CONNECTION_JSON naming a disposable database with DDL permissions"]
async fn writes() {
    super::super::writes(
        &dbsaur::adapters::sqlserver::SqlServerAdapter,
        "WEBDB_TEST_SQLSERVER_CONNECTION_JSON",
        "sqlserver",
    )
    .await;
}

#[tokio::test]
#[ignore = "Requires WEBDB_TEST_SQLSERVER_CONNECTION_JSON naming a disposable SQL Server database"]
async fn ctes_support_exact_counts_and_bounded_pagination() {
    use dbsaur::{
        application::DatabaseAdapter,
        domain::{ConnectionInput, SqlQuery},
    };
    let input: ConnectionInput =
        serde_json::from_str(&std::env::var("WEBDB_TEST_SQLSERVER_CONNECTION_JSON").unwrap())
            .unwrap();
    let adapter = dbsaur::adapters::sqlserver::SqlServerAdapter;
    for page in [0, 1] {
        let result = adapter.query(&input, &SqlQuery { sql: "WITH x AS (SELECT 1 AS n UNION ALL SELECT 2), y AS (SELECT n FROM x) SELECT n FROM y".into(), page: Some(page), page_size: Some(1) }).await.unwrap();
        assert_eq!(result.rows.len(), 1);
        assert_eq!(result.total_rows.as_deref(), Some("2"));
        assert_eq!(result.has_more, page == 0);
        assert!(result.editing.is_none());
    }
}
