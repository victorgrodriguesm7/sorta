//! `get_poster_url` — local poster bytes inlined as a data URL.

use std::path::PathBuf;
use tauri::State;

use super::resolve_drive;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

/// Return the poster for a media row as a base64 `data:` URL ready to be
/// dropped into an `<img src>`. Tries the locally cached poster first
/// (`<HD>/poster/<tmdb_id>.jpg`); if it's missing or unreadable, falls
/// back to the TMDB CDN URL (returned verbatim — the webview can fetch
/// it directly from the public internet).
///
/// Returning bytes inline avoids the Tauri 2 asset-protocol scope
/// problem: the user's HD root is chosen at runtime, so we can't bake a
/// static scope into tauri.conf.json.
#[tauri::command]
pub async fn get_poster_url(
    state: State<'_, AppState>,
    media_id: i64,
    drive_root: Option<PathBuf>,
) -> AppResult<Option<String>> {
    use base64::engine::general_purpose::STANDARD as B64;
    use base64::Engine as _;

    let (pool, drive) = resolve_drive(&state, drive_root.as_deref()).await?;

    let row = crate::db::media::find_by_id(&pool, media_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("media {media_id}")))?;

    if let Some(rel) = row.poster_path.as_deref() {
        let abs = drive.join(rel);
        match std::fs::read(&abs) {
            Ok(bytes) => {
                let mime = match abs
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(|s| s.to_ascii_lowercase())
                    .as_deref()
                {
                    Some("png") => "image/png",
                    Some("webp") => "image/webp",
                    _ => "image/jpeg",
                };
                let encoded = B64.encode(&bytes);
                return Ok(Some(format!("data:{mime};base64,{encoded}")));
            }
            Err(e) => {
                tracing::warn!(
                    "poster: cannot read {}: {e}; falling back to TMDB URL",
                    abs.display()
                );
            }
        }
    }
    Ok(row.poster_url)
}
