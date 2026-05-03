//! Database layer (SQLite via sqlx).
//!
//! The DB lives at `<HD root>/sorta.db`. We use sqlx's runtime migrator
//! (NOT compile-time `query!` macros) so the DB doesn't need to exist at
//! build time — building the app shouldn't require an HD to be plugged in.

pub mod genres;
pub mod media;
pub mod settings;

use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::path::Path;
use std::str::FromStr;

use crate::error::{AppError, AppResult};

pub use genres::*;
pub use media::*;
pub use settings::*;

/// Open (or create) the SQLite DB at the given path and run migrations.
pub async fn open(db_path: &Path) -> AppResult<SqlitePool> {
    let db_path_str = db_path
        .to_str()
        .ok_or_else(|| AppError::InvalidPath(format!("{db_path:?} is not valid UTF-8")))?;

    // sqlite::file:?mode=rwc creates the file if missing.
    let connect_url = format!("sqlite://{db_path_str}?mode=rwc");
    let opts = SqliteConnectOptions::from_str(&connect_url)
        .map_err(|e| AppError::Other(format!("invalid sqlite url: {e}")))?
        .create_if_missing(true)
        .foreign_keys(true);

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(opts)
        .await
        .map_err(|e| AppError::Other(format!("sqlite connect: {e}")))?;

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .map_err(|e| AppError::Other(format!("migrate: {e}")))?;

    Ok(pool)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn open_creates_db_and_runs_migrations() {
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join("sorta.db");
        let pool = open(&db_path).await.expect("open");

        // Check that default settings were seeded.
        let row: (String,) = sqlx::query_as("SELECT value FROM settings WHERE key = ?")
            .bind("movies_folder_label")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(row.0, "Movies");

        let row: (String,) = sqlx::query_as("SELECT value FROM settings WHERE key = ?")
            .bind("series_folder_label")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(row.0, "Series");

        assert!(db_path.exists());
    }

    #[tokio::test]
    async fn open_is_idempotent() {
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join("sorta.db");
        let _pool = open(&db_path).await.unwrap();
        // Re-open shouldn't fail or re-seed twice.
        let pool = open(&db_path).await.unwrap();
        let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM settings")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(row.0, 2);
    }
}
