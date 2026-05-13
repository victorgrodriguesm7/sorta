//! Post-link mutations on episode rows + series-level flags:
//! `list_episodes`, `set_media_is_new`, `update_season_label`.

use std::path::PathBuf;

use tauri::State;

use super::write_target;
use crate::db::media::set_is_new;
use crate::db::settings::{get_setting_or, set_setting, KEY_SEASON_LABEL, KEY_SERIES_LABEL};
use crate::error::AppResult;
use crate::organizer::naming::sanitize_segment;
use crate::state::AppState;

/// List every catalogued episode of a series, ordered by
/// (season, episode). Used by the right-panel "Episodes" section.
#[tauri::command]
pub async fn list_episodes(
    state: State<'_, AppState>,
    media_id: i64,
    drive_root: Option<PathBuf>,
) -> AppResult<Vec<crate::db::episodes::EpisodeRow>> {
    let (pool, _drive) = write_target(&state, drive_root.as_deref()).await?;
    crate::db::episodes::list_episodes(&pool, media_id).await
}

/// Toggle the user-controlled `is_new` flag on a media row.
#[tauri::command]
pub async fn set_media_is_new(
    state: State<'_, AppState>,
    media_id: i64,
    is_new: bool,
    drive_root: Option<PathBuf>,
) -> AppResult<()> {
    let (pool, _drive) = write_target(&state, drive_root.as_deref()).await?;
    set_is_new(&pool, media_id, is_new).await
}

/// Update the Season folder label across every registered drive.
/// Each drive's series folders are renamed in place.
#[tauri::command]
pub async fn update_season_label(
    state: State<'_, AppState>,
    label: String,
) -> AppResult<()> {
    let pools = {
        let s = state.read().await;
        s.all_pools()
    };

    for (hd_root, pool) in pools {
        let old = get_setting_or(&pool, KEY_SEASON_LABEL, "Season").await?;
        set_setting(&pool, KEY_SEASON_LABEL, &label).await?;
        if old == label {
            continue;
        }

        let series_label = get_setting_or(&pool, KEY_SERIES_LABEL, "Series").await?;
        let series_root = hd_root.join(sanitize_segment(&series_label));
        if !series_root.is_dir() {
            continue;
        }

        let old_safe = sanitize_segment(&old);
        let new_safe = sanitize_segment(&label);
        let series_iter = match std::fs::read_dir(&series_root) {
            Ok(it) => it,
            Err(_) => continue,
        };
        for entry in series_iter.flatten() {
            let series_path = entry.path();
            if !series_path.is_dir() {
                continue;
            }
            let season_iter = match std::fs::read_dir(&series_path) {
                Ok(it) => it,
                Err(_) => continue,
            };
            for season in season_iter.flatten() {
                let p = season.path();
                let Some(name) = p.file_name().and_then(|n| n.to_str()) else {
                    continue;
                };
                if let Some(suffix) = name.strip_prefix(&format!("{old_safe} ")) {
                    let new_name = format!("{new_safe} {suffix}");
                    let dest = series_path.join(new_name);
                    if dest.exists() {
                        continue;
                    }
                    let _ = std::fs::rename(&p, &dest);
                }
            }
        }
    }
    Ok(())
}
