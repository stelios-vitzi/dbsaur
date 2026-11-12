use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use tower::ServiceExt;

#[tokio::test]
async fn session_browsing_validation_and_disconnect() {
    let app = dbsaur::router(false);
    let unauthorized = app
        .clone()
        .oneshot(Request::get("/api/tables").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);
    let connection = r#"{"provider":"demo","host":"","port":5432,"database":"","username":"","password":"","ssl":true}"#;
    let connected = app
        .clone()
        .oneshot(
            Request::post("/api/session")
                .header("content-type", "application/json")
                .body(Body::from(connection))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(connected.status(), StatusCode::OK);
    let cookie = connected.headers()["set-cookie"]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();
    for (token, value, status) in [
        (
            "",
            "010100000000000000000000000000000000000000",
            StatusCode::UNAUTHORIZED,
        ),
        (
            cookie.as_str(),
            "'); DROP TABLE places;--",
            StatusCode::BAD_REQUEST,
        ),
        (
            cookie.as_str(),
            "010100000000000000000000000000000000000000",
            StatusCode::BAD_REQUEST,
        ),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::post("/api/geometry")
                    .header("content-type", "application/json")
                    .header("cookie", token)
                    .body(Body::from(serde_json::json!({"value":value}).to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), status);
        assert_eq!(response.headers()["cache-control"], "no-store");
    }
    // Geometry previews have a larger body limit than ordinary API requests.
    let response = app
        .clone()
        .oneshot(
            Request::post("/api/geometry")
                .header("content-type", "application/json")
                .header("cookie", cookie.as_str())
                .body(Body::from(
                    serde_json::json!({"value": "00".repeat(16 * 1024)}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST); // Unsupported demo provider, not 413.
    for (token, confirmation, expected) in [
        ("", "APPLY", StatusCode::UNAUTHORIZED),
        (cookie.as_str(), "", StatusCode::BAD_REQUEST),
        (cookie.as_str(), "APPLY", StatusCode::BAD_REQUEST),
    ] {
        let response = app.clone().oneshot(
            Request::post("/api/mutations").header("content-type", "application/json")
                .header("cookie", token)
                .body(Body::from(serde_json::json!({"confirmation":confirmation,"mutation":{"action":"execute","sql":"DROP TABLE customers"}}).to_string())).unwrap()
        ).await.unwrap();
        assert_eq!(response.status(), expected);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert!(!String::from_utf8_lossy(&body).contains("password"));
    }
    assert!(
        connected.headers()["set-cookie"]
            .to_str()
            .unwrap()
            .contains("HttpOnly; SameSite=Strict")
    );
    for (path, status) in [
        ("/api/database-settings", StatusCode::BAD_REQUEST),
        ("/api/tables", StatusCode::OK),
        (
            "/api/tables?database=outside-session",
            StatusCode::BAD_REQUEST,
        ),
        (
            "/api/rows?schema=public&table=customers&pageSize=5",
            StatusCode::OK,
        ),
        (
            "/api/rows?schema=public&table=customers&pageSize=500",
            StatusCode::OK,
        ),
        (
            "/api/rows?schema=public&table=customers&pageSize=1000",
            StatusCode::OK,
        ),
        (
            "/api/rows?schema=public&table=customers&pageSize=1001",
            StatusCode::BAD_REQUEST,
        ),
        (
            "/api/rows?schema=public&table=customers&sort=missing",
            StatusCode::BAD_REQUEST,
        ),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::get(path)
                    .header("cookie", &cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), status, "{path}");
        if path == "/api/tables" {
            let body = response.into_body().collect().await.unwrap().to_bytes();
            let tables: serde_json::Value = serde_json::from_slice(&body).unwrap();
            let customers = tables
                .as_array()
                .unwrap()
                .iter()
                .find(|table| table["name"] == "customers")
                .unwrap();
            let columns = customers["columns"].as_array().unwrap();
            assert!(columns.iter().any(|column| column == "id"));
            assert!(columns.iter().any(|column| column == "email"));
            assert!(
                customers["primaryKeys"]
                    .as_array()
                    .unwrap()
                    .contains(&serde_json::json!("id"))
            );
            let orders = tables
                .as_array()
                .unwrap()
                .iter()
                .find(|table| table["name"] == "orders")
                .unwrap();
            assert_eq!(orders["foreignKeys"][0]["column"], "customer_id");
            assert_eq!(orders["foreignKeys"][0]["referencedTable"], "customers");
            assert_eq!(orders["foreignKeys"][0]["referencedColumn"], "id");
            continue;
        }
        if path.ends_with("pageSize=5") {
            let body = response.into_body().collect().await.unwrap().to_bytes();
            let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(json["rows"].as_array().unwrap().len(), 5);
            assert_eq!(json["hasMore"], true);
        } else if status == StatusCode::OK && path.starts_with("/api/rows?") {
            let body = response.into_body().collect().await.unwrap().to_bytes();
            let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(json["rows"].as_array().unwrap().len(), 124);
            assert_eq!(json["hasMore"], false);
            assert_eq!(
                json["pageSize"],
                if path.ends_with("pageSize=500") {
                    500
                } else {
                    1000
                }
            );
        }
    }
    for (sql, page, page_size, status, expected_rows, has_more) in [
        (
            "SELECT * FROM \"public\".\"customers\";",
            0,
            5,
            StatusCode::OK,
            5,
            true,
        ),
        (
            "select * from public.orders LIMIT 7",
            1,
            5,
            StatusCode::OK,
            2,
            false,
        ),
        (
            "SELECT * FROM products LIMIT 0",
            0,
            5,
            StatusCode::OK,
            0,
            false,
        ),
        (
            "SELECT * FROM products LIMIT 7",
            2,
            5,
            StatusCode::OK,
            0,
            false,
        ),
        ("", 0, 5, StatusCode::BAD_REQUEST, 0, false),
        (
            "SELECT * FROM customers",
            0,
            500,
            StatusCode::OK,
            124,
            false,
        ),
        (
            "SELECT * FROM customers",
            0,
            1000,
            StatusCode::OK,
            124,
            false,
        ),
        (
            "SELECT * FROM customers",
            0,
            1001,
            StatusCode::BAD_REQUEST,
            0,
            false,
        ),
        (
            "DELETE FROM customers",
            0,
            5,
            StatusCode::BAD_REQUEST,
            0,
            false,
        ),
        (
            "SELECT * FROM customers; DELETE FROM customers",
            0,
            5,
            StatusCode::BAD_REQUEST,
            0,
            false,
        ),
        (
            "SELECT name FROM customers WHERE name ILIKE 'amelia%' AND email IS NOT NULL",
            0,
            5,
            StatusCode::OK,
            5,
            true,
        ),
        (
            "SELECT * FROM generate_series(1, 3)",
            0,
            5,
            StatusCode::BAD_REQUEST,
            0,
            false,
        ),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::post("/api/query")
                    .header("cookie", &cookie)
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({"sql": sql, "page": page, "pageSize": page_size})
                            .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), status, "{sql}");
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        if status == StatusCode::OK {
            assert_eq!(json["rows"].as_array().unwrap().len(), expected_rows);
            assert_eq!(json["hasMore"], has_more);
            assert_eq!(json["page"], page);
        } else {
            assert!(json["message"].is_string());
        }
    }
    let unauthorized_query = app
        .clone()
        .oneshot(
            Request::post("/api/query")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"sql":"SELECT * FROM customers"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(unauthorized_query.status(), StatusCode::UNAUTHORIZED);
    let response = app
        .clone()
        .oneshot(
            Request::delete("/api/session")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let response = app
        .oneshot(
            Request::get("/api/tables")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn registry_advertises_real_database_providers() {
    let response = dbsaur::router(false)
        .oneshot(Request::get("/api/providers").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let providers: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let ids: Vec<_> = providers
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["demo", "mariadb", "mysql", "postgres", "sqlserver"]);
}
