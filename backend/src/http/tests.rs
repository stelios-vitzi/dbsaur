use super::*;
use crate::adapters;
use axum::{body::Body, http::Request};
use http_body_util::BodyExt;
use tower::ServiceExt;

#[test]
fn geometry_preview_input_is_bounded_hexadecimal() {
    for value in ["", "0", "010g", "\\x0100", "'); SELECT 1;--"] {
        assert!(
            GeometryRequest {
                value: value.into()
            }
            .validate()
            .is_err()
        );
    }
    assert!(
        GeometryRequest {
            value: "abCD0123".into()
        }
        .validate()
        .is_ok()
    );
    assert!(
        GeometryRequest {
            value: "00".repeat(512 * 1024)
        }
        .validate()
        .is_ok()
    );
    assert!(
        GeometryRequest {
            value: "00".repeat(512 * 1024 + 1)
        }
        .validate()
        .is_err()
    );
}

struct InstanceAdapter;
#[async_trait::async_trait]
impl DatabaseAdapter for InstanceAdapter {
    async fn mutate(&self, _: &ConnectionInput, _: &Mutation) -> AppResult<MutationResult> {
        Ok(MutationResult { affected_rows: 1 })
    }
    fn maintenance_database(&self) -> Option<&'static str> {
        Some("postgres")
    }
    async fn update_settings(
        &self,
        input: &ConnectionInput,
        request: &UpdateDatabaseSettings,
    ) -> AppResult<()> {
        assert_eq!(request.database, input.database);
        assert_eq!(request.changes["work_mem"].value.as_deref(), Some("16MB"));
        Ok(())
    }
    async fn settings(&self, input: &ConnectionInput) -> AppResult<DatabaseSettings> {
        Ok(DatabaseSettings {
            provider: input.provider.clone(),
            database: input.database.clone(),
            properties: serde_json::Map::new(),
            settings: vec![],
            can_edit: true,
        })
    }
    fn provider(&self) -> Provider {
        Provider {
            id: "postgres",
            name: "Test instance",
        }
    }
    async fn test(&self, input: &ConnectionInput) -> AppResult<()> {
        if input.database == "unavailable" {
            return Err(invalid("Database unavailable."));
        }
        Ok(())
    }
    async fn databases(&self, _: &ConnectionInput) -> AppResult<Vec<String>> {
        Ok(vec![
            "postgres".into(),
            "app\"db".into(),
            "unavailable".into(),
        ])
    }
    async fn tables(&self, input: &ConnectionInput) -> AppResult<Vec<Table>> {
        Ok(vec![Table {
            schema: "public".into(),
            name: input.database.clone(),
            kind: "BASE TABLE".into(),
            columns: vec![],
            primary_keys: vec![],
            referenceable_columns: vec![],
            key_constraints: vec![],
            foreign_keys: vec![],
        }])
    }
    async fn rows(&self, _: &ConnectionInput, _: &BrowseQuery) -> AppResult<TablePage> {
        Err(invalid("Unused"))
    }
    async fn query(&self, _: &ConnectionInput, _: &SqlQuery) -> AppResult<TablePage> {
        Err(invalid("Unused"))
    }
}
async fn call(
    app: &Router,
    method: &str,
    path: &str,
    token: &str,
    value: serde_json::Value,
) -> (StatusCode, String, serde_json::Value) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(path)
                .header("cookie", token)
                .header("content-type", "application/json")
                .body(Body::from(value.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let cookie = response
        .headers()
        .get("set-cookie")
        .and_then(|v| v.to_str().ok())
        .unwrap_or(token)
        .split(';')
        .next()
        .unwrap()
        .to_owned();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        cookie,
        serde_json::from_slice(&bytes).unwrap_or_default(),
    )
}
#[tokio::test]
async fn testing_preserves_sessions_and_read_only_survives_switching_and_vault() {
    let vault = Arc::new(
        adapters::vault::SqliteVault::open(":memory:", &[2; 32])
            .await
            .unwrap(),
    );
    let app = router_with_services(
        false,
        Some(vault),
        DatabaseService::new(vec![Arc::new(InstanceAdapter)]),
    );
    let input = serde_json::json!({"provider":"postgres","host":"localhost","port":5432,"database":"app","username":"user","password":"private-password","ssl":true,"readOnly":true});
    let (status, token, result) =
        call(&app, "POST", "/api/connections/test", "", input.clone()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(token, "", "Testing must not issue a session cookie");
    assert_eq!(result["ok"], true);
    assert!(result["elapsedMs"].is_number());
    assert!(!result.to_string().contains("private-password"));
    assert_eq!(
        call(&app, "GET", "/api/session", "", serde_json::Value::Null)
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );

    let mut instance = input.clone();
    instance["allDatabases"] = true.into();
    let (status, token, session) = call(&app, "POST", "/api/session", "", instance).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(session["readOnly"], true);
    for db in ["other", "unavailable"] {
        let mut tested = input.clone();
        tested["database"] = db.into();
        let (status, returned_token, _) =
            call(&app, "POST", "/api/connections/test", &token, tested).await;
        assert_eq!(
            status,
            if db == "unavailable" {
                StatusCode::BAD_REQUEST
            } else {
                StatusCode::OK
            }
        );
        assert_eq!(returned_token, token);
        assert_eq!(
            call(&app, "GET", "/api/session", &token, serde_json::Value::Null)
                .await
                .2["database"],
            "postgres"
        );
    }
    assert_eq!(
        call(
            &app,
            "POST",
            "/api/connections",
            &token,
            serde_json::json!({"name":"Read only"})
        )
        .await
        .0,
        StatusCode::CREATED
    );
    let saved = call(
        &app,
        "GET",
        "/api/connections",
        &token,
        serde_json::Value::Null,
    )
    .await
    .2;
    let id = saved["connections"][0]["id"].as_str().unwrap();
    let (status, token, session) = call(
        &app,
        "POST",
        &format!("/api/connections/{id}/connect"),
        &token,
        serde_json::Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(session["readOnly"], true);
    let (status, token, session) = call(
        &app,
        "POST",
        "/api/databases",
        &token,
        serde_json::json!({"database":"app\"db"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(session["readOnly"], true);
    assert_eq!(
        call(
            &app,
            "GET",
            "/api/database-settings",
            &token,
            serde_json::Value::Null
        )
        .await
        .2["canEdit"],
        false
    );
    for (method, path, body) in [
        (
            "POST",
            "/api/mutations",
            serde_json::json!({"confirmation":"APPLY","mutation":{"action":"execute","sql":"DROP TABLE customers"}}),
        ),
        (
            "PATCH",
            "/api/database-settings",
            serde_json::json!({"database":"app\"db","confirmation":"APPLY","changes":{"work_mem":{"value":"16MB","expected":null}}}),
        ),
    ] {
        let (status, _, result) = call(&app, method, path, &token, body).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(result["message"].as_str().unwrap().contains("read only"));
    }
}

#[tokio::test]
async fn tls_validation_is_bounded_and_rejects_unsupported_combinations() {
    let app = router_with_services(
        false,
        None,
        DatabaseService::new(vec![Arc::new(InstanceAdapter)]),
    );
    let base = serde_json::json!({"provider":"postgres","host":"localhost","port":5432,"database":"app","username":"user","password":"secret","ssl":true});
    let cert = "-----BEGIN CERTIFICATE-----\nTEST\n-----END CERTIFICATE-----";
    let key = "-----BEGIN PRIVATE KEY-----\nTEST\n-----END PRIVATE KEY-----";
    for extra in [
        serde_json::json!({"tlsCa":"not PEM"}),
        serde_json::json!({"tlsCert":cert}),
        serde_json::json!({"tlsKey":key}),
        serde_json::json!({"tlsCa":cert,"ssl":false}),
        serde_json::json!({"tlsCa":cert,"provider":"sqlserver"}),
        serde_json::json!({"tlsCa":"x".repeat(32769)}),
    ] {
        let mut input = base.clone();
        input
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        assert_eq!(
            call(&app, "POST", "/api/connections/test", "", input)
                .await
                .0,
            StatusCode::BAD_REQUEST
        );
    }
    let mut input = base;
    input["tlsCa"] = cert.into();
    input["tlsCert"] = cert.into();
    input["tlsKey"] = key.into();
    assert_eq!(
        call(&app, "POST", "/api/connections/test", "", input)
            .await
            .0,
        StatusCode::OK
    );
}

#[tokio::test]
async fn instance_switching_and_saved_connections_never_expose_credentials() {
    let vault = Arc::new(
        adapters::vault::SqliteVault::open(":memory:", &[1; 32])
            .await
            .unwrap(),
    );
    let app = router_with_services(
        false,
        Some(vault),
        DatabaseService::new(vec![Arc::new(InstanceAdapter)]),
    );
    let input = serde_json::json!({"provider":"postgres","host":"localhost","port":5432,"database":"","allDatabases":true,"username":"user","password":"private-password","ssl":true});
    let mut single = input.clone();
    single["allDatabases"] = false.into();
    assert_eq!(
        call(&app, "POST", "/api/session", "", single).await.0,
        StatusCode::BAD_REQUEST
    );
    let (status, token, session) = call(&app, "POST", "/api/session", "", input).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(session["database"], "postgres");
    assert_eq!(session["allDatabases"], true);
    assert!(session.get("password").is_none());
    let (status, _, expanded) = call(
        &app,
        "GET",
        "/api/tables?database=app%22db",
        &token,
        serde_json::Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(expanded[0]["name"], "app\"db");
    assert_eq!(
        call(&app, "GET", "/api/session", &token, serde_json::Value::Null)
            .await
            .2["database"],
        "postgres",
        "Expanding another database must preserve the active session"
    );
    assert_eq!(
        call(
            &app,
            "GET",
            "/api/tables?database=outside-instance",
            &token,
            serde_json::Value::Null
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        call(
            &app,
            "GET",
            "/api/tables?database=app%22db",
            "",
            serde_json::Value::Null
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        call(
            &app,
            "GET",
            "/api/databases",
            &token,
            serde_json::Value::Null
        )
        .await
        .2
        .as_array()
        .unwrap()
        .len(),
        3
    );
    for database in ["outside-instance", "unavailable"] {
        assert_eq!(
            call(
                &app,
                "POST",
                "/api/databases",
                &token,
                serde_json::json!({"database":database})
            )
            .await
            .0,
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            call(&app, "GET", "/api/session", &token, serde_json::Value::Null)
                .await
                .2["database"],
            "postgres"
        );
    }
    let (status, next, switched) = call(
        &app,
        "POST",
        "/api/databases",
        &token,
        serde_json::json!({"database":"app\"db"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(switched["database"], "app\"db");
    let (status, _, settings) = call(
        &app,
        "GET",
        "/api/database-settings",
        &next,
        serde_json::Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(settings["database"], "app\"db");
    assert_eq!(settings["provider"], "postgres");
    let patch = serde_json::json!({"database":"app\"db","confirmation":"APPLY","changes":{"work_mem":{"value":"16MB","expected":null}}});
    assert_eq!(
        call(
            &app,
            "PATCH",
            "/api/database-settings",
            &next,
            patch.clone()
        )
        .await
        .0,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        call(&app, "PATCH", "/api/database-settings", "", patch.clone())
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    let mut wrong_database = patch.clone();
    wrong_database["database"] = "postgres".into();
    assert_eq!(
        call(
            &app,
            "PATCH",
            "/api/database-settings",
            &next,
            wrong_database
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    let mut unconfirmed = patch;
    unconfirmed["confirmation"] = "".into();
    assert_eq!(
        call(&app, "PATCH", "/api/database-settings", &next, unconfirmed)
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    assert!(!settings.to_string().contains("private-password"));
    assert_eq!(
        call(
            &app,
            "GET",
            "/api/database-settings",
            "",
            serde_json::Value::Null
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        call(&app, "GET", "/api/tables", &token, serde_json::Value::Null)
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        call(&app, "GET", "/api/tables", &next, serde_json::Value::Null)
            .await
            .2[0]["name"],
        "app\"db"
    );
    assert_eq!(
        call(
            &app,
            "POST",
            "/api/connections",
            "",
            serde_json::json!({"name":"Work"})
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        call(
            &app,
            "POST",
            "/api/connections",
            &next,
            serde_json::json!({"name":"Work"})
        )
        .await
        .0,
        StatusCode::CREATED
    );
    let list = call(&app, "GET", "/api/connections", "", serde_json::Value::Null)
        .await
        .2;
    assert!(!list.to_string().contains("private-password"));
    assert_eq!(list["connections"][0].as_object().unwrap().len(), 2);
    let id = list["connections"][0]["id"].as_str().unwrap();
    let (status, _, restored) = call(
        &app,
        "POST",
        &format!("/api/connections/{id}/connect"),
        &next,
        serde_json::Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(restored["allDatabases"], true);
    assert!(restored.get("password").is_none());
    assert_eq!(
        call(
            &app,
            "DELETE",
            &format!("/api/connections/{id}"),
            "",
            serde_json::Value::Null
        )
        .await
        .0,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        call(
            &app,
            "POST",
            &format!("/api/connections/{id}/connect"),
            "",
            serde_json::Value::Null
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
}
