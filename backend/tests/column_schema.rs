use dbsaur::{adapters::postgres::PostgresAdapter, application::DatabaseAdapter, domain::*};
use std::collections::BTreeMap;

// Never reuse an application connection. This test requires an explicitly
// configured disposable database and only creates uniquely named test tables.
#[tokio::test]
#[ignore = "requires WEBDB_TEST_CONNECTION_JSON for a disposable PostgreSQL database"]
async fn column_definitions_rename_defaults_collation_and_delete() {
    let input: ConnectionInput = serde_json::from_str(
        &std::env::var("WEBDB_TEST_CONNECTION_JSON").expect("configure a disposable test database"),
    )
    .unwrap();
    assert_eq!(input.provider, "postgres");
    assert!(!input.all_databases);
    let adapter = PostgresAdapter;
    let table = format!("column_test_{}", uuid::Uuid::new_v4().simple());
    let child = format!("{table}_child");
    adapter.mutate(&input, &Mutation::Execute { sql: format!("CREATE TABLE public.\"{table}\" (id integer PRIMARY KEY, label varchar(40) NOT NULL DEFAULT 'hello', amount numeric(12,3), retire text, generated integer GENERATED ALWAYS AS (id+1) STORED)") }).await.unwrap();
    let query = BrowseQuery {
        schema: "public".into(),
        table: table.clone(),
        ..Default::default()
    };
    let insert = |id: &str| Mutation::Insert {
        schema: "public".into(),
        table: table.clone(),
        values: BTreeMap::from([("id".into(), Some(id.into()))]),
    };
    adapter.mutate(&input, &insert("1")).await.unwrap();
    let first = adapter.rows(&input, &query).await.unwrap();
    let baseline = first
        .columns
        .iter()
        .find(|column| column.name == "label")
        .unwrap()
        .definition
        .clone()
        .unwrap();
    assert_eq!(baseline.data_type, "character varying(40)");
    assert_eq!(baseline.character_set.as_deref(), Some("UTF8"));
    let renamed = "renamed\"label";
    adapter
        .mutate(
            &input,
            &Mutation::AlterColumn {
                schema: "public".into(),
                table: table.clone(),
                column: "label".into(),
                expected: Box::new(baseline.clone()),
                change: ColumnChange::Rename {
                    name: renamed.into(),
                },
            },
        )
        .await
        .unwrap();
    let after_rename = adapter.rows(&input, &query).await.unwrap();
    assert_eq!(after_rename.rows[0][renamed], "hello");
    let baseline = after_rename
        .columns
        .iter()
        .find(|column| column.name == renamed)
        .unwrap()
        .definition
        .clone()
        .unwrap();
    adapter
        .mutate(
            &input,
            &Mutation::AlterColumn {
                schema: "public".into(),
                table: table.clone(),
                column: renamed.into(),
                expected: Box::new(baseline.clone()),
                change: ColumnChange::Definition {
                    data_type: "varchar(80)".into(),
                    nullable: true,
                    default_expression: Some("'new default'".into()),
                    character_set: baseline.character_set.clone(),
                    collation: Some("pg_catalog.\"C\"".into()),
                },
            },
        )
        .await
        .unwrap();
    // Reusing the old metadata snapshot must fail before changing the column.
    assert!(
        adapter
            .mutate(
                &input,
                &Mutation::AlterColumn {
                    schema: "public".into(),
                    table: table.clone(),
                    column: renamed.into(),
                    expected: Box::new(baseline),
                    change: ColumnChange::Drop
                }
            )
            .await
            .is_err()
    );
    adapter.mutate(&input, &insert("2")).await.unwrap();
    let updated = adapter.rows(&input, &query).await.unwrap();
    assert_eq!(updated.rows[0][renamed], "hello");
    assert_eq!(updated.rows[1][renamed], "new default");
    let definition = updated
        .columns
        .iter()
        .find(|column| column.name == renamed)
        .unwrap()
        .definition
        .as_ref()
        .unwrap();
    assert_eq!(definition.data_type, "character varying(80)");
    assert!(definition.nullable);
    assert_eq!(definition.collation.as_deref(), Some("pg_catalog.\"C\""));
    let generated = updated
        .columns
        .iter()
        .find(|column| column.name == "generated")
        .unwrap()
        .definition
        .clone()
        .unwrap();
    assert!(
        adapter
            .mutate(
                &input,
                &Mutation::AlterColumn {
                    schema: "public".into(),
                    table: table.clone(),
                    column: "generated".into(),
                    expected: Box::new(generated),
                    change: ColumnChange::Definition {
                        data_type: "integer".into(),
                        nullable: true,
                        default_expression: None,
                        character_set: Some("UTF8".into()),
                        collation: None
                    }
                }
            )
            .await
            .is_err()
    );
    let retire = updated
        .columns
        .iter()
        .find(|column| column.name == "retire")
        .unwrap()
        .definition
        .clone()
        .unwrap();
    adapter
        .mutate(
            &input,
            &Mutation::AlterColumn {
                schema: "public".into(),
                table: table.clone(),
                column: "retire".into(),
                expected: Box::new(retire),
                change: ColumnChange::Drop,
            },
        )
        .await
        .unwrap();
    let remaining = adapter.rows(&input, &query).await.unwrap();
    assert_eq!(remaining.rows.len(), 2);
    assert!(
        !remaining
            .columns
            .iter()
            .any(|column| column.name == "retire")
    );
    adapter.mutate(&input,&Mutation::Execute { sql: format!("CREATE TABLE public.\"{child}\" (parent integer REFERENCES public.\"{table}\" (id))") }).await.unwrap();
    let id = remaining
        .columns
        .iter()
        .find(|column| column.name == "id")
        .unwrap()
        .definition
        .clone()
        .unwrap();
    assert!(
        adapter
            .mutate(
                &input,
                &Mutation::AlterColumn {
                    schema: "public".into(),
                    table: table.clone(),
                    column: "id".into(),
                    expected: Box::new(id),
                    change: ColumnChange::Drop
                }
            )
            .await
            .is_err()
    );
    for table in [child, table] {
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
}
