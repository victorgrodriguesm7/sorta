//! Library listing + scan + settings commands.
//!
//! Split into focused submodules to keep each file readable:
//!   - `listing`   — `scan_now`, `list_series`, movie-by-genre fan-out
//!   - `genres`    — genre listing + translation
//!   - `poster`    — `get_poster_url`
//!   - `labels`    — `update_root_label`
//!   - `explorer`  — `open_in_explorer`
//!
//! The shared helpers (`resolve_drive`, `tag_drive`, `sort_by_title`)
//! live here so every submodule can use them without circular `use`s.

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::path::{Path, PathBuf};

use crate::db::media::MediaRow;
use crate::error::{AppError, AppResult};
use crate::scanner::walker::UncataloguedKind;
use crate::state::AppState;

// Submodules are `pub` so `tauri::generate_handler!` in `lib.rs` can
// reference each `#[tauri::command]` by its canonical path (the hidden
// `__cmd__<name>` companion items the macro emits live alongside the
// function and don't follow `pub use` re-exports).
pub mod explorer;
pub mod genres;
pub mod labels;
pub mod listing;
pub mod poster;

#[derive(Debug, Serialize, Deserialize)]
pub struct UncataloguedItem {
    pub folder: PathBuf,
    pub video_filename: String,
    pub kind: UncataloguedKind,
    /// Drive this item was discovered on. Stamped by `scan_now` so
    /// follow-up `link_media` / `link_as_series` knows which pool to
    /// write into without re-deriving from the folder path.
    pub drive_root: PathBuf,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ScanResultDto {
    pub uncatalogued: Vec<UncataloguedItem>,
    pub catalogued_count: usize,
    pub skipped_count: usize,
}

/// Look up the pool for an explicit `drive_root` or fall back to the
/// primary drive (`hd_roots[0]`). Returns `(pool, drive_root)`.
///
/// The optional argument is a compatibility shim: the frontend is
/// being migrated to always thread `drive_root` through, but until
/// every call site is updated we let the backend resolve a sensible
/// default. Once the migration is done we can flip it to required.
pub(crate) async fn resolve_drive(
    state: &AppState,
    drive_root: Option<&Path>,
) -> AppResult<(SqlitePool, PathBuf)> {
    let s = state.read().await;
    if let Some(d) = drive_root {
        let pool = s.pool_for(d)?;
        return Ok((pool, d.to_path_buf()));
    }
    let primary = s
        .hd_root
        .clone()
        .ok_or_else(|| AppError::Other("no drives registered".into()))?;
    let pool = s.pool_for(&primary)?;
    Ok((pool, primary))
}

/// Stamp `drive_root` on every row in `rows`. Tiny helper, but used
/// by every fan-out read so it lives here instead of repeated inline.
pub(crate) fn tag_drive(mut rows: Vec<MediaRow>, drive: &Path) -> Vec<MediaRow> {
    for r in &mut rows {
        r.drive_root = Some(drive.to_path_buf());
    }
    rows
}

/// Sort merged media results by case-insensitive title — each pool
/// is already sorted but a concat isn't.
pub(crate) fn sort_by_title(rows: &mut Vec<MediaRow>) {
    rows.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));
}
