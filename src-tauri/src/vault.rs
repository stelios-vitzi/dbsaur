use std::path::{Path, PathBuf};

pub fn paths(data: &Path) -> (PathBuf, PathBuf) {
    let recovering = !data.join("vault.key").exists() && data.join("connections.sqlite").exists();
    if recovering {
        (
            data.join("vault.recovered.key"),
            data.join("connections.recovered.sqlite"),
        )
    } else {
        (data.join("vault.key"), data.join("connections.sqlite"))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn missing_key_preserves_the_original_vault_and_reuses_the_recovery_store() {
        let data = std::env::temp_dir().join(format!("dbsaur-recovery-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&data).unwrap();
        assert_eq!(super::paths(&data).0, data.join("vault.key"));
        std::fs::write(data.join("connections.sqlite"), b"original vault").unwrap();
        let recovery = super::paths(&data);
        assert_eq!(recovery.0, data.join("vault.recovered.key"));
        assert_eq!(recovery.1, data.join("connections.recovered.sqlite"));
        std::fs::write(&recovery.0, [0; 32]).unwrap();
        assert_eq!(super::paths(&data), recovery);
        assert_eq!(
            std::fs::read(data.join("connections.sqlite")).unwrap(),
            b"original vault"
        );
        std::fs::write(data.join("vault.key"), [1; 32]).unwrap();
        assert_eq!(super::paths(&data).1, data.join("connections.sqlite"));
        for file in ["vault.key", "vault.recovered.key", "connections.sqlite"] {
            std::fs::remove_file(data.join(file)).unwrap();
        }
        std::fs::remove_dir(data).unwrap();
    }
}
