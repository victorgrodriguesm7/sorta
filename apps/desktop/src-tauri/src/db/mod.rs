//! Database layer (SQLite via sqlx).
//!
//! The DB lives at `<HD root>/sorta.db`. We use sqlx's runtime migrator
//! (NOT compile-time `query!` macros) so the DB doesn't need to exist at
//! build time — building the app shouldn't require an HD to be plugged in.

pub mod episodes;
pub mod genres;
pub mod media;
pub mod settings;

use sqlx::migrate::{MigrateError, Migrator};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::path::Path;
use std::str::FromStr;

use crate::error::{AppError, AppResult};

/// The embedded migration set. Pulled out of `open` so the diagnostic
/// path can iterate over it without re-invoking the macro.
static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

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

    run_migrations(&pool).await?;

    // Always overwrite the schema_version row with the constant,
    // regardless of what was on disk. This keeps it accurate even if
    // the user downgraded to an older binary in between (older binary
    // would have left a higher number behind otherwise).
    set_setting(&pool, "schema_version", &CURRENT_SCHEMA_VERSION.to_string()).await?;

    Ok(pool)
}

/// Run `MIGRATOR.run`, transparently auto-healing the
/// `_sqlx_migrations` checksum table when it diverges from the
/// embedded migration set.
///
/// **Why this exists.** sqlx hashes the *raw bytes* of every applied
/// migration and refuses to proceed if a stored hash no longer
/// matches. We've shipped these files with `text=auto` line-ending
/// behaviour, which means a Windows checkout converts them to CRLF
/// while a Linux/macOS checkout keeps them at LF; whichever shape was
/// first written to the user's drive is the only shape that DB will
/// ever agree with. The fix is twofold: pin the files to LF via
/// `.gitattributes`, and — on every boot — overwrite stored checksums
/// with the current embedded ones for *already-applied* versions.
///
/// **Why the auto-heal is safe.** We never edit a published
/// migration's SQL semantics — that's a policy enforced by review.
/// So the only legitimate reason a stored checksum diverges is line
/// endings or whitespace, neither of which changes what got applied.
/// Re-stamping the checksum brings the DB back in line with whatever
/// the current binary expects without re-running any DDL.
///
/// The diagnostic dump is unconditional: every mismatched row gets
/// logged (version, description, both checksums in hex, SQL length)
/// before we touch it, so a postmortem reader can confirm by hand
/// that only cosmetic bytes shifted.
async fn run_migrations(pool: &SqlitePool) -> AppResult<()> {
    // First attempt — happy path.
    match MIGRATOR.run(pool).await {
        Ok(()) => return Ok(()),
        Err(MigrateError::VersionMismatch(v)) => {
            tracing::warn!(
                "migration checksum mismatch on version {v}; dumping diagnostic table and re-stamping stored checksums",
            );
            heal_checksum_drift(pool).await?;
        }
        Err(e) => return Err(AppError::Other(format!("migrate: {e}"))),
    }

    // Retry once. Any *second* mismatch is a real schema discrepancy
    // (not line-ending drift) and must surface as an error.
    MIGRATOR
        .run(pool)
        .await
        .map_err(|e| AppError::Other(format!("migrate (after heal): {e}")))
}

/// Compare every embedded migration's checksum against what
/// `_sqlx_migrations` stores; log the diff; UPDATE the row so the
/// next `MIGRATOR.run` accepts it.
async fn heal_checksum_drift(pool: &SqlitePool) -> AppResult<()> {
    let stored: Vec<(i64, Vec<u8>, String)> = sqlx::query_as(
        "SELECT version, checksum, description FROM _sqlx_migrations ORDER BY version",
    )
    .fetch_all(pool)
    .await
    .map_err(|e| AppError::Other(format!("read _sqlx_migrations: {e}")))?;

    let stored_map: std::collections::HashMap<i64, &Vec<u8>> =
        stored.iter().map(|(v, c, _)| (*v, c)).collect();

    for m in MIGRATOR.iter() {
        let Some(stored_sum) = stored_map.get(&m.version) else {
            continue; // Not applied yet; sqlx will apply it on retry.
        };
        let embedded_sum: &[u8] = m.checksum.as_ref();
        if stored_sum.as_slice() == embedded_sum {
            continue;
        }
        tracing::warn!(
            "  v{} \"{}\": stored {} != embedded {} ({} bytes of SQL)",
            m.version,
            m.description,
            hex(stored_sum),
            hex(embedded_sum),
            m.sql.len(),
        );
        sqlx::query("UPDATE _sqlx_migrations SET checksum = ? WHERE version = ?")
            .bind(embedded_sum)
            .bind(m.version)
            .execute(pool)
            .await
            .map_err(|e| AppError::Other(format!("update checksum v{}: {e}", m.version)))?;
    }
    Ok(())
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        use std::fmt::Write as _;
        let _ = write!(out, "{b:02x}");
    }
    out
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
    async fn open_heals_checksum_drift_in_sqlx_migrations() {
        // Simulates the symptom users hit when the same migration files
        // got hashed once with LF endings (on the build that first wrote
        // the drive) and once with CRLF (on a later build). The bytes
        // differ but the SQL semantics don't, so the second `open` call
        // should rewrite the stored checksum rather than blow up.
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join("sorta.db");
        let pool = open(&db_path).await.unwrap();

        // Corrupt the stored checksum for migration 1.
        sqlx::query("UPDATE _sqlx_migrations SET checksum = ? WHERE version = 1")
            .bind(b"\xde\xad\xbe\xef".to_vec())
            .execute(&pool)
            .await
            .unwrap();
        drop(pool);

        // Without heal logic this would error out with
        // `MigrateError::VersionMismatch(1)`. With heal it succeeds.
        let pool = open(&db_path).await.expect("open should heal");

        // And the checksum is back in sync with the embedded one.
        let stored: (Vec<u8>,) =
            sqlx::query_as("SELECT checksum FROM _sqlx_migrations WHERE version = 1")
                .fetch_one(&pool)
                .await
                .unwrap();
        let embedded = MIGRATOR
            .iter()
            .find(|m| m.version == 1)
            .expect("migration 1 exists")
            .checksum
            .as_ref()
            .to_vec();
        assert_eq!(stored.0, embedded);
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
