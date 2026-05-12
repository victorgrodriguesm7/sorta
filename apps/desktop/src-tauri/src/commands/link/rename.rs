//! `rename_media` — change a catalogued row's title, renaming the
//! on-disk folder and its inner video + sidecars to match.

use std::path::{Path, PathBuf};

use serde::Deserialize;
use tauri::State;

use super::write_target;
use crate::db::genres::{primary_genre_for, GenreRow};
use crate::db::media::{find_by_id, update_folder_path, MediaRow, MediaType};
use crate::db::settings::{get_setting_or, KEY_MOVIES_LABEL, KEY_SERIES_LABEL};
use crate::error::{AppError, AppResult};
use crate::organizer::naming::{folder_name, sanitize_segment};
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct RenameArgs {
    pub media_id: i64,
    pub new_title: String,
    /// Drive the media row lives on. Optional during the frontend
    /// migration — falls back to the primary drive.
    #[serde(default)]
    pub drive_root: Option<PathBuf>,
}

#[tauri::command]
pub async fn rename_media(state: State<'_, AppState>, args: RenameArgs) -> AppResult<MediaRow> {
    let (pool, hd_root) = write_target(&state, args.drive_root.as_deref()).await?;

    let row = find_by_id(&pool, args.media_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("media {}", args.media_id)))?;

    let media_type = MediaType::parse(&row.media_type)
        .ok_or_else(|| AppError::Other(format!("bad media_type {}", row.media_type)))?;
    let primary: Option<GenreRow> = primary_genre_for(&pool, row.id).await?;

    let kind_label = match media_type {
        MediaType::Movie => get_setting_or(&pool, KEY_MOVIES_LABEL, "Movies").await?,
        MediaType::Tv => get_setting_or(&pool, KEY_SERIES_LABEL, "Series").await?,
    };

    let mut parent = hd_root.join(sanitize_segment(&kind_label));
    if let Some(g) = &primary {
        if matches!(media_type, MediaType::Movie) {
            parent = parent.join(sanitize_segment(g.display_name()));
        }
    }
    let new_folder_name = folder_name(&args.new_title, row.tmdb_id);
    let new_folder = parent.join(&new_folder_name);
    let old_folder = hd_root.join(&row.folder_path);

    if old_folder == new_folder {
        return Ok(row);
    }
    if new_folder.exists() {
        return Err(AppError::Conflict(format!(
            "folder already exists: {}",
            new_folder.display()
        )));
    }
    std::fs::create_dir_all(new_folder.parent().unwrap()).map_err(AppError::from)?;
    std::fs::rename(&old_folder, &new_folder).map_err(AppError::from)?;

    // Rename the inner video and any sidecars.
    rename_inside_folder(&new_folder, &new_folder_name)?;

    let new_rel = new_folder
        .strip_prefix(&hd_root)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| new_folder.to_string_lossy().to_string());
    update_folder_path(&pool, row.id, &new_rel).await?;

    let mut updated = find_by_id(&pool, row.id).await?.unwrap();
    updated.drive_root = Some(hd_root);
    Ok(updated)
}

/// Rename the main video + any sidecars inside `folder` to use
/// `new_stem` as their basename. The original stem is read off the
/// first video file found; sidecars are renamed via
/// `rename_sidecar` so things like `.en.srt` keep their tag suffix.
fn rename_inside_folder(folder: &Path, new_stem: &str) -> AppResult<()> {
    use crate::organizer::sidecars::rename_sidecar;
    use crate::scanner::classify::{is_sidecar_file, is_video_file};

    // Find the main video first.
    let mut old_stem: Option<String> = None;
    let mut video_path: Option<PathBuf> = None;
    for entry in std::fs::read_dir(folder).map_err(AppError::from)? {
        let entry = entry.map_err(AppError::from)?;
        let path = entry.path();
        if path.is_file() && is_video_file(&path) {
            old_stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .map(String::from);
            video_path = Some(path);
            break;
        }
    }
    let Some(video_path) = video_path else {
        return Ok(());
    };
    let Some(old_stem) = old_stem else {
        return Ok(());
    };
    let video_ext = video_path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    let new_video = folder.join(format!("{new_stem}.{video_ext}"));
    if video_path != new_video {
        std::fs::rename(&video_path, &new_video).map_err(AppError::from)?;
    }

    // Rename sidecars.
    for entry in std::fs::read_dir(folder).map_err(AppError::from)? {
        let entry = entry.map_err(AppError::from)?;
        let path = entry.path();
        if !path.is_file() || !is_sidecar_file(&path) {
            continue;
        }
        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n,
            None => continue,
        };
        if let Some(new_name) = rename_sidecar(name, &old_stem, new_stem) {
            let dest = folder.join(new_name);
            if dest != path {
                std::fs::rename(&path, &dest).map_err(AppError::from)?;
            }
        }
    }
    Ok(())
}
