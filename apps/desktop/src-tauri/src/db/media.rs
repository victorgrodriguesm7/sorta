//! Media table queries.

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MediaType {
    Movie,
    Tv,
}

impl MediaType {
    pub fn as_db_str(self) -> &'static str {
        match self {
            MediaType::Movie => "movie",
            MediaType::Tv => "tv",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "movie" => Some(MediaType::Movie),
            "tv" => Some(MediaType::Tv),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct MediaRow {
    pub id: i64,
    pub tmdb_id: i64,
    pub media_type: String,
    pub title: String,
    pub original_title: Option<String>,
    pub runtime_minutes: Option<i64>,
    pub poster_path: Option<String>,
    pub poster_url: Option<String>,
    pub folder_path: String,
    /// ISO 8601 UTC timestamp (e.g. `2026-05-11T12:34:56Z`). The DB
    /// default is `strftime('now')`, so older rows backfilled by the
    /// migration share the upgrade-time timestamp.
    #[serde(default)]
    pub catalogued_at: String,
    /// User-controlled "Mark as new" flag set at cataloging time.
    /// Stored as 0/1 in SQLite.
    #[serde(default)]
    pub is_new: bool,
    /// Drive this row was loaded from. Not stored in SQLite — stamped
    /// by the command layer after a fan-out fetch so the frontend
    /// (and write-side dispatch) can route operations back to the
    /// originating pool. `None` for unit tests that bypass the
    /// command layer.
    #[sqlx(skip)]
    #[serde(default)]
    pub drive_root: Option<std::path::PathBuf>,
}

#[derive(Debug, Clone)]
pub struct NewMedia<'a> {
    pub tmdb_id: i64,
    pub media_type: MediaType,
    pub title: &'a str,
    pub original_title: Option<&'a str>,
    pub runtime_minutes: Option<i64>,
    pub poster_path: Option<&'a str>,
    pub poster_url: Option<&'a str>,
    pub folder_path: &'a str,
    /// Defaults to false. Caller passes the value of the UI checkbox.
    pub is_new: bool,
}

/// Insert a new media row, returning the row id. `catalogued_at` is
/// filled inline by SQLite's `strftime`, not as a column default —
/// `ALTER TABLE ADD COLUMN` doesn't accept non-constant defaults, so
/// the schema's literal `''` placeholder only applies to the moment
/// the column is created during migration. Keeping the timestamp on
/// the SQL side means clock drift on the JS layer (or a misbehaving
/// caller) can't poison the column with bogus values.
pub async fn insert_media(pool: &SqlitePool, m: &NewMedia<'_>) -> AppResult<i64> {
    let res = sqlx::query(
        "INSERT INTO media \
         (tmdb_id, media_type, title, original_title, runtime_minutes, poster_path, poster_url, folder_path, is_new, catalogued_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))",
    )
    .bind(m.tmdb_id)
    .bind(m.media_type.as_db_str())
    .bind(m.title)
    .bind(m.original_title)
    .bind(m.runtime_minutes)
    .bind(m.poster_path)
    .bind(m.poster_url)
    .bind(m.folder_path)
    .bind(if m.is_new { 1_i64 } else { 0 })
    .execute(pool)
    .await
    .map_err(|e| AppError::Other(format!("insert_media: {e}")))?;
    Ok(res.last_insert_rowid())
}

/// Toggle the `is_new` flag on an existing row. Used by the UI's
/// "Mark as new" / "Clear new" affordance on the right panel.
pub async fn set_is_new(pool: &SqlitePool, id: i64, is_new: bool) -> AppResult<()> {
    sqlx::query("UPDATE media SET is_new = ? WHERE id = ?")
        .bind(if is_new { 1_i64 } else { 0 })
        .bind(id)
        .execute(pool)
        .await
        .map_err(|e| AppError::Other(format!("set_is_new: {e}")))?;
    Ok(())
}

/// Fetch by `(tmdb_id, media_type)`.
pub async fn find_by_tmdb_id(
    pool: &SqlitePool,
    tmdb_id: i64,
    media_type: MediaType,
) -> AppResult<Option<MediaRow>> {
    sqlx::query_as::<_, MediaRow>(
        "SELECT * FROM media WHERE tmdb_id = ? AND media_type = ?",
    )
    .bind(tmdb_id)
    .bind(media_type.as_db_str())
    .fetch_optional(pool)
    .await
    .map_err(|e| AppError::Other(format!("find_by_tmdb_id: {e}")))
}

/// Fetch by primary key.
pub async fn find_by_id(pool: &SqlitePool, id: i64) -> AppResult<Option<MediaRow>> {
    sqlx::query_as::<_, MediaRow>("SELECT * FROM media WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(|e| AppError::Other(format!("find_by_id: {e}")))
}

/// Update a row's `folder_path`. Used when the user renames or relinks.
pub async fn update_folder_path(pool: &SqlitePool, id: i64, new_folder: &str) -> AppResult<()> {
    sqlx::query("UPDATE media SET folder_path = ? WHERE id = ?")
        .bind(new_folder)
        .bind(id)
        .execute(pool)
        .await
        .map_err(|e| AppError::Other(format!("update_folder_path: {e}")))?;
    Ok(())
}

/// Delete a media row (cascades into media_genres).
pub async fn delete_media(pool: &SqlitePool, id: i64) -> AppResult<()> {
    sqlx::query("DELETE FROM media WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await
        .map_err(|e| AppError::Other(format!("delete_media: {e}")))?;
    Ok(())
}

/// Return the absolute folder path on disk that holds a media row's
/// content. Used by callers that need to walk the folder (compression,
/// size totals, etc.).
pub fn media_folder(hd_root: &std::path::Path, row: &MediaRow) -> std::path::PathBuf {
    hd_root.join(&row.folder_path)
}

/// List all media of a given type.
pub async fn list_by_type(pool: &SqlitePool, media_type: MediaType) -> AppResult<Vec<MediaRow>> {
    sqlx::query_as::<_, MediaRow>(
        "SELECT * FROM media WHERE media_type = ? ORDER BY title COLLATE NOCASE",
    )
    .bind(media_type.as_db_str())
    .fetch_all(pool)
    .await
    .map_err(|e| AppError::Other(format!("list_by_type: {e}")))
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
    async fn insert_then_find_by_tmdb_id() {
        let (_tmp, pool) = fresh().await;
        let id = insert_media(
            &pool,
            &NewMedia {
                tmdb_id: 27205,
                media_type: MediaType::Movie,
                title: "Inception",
                original_title: Some("Inception"),
                runtime_minutes: Some(148),
                poster_path: Some("poster/27205.jpg"),
                poster_url: Some("https://img.tmdb.org/x.jpg"),
                folder_path: "Movies/Action/Inception [tmdb-27205]",
                is_new: false,
            },
        )
        .await
        .unwrap();
        assert!(id > 0);

        let row = find_by_tmdb_id(&pool, 27205, MediaType::Movie)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(row.title, "Inception");
        assert_eq!(row.runtime_minutes, Some(148));
    }

    #[tokio::test]
    async fn unique_tmdb_per_media_type() {
        let (_tmp, pool) = fresh().await;
        let m = NewMedia {
            tmdb_id: 1,
            media_type: MediaType::Movie,
            title: "A",
            original_title: None,
            runtime_minutes: None,
            poster_path: None,
            poster_url: None,
            folder_path: "Movies/A [tmdb-1]",
            is_new: false,
        };
        insert_media(&pool, &m).await.unwrap();
        let dup = insert_media(&pool, &m).await;
        assert!(dup.is_err(), "duplicate (tmdb_id, media_type) should fail");
    }

    #[tokio::test]
    async fn insert_stamps_catalogued_at_iso_8601() {
        // Locks in two things at once:
        //   1. `insert_media` doesn't rely on the column default
        //      (which is "" because SQLite ALTER TABLE forbids
        //      non-constant defaults — see migration 0004).
        //   2. The format matches what the reader expects: ISO 8601
        //      UTC with a trailing Z, 20 characters total.
        let (_tmp, pool) = fresh().await;
        let id = insert_media(
            &pool,
            &NewMedia {
                tmdb_id: 9001,
                media_type: MediaType::Movie,
                title: "Stamp Test",
                original_title: None,
                runtime_minutes: None,
                poster_path: None,
                poster_url: None,
                folder_path: "Movies/x [tmdb-9001]",
                is_new: false,
            },
        )
        .await
        .unwrap();

        let row = find_by_id(&pool, id).await.unwrap().unwrap();
        assert!(!row.catalogued_at.is_empty(), "catalogued_at must be set");
        assert!(
            row.catalogued_at.ends_with('Z'),
            "expected ISO 8601 UTC, got {:?}",
            row.catalogued_at,
        );
        assert_eq!(
            row.catalogued_at.len(),
            20,
            "expected 20-char ISO 8601, got {:?}",
            row.catalogued_at,
        );
    }

    #[tokio::test]
    async fn update_folder_path_works() {
        let (_tmp, pool) = fresh().await;
        let id = insert_media(
            &pool,
            &NewMedia {
                tmdb_id: 1,
                media_type: MediaType::Movie,
                title: "A",
                original_title: None,
                runtime_minutes: None,
                poster_path: None,
                poster_url: None,
                folder_path: "old/path",
                is_new: false,
            },
        )
        .await
        .unwrap();
        update_folder_path(&pool, id, "new/path").await.unwrap();
        let row = find_by_id(&pool, id).await.unwrap().unwrap();
        assert_eq!(row.folder_path, "new/path");
    }
}
