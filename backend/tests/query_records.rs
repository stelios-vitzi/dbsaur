use dbsaur::{
    adapters::postgres::{PostgresAdapter, quote_identifier},
    application::DatabaseAdapter,
    domain::*,
};
use std::collections::BTreeMap;

#[tokio::test]
#[ignore = "requires WEBDB_TEST_CONNECTION_JSON for a disposable PostgreSQL database"]
async fn projected_query_records_keep_exact_snapshots_and_page_counts() {
    let input: ConnectionInput = serde_json::from_str(
        &std::env::var("WEBDB_TEST_CONNECTION_JSON").expect("configure a disposable database"),
    )
    .unwrap();
    assert_eq!(input.provider, "postgres");
    assert!(!input.all_databases);
    let adapter = PostgresAdapter;
    let table = format!("query_Test_{}", uuid::Uuid::new_v4().simple());
    let target = format!("public.{}", quote_identifier(&table));
    adapter.mutate(&input,&Mutation::Execute {sql:format!("CREATE TABLE {target} (id integer PRIMARY KEY,name varchar(80),happened timestamp(6) with time zone,generated integer GENERATED ALWAYS AS (id+1) STORED)")}).await.unwrap();
    let values = (1..=55)
        .map(|id| format!("({id},'record {id}','2026-11-12T01:02:03.123456+02:30')"))
        .collect::<Vec<_>>()
        .join(",");
    adapter
        .mutate(
            &input,
            &Mutation::Execute {
                sql: format!("INSERT INTO {target} (id,name,happened) VALUES {values}"),
            },
        )
        .await
        .unwrap();
    let browse = adapter
        .rows(
            &input,
            &BrowseQuery {
                schema: "public".into(),
                table: table.clone(),
                page_size: Some(25),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(browse.total_rows.as_deref(), Some("55"));
    let sql = format!(
        "SELECT t.name AS \"Display name\",t.happened FROM {target} t WHERE id<=40 ORDER BY id"
    );
    let result = adapter
        .query(
            &input,
            &SqlQuery {
                sql: sql.clone(),
                page: Some(0),
                page_size: Some(25),
            },
        )
        .await
        .unwrap();
    assert_eq!(result.total_rows.as_deref(), Some("40"));
    assert_eq!(result.rows.len(), 25);
    assert_eq!(result.columns.len(), 2);
    assert!(result.columns.iter().all(|column| !column.read_only));
    assert!(result.rows[0].keys().all(|key| !key.starts_with("dbsaur_")));
    let editing = result.editing.unwrap();
    assert_eq!(editing.schema, "public");
    assert_eq!(editing.table, table);
    assert_eq!(editing.columns["Display name"], "name");
    assert_eq!(editing.originals[0].len(), 4);
    let original = editing.originals[0].clone();
    let change = Mutation::Update {
        schema: editing.schema,
        table: editing.table,
        original: original.clone(),
        values: BTreeMap::from([("name".into(), Some("edited from query".into()))]),
    };
    adapter.mutate(&input, &change).await.unwrap();
    assert!(
        adapter.mutate(&input, &change).await.is_err(),
        "stale query snapshots must be rejected"
    );
    let second = adapter
        .query(
            &input,
            &SqlQuery {
                sql: sql.clone(),
                page: Some(1),
                page_size: Some(25),
            },
        )
        .await
        .unwrap();
    assert_eq!(second.total_rows.as_deref(), Some("40"));
    assert_eq!(second.rows.len(), 15);
    assert!(!second.has_more);
    let limited = adapter
        .query(
            &input,
            &SqlQuery {
                sql: format!("SELECT * FROM {target} ORDER BY id LIMIT 17"),
                page: None,
                page_size: Some(25),
            },
        )
        .await
        .unwrap();
    assert_eq!(limited.total_rows.as_deref(), Some("17"));
    assert!(limited.editing.is_some());
    assert!(
        limited
            .columns
            .iter()
            .find(|column| column.name == "generated")
            .unwrap()
            .read_only
    );
    let reread = adapter
        .query(
            &input,
            &SqlQuery {
                sql: format!("SELECT name AS id FROM {target} WHERE id=1"),
                page: None,
                page_size: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(reread.rows[0]["id"], "edited from query");
    assert_eq!(reread.editing.unwrap().columns["id"], "name");
    for sql in [
        format!("SELECT count(*) AS total FROM {target}"),
        format!("SELECT a.name,b.id FROM {target} a JOIN {target} b ON a.id=b.id"),
    ] {
        let result = adapter
            .query(
                &input,
                &SqlQuery {
                    sql,
                    page: None,
                    page_size: None,
                },
            )
            .await
            .unwrap();
        assert!(result.editing.is_none());
        assert!(result.columns.iter().all(|column| column.read_only));
    }
    adapter
        .mutate(
            &input,
            &Mutation::DropTable {
                schema: "public".into(),
                table,
            },
        )
        .await
        .unwrap();
}
