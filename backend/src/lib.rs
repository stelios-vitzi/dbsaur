pub mod adapters;
pub mod application;
pub mod domain;
mod http;
mod sessions;

use application::{ConnectionRepository, DatabaseService};
use axum::Router;
use std::sync::Arc;

pub fn router(secure_cookies: bool) -> Router {
    router_with_repository(secure_cookies, None)
}
pub fn router_with_repository(
    secure_cookies: bool,
    connections: Option<Arc<dyn ConnectionRepository>>,
) -> Router {
    http::router_with_services(
        secure_cookies,
        connections,
        DatabaseService::new(vec![
            Arc::new(adapters::postgres::PostgresAdapter),
            Arc::new(adapters::demo::DemoAdapter),
            Arc::new(adapters::mysql::MySqlAdapter),
            Arc::new(adapters::mariadb::MariaDbAdapter),
            Arc::new(adapters::sqlserver::SqlServerAdapter),
            Arc::new(adapters::sqlite::SqliteAdapter),
            Arc::new(adapters::mongodb::MongoAdapter),
        ]),
    )
}
