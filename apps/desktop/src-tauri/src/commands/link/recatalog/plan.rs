//! `plan_recatalog_series` — discover what's already on disk under a
//! catalogued TV row, so the Re-Catalog modal can pre-fill before the
//! user commits.

use std::path::PathBuf;

use serde::Serialize;
use tauri::State;

use crate::commands::link::write_target;
use crate::db::media::find_by_id;
use crate::db::settings::{get_setting_or, KEY_SEASON_LABEL};
use crate::error::{AppError, AppResult};
use crate::state::AppState;

#[derive(Debug, Serialize, Clone)]
pub struct RecatalogPlanSeason {
    pub season_number: i64,
    pub season_folder: PathBuf,
    pub video_filenames: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct RecatalogPlan {
    pub media_id: i64,
    pub tmdb_id: i64,
    pub title: String,
    pub poster_path: Option<String>,
    pub poster_url: Option<String>,
    pub series_folder: PathBuf,
    pub seasons: Vec<RecatalogPlanSeason>,
}

/// Discover the seasons + video files that live under the catalogued
/// folder of an existing TV row, so the UI can pre-fill the
/// Re-Catalog modal before the user commits.
#[tauri::command]
pub async fn plan_recatalog_series(
    state: State<'_, AppState>,
    media_id: i64,
    drive_root: Option<PathBuf>,
) -> AppResult<RecatalogPlan> {
    use crate::organizer::recatalog::discover_seasons;

    let (pool, hd_root) = write_target(&state, drive_root.as_deref()).await?;

    let row = find_by_id(&pool, media_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("media {media_id}")))?;
    if row.media_type != "tv" {
        return Err(AppError::Other(
            "recatalog is only supported for TV series".into(),
        ));
    }
    let series_folder = hd_root.join(&row.folder_path);
    if !series_folder.is_dir() {
        return Err(AppError::NotFound(format!(
            "series folder missing: {}",
            series_folder.display()
        )));
    }

    let season_label = get_setting_or(&pool, KEY_SEASON_LABEL, "Season").await?;
    let discovered =
        discover_seasons(&series_folder, &season_label).map_err(AppError::from)?;

    let seasons = discovered
        .into_iter()
        .map(|s| RecatalogPlanSeason {
            season_number: s.season_number,
            season_folder: s.season_folder,
            video_filenames: s
                .files
                .into_iter()
                .filter_map(|p| p.file_name().and_then(|n| n.to_str()).map(String::from))
                .collect(),
        })
        .collect();

    Ok(RecatalogPlan {
        media_id: row.id,
        tmdb_id: row.tmdb_id,
        title: row.title,
        poster_path: row.poster_path,
        poster_url: row.poster_url,
        series_folder,
        seasons,
    })
}
