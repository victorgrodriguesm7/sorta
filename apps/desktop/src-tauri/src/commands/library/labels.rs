//! `update_root_label` — rename the Movies / Series folder on every
//! drive in one go.

use tauri::State;

use crate::db::settings::{get_setting_or, set_setting, KEY_MOVIES_LABEL, KEY_SERIES_LABEL};
use crate::error::{AppError, AppResult};
use crate::organizer::execute::merge_genre_folders;
use crate::organizer::naming::sanitize_segment;
use crate::state::AppState;

/// Update the `Movies` / `Series` folder label on every registered
/// drive. Each drive owns its own copy of the setting + an on-disk
/// folder, so the rename has to be applied per-drive — otherwise
/// drives would drift out of sync (one drive's "Films" vs another's
/// "Movies").
#[tauri::command]
pub async fn update_root_label(
    state: State<'_, AppState>,
    kind: String,
    label: String,
) -> AppResult<()> {
    let key = match kind.as_str() {
        "movie" => KEY_MOVIES_LABEL,
        "tv" => KEY_SERIES_LABEL,
        other => return Err(AppError::Other(format!("unknown kind: {other}"))),
    };
    let pools = {
        let s = state.read().await;
        s.all_pools()
    };
    for (drive, pool) in pools {
        let old = get_setting_or(
            &pool,
            key,
            if key == KEY_MOVIES_LABEL { "Movies" } else { "Series" },
        )
        .await?;
        set_setting(&pool, key, &label).await?;

        if old != label {
            let from = drive.join(sanitize_segment(&old));
            let to = drive.join(sanitize_segment(&label));
            if from.exists() {
                if to.exists() {
                    merge_genre_folders(&from, &to)?;
                } else {
                    std::fs::rename(&from, &to).map_err(AppError::from)?;
                }
            }
        }
    }
    Ok(())
}
