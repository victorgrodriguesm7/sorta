//! Per-media-row genre commands: `list_media_genres`,
//! `reorder_media_genres`.

use std::path::PathBuf;

use serde::Deserialize;
use tauri::State;

use super::write_target;
use crate::db::genres::{list_genres, primary_genre_for, set_media_genres, GenreRow};
use crate::db::media::{find_by_id, update_folder_path, MediaRow, MediaType};
use crate::db::settings::{get_setting_or, KEY_MOVIES_LABEL};
use crate::error::{AppError, AppResult};
use crate::organizer::naming::sanitize_segment;
use crate::state::AppState;

/// List the genres a media row has, in their stored order. The first
/// entry is the primary (`is_primary = 1`); the rest are returned in
/// `genres.id` order. Useful for the right-panel reorder UI.
#[tauri::command]
pub async fn list_media_genres(
    state: State<'_, AppState>,
    media_id: i64,
    drive_root: Option<PathBuf>,
) -> AppResult<Vec<GenreRow>> {
    let (pool, _drive) = write_target(&state, drive_root.as_deref()).await?;
    sqlx::query_as::<_, GenreRow>(
        "SELECT g.id, g.media_type, g.canonical_name, g.translated_name \
         FROM genres g \
         JOIN media_genres mg ON mg.genre_id = g.id AND mg.media_type = g.media_type \
         WHERE mg.media_id = ? \
         ORDER BY mg.is_primary DESC, g.id ASC",
    )
    .bind(media_id)
    .fetch_all(&pool)
    .await
    .map_err(|e| AppError::Other(format!("list_media_genres: {e}")))
}

#[derive(Debug, Deserialize)]
pub struct ReorderGenresArgs {
    pub media_id: i64,
    /// Ordered list of genre ids. Index 0 = primary.
    pub genre_ids: Vec<i64>,
    #[serde(default)]
    pub drive_root: Option<PathBuf>,
}

/// Replace a media row's genres with `genre_ids` in the given order.
/// `genre_ids[0]` becomes the new primary; the on-disk genre folder for
/// movies is moved if the primary changed.
#[tauri::command]
pub async fn reorder_media_genres(
    state: State<'_, AppState>,
    args: ReorderGenresArgs,
) -> AppResult<MediaRow> {
    if args.genre_ids.is_empty() {
        return Err(AppError::Other("genre_ids cannot be empty".into()));
    }

    let (pool, hd_root) = write_target(&state, args.drive_root.as_deref()).await?;

    let row = find_by_id(&pool, args.media_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("media {}", args.media_id)))?;
    let media_type = MediaType::parse(&row.media_type)
        .ok_or_else(|| AppError::Other(format!("bad media_type {}", row.media_type)))?;

    // Validate that every requested genre id exists in the genres table for this media_type.
    let known: std::collections::HashSet<i64> = list_genres(&pool, media_type)
        .await?
        .into_iter()
        .map(|g| g.id)
        .collect();
    for gid in &args.genre_ids {
        if !known.contains(gid) {
            return Err(AppError::NotFound(format!(
                "genre {gid} not known for {:?}",
                media_type
            )));
        }
    }

    let old_primary = primary_genre_for(&pool, args.media_id).await?;

    // Replace media_genres rows with the new ordering.
    let pairs: Vec<(i64, bool)> = args
        .genre_ids
        .iter()
        .enumerate()
        .map(|(i, id)| (*id, i == 0))
        .collect();
    set_media_genres(&pool, args.media_id, media_type, &pairs).await?;

    // For movies, if the primary genre changed, the on-disk folder must
    // move from the old genre folder into the new one.
    if matches!(media_type, MediaType::Movie) {
        let new_primary = primary_genre_for(&pool, args.media_id).await?;
        let old_name = old_primary.as_ref().map(|g| g.display_name().to_string());
        let new_name = new_primary.as_ref().map(|g| g.display_name().to_string());
        if old_name != new_name {
            let movies_label =
                get_setting_or(&pool, KEY_MOVIES_LABEL, "Movies").await?;
            let movies_root = hd_root.join(sanitize_segment(&movies_label));
            let folder_basename = std::path::Path::new(&row.folder_path)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("");
            let from = hd_root.join(&row.folder_path);
            let to = match &new_name {
                Some(n) => movies_root.join(sanitize_segment(n)).join(folder_basename),
                None => movies_root.join(folder_basename),
            };
            if from.exists() && from != to {
                if let Some(parent) = to.parent() {
                    std::fs::create_dir_all(parent).map_err(AppError::from)?;
                }
                if to.exists() {
                    return Err(AppError::Conflict(format!(
                        "destination already exists: {}",
                        to.display()
                    )));
                }
                std::fs::rename(&from, &to).map_err(AppError::from)?;
                let new_rel = to
                    .strip_prefix(&hd_root)
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|_| to.to_string_lossy().to_string());
                update_folder_path(&pool, args.media_id, &new_rel).await?;
            }
        }
    }

    let mut updated = find_by_id(&pool, args.media_id).await?.unwrap();
    updated.drive_root = Some(hd_root);
    Ok(updated)
}
