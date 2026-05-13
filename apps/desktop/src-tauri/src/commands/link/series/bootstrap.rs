//! Helper for `link_as_series`: either reuse the existing series row
//! or create a fresh one — pulling TMDB metadata, upserting genres,
//! downloading the poster, and inserting the `media` row in one go.

use std::path::{Path, PathBuf};

use sqlx::SqlitePool;

use super::LinkAsSeriesArgs;
use crate::commands::link::save_poster;
use crate::db::genres::{set_media_genres, upsert_genre};
use crate::db::media::{find_by_tmdb_id, insert_media, MediaType, NewMedia};
use crate::db::settings::{get_setting_or, KEY_SERIES_LABEL};
use crate::error::{AppError, AppResult};
use crate::organizer::naming::{folder_name, sanitize_segment};
use crate::tmdb::TmdbClient;

/// Returns `(media_id, series_folder_abs)` for the series row that
/// will own the batch's episodes. Idempotent: when the series already
/// exists we just resolve its folder; otherwise we fetch TMDB,
/// persist genres + poster, and insert the row.
pub(super) async fn get_or_create_series_row(
    pool: &SqlitePool,
    hd_root: &Path,
    tmdb: &TmdbClient,
    args: &LinkAsSeriesArgs,
) -> AppResult<(i64, PathBuf)> {
    if let Some(row) = find_by_tmdb_id(pool, args.tmdb_id, MediaType::Tv).await? {
        let folder = hd_root.join(&row.folder_path);
        if !folder.exists() {
            return Err(AppError::NotFound(format!(
                "series folder vanished: {}",
                folder.display()
            )));
        }
        return Ok((row.id, folder));
    }

    let series_label = get_setting_or(pool, KEY_SERIES_LABEL, "Series").await?;
    let series_root = hd_root.join(sanitize_segment(&series_label));

    // Pull metadata from TMDB.
    let details = tmdb.get_tv(args.tmdb_id).await?;
    let title = details.name.clone();
    let original_title = details.original_name.clone();
    let runtime = details.primary_runtime();

    // Persist all genres up front (FK from media_genres).
    for g in &details.genres {
        upsert_genre(pool, g.id, MediaType::Tv, &g.name).await?;
    }

    let folder = series_root.join(folder_name(&title, args.tmdb_id));
    std::fs::create_dir_all(&folder).map_err(AppError::from)?;

    // Best-effort poster download.
    let (poster_local, poster_remote) = if let Some(pp) = details.poster_path.as_deref() {
        match save_poster(hd_root, args.tmdb_id, pp).await {
            Ok((local, url)) => (Some(local), Some(url)),
            Err(_) => (None, Some(TmdbClient::poster_url(pp, "w500"))),
        }
    } else {
        (None, None)
    };

    let folder_rel = folder
        .strip_prefix(hd_root)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| folder.to_string_lossy().to_string());

    let media_id = insert_media(
        pool,
        &NewMedia {
            tmdb_id: args.tmdb_id,
            media_type: MediaType::Tv,
            title: &title,
            original_title: original_title.as_deref(),
            runtime_minutes: runtime,
            poster_path: poster_local.as_deref(),
            poster_url: poster_remote.as_deref(),
            folder_path: &folder_rel,
            is_new: args.is_new,
        },
    )
    .await?;

    let pairs: Vec<(i64, bool)> = details
        .genres
        .iter()
        .enumerate()
        .map(|(i, g)| (g.id, i == 0))
        .collect();
    if !pairs.is_empty() {
        set_media_genres(pool, media_id, MediaType::Tv, &pairs).await?;
    }
    Ok((media_id, folder))
}
