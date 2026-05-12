//! Linking, renaming, and cataloging commands.
//!
//! Split into focused submodules to keep each file under ~300 lines:
//!   - `single`    — `link_media` (single movie / one-folder series link)
//!   - `series`    — `link_as_series` (multi-episode batch)
//!   - `rename`    — `rename_media`
//!   - `genres`    — `list_media_genres`, `reorder_media_genres`
//!   - `episodes`  — `list_episodes`, `set_media_is_new`, `update_season_label`
//!   - `unlink`    — `unlink_media`
//!   - `recatalog` — `plan_recatalog_series`, `recatalog_series`
//!
//! Shared helpers (`write_target`, `save_poster`, `save_episode_still`,
//! `default_true_series`) live here so every submodule can reach them
//! without circular `use`s.

use std::path::{Path, PathBuf};

use sqlx::SqlitePool;

use crate::commands::library::resolve_drive;
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::tmdb::TmdbClient;

// Submodules are `pub` so `tauri::generate_handler!` in `lib.rs` can
// reference each `#[tauri::command]` by its canonical path (the hidden
// `__cmd__<name>` companion items the macro emits live alongside the
// function and don't follow `pub use` re-exports).
pub mod episodes;
pub mod genres;
pub mod recatalog;
pub mod rename;
pub mod series;
pub mod single;
pub mod unlink;

/// Resolve the drive + pool a write should target. If the caller
/// passes `drive_root`, that wins. Otherwise we fall back to the
/// primary drive — used by old single-drive callers during the
/// frontend migration.
pub(crate) async fn write_target(
    state: &AppState,
    drive_root: Option<&Path>,
) -> AppResult<(SqlitePool, PathBuf)> {
    resolve_drive(state, drive_root).await
}

/// Default for serde-derived `rename` / `download_episode_posters`
/// flags. Shared between `series::link_as_series` and
/// `recatalog::recatalog_series` so the two commands stay in sync —
/// changing the default in one place changes it everywhere.
pub(crate) fn default_true_series() -> bool {
    true
}

/// Download one episode still into `<HD>/poster/episodes/` and return
/// its relative path. Naming convention: `{tv_id}_s{NN}e{NN}.jpg` —
/// stable, matches the on-disk folder/file layout, and keeps the
/// episode directory flat so the reader can mass-iterate it cheaply.
pub(crate) async fn save_episode_still(
    hd_root: &Path,
    tv_id: i64,
    season: i64,
    episode: i64,
    still_path: &str,
) -> AppResult<String> {
    let dest_dir = hd_root.join("poster").join("episodes");
    std::fs::create_dir_all(&dest_dir).map_err(AppError::from)?;
    let url = TmdbClient::still_url(still_path, "w300");
    let bytes = reqwest::get(&url)
        .await
        .map_err(|e| AppError::Other(format!("episode still download: {e}")))?
        .error_for_status()
        .map_err(|e| AppError::Other(format!("episode still status: {e}")))?
        .bytes()
        .await
        .map_err(|e| AppError::Other(format!("episode still body: {e}")))?;
    let rel = format!("poster/episodes/{tv_id}_s{season:02}e{episode:02}.jpg");
    std::fs::write(hd_root.join(&rel), &bytes).map_err(AppError::from)?;
    Ok(rel)
}

pub(crate) async fn save_poster(
    hd_root: &Path,
    tmdb_id: i64,
    poster_path: &str,
) -> AppResult<(String, String)> {
    let dest_dir = hd_root.join("poster");
    std::fs::create_dir_all(&dest_dir).map_err(AppError::from)?;
    let url = TmdbClient::poster_url(poster_path, "w500");
    let bytes = reqwest::get(&url)
        .await
        .map_err(|e| AppError::Other(format!("poster download: {e}")))?
        .error_for_status()
        .map_err(|e| AppError::Other(format!("poster status: {e}")))?
        .bytes()
        .await
        .map_err(|e| AppError::Other(format!("poster body: {e}")))?;
    let local_rel = format!("poster/{tmdb_id}.jpg");
    std::fs::write(hd_root.join(&local_rel), &bytes).map_err(AppError::from)?;
    Ok((local_rel, url))
}
