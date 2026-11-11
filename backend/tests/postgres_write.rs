use dbsaur::adapters::sqlx;
use dbsaur::{
    adapters::postgres::{PostgresAdapter, quote_identifier},
    application::DatabaseAdapter,
    domain::*,
};
use sqlx::{
    Connection, Executor, PgConnection,
    postgres::{PgConnectOptions, PgSslMode},
};
use std::collections::BTreeMap;

fn values(items: &[(&str, Option<&str>)]) -> BTreeMap<String, Option<String>> {
    items
        .iter()
        .map(|(key, value)| (key.to_string(), value.map(str::to_owned)))
        .collect()
}

// Explicitly configured disposable database only; no application connection is reused.
#[tokio::test]
#[ignore = "requires WEBDB_TEST_CONNECTION_JSON for a disposable PostgreSQL database"]
async fn writes_enforce_constraints_preserve_types_and_reject_stale_records() {
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
    let schema = format!("write_test_{}", uuid::Uuid::new_v4().simple());
    let namespace = quote_identifier(&schema);
    setup
        .execute(sqlx::AssertSqlSafe(
            format!("CREATE SCHEMA {namespace}").as_str(),
        ))
        .await
        .unwrap();
    let table = "odd\"table".to_string();
    let qualified = format!("{namespace}.{}", quote_identifier(&table));
    let adapter = PostgresAdapter;
    for sql in [
        format!("CREATE TABLE {namespace}.parent (id integer PRIMARY KEY)"),
        format!("INSERT INTO {namespace}.parent VALUES (1)"),
        format!(
            "CREATE TABLE {qualified} (id bigint PRIMARY KEY, parent_id integer REFERENCES {namespace}.parent(id), amount numeric(24,4) CHECK(amount>0), note varchar(5) NOT NULL DEFAULT 'hello', unique_note text UNIQUE, payload jsonb, tags integer[], computed bigint GENERATED ALWAYS AS (id+1) STORED)"
        ),
    ] {
        adapter
            .mutate(&input, &Mutation::Execute { sql })
            .await
            .unwrap();
    }
    let insert = |fields| Mutation::Insert {
        schema: schema.clone(),
        table: table.clone(),
        values: fields,
    };
    adapter
        .mutate(
            &input,
            &insert(values(&[
                ("id", Some("9007199254740993")),
                ("parent_id", Some("1")),
                ("amount", Some("12345678901234.1234")),
                ("payload", Some("{\"n\":9007199254740993}")),
                ("tags", Some("{1,2}")),
            ])),
        )
        .await
        .unwrap();
    let query = BrowseQuery {
        schema: schema.clone(),
        table: table.clone(),
        ..Default::default()
    };
    let first = adapter.rows(&input, &query).await.unwrap().rows.remove(0);
    assert_eq!(first["note"], "hello");
    assert_eq!(first["amount"], "12345678901234.1234");
    let update = |original, fields| Mutation::Update {
        schema: schema.clone(),
        table: table.clone(),
        original,
        values: fields,
    };
    for (fields, expected) in [
        (values(&[("parent_id", Some("999"))]), "Foreign key"),
        (values(&[("amount", Some("-1"))]), "Check constraint"),
        (values(&[("note", None)]), "Required field"),
        (values(&[("note", Some("too long"))]), "Invalid value"),
        (values(&[("id", Some("not a bigint"))]), "Invalid value"),
        (values(&[("computed", Some("1"))]), "Generated columns"),
        (values(&[("note\" = NULL;--", Some("x"))]), "Unknown column"),
    ] {
        let failure = adapter
            .mutate(&input, &update(first.clone(), fields))
            .await
            .unwrap_err();
        assert!(failure.1.starts_with(expected), "{}", failure.1);
        assert_eq!(adapter.rows(&input, &query).await.unwrap().rows[0], first);
    }
    adapter
        .mutate(
            &input,
            &update(
                first.clone(),
                values(&[("note", Some("new")), ("payload", Some("null"))]),
            ),
        )
        .await
        .unwrap();
    assert!(
        adapter
            .mutate(
                &input,
                &update(first.clone(), values(&[("note", Some("stale"))]))
            )
            .await
            .is_err()
    );
    assert!(
        adapter
            .mutate(&input, &insert(values(&[("id", Some("9007199254740993"))])))
            .await
            .unwrap_err()
            .1
            .starts_with("Unique constraint")
    );
    for sql in [
        format!("UPDATE {qualified} SET note='bad'; DELETE FROM {qualified}"),
        "COMMIT".into(),
    ] {
        assert!(
            adapter
                .mutate(&input, &Mutation::Execute { sql })
                .await
                .is_err()
        );
    }
    let current = adapter.rows(&input, &query).await.unwrap().rows.remove(0);
    assert_eq!(current["note"], "new");
    adapter
        .mutate(
            &input,
            &Mutation::Delete {
                schema: schema.clone(),
                table: table.clone(),
                original: current,
            },
        )
        .await
        .unwrap();
    assert!(adapter.rows(&input, &query).await.unwrap().rows.is_empty());
    adapter
        .mutate(
            &input,
            &Mutation::DropTable {
                schema: schema.clone(),
                table,
            },
        )
        .await
        .unwrap();
    // Tables without keys can be edited only when their full record is unique.
    for sql in [
        format!("CREATE TABLE {namespace}.duplicates (note text)"),
        format!("INSERT INTO {namespace}.duplicates VALUES ('same'), ('same'), ('unique')"),
    ] {
        adapter
            .mutate(&input, &Mutation::Execute { sql })
            .await
            .unwrap();
    }
    let duplicate = serde_json::json!({"note":"same"})
        .as_object()
        .unwrap()
        .clone();
    assert!(
        adapter
            .mutate(
                &input,
                &Mutation::Delete {
                    schema: schema.clone(),
                    table: "duplicates".into(),
                    original: duplicate
                }
            )
            .await
            .is_err()
    );
    let unique = serde_json::json!({"note":"unique"})
        .as_object()
        .unwrap()
        .clone();
    adapter
        .mutate(
            &input,
            &Mutation::Update {
                schema: schema.clone(),
                table: "duplicates".into(),
                original: unique,
                values: values(&[("note", Some(""))]),
            },
        )
        .await
        .unwrap();
    assert!(
        adapter
            .mutate(
                &input,
                &Mutation::DropTable {
                    schema: schema.clone(),
                    table: "parent".into()
                }
            )
            .await
            .is_ok()
    );

    // Database lifecycle uses a separate instance-only, non-transactional path.
    let database = format!("write_db_{}", uuid::Uuid::new_v4().simple());
    assert!(
        adapter
            .mutate(
                &input,
                &Mutation::CreateDatabase {
                    name: database.clone()
                }
            )
            .await
            .is_err()
    );
    let mut instance = input.clone();
    instance.all_databases = true;
    adapter
        .mutate(
            &instance,
            &Mutation::CreateDatabase {
                name: database.clone(),
            },
        )
        .await
        .unwrap();
    assert!(
        adapter
            .databases(&instance)
            .await
            .unwrap()
            .contains(&database)
    );
    adapter
        .mutate(
            &instance,
            &Mutation::DropDatabase {
                name: database.clone(),
            },
        )
        .await
        .unwrap();
    assert!(
        !adapter
            .databases(&instance)
            .await
            .unwrap()
            .contains(&database)
    );
    assert!(
        adapter
            .mutate(
                &instance,
                &Mutation::DropDatabase {
                    name: input.database.clone()
                }
            )
            .await
            .is_err()
    );
    setup
        .execute(sqlx::AssertSqlSafe(
            format!("DROP SCHEMA {namespace} CASCADE").as_str(),
        ))
        .await
        .unwrap();
}
