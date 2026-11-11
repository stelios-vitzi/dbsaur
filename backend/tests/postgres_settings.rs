use dbsaur::adapters::sqlx;
use dbsaur::{
    adapters::postgres::{PostgresAdapter, quote_identifier},
    application::DatabaseAdapter,
    domain::{ConnectionInput, DatabaseSettings, UpdateDatabaseSettings},
};
use sqlx::{
    Connection, Executor, PgConnection,
    postgres::{PgConnectOptions, PgSslMode},
};

fn change(database: &str, changes: serde_json::Value) -> UpdateDatabaseSettings {
    serde_json::from_value(
        serde_json::json!({"database": database, "confirmation": "APPLY", "changes": changes}),
    )
    .unwrap()
}
fn setting<'a>(settings: &'a DatabaseSettings, name: &str) -> &'a serde_json::Value {
    settings
        .settings
        .iter()
        .find(|setting| setting["name"] == name)
        .unwrap()
}

// This test creates and drops its own randomly named DB. The configured user
// must have CREATEDB; never point this test at a non-disposable instance.
#[tokio::test]
#[ignore = "requires WEBDB_TEST_CONNECTION_JSON for a disposable PostgreSQL instance with CREATEDB"]
async fn database_defaults_are_quoted_atomic_resettable_and_detect_conflicts() {
    let mut input: ConnectionInput = serde_json::from_str(
        &std::env::var("WEBDB_TEST_CONNECTION_JSON").expect("configure a disposable instance"),
    )
    .unwrap();
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
    let mut admin = PgConnection::connect_with(&options).await.unwrap();
    let database = format!("webdb_{}\"db", uuid::Uuid::new_v4().simple());
    let quoted = quote_identifier(&database);
    admin
        .execute(sqlx::AssertSqlSafe(
            format!("CREATE DATABASE {quoted}").as_str(),
        ))
        .await
        .unwrap();
    input.database = database.clone();
    input.all_databases = false;
    let adapter = PostgresAdapter;
    let before = adapter.settings(&input).await.unwrap();
    assert!(before.can_edit);
    assert_eq!(
        setting(&before, "work_mem")["databaseValue"],
        serde_json::Value::Null
    );
    assert_eq!(setting(&before, "shared_buffers")["editable"], false);
    adapter
        .update_settings(
            &input,
            &change(
                &database,
                serde_json::json!({"work_mem":{"value":"16MB","expected":null}}),
            ),
        )
        .await
        .unwrap();
    let saved = adapter.settings(&input).await.unwrap();
    assert_eq!(setting(&saved, "work_mem")["databaseValue"], "16MB");
    assert_eq!(setting(&saved, "work_mem")["setting"], "16384");
    assert!(
        adapter
            .update_settings(
                &input,
                &change(
                    &database,
                    serde_json::json!({"work_mem":{"value":"32MB","expected":null}})
                )
            )
            .await
            .is_err()
    );
    let mut invalid = change(
        &database,
        serde_json::json!({"work_mem":{"value":"not-memory","expected":"16MB"}}),
    );
    invalid.connection_limit = Some(5);
    invalid.expected_connection_limit = Some(-1);
    assert!(adapter.update_settings(&input, &invalid).await.is_err());
    let after_invalid = adapter.settings(&input).await.unwrap();
    assert_eq!(
        after_invalid.properties["datconnlimit"], -1,
        "failed batch must roll back the connection limit"
    );
    assert_eq!(setting(&after_invalid, "work_mem")["databaseValue"], "16MB");
    let literal = "webdb'; SELECT pg_sleep(99); --";
    adapter
        .update_settings(
            &input,
            &change(
                &database,
                serde_json::json!({"application_name":{"value":literal,"expected":null}}),
            ),
        )
        .await
        .unwrap();
    assert_eq!(
        setting(&adapter.settings(&input).await.unwrap(), "application_name")["databaseValue"],
        literal
    );
    adapter
        .update_settings(
            &input,
            &change(
                &database,
                serde_json::json!({"work_mem":{"value":null,"expected":"16MB"}}),
            ),
        )
        .await
        .unwrap();
    assert_eq!(
        setting(&adapter.settings(&input).await.unwrap(), "work_mem")["databaseValue"],
        serde_json::Value::Null
    );
    for name in [
        "shared_buffers",
        "role",
        "archive_command",
        "work_mem; SELECT 1",
    ] {
        assert!(
            adapter
                .update_settings(
                    &input,
                    &change(
                        &database,
                        serde_json::json!({name:{"value":"x","expected":null}})
                    )
                )
                .await
                .is_err()
        );
    }
    admin
        .execute(sqlx::AssertSqlSafe(
            format!("DROP DATABASE {quoted}").as_str(),
        ))
        .await
        .unwrap();
}
