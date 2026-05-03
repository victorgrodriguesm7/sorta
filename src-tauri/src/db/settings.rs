//! Key/value settings table queries.

use sqlx::SqlitePool;

use crate::error::{AppError, AppResult};

pub const KEY_MOVIES_LABEL: &str = "movies_folder_label";
pub const KEY_SERIES_LABEL: &str = "series_folder_label";

/// Get a setting by key.
pub async fn get_setting(pool: &SqlitePool, key: &str) -> AppResult<Option<String>> {
    let row: Option<(String,)> = sqlx::query_as("SELECT value FROM settings WHERE key = ?")
        .bind(key)
        .fetch_optional(pool)
        .await
        .map_err(|e| AppError::Other(format!("get_setting: {e}")))?;
    Ok(row.map(|r| r.0))
}

/// Get a setting with a default if missing.
pub async fn get_setting_or(pool: &SqlitePool, key: &str, default: &str) -> AppResult<String> {
    Ok(get_setting(pool, key).await?.unwrap_or_else(|| default.to_string()))
}

/// Set a setting (insert or update).
pub async fn set_setting(pool: &SqlitePool, key: &str, value: &str) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO settings(key, value) VALUES (?, ?) \
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
    )
    .bind(key)
    .bind(value)
    .execute(pool)
    .await
    .map_err(|e| AppError::Other(format!("set_setting: {e}")))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open;
    use tempfile::TempDir;

    async fn fresh() -> (TempDir, SqlitePool) {
        let tmp = TempDir::new().unwrap();
        let pool = open(&tmp.path().join("sorta.db")).await.unwrap();
        (tmp, pool)
    }

    #[tokio::test]
    async fn get_set_roundtrip() {
        let (_tmp, pool) = fresh().await;
        assert_eq!(get_setting(&pool, "missing").await.unwrap(), None);
        set_setting(&pool, "k", "v1").await.unwrap();
        assert_eq!(get_setting(&pool, "k").await.unwrap().as_deref(), Some("v1"));
        set_setting(&pool, "k", "v2").await.unwrap();
        assert_eq!(get_setting(&pool, "k").await.unwrap().as_deref(), Some("v2"));
    }

    #[tokio::test]
    async fn defaults_seeded() {
        let (_tmp, pool) = fresh().await;
        assert_eq!(
            get_setting(&pool, KEY_MOVIES_LABEL).await.unwrap().as_deref(),
            Some("Movies")
        );
        assert_eq!(
            get_setting(&pool, KEY_SERIES_LABEL).await.unwrap().as_deref(),
            Some("Series")
        );
    }
}
