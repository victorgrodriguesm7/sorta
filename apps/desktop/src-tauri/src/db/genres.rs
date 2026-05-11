//! Genres + media_genres table queries.

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::db::media::MediaType;
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct GenreRow {
    pub id: i64,
    pub media_type: String,
    pub canonical_name: String,
    pub translated_name: Option<String>,
}

impl GenreRow {
    /// Display name: translated if set, else canonical.
    pub fn display_name(&self) -> &str {
        self.translated_name.as_deref().unwrap_or(&self.canonical_name)
    }
}

/// Insert or refresh a genre's canonical name. Preserves any existing
/// `translated_name` (the user's customization).
pub async fn upsert_genre(
    pool: &SqlitePool,
    id: i64,
    media_type: MediaType,
    canonical_name: &str,
) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO genres(id, media_type, canonical_name) VALUES (?, ?, ?) \
         ON CONFLICT(id, media_type) DO UPDATE SET canonical_name = excluded.canonical_name",
    )
    .bind(id)
    .bind(media_type.as_db_str())
    .bind(canonical_name)
    .execute(pool)
    .await
    .map_err(|e| AppError::Other(format!("upsert_genre: {e}")))?;
    Ok(())
}

/// Set or clear a genre's translated name.
pub async fn set_genre_translation(
    pool: &SqlitePool,
    id: i64,
    media_type: MediaType,
    translated: Option<&str>,
) -> AppResult<()> {
    sqlx::query(
        "UPDATE genres SET translated_name = ? WHERE id = ? AND media_type = ?",
    )
    .bind(translated)
    .bind(id)
    .bind(media_type.as_db_str())
    .execute(pool)
    .await
    .map_err(|e| AppError::Other(format!("set_genre_translation: {e}")))?;
    Ok(())
}

/// List all genres for a given media type, ordered by display name.
pub async fn list_genres(pool: &SqlitePool, media_type: MediaType) -> AppResult<Vec<GenreRow>> {
    sqlx::query_as::<_, GenreRow>(
        "SELECT * FROM genres WHERE media_type = ? \
         ORDER BY COALESCE(translated_name, canonical_name) COLLATE NOCASE",
    )
    .bind(media_type.as_db_str())
    .fetch_all(pool)
    .await
    .map_err(|e| AppError::Other(format!("list_genres: {e}")))
}

/// Detect which genres collide on display name (i.e. should be visually
/// merged in the UI and merged on disk). Returns a list of groups (each
/// group is a Vec of GenreRow sharing the same display name).
pub async fn list_merged_groups(
    pool: &SqlitePool,
    media_type: MediaType,
) -> AppResult<Vec<Vec<GenreRow>>> {
    let all = list_genres(pool, media_type).await?;
    let mut groups: std::collections::BTreeMap<String, Vec<GenreRow>> = Default::default();
    for g in all {
        groups
            .entry(g.display_name().to_lowercase())
            .or_default()
            .push(g);
    }
    Ok(groups.into_values().collect())
}

/// Replace a media row's genres. Pass `(genre_id, is_primary)` pairs;
/// exactly one should be marked primary.
pub async fn set_media_genres(
    pool: &SqlitePool,
    media_id: i64,
    media_type: MediaType,
    genres: &[(i64, bool)],
) -> AppResult<()> {
    let mut tx = pool
        .begin()
        .await
        .map_err(|e| AppError::Other(format!("begin: {e}")))?;

    sqlx::query("DELETE FROM media_genres WHERE media_id = ?")
        .bind(media_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::Other(format!("delete media_genres: {e}")))?;

    for (gid, is_primary) in genres {
        sqlx::query(
            "INSERT INTO media_genres(media_id, genre_id, media_type, is_primary) \
             VALUES (?, ?, ?, ?)",
        )
        .bind(media_id)
        .bind(*gid)
        .bind(media_type.as_db_str())
        .bind(if *is_primary { 1 } else { 0 })
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::Other(format!("insert media_genres: {e}")))?;
    }

    tx.commit()
        .await
        .map_err(|e| AppError::Other(format!("commit: {e}")))?;
    Ok(())
}

/// Get the primary genre for a media row (if any).
pub async fn primary_genre_for(
    pool: &SqlitePool,
    media_id: i64,
) -> AppResult<Option<GenreRow>> {
    sqlx::query_as::<_, GenreRow>(
        "SELECT g.* FROM genres g \
         JOIN media_genres mg ON mg.genre_id = g.id AND mg.media_type = g.media_type \
         WHERE mg.media_id = ? AND mg.is_primary = 1 \
         LIMIT 1",
    )
    .bind(media_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| AppError::Other(format!("primary_genre_for: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::media::{insert_media, MediaType, NewMedia};
    use crate::db::open;
    use tempfile::TempDir;

    async fn fresh() -> (TempDir, SqlitePool) {
        let tmp = TempDir::new().unwrap();
        let pool = open(&tmp.path().join("sorta.db")).await.unwrap();
        (tmp, pool)
    }

    #[tokio::test]
    async fn upsert_then_translate_genre() {
        let (_tmp, pool) = fresh().await;
        upsert_genre(&pool, 28, MediaType::Movie, "Action").await.unwrap();
        set_genre_translation(&pool, 28, MediaType::Movie, Some("Ação"))
            .await
            .unwrap();

        let list = list_genres(&pool, MediaType::Movie).await.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].display_name(), "Ação");
    }

    #[tokio::test]
    async fn upsert_preserves_translation() {
        let (_tmp, pool) = fresh().await;
        upsert_genre(&pool, 28, MediaType::Movie, "Action").await.unwrap();
        set_genre_translation(&pool, 28, MediaType::Movie, Some("Ação"))
            .await
            .unwrap();
        // Refreshing the canonical name (e.g. on TMDB sync) keeps translation.
        upsert_genre(&pool, 28, MediaType::Movie, "Action").await.unwrap();
        let list = list_genres(&pool, MediaType::Movie).await.unwrap();
        assert_eq!(list[0].translated_name.as_deref(), Some("Ação"));
    }

    #[tokio::test]
    async fn merged_groups_detected_when_translations_collide() {
        let (_tmp, pool) = fresh().await;
        upsert_genre(&pool, 28, MediaType::Movie, "Action").await.unwrap();
        upsert_genre(&pool, 12, MediaType::Movie, "Adventure").await.unwrap();
        upsert_genre(&pool, 10751, MediaType::Movie, "Family").await.unwrap();
        set_genre_translation(&pool, 28, MediaType::Movie, Some("Aventura"))
            .await
            .unwrap();
        set_genre_translation(&pool, 12, MediaType::Movie, Some("Aventura"))
            .await
            .unwrap();

        let groups = list_merged_groups(&pool, MediaType::Movie).await.unwrap();
        // Two groups: ["Aventura" (28+12), "Family"]
        assert_eq!(groups.len(), 2);
        let merged = groups
            .iter()
            .find(|g| g.len() == 2)
            .expect("merged group exists");
        let ids: Vec<i64> = merged.iter().map(|g| g.id).collect();
        assert!(ids.contains(&28));
        assert!(ids.contains(&12));
    }

    #[tokio::test]
    async fn set_media_genres_fails_when_secondary_genre_not_upserted() {
        // Regression: previously link_media only upserted the PRIMARY movie
        // genre, then called set_media_genres with both primary + secondary
        // ids. The secondary FK violated `(genre_id, media_type)
        // REFERENCES genres(id, media_type)`, the whole transaction rolled
        // back, and the movie ended up invisible in every genre bucket.
        let (_tmp, pool) = fresh().await;
        upsert_genre(&pool, 28, MediaType::Movie, "Action").await.unwrap();
        // NOTE: 12 (Adventure) intentionally NOT upserted.
        let media_id = insert_media(
            &pool,
            &NewMedia {
                tmdb_id: 1,
                media_type: MediaType::Movie,
                title: "T",
                original_title: None,
                runtime_minutes: None,
                poster_path: None,
                poster_url: None,
                folder_path: "Movies/Action/T [tmdb-1]",
                is_new: false,
            },
        )
        .await
        .unwrap();

        let res =
            set_media_genres(&pool, media_id, MediaType::Movie, &[(28, true), (12, false)]).await;
        assert!(res.is_err(), "FK on missing genre should fail");

        // And, crucially, the failed transaction should leave NO rows
        // (the bug symptom: the movie has no genre links at all).
        let count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM media_genres WHERE media_id = ?")
                .bind(media_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(count.0, 0);
    }

    #[tokio::test]
    async fn set_media_genres_succeeds_when_all_genres_upserted_first() {
        // Demonstrates the fix: upsert ALL referenced genres before calling
        // set_media_genres, even the secondary ones.
        let (_tmp, pool) = fresh().await;
        upsert_genre(&pool, 28, MediaType::Movie, "Action").await.unwrap();
        upsert_genre(&pool, 12, MediaType::Movie, "Adventure").await.unwrap();
        let media_id = insert_media(
            &pool,
            &NewMedia {
                tmdb_id: 1,
                media_type: MediaType::Movie,
                title: "T",
                original_title: None,
                runtime_minutes: None,
                poster_path: None,
                poster_url: None,
                folder_path: "Movies/Action/T [tmdb-1]",
                is_new: false,
            },
        )
        .await
        .unwrap();
        set_media_genres(&pool, media_id, MediaType::Movie, &[(28, true), (12, false)])
            .await
            .unwrap();
        let primary = primary_genre_for(&pool, media_id).await.unwrap().unwrap();
        assert_eq!(primary.id, 28);
        let count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM media_genres WHERE media_id = ?")
                .bind(media_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(count.0, 2);
    }

    #[tokio::test]
    async fn set_media_genres_replaces_and_marks_primary() {
        let (_tmp, pool) = fresh().await;
        upsert_genre(&pool, 28, MediaType::Movie, "Action").await.unwrap();
        upsert_genre(&pool, 12, MediaType::Movie, "Adventure").await.unwrap();
        let media_id = insert_media(
            &pool,
            &NewMedia {
                tmdb_id: 1,
                media_type: MediaType::Movie,
                title: "T",
                original_title: None,
                runtime_minutes: None,
                poster_path: None,
                poster_url: None,
                folder_path: "Movies/Action/T [tmdb-1]",
                is_new: false,
            },
        )
        .await
        .unwrap();

        set_media_genres(&pool, media_id, MediaType::Movie, &[(28, true), (12, false)])
            .await
            .unwrap();

        let primary = primary_genre_for(&pool, media_id).await.unwrap().unwrap();
        assert_eq!(primary.id, 28);

        // Replacing should drop the old set.
        set_media_genres(&pool, media_id, MediaType::Movie, &[(12, true)])
            .await
            .unwrap();
        let primary = primary_genre_for(&pool, media_id).await.unwrap().unwrap();
        assert_eq!(primary.id, 12);
    }
}
