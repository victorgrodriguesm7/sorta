//! Library listing + scan + settings commands.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::State;

use crate::db::genres::{list_genres, set_genre_translation, GenreRow};
use crate::db::media::{list_by_type, MediaRow, MediaType};
use crate::db::settings::{
    get_setting_or, set_setting, KEY_MOVIES_LABEL, KEY_SERIES_LABEL,
};
use crate::error::{AppError, AppResult};
use crate::organizer::execute::merge_genre_folders;
use crate::organizer::naming::sanitize_segment;
use crate::scanner::walker::{scan, ScanReport};
use crate::state::AppState;

#[derive(Debug, Serialize, Deserialize)]
pub struct UncataloguedItem {
    pub folder: PathBuf,
    pub video_filename: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ScanResultDto {
    pub uncatalogued: Vec<UncataloguedItem>,
    pub catalogued_count: usize,
    pub skipped_count: usize,
}

async fn require_hd_root(state: &AppState) -> AppResult<PathBuf> {
    let s = state.read().await;
    s.hd_root.clone().ok_or_else(|| AppError::Other("HD root not configured".into()))
}

#[tauri::command]
pub async fn scan_now(state: State<'_, AppState>) -> AppResult<ScanResultDto> {
    let root = require_hd_root(&state).await?;
    let report: ScanReport = scan(&root)?;
    Ok(ScanResultDto {
        uncatalogued: report
            .uncatalogued
            .into_iter()
            .map(|u| UncataloguedItem {
                folder: u.folder,
                video_filename: u.video_filename,
            })
            .collect(),
        catalogued_count: report.catalogued.len(),
        skipped_count: report.skipped.len(),
    })
}

#[tauri::command]
pub async fn list_movies_by_genre(
    state: State<'_, AppState>,
    genre_id: i64,
) -> AppResult<Vec<MediaRow>> {
    let pool = {
        let s = state.read().await;
        s.db.clone()
            .ok_or_else(|| AppError::Other("DB not initialized".into()))?
    };
    sqlx::query_as::<_, MediaRow>(
        "SELECT m.* FROM media m \
         JOIN media_genres mg ON mg.media_id = m.id \
         WHERE mg.genre_id = ? AND m.media_type = 'movie' AND mg.is_primary = 1 \
         ORDER BY m.title COLLATE NOCASE",
    )
    .bind(genre_id)
    .fetch_all(&pool)
    .await
    .map_err(|e| AppError::Other(format!("list_movies_by_genre: {e}")))
}

#[tauri::command]
pub async fn list_series(state: State<'_, AppState>) -> AppResult<Vec<MediaRow>> {
    let pool = {
        let s = state.read().await;
        s.db.clone()
            .ok_or_else(|| AppError::Other("DB not initialized".into()))?
    };
    list_by_type(&pool, MediaType::Tv).await
}

#[tauri::command]
pub async fn list_movie_genres(state: State<'_, AppState>) -> AppResult<Vec<GenreRow>> {
    let pool = {
        let s = state.read().await;
        s.db.clone()
            .ok_or_else(|| AppError::Other("DB not initialized".into()))?
    };
    list_genres(&pool, MediaType::Movie).await
}

/// Set a genre's translated name. If the new translation collides with
/// another genre's display name, the underlying folders are physically
/// merged on disk.
#[tauri::command]
pub async fn update_genre_translation(
    state: State<'_, AppState>,
    genre_id: i64,
    translated: Option<String>,
) -> AppResult<()> {
    let (pool, hd_root, movies_label) = {
        let s = state.read().await;
        let pool = s.db.clone().ok_or_else(|| AppError::Other("DB not initialized".into()))?;
        let hd = s.hd_root.clone().ok_or_else(|| AppError::Other("HD not set".into()))?;
        (pool, hd, ())
    };
    let _ = movies_label;

    // Capture the "before" display name so we know which folder to rename from.
    let genres_before = list_genres(&pool, MediaType::Movie).await?;
    let target = genres_before
        .iter()
        .find(|g| g.id == genre_id)
        .ok_or_else(|| AppError::NotFound(format!("genre {genre_id}")))?;
    let old_display = target.display_name().to_string();

    set_genre_translation(&pool, genre_id, MediaType::Movie, translated.as_deref()).await?;

    // After: figure out new display name.
    let genres_after = list_genres(&pool, MediaType::Movie).await?;
    let updated = genres_after
        .iter()
        .find(|g| g.id == genre_id)
        .ok_or_else(|| AppError::NotFound(format!("genre {genre_id}")))?;
    let new_display = updated.display_name().to_string();

    if old_display == new_display {
        return Ok(());
    }

    let movies_label_value =
        get_setting_or(&pool, KEY_MOVIES_LABEL, "Movies").await?;
    let movies_root = hd_root.join(sanitize_segment(&movies_label_value));
    let from = movies_root.join(sanitize_segment(&old_display));
    let to = movies_root.join(sanitize_segment(&new_display));

    if from.exists() {
        merge_genre_folders(&from, &to)?;
    }
    Ok(())
}

#[tauri::command]
pub async fn update_root_label(
    state: State<'_, AppState>,
    kind: String,
    label: String,
) -> AppResult<()> {
    let (pool, hd_root) = {
        let s = state.read().await;
        let pool = s.db.clone().ok_or_else(|| AppError::Other("DB not initialized".into()))?;
        let hd = s.hd_root.clone().ok_or_else(|| AppError::Other("HD not set".into()))?;
        (pool, hd)
    };
    let key = match kind.as_str() {
        "movie" => KEY_MOVIES_LABEL,
        "tv" => KEY_SERIES_LABEL,
        other => return Err(AppError::Other(format!("unknown kind: {other}"))),
    };
    let old = get_setting_or(&pool, key, if key == KEY_MOVIES_LABEL { "Movies" } else { "Series" })
        .await?;
    set_setting(&pool, key, &label).await?;

    if old != label {
        let from = hd_root.join(sanitize_segment(&old));
        let to = hd_root.join(sanitize_segment(&label));
        if from.exists() {
            // Rename root folder; if `to` already exists, merge into it.
            if to.exists() {
                merge_genre_folders(&from, &to)?;
            } else {
                std::fs::rename(&from, &to).map_err(AppError::from)?;
            }
        }
    }
    Ok(())
}
