//! `<HD>/manifest.json` — a small companion file written by the
//! desktop alongside `sorta.db`. External readers (e.g. the planned
//! TV-side Android client) consult it for a quick health/version
//! check before opening the SQLite database.
//!
//! Schema is intentionally minimal and additive — readers should
//! tolerate unknown extra fields and never refuse to open a DB
//! whose `schema_version` is *less than or equal to* the version
//! they were built against.

use std::path::Path;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::db::CURRENT_SCHEMA_VERSION;
use crate::error::{AppError, AppResult};

const MANIFEST_FILENAME: &str = "manifest.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    /// Mirrors `settings.schema_version` for callers that don't want
    /// to open SQLite first.
    pub schema_version: u32,
    /// Sorta desktop version that wrote this manifest (CARGO_PKG_VERSION).
    pub app_version: String,
    /// ISO-8601 UTC, second precision: "2026-05-15T12:34:56Z".
    pub generated_at: String,
    pub counts: ManifestCounts,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ManifestCounts {
    pub media_total: i64,
    pub movies: i64,
    pub series: i64,
}

/// Build a manifest snapshot from the live DB (no I/O to disk yet).
pub async fn build(pool: &SqlitePool) -> AppResult<Manifest> {
    let movies: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM media WHERE media_type = 'movie'")
            .fetch_one(pool)
            .await
            .map_err(|e| AppError::Other(format!("count movies: {e}")))?;
    let series: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM media WHERE media_type = 'tv'")
        .fetch_one(pool)
        .await
        .map_err(|e| AppError::Other(format!("count series: {e}")))?;

    Ok(Manifest {
        schema_version: CURRENT_SCHEMA_VERSION,
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        generated_at: Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        counts: ManifestCounts {
            media_total: movies.0 + series.0,
            movies: movies.0,
            series: series.0,
        },
    })
}

/// Build + write `<hd_root>/manifest.json` atomically (write to a
/// `.tmp` sibling, rename in place). Best-effort: the caller is the
/// command finishing some mutation, and a failed manifest write
/// should not roll the mutation back.
pub async fn write(hd_root: &Path, pool: &SqlitePool) -> AppResult<()> {
    let manifest = build(pool).await?;
    let json = serde_json::to_string_pretty(&manifest)
        .map_err(|e| AppError::Other(format!("manifest encode: {e}")))?;
    let dest = hd_root.join(MANIFEST_FILENAME);
    let tmp = hd_root.join(format!("{MANIFEST_FILENAME}.tmp"));
    std::fs::write(&tmp, json).map_err(AppError::from)?;
    std::fs::rename(&tmp, &dest).map_err(AppError::from)?;
    Ok(())
}

/// Best-effort wrapper used inside command handlers. Logs but never
/// errors out — manifest staleness is a UX issue, not a correctness
/// issue.
pub async fn write_best_effort(hd_root: &Path, pool: &SqlitePool) {
    if let Err(e) = write(hd_root, pool).await {
        tracing::warn!("failed to write manifest: {e:?}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open;
    use tempfile::TempDir;

    #[tokio::test]
    async fn build_reflects_db_state() {
        let tmp = TempDir::new().unwrap();
        let pool = open(&tmp.path().join("sorta.db")).await.unwrap();
        let m = build(&pool).await.unwrap();
        assert_eq!(m.schema_version, CURRENT_SCHEMA_VERSION);
        assert_eq!(m.counts.media_total, 0);
        // Insert one movie + one series row.
        for (typ, id) in [("movie", 1), ("tv", 2)] {
            sqlx::query(
                "INSERT INTO media \
                 (tmdb_id, media_type, title, folder_path) \
                 VALUES (?, ?, ?, ?)",
            )
            .bind(id)
            .bind(typ)
            .bind(format!("title-{id}"))
            .bind(format!("folder-{id}"))
            .execute(&pool)
            .await
            .unwrap();
        }
        let m = build(&pool).await.unwrap();
        assert_eq!(m.counts.movies, 1);
        assert_eq!(m.counts.series, 1);
        assert_eq!(m.counts.media_total, 2);
    }

    #[tokio::test]
    async fn write_creates_a_parseable_file() {
        let tmp = TempDir::new().unwrap();
        let pool = open(&tmp.path().join("sorta.db")).await.unwrap();
        write(tmp.path(), &pool).await.unwrap();
        let bytes = std::fs::read(tmp.path().join(MANIFEST_FILENAME)).unwrap();
        let parsed: Manifest = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(parsed.schema_version, CURRENT_SCHEMA_VERSION);
        // ISO 8601 sanity check — must end in Z and be 20 chars.
        assert!(parsed.generated_at.ends_with('Z'));
        assert_eq!(parsed.generated_at.len(), 20);
    }
}
