//! The SQLx API used by our database adapters, pinned to matching driver/core versions.
//!
//! Depending on the drivers directly avoids the umbrella crate's optional SQLite
//! dependency, whose native-library constraint prevents using patched SQLite in the vault.
pub use sqlx_core::{
    Error,
    column::Column,
    connection::{ConnectOptions, Connection},
    executor::Executor,
    query::query,
    query_as::query_as,
    query_scalar::query_scalar,
    row::Row,
    sql_str::{AssertSqlSafe, SqlSafeStr},
    statement::Statement,
    transaction::Transaction,
    type_info::TypeInfo,
};
pub use sqlx_mysql::{self as mysql, MySqlConnection};
pub use sqlx_postgres::{self as postgres, PgConnection, Postgres};
