//! Database layer (SQLite via sqlx).
//!
//! The DB lives at `<HD root>/sorta.db`. We use sqlx's runtime migrator
//! (NOT compile-time `query!` macros) so the DB doesn't need to exist at
//! build time — building the app shouldn't require an HD to be plugged in.

pub mod episodes;
pub mod genres;
pub mod media;
pub mod settings;

use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::path::Path;
use std::str::FromStr;

use crate::error::{AppError, AppResult};

pub use episodes::*;
pub use genres::*;
pub use media::*;
pub use settings::*;

/// Bumped whenever a new migration is added that an external reader
/// could not be expected to handle. The TV-side reader compares this
/// against the value stored in the `settings.schema_version` row and
/// refuses to open the DB if the on-disk number is newer than what
/// it knows.
pub const CURRENT_SCHEMA_VERSION: u32 = 4;

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

    // Always overwrite the schema_version row with the constant,
    // regardless of what was on disk. This keeps it accurate even if
    // the user downgraded to an older binary in between (older binary
    // would have left a higher number behind otherwise).
    set_setting(&pool, "schema_version", &CURRENT_SCHEMA_VERSION.to_string()).await?;

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
    async fn open_writes_current_schema_version() {
        let tmp = TempDir::new().unwrap();
        let pool = open(&tmp.path().join("sorta.db")).await.unwrap();
        let row: (String,) =
            sqlx::query_as("SELECT value FROM settings WHERE key = 'schema_version'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(row.0, CURRENT_SCHEMA_VERSION.to_string());
    }

    #[tokio::test]
    async fn vacuum_into_produces_a_usable_copy() {
        // The backup command shells out to `VACUUM INTO`. Lock that
        // behaviour in: starting from a fresh DB with the seeded
        // settings, the backup file should itself be a valid SQLite
        // DB containing the same rows.
        let tmp = TempDir::new().unwrap();
        let live = tmp.path().join("live.db");
        let pool = open(&live).await.unwrap();
        // Bump the schema with a row so we know data round-trips.
        sqlx::query("INSERT INTO settings(key, value) VALUES (?, ?)")
            .bind("backup_marker")
            .bind("hello")
            .execute(&pool)
            .await
            .unwrap();

        let dest = tmp.path().join("snapshot.db");
        let dest_str = dest.to_string_lossy().replace('\'', "''");
        sqlx::query(&format!("VACUUM INTO '{dest_str}'"))
            .execute(&pool)
            .await
            .unwrap();
        assert!(dest.is_file());

        // Open the snapshot and verify our marker survived.
        let restored = open(&dest).await.unwrap();
        let row: (String,) =
            sqlx::query_as("SELECT value FROM settings WHERE key = 'backup_marker'")
                .fetch_one(&restored)
                .await
                .unwrap();
        assert_eq!(row.0, "hello");
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
        assert_eq!(row.0, 4); // movies_label, series_label, season_label, schema_version
    }
}
