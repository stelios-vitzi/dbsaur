use super::{connect, sensitive_setting};
use crate::adapters::sqlx;
use crate::{application::*, domain::*};
use sqlx::{Connection, Row};

pub(super) fn editable(name: &str, context: &str, superuser: bool) -> bool {
    !sensitive_setting(name)
        && !matches!(
            name,
            "role"
                | "session_authorization"
                | "transaction_read_only"
                | "transaction_deferrable"
                | "transaction_isolation"
                | "in_hot_standby"
        )
        && (context == "user" || (context == "superuser" && superuser))
}

pub(super) async fn permissions(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
) -> AppResult<(bool, bool)> {
    Ok(sqlx::query_as("SELECT (r.rolsuper OR pg_has_role(d.datdba, 'USAGE')), r.rolsuper FROM pg_database d CROSS JOIN pg_roles r WHERE d.datname = current_database() AND r.rolname = current_user")
        .fetch_one(&mut **tx).await?)
}

fn settings_error(error: sqlx::Error) -> AppError {
    match &error {
        sqlx::Error::Database(db) => match db.code().as_deref() {
            Some("42501") => invalid(
                "Permission denied. Only the database owner or an authorized administrator can save these settings.",
            ),
            Some("22023" | "22003" | "22P02" | "0A000" | "55P02") => invalid(
                "A setting value is invalid or cannot be applied to this database. Check the allowed values and ranges.",
            ),
            _ => invalid(
                "The database rejected these settings. No changes were saved. Check values and permissions.",
            ),
        },
        _ => error.into(),
    }
}

pub(super) async fn update(
    input: &ConnectionInput,
    request: &UpdateDatabaseSettings,
) -> AppResult<()> {
    request.validate().map_err(invalid)?;
    if request.database != input.database {
        return Err(invalid(
            "The selected database changed. Reopen settings before saving.",
        ));
    }
    let mut connection = connect(input).await?;
    let mut tx = connection.begin().await?;
    sqlx::query("SET LOCAL statement_timeout = '10s'")
        .execute(&mut *tx)
        .await?;
    sqlx::query("SET LOCAL lock_timeout = '2s'")
        .execute(&mut *tx)
        .await?;
    let (owner, superuser) = permissions(&mut tx).await?;
    if !owner {
        return Err(invalid(
            "Only the database owner or an authorized administrator can save settings.",
        ));
    }
    // Serialize DBsaur saves for this database, and compare the saved overrides
    // rather than session values (which may include role-specific overrides).
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended(current_database(), 68329))")
        .execute(&mut *tx)
        .await?;
    let metadata = sqlx::query("SELECT s.name, s.context, (SELECT substr(config, strpos(config, '=') + 1) FROM pg_db_role_setting dr CROSS JOIN LATERAL unnest(dr.setconfig) config WHERE dr.setdatabase = (SELECT oid FROM pg_database WHERE datname = current_database()) AND dr.setrole = 0 AND split_part(config, '=', 1) = s.name) AS value FROM pg_settings s")
        .fetch_all(&mut *tx).await?;
    for (name, change) in &request.changes {
        let setting = metadata
            .iter()
            .find(|s| s.get::<String, _>("name") == *name)
            .ok_or_else(|| invalid("Unknown setting. Reload database settings."))?;
        if !editable(name, setting.get("context"), superuser) {
            return Err(invalid(
                "This setting cannot be changed at database scope with your permissions.",
            ));
        }
        if setting.get::<Option<String>, _>("value") != change.expected {
            return Err(invalid(
                "Settings changed since you opened this window. Reload settings and review your changes.",
            ));
        }
    }
    if let Some(limit) = request.connection_limit {
        let current: i32 = sqlx::query_scalar(
            "SELECT datconnlimit FROM pg_database WHERE datname = current_database()",
        )
        .fetch_one(&mut *tx)
        .await?;
        if Some(current) != request.expected_connection_limit {
            return Err(invalid(
                "The connection limit changed. Reload settings before saving.",
            ));
        }
        // DDL has no bind slots. PostgreSQL's format quotes the identifier;
        // the numeric value is parsed and validated before formatting.
        let sql: String = sqlx::query_scalar(
            "SELECT format('ALTER DATABASE %I CONNECTION LIMIT %s', $1::text, $2::int)",
        )
        .bind(&input.database)
        .bind(limit)
        .fetch_one(&mut *tx)
        .await?;
        sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .execute(&mut *tx)
            .await
            .map_err(settings_error)?;
    }
    for (name, change) in &request.changes {
        // Never interpolate user values into SQL. The server formats bound
        // values with %L and identifiers with %I, including quoted DB names.
        let sql: String = if let Some(value) = &change.value {
            sqlx::query_scalar(
                "SELECT format('ALTER DATABASE %I SET %I TO %L', $1::text, $2::text, $3::text)",
            )
            .bind(&input.database)
            .bind(name)
            .bind(value)
            .fetch_one(&mut *tx)
            .await?
        } else {
            sqlx::query_scalar("SELECT format('ALTER DATABASE %I RESET %I', $1::text, $2::text)")
                .bind(&input.database)
                .bind(name)
                .fetch_one(&mut *tx)
                .await?
        };
        sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .execute(&mut *tx)
            .await
            .map_err(settings_error)?;
    }
    tx.commit().await.map_err(settings_error)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::domain::UpdateDatabaseSettings;
    #[test]
    fn validates_confirmation_bounds_and_resets() {
        let base = serde_json::json!({"database":"app", "confirmation":"APPLY", "changes":{"work_mem":{"value":null,"expected":"4MB"}}});
        let parse = |v| {
            serde_json::from_value::<UpdateDatabaseSettings>(v)
                .unwrap()
                .validate()
        };
        assert!(parse(base.clone()).is_ok());
        for value in [
            serde_json::json!({"confirmation":"", "database":"app", "changes":{}}),
            serde_json::json!({"confirmation":"APPLY", "database":"app", "changes":{}}),
            serde_json::json!({"confirmation":"APPLY", "database":"app", "connectionLimit":-2,"expectedConnectionLimit":-1}),
            serde_json::json!({"confirmation":"APPLY", "database":"app", "connectionLimit":3}),
        ] {
            assert!(parse(value).is_err());
        }
        let mut oversized = base;
        oversized["changes"]["work_mem"]["value"] = "x".repeat(4097).into();
        assert!(parse(oversized).is_err());
    }
    #[test]
    fn only_permitted_database_defaults_are_editable() {
        assert!(super::editable("work_mem", "user", false));
        assert!(!super::editable("shared_buffers", "postmaster", true));
        assert!(!super::editable("log_statement", "superuser", false));
        assert!(super::editable("log_statement", "superuser", true));
        assert!(!super::editable(
            "session_preload_libraries",
            "superuser",
            false
        ));
        for name in [
            "primary_conninfo",
            "archive_command",
            "role",
            "session_authorization",
            "transaction_read_only",
        ] {
            assert!(!super::editable(name, "user", true));
        }
    }
}
