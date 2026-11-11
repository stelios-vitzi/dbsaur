#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod server;
mod updater;

use std::{fs, io::Write, sync::Arc};
use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            // A stable origin preserves saved SQL and UI preferences between launches.
            // Bind before showing the window; never reuse an unrelated local service.
            let listener = std::net::TcpListener::bind(server::ADDRESS)?;
            listener.set_nonblocking(true)?;
            let data = app.path().app_local_data_dir()?;
            fs::create_dir_all(&data)?;
            let key_path = data.join("vault.key");
            let key = match fs::read(&key_path) {
                Ok(key) => key,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    if data.join("connections.sqlite").exists() {
                        return Err(
                            "Desktop vault key is missing; restore vault.key from backup".into(),
                        );
                    }
                    let key: Vec<u8> = [uuid::Uuid::new_v4(), uuid::Uuid::new_v4()]
                        .iter()
                        .flat_map(|id| id.as_bytes().to_vec())
                        .collect();
                    let mut options = fs::OpenOptions::new();
                    options.write(true).create_new(true);
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::OpenOptionsExt;
                        options.mode(0o600);
                    }
                    options.open(&key_path)?.write_all(&key)?;
                    key
                }
                Err(error) => return Err(error.into()),
            };
            let key: [u8; 32] = key.try_into().map_err(|_| "Invalid desktop vault key")?;
            let repository =
                tauri::async_runtime::block_on(dbsaur::adapters::vault::SqliteVault::open(
                    data.join("connections.sqlite")
                        .to_str()
                        .ok_or("Invalid data path")?,
                    &key,
                ))
                .map_err(|_| "Unable to open the desktop connection store")?;
            let token = uuid::Uuid::new_v4().to_string();
            let handle = app.handle().clone();
            let assets = Arc::new(app.asset_resolver());
            let router = tauri::async_runtime::block_on(async {
                dbsaur::router_with_repository(false, Some(Arc::new(repository)))
            })
            .merge(updater::routes(app.handle().clone()))
            .fallback(move |request: axum::extract::Request| {
                let assets = assets.clone();
                async move {
                    use axum::{
                        http::{StatusCode, header},
                        response::IntoResponse,
                    };
                    if !matches!(
                        *request.method(),
                        axum::http::Method::GET | axum::http::Method::HEAD
                    ) || request.uri().path().starts_with("/api/")
                    {
                        return StatusCode::NOT_FOUND.into_response();
                    }
                    let path = request.uri().path().trim_start_matches('/');
                    let asset = assets
                        .get(path.to_string())
                        .or_else(|| assets.get("index.html".into()));
                    match asset {
                        Some(asset) => {
                            ([(header::CONTENT_TYPE, asset.mime_type)], asset.bytes).into_response()
                        }
                        None => StatusCode::NOT_FOUND.into_response(),
                    }
                }
            });
            let router = server::protect(router, token.clone());
            tauri::async_runtime::spawn(async move {
                let result = async {
                    let listener = tokio::net::TcpListener::from_std(listener)?;
                    axum::serve(listener, router).await
                }
                .await;
                if result.is_err() {
                    // Never leave a window connected to a failed/replaced local API.
                    handle.exit(1);
                }
            });
            WebviewWindowBuilder::new(
                app,
                "main",
                WebviewUrl::External(format!("{}/_desktop/{token}", server::ORIGIN).parse()?),
            )
            .title("DBsaur")
            .inner_size(1440.0, 900.0)
            .min_inner_size(900.0, 600.0)
            .on_navigation(|url| url.origin().ascii_serialization() == server::ORIGIN)
            .build()?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("Unable to start DBsaur desktop");
}
