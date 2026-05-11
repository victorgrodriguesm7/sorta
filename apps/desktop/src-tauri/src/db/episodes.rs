//! Episode table queries.
//!
//! One row per video file moved into a series' season folder by
//! `link_as_series`. TMDB metadata (title, overview, air_date, still)
//! is pulled at link time so the reader can render real episode
//! titles + per-episode artwork without making any network calls.
//!
//! `media_id` joins back to the parent series row in `media`.

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct EpisodeRow {
    pub id: i64,
    pub media_id: i64,
    pub season_number: i64,
    pub episode_number: i64,
    pub title: Option<String>,
    pub overview: Option<String>,
    pub air_date: Option<String>,
    pub runtime_minutes: Option<i64>,
    /// Local cached still image, relative to HD root.
    pub still_path: Option<String>,
    /// TMDB CDN fallback URL.
    pub still_url: Option<String>,
    pub file_path: Option<String>,
}

#[derive(Debug, Clone)]
pub struct NewEpisode<'a> {
    pub media_id: i64,
    pub season_number: i64,
    pub episode_number: i64,
    pub title: Option<&'a str>,
    pub overview: Option<&'a str>,
    pub air_date: Option<&'a str>,
    pub runtime_minutes: Option<i64>,
    pub still_path: Option<&'a str>,
    pub still_url: Option<&'a str>,
    pub file_path: Option<&'a str>,
}

/// Insert one episode row, returning its id. Uses `INSERT OR REPLACE`
/// against the `(media_id, season_number, episode_number)` uniqueness
/// constraint so re-linking a season (e.g. after a botched first pass)
/// updates rather than duplicates.
pub async fn upsert_episode(pool: &SqlitePool, e: &NewEpisode<'_>) -> AppResult<i64> {
    let res = sqlx::query(
        "INSERT INTO episodes \
         (media_id, season_number, episode_number, title, overview, air_date, \
          runtime_minutes, still_path, still_url, file_path) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
         ON CONFLICT(media_id, season_number, episode_number) DO UPDATE SET \
            title           = excluded.title, \
            overview        = excluded.overview, \
            air_date        = excluded.air_date, \
            runtime_minutes = excluded.runtime_minutes, \
            still_path      = COALESCE(excluded.still_path, episodes.still_path), \
            still_url       = COALESCE(excluded.still_url,  episodes.still_url), \
            file_path       = COALESCE(excluded.file_path,  episodes.file_path)",
    )
    .bind(e.media_id)
    .bind(e.season_number)
    .bind(e.episode_number)
    .bind(e.title)
    .bind(e.overview)
    .bind(e.air_date)
    .bind(e.runtime_minutes)
    .bind(e.still_path)
    .bind(e.still_url)
    .bind(e.file_path)
    .execute(pool)
    .await
    .map_err(|e| AppError::Other(format!("upsert_episode: {e}")))?;
    Ok(res.last_insert_rowid())
}

/// List every episode of a series, ordered for display.
pub async fn list_episodes(pool: &SqlitePool, media_id: i64) -> AppResult<Vec<EpisodeRow>> {
    sqlx::query_as::<_, EpisodeRow>(
        "SELECT * FROM episodes \
         WHERE media_id = ? \
         ORDER BY season_number, episode_number",
    )
    .bind(media_id)
    .fetch_all(pool)
    .await
    .map_err(|e| AppError::Other(format!("list_episodes: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::media::{insert_media, MediaType, NewMedia};
    use crate::db::open;
    use tempfile::TempDir;

    async fn series_with_id(pool: &SqlitePool) -> i64 {
        insert_media(
            pool,
            &NewMedia {
                tmdb_id: 1399,
                media_type: MediaType::Tv,
                title: "GoT",
                original_title: None,
                runtime_minutes: None,
                poster_path: None,
                poster_url: None,
                folder_path: "Series/GoT [tmdb-1399]",
                is_new: true,
            },
        )
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn insert_then_list_in_order() {
        let tmp = TempDir::new().unwrap();
        let pool = open(&tmp.path().join("sorta.db")).await.unwrap();
        let media_id = series_with_id(&pool).await;

        // Insert out of order; expect ordered output.
        for (s, e, title) in [(1, 2, "Two"), (1, 1, "One"), (2, 1, "Three")] {
            upsert_episode(
                &pool,
                &NewEpisode {
                    media_id,
                    season_number: s,
                    episode_number: e,
                    title: Some(title),
                    overview: None,
                    air_date: None,
                    runtime_minutes: None,
                    still_path: None,
                    still_url: None,
                    file_path: None,
                },
            )
            .await
            .unwrap();
        }
        let eps = list_episodes(&pool, media_id).await.unwrap();
        let titles: Vec<_> = eps.iter().map(|e| e.title.clone().unwrap()).collect();
        assert_eq!(titles, vec!["One", "Two", "Three"]);
    }

    #[tokio::test]
    async fn upsert_replaces_metadata_but_keeps_still_path() {
        // Locked-in behaviour: re-linking a season after the user edited
        // TMDB metadata should refresh the title/overview/etc., but a
        // previously downloaded still on disk should not be wiped just
        // because the second pass skipped the download.
        let tmp = TempDir::new().unwrap();
        let pool = open(&tmp.path().join("sorta.db")).await.unwrap();
        let media_id = series_with_id(&pool).await;

        upsert_episode(
            &pool,
            &NewEpisode {
                media_id,
                season_number: 1,
                episode_number: 1,
                title: Some("Pilot"),
                overview: Some("first"),
                air_date: Some("2011-04-17"),
                runtime_minutes: Some(60),
                still_path: Some("poster/episodes/1399_s01e01.jpg"),
                still_url: Some("https://x/y.jpg"),
                file_path: Some("Series/GoT [tmdb-1399]/Season 1/S01E01.Pilot.mkv"),
            },
        )
        .await
        .unwrap();

        // Re-link with refreshed metadata but no still re-download.
        upsert_episode(
            &pool,
            &NewEpisode {
                media_id,
                season_number: 1,
                episode_number: 1,
                title: Some("Winter Is Coming"),
                overview: Some("better overview"),
                air_date: Some("2011-04-17"),
                runtime_minutes: Some(62),
                still_path: None,
                still_url: None,
                file_path: None,
            },
        )
        .await
        .unwrap();

        let eps = list_episodes(&pool, media_id).await.unwrap();
        assert_eq!(eps.len(), 1);
        assert_eq!(eps[0].title.as_deref(), Some("Winter Is Coming"));
        assert_eq!(
            eps[0].still_path.as_deref(),
            Some("poster/episodes/1399_s01e01.jpg"),
        );
        assert_eq!(eps[0].runtime_minutes, Some(62));
    }

    #[tokio::test]
    async fn cascade_delete_on_media_removes_episodes() {
        use crate::db::media::delete_media;
        let tmp = TempDir::new().unwrap();
        let pool = open(&tmp.path().join("sorta.db")).await.unwrap();
        let media_id = series_with_id(&pool).await;
        upsert_episode(
            &pool,
            &NewEpisode {
                media_id,
                season_number: 1,
                episode_number: 1,
                title: Some("Pilot"),
                overview: None,
                air_date: None,
                runtime_minutes: None,
                still_path: None,
                still_url: None,
                file_path: None,
            },
        )
        .await
        .unwrap();
        delete_media(&pool, media_id).await.unwrap();
        let eps = list_episodes(&pool, media_id).await.unwrap();
        assert!(eps.is_empty());
    }
}
