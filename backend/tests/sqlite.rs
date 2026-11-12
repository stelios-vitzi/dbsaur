use dbsaur::{adapters::sqlite::SqliteAdapter, application::DatabaseAdapter, domain::*};
use serde_json::json;
use std::collections::BTreeMap;

struct Fixture {
    path: std::path::PathBuf,
    input: ConnectionInput,
}
impl Fixture {
    fn new() -> Self {
        let path =
            std::env::temp_dir().join(format!("dbsaur-sqlite-{}.sqlite", uuid::Uuid::new_v4()));
        let connection = rusqlite::Connection::open(&path).unwrap();
        connection.execute_batch("PRAGMA foreign_keys=ON;
            CREATE TABLE parent (id INTEGER PRIMARY KEY);
            INSERT INTO parent VALUES (1);
            CREATE TABLE \"odd\"\"table\" (id INTEGER PRIMARY KEY, parent INTEGER REFERENCES parent,
              note TEXT DEFAULT 'default', data BLOB, computed TEXT GENERATED ALWAYS AS (note || '!') STORED);
            INSERT INTO \"odd\"\"table\" (id,parent,note,data) VALUES (9007199254740993,1,'original',X'00FF');
            CREATE VIEW example_view AS SELECT * FROM \"odd\"\"table\";
            CREATE UNIQUE INDEX partial_note ON \"odd\"\"table\"(note) WHERE parent IS NOT NULL;").unwrap();
        let input = ConnectionInput {
            provider: "sqlite".into(),
            host: String::new(),
            port: 0,
            database: path.to_str().unwrap().into(),
            username: String::new(),
            password: String::new(),
            ssl: false,
            all_databases: false,
            read_only: false,
            tls_ca: None,
            tls_cert: None,
            tls_key: None,
        };
        Self { path, input }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}
fn browse() -> BrowseQuery {
    BrowseQuery {
        schema: "main".into(),
        table: "odd\"table".into(),
        page_size: Some(1),
        ..Default::default()
    }
}
fn query(sql: &str) -> SqlQuery {
    SqlQuery {
        sql: sql.into(),
        page: None,
        page_size: Some(1),
    }
}
fn values(items: &[(&str, Option<&str>)]) -> BTreeMap<String, Option<String>> {
    items
        .iter()
        .map(|(k, v)| ((*k).into(), v.map(str::to_owned)))
        .collect()
}

#[tokio::test]
async fn sqlite_browsing_metadata_and_exact_values() {
    let fixture = Fixture::new();
    let adapter = SqliteAdapter;
    adapter.test(&fixture.input).await.unwrap();
    let tables = adapter.tables(&fixture.input).await.unwrap();
    let table = tables.iter().find(|t| t.name == "odd\"table").unwrap();
    assert_eq!(table.primary_keys, ["id"]);
    assert_eq!(table.foreign_keys[0].referenced_column, "id");
    assert!(!table.referenceable_columns.contains(&"note".into()));
    let page = adapter.rows(&fixture.input, &browse()).await.unwrap();
    assert_eq!(page.rows[0]["id"], "9007199254740993");
    assert_eq!(page.rows[0]["data"], "00ff");
    assert!(
        page.columns
            .iter()
            .find(|c| c.name == "computed")
            .unwrap()
            .read_only
    );
    let result = adapter
        .query(
            &fixture.input,
            &query("WITH c AS (SELECT 1 AS n UNION ALL SELECT 2) SELECT n FROM c ORDER BY n"),
        )
        .await
        .unwrap();
    assert!(result.has_more);
    assert_eq!(result.rows[0]["n"], "1");
    assert!(result.columns.iter().all(|c| c.read_only));
    let result = adapter
        .query(
            &fixture.input,
            &query("SELECT NULL AS empty, 9007199254740993 AS exact_value"),
        )
        .await
        .unwrap();
    assert_eq!(result.rows[0]["empty"], json!(null));
    assert_eq!(result.rows[0]["exact_value"], "9007199254740993");
    let view = BrowseQuery {
        table: "example_view".into(),
        ..browse()
    };
    assert!(
        adapter
            .rows(&fixture.input, &view)
            .await
            .unwrap()
            .columns
            .iter()
            .all(|c| c.read_only)
    );
}

#[tokio::test]
async fn sqlite_writes_are_confirmed_separate_atomic_and_detect_stale_rows() {
    let fixture = Fixture::new();
    let adapter = SqliteAdapter;
    let original = adapter
        .rows(&fixture.input, &browse())
        .await
        .unwrap()
        .rows
        .remove(0);
    let update = Mutation::Update {
        schema: "main".into(),
        table: "odd\"table".into(),
        original: original.clone(),
        values: values(&[("note", Some("quoted '; DROP TABLE parent;--"))]),
    };
    assert_eq!(
        adapter
            .mutate(&fixture.input, &update)
            .await
            .unwrap()
            .affected_rows,
        1
    );
    assert!(adapter.mutate(&fixture.input, &update).await.is_err());
    let invalid_fk = Mutation::Insert {
        schema: "main".into(),
        table: "odd\"table".into(),
        values: values(&[("parent", Some("999"))]),
    };
    assert!(adapter.mutate(&fixture.input, &invalid_fk).await.is_err());
    let insert = Mutation::Insert {
        schema: "main".into(),
        table: "odd\"table".into(),
        values: values(&[("id", Some("2")), ("data", Some("cafe")), ("note", None)]),
    };
    assert_eq!(
        adapter
            .mutate(&fixture.input, &insert)
            .await
            .unwrap()
            .affected_rows,
        1
    );
    let result = adapter.rows(&fixture.input, &browse()).await.unwrap();
    assert_eq!(result.rows[0]["data"], "cafe");
    assert_eq!(result.rows[0]["note"], json!(null));
    let delete = Mutation::Delete {
        schema: "main".into(),
        table: "odd\"table".into(),
        original: result.rows[0].clone(),
    };
    assert_eq!(
        adapter
            .mutate(&fixture.input, &delete)
            .await
            .unwrap()
            .affected_rows,
        1
    );
    assert!(adapter.mutate(&fixture.input, &delete).await.is_err());
    let create = Mutation::Execute {
        sql: "CREATE TABLE main.created (id INTEGER PRIMARY KEY)".into(),
    };
    adapter.mutate(&fixture.input, &create).await.unwrap();
    adapter
        .mutate(
            &fixture.input,
            &Mutation::DropTable {
                schema: "main".into(),
                table: "created".into(),
            },
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn sqlite_rejects_writes_in_queries_and_never_creates_files() {
    let fixture = Fixture::new();
    let adapter = SqliteAdapter;
    assert!(
        adapter
            .query(
                &fixture.input,
                &query("SELECT 1 AS repeated, 2 AS repeated")
            )
            .await
            .is_err()
    );
    for sql in [
        "DELETE FROM parent",
        "SELECT 1; DROP TABLE parent",
        "PRAGMA writable_schema=ON",
        "ATTACH DATABASE ':memory:' AS other",
    ] {
        assert!(adapter.query(&fixture.input, &query(sql)).await.is_err());
    }
    for sql in [
        "COMMIT",
        "PRAGMA foreign_keys=OFF",
        "ATTACH DATABASE ':memory:' AS other",
        "DROP TABLE parent; SELECT 1",
    ] {
        assert!(
            adapter
                .mutate(&fixture.input, &Mutation::Execute { sql: sql.into() })
                .await
                .is_err()
        );
    }
    let mut input = fixture.input.clone();
    input.read_only = true;
    assert!(
        adapter
            .mutate(
                &input,
                &Mutation::DropTable {
                    schema: "main".into(),
                    table: "parent".into()
                }
            )
            .await
            .is_err()
    );
    input.database.push_str(".missing");
    assert!(adapter.test(&input).await.is_err());
    assert!(!std::path::Path::new(&input.database).exists());
    input.database = "relative.sqlite".into();
    assert!(input.validate().is_err());
    input = fixture.input.clone();
    input.all_databases = true;
    assert!(input.validate().is_err());
    input = fixture.input.clone();
    input.ssl = true;
    assert!(input.validate().is_err());
}

#[tokio::test]
async fn sqlite_index_replacement_checks_snapshots_and_rolls_back_failures() {
    let fixture = Fixture::new();
    let adapter = SqliteAdapter;
    let index = IndexInfo {
        name: "index\"name".into(),
        columns: vec!["note".into()],
        unique: false,
        editable: true,
    };
    let change = |expected, index| Mutation::AlterIndex {
        schema: "main".into(),
        table: "odd\"table".into(),
        expected,
        index,
    };
    adapter
        .mutate(&fixture.input, &change(None, Some(index.clone())))
        .await
        .unwrap();
    let indexes = adapter
        .indexes(&fixture.input, "main", "odd\"table")
        .await
        .unwrap();
    assert!(indexes.iter().any(|i| i == &index));
    let partial = indexes.iter().find(|i| i.name == "partial_note").unwrap();
    assert!(!partial.editable);
    assert!(
        adapter
            .mutate(&fixture.input, &change(Some(partial.clone()), None))
            .await
            .is_err()
    );
    let invalid = IndexInfo {
        columns: vec!["missing".into()],
        ..index.clone()
    };
    assert!(
        adapter
            .mutate(&fixture.input, &change(Some(index.clone()), Some(invalid)))
            .await
            .is_err()
    );
    assert!(
        adapter
            .indexes(&fixture.input, "main", "odd\"table")
            .await
            .unwrap()
            .contains(&index)
    );
    adapter
        .mutate(&fixture.input, &change(Some(index.clone()), None))
        .await
        .unwrap();
    assert!(
        adapter
            .mutate(&fixture.input, &change(Some(index), None))
            .await
            .is_err()
    );
}

#[tokio::test]
async fn sqlite_dynamic_primary_keys_and_advanced_indexes_are_safe() {
    let fixture = Fixture::new();
    let adapter = SqliteAdapter;
    let c = rusqlite::Connection::open(&fixture.path).unwrap();
    c.execute_batch(
        "CREATE TABLE dynamic_keys (id PRIMARY KEY, note TEXT);
        INSERT INTO dynamic_keys VALUES (9007199254740993, 'integer'), (X'CAFE', 'blob');
        CREATE INDEX descending_note ON dynamic_keys(note DESC);
        CREATE INDEX collated_note ON dynamic_keys(note COLLATE NOCASE);
        CREATE INDEX expression_note ON dynamic_keys(lower(note));",
    )
    .unwrap();
    for index in adapter
        .indexes(&fixture.input, "main", "dynamic_keys")
        .await
        .unwrap()
    {
        assert!(!index.editable);
    }
    let browse = BrowseQuery {
        schema: "main".into(),
        table: "dynamic_keys".into(),
        ..Default::default()
    };
    for original in adapter.rows(&fixture.input, &browse).await.unwrap().rows {
        adapter
            .mutate(
                &fixture.input,
                &Mutation::Update {
                    schema: "main".into(),
                    table: "dynamic_keys".into(),
                    original,
                    values: values(&[("note", Some("updated"))]),
                },
            )
            .await
            .unwrap();
    }
    c.execute_batch("INSERT INTO dynamic_keys VALUES ('9007199254740993', 'updated')")
        .unwrap();
    let original = adapter
        .rows(&fixture.input, &browse)
        .await
        .unwrap()
        .rows
        .remove(0);
    assert!(
        adapter
            .mutate(
                &fixture.input,
                &Mutation::Delete {
                    schema: "main".into(),
                    table: "dynamic_keys".into(),
                    original
                }
            )
            .await
            .is_err()
    );
}
