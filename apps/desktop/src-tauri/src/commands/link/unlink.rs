//! `unlink_media` — drop a row from the DB, optionally renaming the
//! on-disk folder back to a non-catalogued state.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::State;

use super::write_target;
use crate::db::media::{delete_media, find_by_id, MediaType};
use crate::error::{AppError, AppResult};
use crate::organizer::naming::strip_tmdb_tag;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct UnlinkArgs {
    pub media_id: i64,
    /// If true (default), the on-disk folder is renamed to drop the
    /// `[tmdb-id]` suffix so it shows up under Uncatalogued again. The
    /// inner video + sidecars (movies only) are also untagged.
    /// If false, the folder/files are left untouched and only the DB
    /// row is removed.
    #[serde(default = "default_true")]
    pub rename_back: bool,
    #[serde(default)]
    pub drive_root: Option<PathBuf>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Serialize)]
pub struct UnlinkResult {
    pub removed_media_id: i64,
    pub new_folder_path: Option<PathBuf>,
    pub poster_deleted: bool,
}

/// Roll back a previous link: delete the DB row (cascades to
/// `media_genres`), drop the cached poster file (best-effort), and
/// optionally rename the on-disk folder/files back to a non-catalogued
/// state so they reappear in Uncatalogued.
#[tauri::command]
pub async fn unlink_media(
    state: State<'_, AppState>,
    args: UnlinkArgs,
) -> AppResult<UnlinkResult> {
    let (pool, hd_root) = write_target(&state, args.drive_root.as_deref()).await?;

    let row = find_by_id(&pool, args.media_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("media {}", args.media_id)))?;
    let media_type = MediaType::parse(&row.media_type)
        .ok_or_else(|| AppError::Other(format!("bad media_type {}", row.media_type)))?;

    let mut new_folder_path: Option<PathBuf> = None;

    if args.rename_back {
        let folder = hd_root.join(&row.folder_path);
        if folder.is_dir() {
            let basename = folder
                .file_name()
                .and_then(|n| n.to_str())
                .ok_or_else(|| AppError::Other("folder has no basename".into()))?
                .to_string();
            if let Some(bare_title) = strip_tmdb_tag(&basename) {
                let parent = folder
                    .parent()
                    .ok_or_else(|| AppError::Other("folder has no parent".into()))?;
                let mut dest = parent.join(&bare_title);
                // If something already lives there, append `(unlinked)`
                // rather than failing.
                if dest.exists() && dest != folder {
                    dest = parent.join(format!("{bare_title} (unlinked)"));
                }
                if dest != folder {
                    std::fs::rename(&folder, &dest).map_err(AppError::from)?;
                }
                // For movies, also untag the inner video + sidecars.
                if matches!(media_type, MediaType::Movie) {
                    let _ = untag_files_in_folder(&dest, &basename, &bare_title);
                }
                new_folder_path = Some(dest);
            } else {
                tracing::warn!(
                    "unlink: folder {} doesn't match catalogued convention; leaving as-is",
                    folder.display()
                );
            }
        }
    }

    delete_media(&pool, args.media_id).await?;

    // Best-effort poster cleanup.
    let mut poster_deleted = false;
    if let Some(rel) = row.poster_path.as_deref() {
        let p = hd_root.join(rel);
        if p.is_file() {
            if let Err(e) = std::fs::remove_file(&p) {
                tracing::warn!("unlink: cannot delete poster {}: {e}", p.display());
            } else {
                poster_deleted = true;
            }
        }
    }

    crate::manifest::write_best_effort(&hd_root, &pool).await;

    Ok(UnlinkResult {
        removed_media_id: args.media_id,
        new_folder_path,
        poster_deleted,
    })
}

/// Rename `<old_basename>.*` files in `folder` to `<new_basename>.*`,
/// preserving any tag suffix between the basename and the extension
/// (handled by `rename_sidecar`). The main video file is renamed too.
fn untag_files_in_folder(
    folder: &Path,
    old_basename: &str,
    new_basename: &str,
) -> AppResult<()> {
    use crate::organizer::sidecars::rename_sidecar;
    use crate::scanner::classify::{is_sidecar_file, is_video_file};

    let entries = std::fs::read_dir(folder).map_err(AppError::from)?;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n,
            None => continue,
        };
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        let stem = match path.file_stem().and_then(|s| s.to_str()) {
            Some(s) => s,
            None => continue,
        };

        let new_name: Option<String> = if is_video_file(&path) && stem == old_basename {
            Some(if ext.is_empty() {
                new_basename.to_string()
            } else {
                format!("{new_basename}.{ext}")
            })
        } else if is_sidecar_file(&path) {
            rename_sidecar(name, old_basename, new_basename)
        } else {
            None
        };

        if let Some(new_name) = new_name {
            let dest = folder.join(new_name);
            if dest != path && !dest.exists() {
                let _ = std::fs::rename(&path, &dest);
            }
        }
    }
    Ok(())
}
