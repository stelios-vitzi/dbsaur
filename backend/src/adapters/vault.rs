use crate::{application::*, domain::*};
use aes_gcm::{
    Aes256Gcm, KeyInit, Nonce,
    aead::{Aead, Generate, Payload},
};
use async_trait::async_trait;
use rusqlite::{Connection, OptionalExtension, params};
use std::sync::{Arc, Mutex};

pub struct SqliteVault {
    connection: Arc<Mutex<Connection>>,
    cipher: Aes256Gcm,
}
fn unavailable() -> AppError {
    AppError(
        ErrorKind::Internal,
        "Saved connections are unavailable. Check the local store and encryption key.",
    )
}
impl SqliteVault {
    fn encrypted(&self, id: &str, input: &ConnectionInput) -> AppResult<(Vec<u8>, Vec<u8>)> {
        let nonce = Nonce::generate();
        let plaintext = serde_json::to_vec(input).map_err(|_| unavailable())?;
        let ciphertext = self
            .cipher
            .encrypt(
                &nonce,
                Payload {
                    msg: &plaintext,
                    aad: id.as_bytes(),
                },
            )
            .map_err(|_| unavailable())?;
        Ok((nonce.to_vec(), ciphertext))
    }
    pub async fn open(path: &str, key: &[u8; 32]) -> AppResult<Self> {
        let path = path.to_owned();
        let connection = tokio::task::spawn_blocking(move || -> rusqlite::Result<Connection> {
            let connection = Connection::open(path)?;
            connection.busy_timeout(std::time::Duration::from_secs(5))?;
            connection.pragma_update(None, "secure_delete", "ON")?;
            connection.execute_batch("CREATE TABLE IF NOT EXISTS saved_connections (id TEXT PRIMARY KEY, name TEXT NOT NULL, nonce BLOB NOT NULL, ciphertext BLOB NOT NULL)")?;
            Ok(connection)
        }).await.map_err(|_| unavailable())?.map_err(|_| unavailable())?;
        Ok(Self {
            connection: Arc::new(Mutex::new(connection)),
            cipher: Aes256Gcm::new_from_slice(key).map_err(|_| unavailable())?,
        })
    }

    // SQLite stays on the blocking pool; callers keep the existing async repository contract.
    async fn with_connection<T: Send + 'static>(
        &self,
        operation: impl FnOnce(&mut Connection) -> rusqlite::Result<T> + Send + 'static,
    ) -> AppResult<T> {
        let connection = Arc::clone(&self.connection);
        tokio::task::spawn_blocking(move || {
            let mut connection = connection.lock().map_err(|_| unavailable())?;
            operation(&mut connection).map_err(|_| unavailable())
        })
        .await
        .map_err(|_| unavailable())?
    }
}
#[async_trait]
impl ConnectionRepository for SqliteVault {
    async fn list(&self) -> AppResult<Vec<SavedConnection>> {
        self.with_connection(|connection| {
            let mut statement =
                connection.prepare("SELECT id, name FROM saved_connections ORDER BY name, id")?;
            statement
                .query_map([], |row| {
                    Ok(SavedConnection {
                        id: row.get(0)?,
                        name: row.get(1)?,
                    })
                })?
                .collect()
        })
        .await
    }
    async fn save(&self, name: &str, input: &ConnectionInput) -> AppResult<String> {
        let id = uuid::Uuid::new_v4().to_string();
        let (nonce, ciphertext) = self.encrypted(&id, input)?;
        let saved_id = id.clone();
        let name = name.to_owned();
        let affected = self.with_connection(move |connection| {
            connection.execute("INSERT INTO saved_connections (id, name, nonce, ciphertext) SELECT ?, ?, ?, ? WHERE (SELECT COUNT(*) FROM saved_connections) < 100", params![saved_id, name, &nonce[..], ciphertext])
        }).await?;
        (affected == 1)
            .then_some(id)
            .ok_or_else(|| invalid("Remove a saved connection before adding more (limit 100)."))
    }
    async fn update(&self, id: &str, name: &str, input: &ConnectionInput) -> AppResult<()> {
        let (nonce, ciphertext) = self.encrypted(id, input)?;
        let id = id.to_owned();
        let name = name.to_owned();
        let affected = self
            .with_connection(move |connection| {
                connection.execute(
                    "UPDATE saved_connections SET name = ?, nonce = ?, ciphertext = ? WHERE id = ?",
                    params![name, nonce, ciphertext, id],
                )
            })
            .await?;
        if affected != 1 {
            return Err(AppError(ErrorKind::NotFound, "Saved connection not found."));
        }
        Ok(())
    }
    async fn load(&self, id: &str) -> AppResult<ConnectionInput> {
        let saved_id = id.to_owned();
        let (nonce, ciphertext): (Vec<u8>, Vec<u8>) = self
            .with_connection(move |connection| {
                connection
                    .query_row(
                        "SELECT nonce, ciphertext FROM saved_connections WHERE id = ?",
                        [saved_id],
                        |row| Ok((row.get(0)?, row.get(1)?)),
                    )
                    .optional()
            })
            .await?
            .ok_or(AppError(ErrorKind::NotFound, "Saved connection not found."))?;
        let nonce: [u8; 12] = nonce.try_into().map_err(|_| unavailable())?;
        let plaintext = self
            .cipher
            .decrypt(
                &Nonce::from(nonce),
                Payload {
                    msg: &ciphertext,
                    aad: id.as_bytes(),
                },
            )
            .map_err(|_| unavailable())?;
        serde_json::from_slice(&plaintext).map_err(|_| unavailable())
    }
    async fn delete(&self, id: &str) -> AppResult<()> {
        let id = id.to_owned();
        self.with_connection(move |connection| {
            connection.execute("DELETE FROM saved_connections WHERE id = ?", [id])
        })
        .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn encrypted_roundtrip_tampering_and_delete() {
        let vault = SqliteVault::open(":memory:", &[7; 32]).await.unwrap();
        let input: ConnectionInput = serde_json::from_value(serde_json::json!({"provider":"postgres","host":"private-host","port":5432,"database":"","allDatabases":true,"username":"alice","password":"secret-password","ssl":true})).unwrap();
        vault.save("Work", &input).await.unwrap();
        let saved = vault.list().await.unwrap();
        assert_eq!(saved[0].name, "Work");
        let id = &saved[0].id;
        assert_eq!(vault.load(id).await.unwrap().password, "secret-password");
        let bytes: Vec<u8> = vault
            .with_connection(|connection| {
                connection.query_row("SELECT ciphertext FROM saved_connections", [], |row| {
                    row.get(0)
                })
            })
            .await
            .unwrap();
        assert!(!bytes.windows(15).any(|w| w == b"secret-password"));
        vault
            .with_connection(|connection| {
                connection.execute("UPDATE saved_connections SET id = 'tampered'", [])
            })
            .await
            .unwrap();
        assert!(vault.load("tampered").await.is_err());
        vault.delete("tampered").await.unwrap();
        assert!(vault.list().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn persists_across_restart_and_rejects_wrong_key() {
        let path = std::env::temp_dir().join(format!("webdb-test-{}.sqlite", uuid::Uuid::new_v4()));
        let path_str = path.to_str().unwrap();
        let vault = SqliteVault::open(path_str, &[3; 32]).await.unwrap();
        let input: ConnectionInput = serde_json::from_value(serde_json::json!({"provider":"postgres","host":"host","port":5432,"database":"app","username":"user","password":"test-secret","ssl":true})).unwrap();
        vault.save("Private", &input).await.unwrap();
        let id = vault.list().await.unwrap().remove(0).id;
        drop(vault);
        let bytes = std::fs::read(&path).unwrap();
        assert!(!bytes.windows(11).any(|w| w == b"test-secret"));
        let wrong = SqliteVault::open(path_str, &[4; 32]).await.unwrap();
        assert!(wrong.load(&id).await.is_err());
        drop(wrong);
        let restored = SqliteVault::open(path_str, &[3; 32]).await.unwrap();
        assert_eq!(restored.load(&id).await.unwrap().password, "test-secret");
        restored.delete(&id).await.unwrap();
        drop(restored);
        std::fs::remove_file(path).unwrap();
    }

    #[tokio::test]
    async fn opens_existing_vault_format_and_preserves_legacy_ciphertext() {
        let path =
            std::env::temp_dir().join(format!("webdb-legacy-{}.sqlite", uuid::Uuid::new_v4()));
        // Synthetic AES-256-GCM fixture: key [3; 32], nonce [9; 12], AAD "legacy-entry".
        // Generated independently using Node's crypto API, including the 16-byte tag.
        let ciphertext: Vec<u8> = vec![
            127, 42, 29, 169, 225, 11, 37, 13, 239, 124, 74, 105, 133, 197, 217, 45, 224, 217, 8,
            229, 209, 222, 119, 12, 226, 37, 29, 94, 23, 240, 228, 178, 84, 235, 100, 73, 187, 12,
            55, 119, 30, 214, 132, 112, 218, 228, 142, 12, 208, 62, 231, 222, 111, 224, 227, 226,
            12, 201, 25, 99, 254, 103, 197, 187, 189, 135, 254, 32, 20, 217, 112, 114, 180, 60, 91,
            120, 249, 198, 230, 65, 191, 59, 225, 209, 23, 218, 108, 126, 91, 205, 79, 242, 215,
            200, 59, 10, 227, 26, 73, 237, 181, 145, 155, 182, 250, 127, 56, 109, 149, 123, 103,
            245, 204, 212, 162, 233, 138, 211, 190, 161, 92, 23, 170, 87, 239, 138, 106, 234, 255,
            107, 195, 126, 201, 236, 34, 193, 194, 58,
        ];
        let connection = Connection::open(&path).unwrap();
        connection.execute_batch("CREATE TABLE saved_connections (id TEXT PRIMARY KEY, name TEXT NOT NULL, nonce BLOB NOT NULL, ciphertext BLOB NOT NULL)").unwrap();
        connection
            .execute(
                "INSERT INTO saved_connections VALUES (?, ?, ?, ?)",
                params!["legacy-entry", "Legacy", &[9u8; 12][..], ciphertext],
            )
            .unwrap();
        drop(connection);

        let vault = SqliteVault::open(path.to_str().unwrap(), &[3; 32])
            .await
            .unwrap();
        assert_eq!(vault.list().await.unwrap()[0].name, "Legacy");
        assert_eq!(
            vault.load("legacy-entry").await.unwrap().password,
            "legacy-secret"
        );
        drop(vault);
        std::fs::remove_file(path).unwrap();
    }

    #[tokio::test]
    async fn saved_connection_limit_remains_atomic_under_concurrent_saves() {
        let vault = SqliteVault::open(":memory:", &[7; 32]).await.unwrap();
        let input: ConnectionInput = serde_json::from_value(serde_json::json!({"provider":"postgres","host":"host","port":5432,"database":"app","username":"user","password":"test-secret","ssl":true})).unwrap();
        for _ in 0..99 {
            vault.save("Saved", &input).await.unwrap();
        }
        let (first, second) =
            tokio::join!(vault.save("First", &input), vault.save("Second", &input));
        assert_ne!(first.is_ok(), second.is_ok());
        assert_eq!(vault.list().await.unwrap().len(), 100);
    }
}
