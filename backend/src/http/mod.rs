use crate::application::*;
use crate::domain::*;
use crate::sessions::{self, SessionStore};
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Path, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use std::sync::Arc;

struct AppState {
    database: DatabaseService,
    sessions: SessionStore,
    secure_cookies: bool,
    operations: tokio::sync::Semaphore,
    connections: Option<Arc<dyn ConnectionRepository>>,
}
type Shared = Arc<AppState>;

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = match self.0 {
            ErrorKind::InvalidInput => StatusCode::BAD_REQUEST,
            ErrorKind::NotConnected => StatusCode::UNAUTHORIZED,
            ErrorKind::NotFound => StatusCode::NOT_FOUND,
            ErrorKind::Busy => StatusCode::TOO_MANY_REQUESTS,
            ErrorKind::Upstream => StatusCode::BAD_GATEWAY,
            ErrorKind::Timeout => StatusCode::GATEWAY_TIMEOUT,
            ErrorKind::Internal => StatusCode::INTERNAL_SERVER_ERROR,
        };
        (status, Json(serde_json::json!({ "message": self.1 }))).into_response()
    }
}
fn session_token(headers: &HeaderMap) -> Option<&str> {
    headers
        .get("cookie")?
        .to_str()
        .ok()?
        .split(';')
        .find_map(|part| part.trim().strip_prefix("webdb_session="))
}

pub(crate) fn router_with_services(
    secure_cookies: bool,
    connections: Option<Arc<dyn ConnectionRepository>>,
    database: DatabaseService,
) -> Router {
    let state = Arc::new(AppState {
        database,
        sessions: SessionStore::default(),
        secure_cookies,
        operations: tokio::sync::Semaphore::new(16),
        connections,
    });
    let weak_state = Arc::downgrade(&state);
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
        loop {
            interval.tick().await;
            let Some(state) = weak_state.upgrade() else {
                break;
            };
            state.sessions.purge_expired();
        }
    });
    Router::new()
        .route(
            "/api/health",
            get(|| async { Json(serde_json::json!({"status":"ok"})) }),
        )
        .route("/api/providers", get(providers))
        .route(
            "/api/database-settings",
            get(database_settings).patch(update_database_settings),
        )
        .route(
            "/api/session",
            post(connect)
                .get(current_session)
                .delete(disconnect)
                .layer(DefaultBodyLimit::max(256 * 1024)),
        )
        .route(
            "/api/connections/test",
            post(test_connection).layer(DefaultBodyLimit::max(256 * 1024)),
        )
        .route("/api/tables", get(tables))
        .route("/api/databases", get(databases).post(select_database))
        .route(
            "/api/connections",
            get(saved_connections).post(save_connection),
        )
        .route(
            "/api/connections/{id}",
            axum::routing::delete(delete_connection),
        )
        .route("/api/connections/{id}/connect", post(connect_saved))
        .route("/api/rows", get(rows))
        .route("/api/query", post(query))
        .route(
            "/api/geometry",
            post(geometry).layer(DefaultBodyLimit::max(2 * 1024 * 1024)),
        )
        .route("/api/mutations", post(mutate))
        .layer(DefaultBodyLimit::max(16 * 1024))
        .layer(middleware::from_fn(security_headers))
        .with_state(state)
}
async fn security_headers(request: axum::extract::Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    response
        .headers_mut()
        .insert("cache-control", HeaderValue::from_static("no-store"));
    response.headers_mut().insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    response
}
async fn providers(State(state): State<Shared>) -> Json<Vec<Provider>> {
    Json(state.database.providers())
}

async fn database_settings(
    State(state): State<Shared>,
    headers: HeaderMap,
) -> AppResult<Json<DatabaseSettings>> {
    let _permit = state
        .operations
        .try_acquire()
        .map_err(|_| AppError(ErrorKind::Busy, "Too many requests. Try again shortly."))?;
    let input = state.sessions.get(session_token(&headers))?;
    let mut settings = state
        .database
        .resolve(&input.provider)?
        .settings(&input)
        .await?;
    if input.read_only {
        settings.can_edit = false;
        for setting in &mut settings.settings {
            setting["editable"] = false.into();
        }
    }
    Ok(Json(settings))
}

async fn update_database_settings(
    State(state): State<Shared>,
    headers: HeaderMap,
    Json(request): Json<UpdateDatabaseSettings>,
) -> AppResult<StatusCode> {
    let input = state.sessions.get(session_token(&headers))?;
    ensure_writable(&input)?;
    request.validate().map_err(invalid)?;
    if request.database != input.database {
        return Err(invalid(
            "The selected database changed. Reopen settings before saving.",
        ));
    }
    let _permit = state
        .operations
        .try_acquire()
        .map_err(|_| AppError(ErrorKind::Busy, "Too many requests. Try again shortly."))?;
    state
        .database
        .resolve(&input.provider)?
        .update_settings(&input, &request)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

fn session_info(input: &ConnectionInput) -> serde_json::Value {
    serde_json::json!({ "provider": input.provider, "database": if input.provider == "demo" { "sample_workspace" } else { &input.database }, "allDatabases": input.all_databases, "readOnly": input.read_only || input.provider == "demo", "host": if input.provider == "demo" { "Sample data" } else { &input.host }, "expiresIn": sessions::TTL_SECONDS })
}
fn ensure_writable(input: &ConnectionInput) -> AppResult<()> {
    if input.read_only {
        return Err(invalid(
            "This connection is read only. Reconnect with read-only mode disabled to make changes.",
        ));
    }
    Ok(())
}

async fn test_input(state: &AppState, input: &mut ConnectionInput) -> AppResult<()> {
    input.validate().map_err(invalid)?;
    let adapter = state.database.resolve(&input.provider)?;
    if input.all_databases {
        input.database = adapter
            .maintenance_database()
            .ok_or_else(|| invalid("Instance browsing is unavailable for this provider."))?
            .into();
    }
    tokio::time::timeout(std::time::Duration::from_secs(15), adapter.test(input))
        .await
        .map_err(|_| {
            AppError(
                ErrorKind::Timeout,
                "Connection test timed out. Check the host, port, and TLS settings.",
            )
        })?
}

async fn test_connection(
    State(state): State<Shared>,
    Json(mut input): Json<ConnectionInput>,
) -> AppResult<Json<serde_json::Value>> {
    let _permit = state
        .operations
        .try_acquire()
        .map_err(|_| AppError(ErrorKind::Busy, "Too many requests. Try again shortly."))?;
    let started = std::time::Instant::now();
    test_input(&state, &mut input).await?;
    Ok(Json(
        serde_json::json!({"ok": true, "elapsedMs": started.elapsed().as_millis()}),
    ))
}
async fn current_session(
    State(state): State<Shared>,
    headers: HeaderMap,
) -> AppResult<Json<serde_json::Value>> {
    Ok(Json(session_info(
        &state.sessions.get(session_token(&headers))?,
    )))
}

fn cookie(state: &AppState, id: &str, max_age: u64) -> HeaderValue {
    HeaderValue::from_str(&format!(
        "webdb_session={id}; HttpOnly; SameSite=Strict; Path=/api; Max-Age={max_age}{}",
        if state.secure_cookies { "; Secure" } else { "" }
    ))
    .expect("generated cookie is valid")
}
async fn connect(
    State(state): State<Shared>,
    headers: HeaderMap,
    Json(mut input): Json<ConnectionInput>,
) -> AppResult<(HeaderMap, Json<serde_json::Value>)> {
    let _permit = state
        .operations
        .try_acquire()
        .map_err(|_| AppError(ErrorKind::Busy, "Too many requests. Try again shortly."))?;
    test_input(&state, &mut input).await?;
    let info = session_info(&input);
    let id = state.sessions.insert(input, session_token(&headers))?;
    let mut headers = HeaderMap::new();
    headers.insert("set-cookie", cookie(&state, &id, sessions::TTL_SECONDS));
    Ok((headers, Json(info)))
}

async fn databases(
    State(state): State<Shared>,
    headers: HeaderMap,
) -> AppResult<Json<Vec<String>>> {
    let _permit = state
        .operations
        .try_acquire()
        .map_err(|_| AppError(ErrorKind::Busy, "Too many requests. Try again shortly."))?;
    let input = state.sessions.get(session_token(&headers))?;
    if !input.all_databases {
        return Ok(Json(vec![input.database]));
    }
    Ok(Json(
        state
            .database
            .resolve(&input.provider)?
            .databases(&input)
            .await?,
    ))
}
async fn select_database(
    State(state): State<Shared>,
    headers: HeaderMap,
    Json(selection): Json<SelectDatabase>,
) -> AppResult<(HeaderMap, Json<serde_json::Value>)> {
    let _permit = state
        .operations
        .try_acquire()
        .map_err(|_| AppError(ErrorKind::Busy, "Too many requests. Try again shortly."))?;
    let mut input = state.sessions.get(session_token(&headers))?;
    if !input.all_databases {
        return Err(invalid("Connect in instance mode to switch databases."));
    }
    let adapter = state.database.resolve(&input.provider)?;
    if !adapter
        .databases(&input)
        .await?
        .contains(&selection.database)
    {
        return Err(invalid("Choose an accessible database from this instance."));
    }
    input.database = selection.database;
    adapter.test(&input).await?;
    let info = session_info(&input);
    let id = state.sessions.insert(input, session_token(&headers))?;
    let mut headers = HeaderMap::new();
    headers.insert("set-cookie", cookie(&state, &id, sessions::TTL_SECONDS));
    Ok((headers, Json(info)))
}
fn repository(state: &AppState) -> AppResult<&dyn ConnectionRepository> {
    state.connections.as_deref().ok_or_else(|| {
        invalid("The desktop connection store is unavailable. Restart DBsaur and try again.")
    })
}
async fn saved_connections(State(state): State<Shared>) -> AppResult<Json<serde_json::Value>> {
    let items = match &state.connections {
        Some(store) => store.list().await?,
        None => vec![],
    };
    Ok(Json(
        serde_json::json!({"enabled": state.connections.is_some(), "connections": items}),
    ))
}
async fn save_connection(
    State(state): State<Shared>,
    headers: HeaderMap,
    Json(request): Json<SaveConnection>,
) -> AppResult<(StatusCode, Json<serde_json::Value>)> {
    let input = state.sessions.get(session_token(&headers))?;
    let name = request.name.trim();
    if name.is_empty()
        || name.len() > 128
        || name.chars().any(char::is_control)
        || input.provider == "demo"
    {
        return Err(invalid(
            "Enter a connection name of at most 128 characters for a real database.",
        ));
    }
    let id = repository(&state)?.save(name, &input).await?;
    Ok((StatusCode::CREATED, Json(serde_json::json!({ "id": id }))))
}
async fn delete_connection(
    State(state): State<Shared>,
    Path(id): Path<String>,
) -> AppResult<StatusCode> {
    repository(&state)?.delete(&id).await?;
    Ok(StatusCode::NO_CONTENT)
}
async fn connect_saved(
    State(state): State<Shared>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> AppResult<(HeaderMap, Json<serde_json::Value>)> {
    let input = repository(&state)?.load(&id).await?;
    connect(State(state), headers, Json(input)).await
}
async fn disconnect(State(state): State<Shared>, headers: HeaderMap) -> (HeaderMap, StatusCode) {
    state.sessions.remove(session_token(&headers));
    let mut headers = HeaderMap::new();
    headers.insert("set-cookie", cookie(&state, "", 0));
    (headers, StatusCode::NO_CONTENT)
}
async fn tables(
    State(state): State<Shared>,
    headers: HeaderMap,
    Query(query): Query<DatabaseTablesQuery>,
) -> AppResult<Json<Vec<Table>>> {
    let _permit = state
        .operations
        .try_acquire()
        .map_err(|_| AppError(ErrorKind::Busy, "Too many requests. Try again shortly."))?;
    let mut input = state.sessions.get(session_token(&headers))?;
    if let Some(database) = query.database
        && database != input.database
    {
        if !input.all_databases
            || !state
                .database
                .resolve(&input.provider)?
                .databases(&input)
                .await?
                .contains(&database)
        {
            return Err(invalid("Choose an accessible database from this instance."));
        }
        input.database = database;
    }
    Ok(Json(
        state
            .database
            .resolve(&input.provider)?
            .tables(&input)
            .await?,
    ))
}
async fn rows(
    State(state): State<Shared>,
    headers: HeaderMap,
    Query(query): Query<BrowseQuery>,
) -> AppResult<Json<TablePage>> {
    let _permit = state
        .operations
        .try_acquire()
        .map_err(|_| AppError(ErrorKind::Busy, "Too many requests. Try again shortly."))?;
    query.bounds().map_err(invalid)?;
    let input = state.sessions.get(session_token(&headers))?;
    Ok(Json(
        state
            .database
            .resolve(&input.provider)?
            .rows(&input, &query)
            .await?,
    ))
}

async fn query(
    State(state): State<Shared>,
    headers: HeaderMap,
    Json(query): Json<SqlQuery>,
) -> AppResult<Json<TablePage>> {
    let _permit = state
        .operations
        .try_acquire()
        .map_err(|_| AppError(ErrorKind::Busy, "Too many requests. Try again shortly."))?;
    let input = state.sessions.get(session_token(&headers))?;
    query.bounds().map_err(invalid)?;
    Ok(Json(
        state
            .database
            .resolve(&input.provider)?
            .query(&input, &query)
            .await?,
    ))
}

async fn geometry(
    State(state): State<Shared>,
    headers: HeaderMap,
    Json(request): Json<GeometryRequest>,
) -> AppResult<Json<GeometryPreview>> {
    let input = state.sessions.get(session_token(&headers))?;
    request.validate().map_err(invalid)?;
    let _permit = state
        .operations
        .try_acquire()
        .map_err(|_| AppError(ErrorKind::Busy, "Too many requests. Try again shortly."))?;
    Ok(Json(
        tokio::time::timeout(
            std::time::Duration::from_secs(15),
            state
                .database
                .resolve(&input.provider)?
                .geometry(&input, &request),
        )
        .await
        .map_err(|_| {
            AppError(
                ErrorKind::Timeout,
                "Map preview timed out. Try a smaller geometry.",
            )
        })??,
    ))
}

async fn mutate(
    State(state): State<Shared>,
    headers: HeaderMap,
    Json(request): Json<MutationRequest>,
) -> AppResult<Json<MutationResult>> {
    let input = state.sessions.get(session_token(&headers))?;
    ensure_writable(&input)?;
    if request.confirmation != "APPLY" {
        return Err(invalid("Review the change and type APPLY to confirm."));
    }
    let _permit = state
        .operations
        .try_acquire()
        .map_err(|_| AppError(ErrorKind::Busy, "Too many requests. Try again shortly."))?;
    Ok(Json(
        state
            .database
            .resolve(&input.provider)?
            .mutate(&input, &request.mutation)
            .await?,
    ))
}

#[cfg(test)]
mod tests;
