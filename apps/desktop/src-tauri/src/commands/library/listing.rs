//! `scan_now` + media listing commands.
//!
//! Every read here fans out across `all_pools()` and tags rows with
//! their originating `drive_root`, so the frontend can route follow-up
//! writes back to the correct pool.

use sqlx::SqlitePool;
use std::path::PathBuf;
use tauri::State;

use super::{sort_by_title, tag_drive, ScanResultDto, UncataloguedItem};
use crate::db::media::{list_by_type, MediaRow, MediaType};
use crate::error::{AppError, AppResult};
use crate::scanner::walker::{scan, ScanReport};
use crate::state::AppState;

#[tauri::command]
pub async fn scan_now(state: State<'_, AppState>) -> AppResult<ScanResultDto> {
    // Snapshot the drive list so we don't hold the read lock across
    // `scan()` calls (each one walks a full filesystem tree and can
    // take seconds on a cold cache).
    let drives: Vec<PathBuf> = {
        let s = state.read().await;
        s.drives.keys().cloned().collect()
    };

    let mut uncatalogued = Vec::new();
    let mut catalogued_count = 0usize;
    let mut skipped_count = 0usize;
    for drive in &drives {
        let report: ScanReport = match scan(drive) {
            Ok(r) => r,
            Err(e) => {
                // One unmounted/erroring drive shouldn't kill the
                // whole scan — log and keep going so the user still
                // sees content from healthy drives.
                tracing::warn!("scan_now: drive {} failed: {e:?}", drive.display());
                continue;
            }
        };
        catalogued_count += report.catalogued.len();
        skipped_count += report.skipped.len();
        for u in report.uncatalogued {
            uncatalogued.push(UncataloguedItem {
                folder: u.folder,
                video_filename: u.video_filename,
                kind: u.kind,
                drive_root: drive.clone(),
            });
        }
    }

    Ok(ScanResultDto {
        uncatalogued,
        catalogued_count,
        skipped_count,
    })
}

#[tauri::command]
pub async fn list_movies_by_genre(
    state: State<'_, AppState>,
    genre_id: i64,
) -> AppResult<Vec<MediaRow>> {
    list_movies_by_genres(state, vec![genre_id]).await
}

/// Per-pool implementation. The fan-out wrapper below tags each pool's
/// results with its drive root before merging.
async fn list_movies_by_genres_pool(
    pool: &SqlitePool,
    genre_ids: &[i64],
) -> AppResult<Vec<MediaRow>> {
    // Build "?, ?, ?" placeholders dynamically — sqlx doesn't have a
    // built-in IN-list binder for sqlite.
    let placeholders = std::iter::repeat("?")
        .take(genre_ids.len())
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "SELECT DISTINCT m.* FROM media m \
         JOIN media_genres mg ON mg.media_id = m.id \
         WHERE mg.genre_id IN ({placeholders}) \
           AND m.media_type = 'movie' \
           AND mg.is_primary = 1 \
         ORDER BY m.title COLLATE NOCASE"
    );
    let mut q = sqlx::query_as::<_, MediaRow>(&sql);
    for id in genre_ids {
        q = q.bind(*id);
    }
    q.fetch_all(pool)
        .await
        .map_err(|e| AppError::Other(format!("list_movies_by_genres: {e}")))
}

/// List every movie whose primary genre is *any* of `genre_ids`, across
/// every registered drive. Used by the LeftPanel when several genres
/// are visually merged under the same translated display name (e.g.
/// Action + Adventure both → "Aventura"): clicking the merged bucket
/// must surface movies whose primary is either of the underlying TMDB
/// ids.
#[tauri::command]
pub async fn list_movies_by_genres(
    state: State<'_, AppState>,
    genre_ids: Vec<i64>,
) -> AppResult<Vec<MediaRow>> {
    if genre_ids.is_empty() {
        return Ok(vec![]);
    }
    let pools = {
        let s = state.read().await;
        s.all_pools()
    };
    let mut out = Vec::new();
    for (drive, pool) in pools {
        let rows = list_movies_by_genres_pool(&pool, &genre_ids).await?;
        out.extend(tag_drive(rows, &drive));
    }
    sort_by_title(&mut out);
    Ok(out)
}

#[tauri::command]
pub async fn list_series(state: State<'_, AppState>) -> AppResult<Vec<MediaRow>> {
    let pools = {
        let s = state.read().await;
        s.all_pools()
    };
    let mut out = Vec::new();
    for (drive, pool) in pools {
        let rows = list_by_type(&pool, MediaType::Tv).await?;
        out.extend(tag_drive(rows, &drive));
    }
    sort_by_title(&mut out);
    Ok(out)
}
