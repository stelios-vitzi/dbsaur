use std::{sync::Arc, time::Duration};

use axum::{Json, Router, extract::State, http::StatusCode, routing::get};
use serde::Serialize;
use tauri_plugin_updater::{Update, UpdaterExt};
use tokio::sync::Mutex;

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct Status {
    phase: &'static str,
    version: Option<String>,
    downloaded: u64,
    total: Option<u64>,
    message: Option<&'static str>,
}

struct Updater {
    app: tauri::AppHandle,
    status: Mutex<Status>,
    pending: Mutex<Option<Update>>,
}

pub fn routes(app: tauri::AppHandle) -> Router {
    let state = Arc::new(Updater {
        app,
        status: Mutex::new(Status {
            phase: "checking",
            ..Status::default()
        }),
        pending: Mutex::new(None),
    });
    let startup = state.clone();
    tauri::async_runtime::spawn(async move { check(startup).await });
    Router::new()
        .route("/api/desktop/update", get(status))
        .route("/api/desktop/update/check", axum::routing::post(recheck))
        .route("/api/desktop/update/install", axum::routing::post(install))
        .with_state(state)
}

async fn status(State(state): State<Arc<Updater>>) -> Json<Status> {
    Json(state.status.lock().await.clone())
}

async fn check(state: Arc<Updater>) {
    #[cfg(target_os = "linux")]
    if std::env::var_os("APPIMAGE").is_none() {
        let mut status = state.status.lock().await;
        status.phase = "unavailable";
        status.message = Some("Use your package manager to update this installation.");
        return;
    }
    let configured = state
        .app
        .config()
        .plugins
        .0
        .get("updater")
        .and_then(|config| config.get("pubkey"))
        .and_then(|key| key.as_str())
        .is_some_and(|key| !key.trim().is_empty());
    if !configured {
        let mut status = state.status.lock().await;
        status.phase = "unavailable";
        status.message = Some("Updates are not configured in this build.");
        return;
    }
    let result = async {
        state
            .app
            .updater_builder()
            .timeout(Duration::from_secs(20))
            .build()?
            .check()
            .await
    }
    .await;
    let mut pending = state.pending.lock().await;
    let mut status = state.status.lock().await;
    *status = Status::default();
    match result {
        Ok(Some(update)) => {
            status.phase = "available";
            status.version = Some(update.version.clone());
            *pending = Some(update);
        }
        Ok(None) => {
            status.phase = "current";
            *pending = None;
        }
        Err(_) => {
            status.phase = "error";
            status.message =
                Some("Could not check for updates. Check your connection and try again.");
        }
    }
}

async fn recheck(State(state): State<Arc<Updater>>) -> StatusCode {
    {
        let mut status = state.status.lock().await;
        if matches!(status.phase, "checking" | "downloading" | "installing") {
            return StatusCode::CONFLICT;
        }
        status.phase = "checking";
        status.message = None;
    }
    tauri::async_runtime::spawn(check(state));
    StatusCode::ACCEPTED
}

async fn install(State(state): State<Arc<Updater>>) -> StatusCode {
    let update = {
        let mut pending = state.pending.lock().await;
        let mut status = state.status.lock().await;
        if status.phase != "available" {
            return StatusCode::CONFLICT;
        }
        let Some(update) = pending.take() else {
            return StatusCode::CONFLICT;
        };
        status.phase = "downloading";
        status.downloaded = 0;
        status.total = None;
        update
    };
    tauri::async_runtime::spawn(async move {
        // Download verifies the signature before installation. Keep the UI responsive.
        let progress = state.clone();
        let mut downloaded = 0;
        let result = update
            .download(
                move |chunk, total| {
                    downloaded += chunk as u64;
                    // Callbacks run on this async task; use a nonblocking lock for progress only.
                    if let Ok(mut status) = progress.status.try_lock() {
                        status.downloaded = downloaded;
                        status.total = total;
                    }
                },
                || {},
            )
            .await;
        let result = match result {
            Ok(bytes) => {
                state.status.lock().await.phase = "installing";
                let installer = update.clone();
                tauri::async_runtime::spawn_blocking(move || installer.install(bytes))
                    .await
                    .unwrap_or_else(|_| {
                        Err(tauri_plugin_updater::Error::Io(std::io::Error::other(
                            "Installation failed",
                        )))
                    })
            }
            Err(error) => Err(error),
        };
        if result.is_ok() {
            state.app.restart();
        }
        let mut status = state.status.lock().await;
        status.phase = "error";
        status.message = Some("Update failed. Check for updates to retry.");
    });
    StatusCode::ACCEPTED
}
