//! Library listing + scan + settings commands.

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::path::{Path, PathBuf};
use tauri::State;

use crate::db::genres::{list_genres, set_genre_translation, GenreRow};
use crate::db::media::{list_by_type, MediaRow, MediaType};
use crate::db::settings::{
    get_setting_or, set_setting, KEY_MOVIES_LABEL, KEY_SERIES_LABEL,
};
use crate::error::{AppError, AppResult};
use crate::organizer::execute::merge_genre_folders;
use crate::organizer::naming::sanitize_segment;
use crate::scanner::walker::{scan, ScanReport, UncataloguedKind};
use crate::state::AppState;

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
fn tag_drive(mut rows: Vec<MediaRow>, drive: &Path) -> Vec<MediaRow> {
    for r in &mut rows {
        r.drive_root = Some(drive.to_path_buf());
    }
    rows
}

/// Sort merged media results by case-insensitive title — each pool
/// is already sorted but a concat isn't.
fn sort_by_title(rows: &mut Vec<MediaRow>) {
    rows.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));
}

#[tauri::command]
pub async fn scan_now(state: State<'_, AppState>) -> AppResult<ScanResultDto> {
    // Snapshot the drive list so we don't hold the read lock across
    // `scan()` calls (each one walks a full filesystem tree and can
    // take seconds on a cold cache).
    let drives: Vec<PathBuf> = {
        let s = state.read().await;
        s.drives.keys().cloned().collect()
    };

    let mut uncatalogued = Vec::new();
    let mut catalogued_count = 0usize;
    let mut skipped_count = 0usize;
    for drive in &drives {
        let report: ScanReport = match scan(drive) {
            Ok(r) => r,
            Err(e) => {
                // One unmounted/erroring drive shouldn't kill the
                // whole scan — log and keep going so the user still
                // sees content from healthy drives.
                tracing::warn!("scan_now: drive {} failed: {e:?}", drive.display());
                continue;
            }
        };
        catalogued_count += report.catalogued.len();
        skipped_count += report.skipped.len();
        for u in report.uncatalogued {
            uncatalogued.push(UncataloguedItem {
                folder: u.folder,
                video_filename: u.video_filename,
                kind: u.kind,
                drive_root: drive.clone(),
            });
        }
    }

    Ok(ScanResultDto {
        uncatalogued,
        catalogued_count,
        skipped_count,
    })
}

#[tauri::command]
pub async fn list_movies_by_genre(
    state: State<'_, AppState>,
    genre_id: i64,
) -> AppResult<Vec<MediaRow>> {
    list_movies_by_genres(state, vec![genre_id]).await
}

/// Per-pool implementation of `list_movies_by_genres`. The fan-out
/// wrapper below tags each pool's results with its drive root before
/// merging.
async fn list_movies_by_genres_pool(
    pool: &SqlitePool,
    genre_ids: &[i64],
) -> AppResult<Vec<MediaRow>> {
    // Build "?, ?, ?" placeholders dynamically — sqlx doesn't have a
    // built-in IN-list binder for sqlite.
    let placeholders = std::iter::repeat("?")
        .take(genre_ids.len())
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "SELECT DISTINCT m.* FROM media m \
         JOIN media_genres mg ON mg.media_id = m.id \
         WHERE mg.genre_id IN ({placeholders}) \
           AND m.media_type = 'movie' \
           AND mg.is_primary = 1 \
         ORDER BY m.title COLLATE NOCASE"
    );
    let mut q = sqlx::query_as::<_, MediaRow>(&sql);
    for id in genre_ids {
        q = q.bind(*id);
    }
    q.fetch_all(pool)
        .await
        .map_err(|e| AppError::Other(format!("list_movies_by_genres: {e}")))
}

/// List every movie whose primary genre is *any* of `genre_ids`, across
/// every registered drive. Used by the LeftPanel when several genres
/// are visually merged under the same translated display name (e.g.
/// Action + Adventure both → "Aventura"): clicking the merged bucket
/// must surface movies whose primary is either of the underlying TMDB
/// ids. Rows are tagged with their originating `drive_root` so the
/// right panel can route follow-up writes back to the correct pool.
#[tauri::command]
pub async fn list_movies_by_genres(
    state: State<'_, AppState>,
    genre_ids: Vec<i64>,
) -> AppResult<Vec<MediaRow>> {
    if genre_ids.is_empty() {
        return Ok(vec![]);
    }
    let pools = {
        let s = state.read().await;
        s.all_pools()
    };
    let mut out = Vec::new();
    for (drive, pool) in pools {
        let rows = list_movies_by_genres_pool(&pool, &genre_ids).await?;
        out.extend(tag_drive(rows, &drive));
    }
    sort_by_title(&mut out);
    Ok(out)
}

#[tauri::command]
pub async fn list_series(state: State<'_, AppState>) -> AppResult<Vec<MediaRow>> {
    let pools = {
        let s = state.read().await;
        s.all_pools()
    };
    let mut out = Vec::new();
    for (drive, pool) in pools {
        let rows = list_by_type(&pool, MediaType::Tv).await?;
        out.extend(tag_drive(rows, &drive));
    }
    sort_by_title(&mut out);
    Ok(out)
}

/// Merge a vec of genres from one drive into the running result. Genres
/// from different drives sharing the same TMDB id collapse into one
/// entry; if any drive has a non-null `translated_name`, that one wins.
fn merge_genres(acc: &mut Vec<GenreRow>, incoming: Vec<GenreRow>) {
    use std::collections::HashMap;
    // Build a quick (id, media_type) -> index map once so we don't do
    // O(n²) lookups when many drives are registered.
    let mut idx: HashMap<(i64, String), usize> = acc
        .iter()
        .enumerate()
        .map(|(i, g)| ((g.id, g.media_type.clone()), i))
        .collect();
    for g in incoming {
        match idx.get(&(g.id, g.media_type.clone())) {
            Some(&i) => {
                if acc[i].translated_name.is_none() && g.translated_name.is_some() {
                    acc[i].translated_name = g.translated_name;
                }
            }
            None => {
                idx.insert((g.id, g.media_type.clone()), acc.len());
                acc.push(g);
            }
        }
    }
}

#[tauri::command]
pub async fn list_movie_genres(state: State<'_, AppState>) -> AppResult<Vec<GenreRow>> {
    let pools = {
        let s = state.read().await;
        s.all_pools()
    };
    let mut merged: Vec<GenreRow> = Vec::new();
    for (_drive, pool) in pools {
        let rows = list_genres(&pool, MediaType::Movie).await?;
        merge_genres(&mut merged, rows);
    }
    merged.sort_by(|a, b| {
        let an = a.translated_name.as_deref().unwrap_or(&a.canonical_name).to_lowercase();
        let bn = b.translated_name.as_deref().unwrap_or(&b.canonical_name).to_lowercase();
        an.cmp(&bn)
    });
    Ok(merged)
}

/// List only the movie genres that are the **primary** genre of at
/// least one linked movie, on at least one drive. Drives the LeftPanel
/// so empty buckets stay hidden.
#[tauri::command]
pub async fn list_movie_genres_in_use(
    state: State<'_, AppState>,
) -> AppResult<Vec<GenreRow>> {
    let pools = {
        let s = state.read().await;
        s.all_pools()
    };
    let mut merged: Vec<GenreRow> = Vec::new();
    for (_drive, pool) in pools {
        let rows = sqlx::query_as::<_, GenreRow>(
            "SELECT DISTINCT g.id, g.media_type, g.canonical_name, g.translated_name \
             FROM genres g \
             JOIN media_genres mg ON mg.genre_id = g.id AND mg.media_type = g.media_type \
             JOIN media       m  ON m.id = mg.media_id \
             WHERE g.media_type = 'movie' \
               AND m.media_type  = 'movie' \
               AND mg.is_primary = 1 \
             ORDER BY COALESCE(g.translated_name, g.canonical_name) COLLATE NOCASE",
        )
        .fetch_all(&pool)
        .await
        .map_err(|e| AppError::Other(format!("list_movie_genres_in_use: {e}")))?;
        merge_genres(&mut merged, rows);
    }
    merged.sort_by(|a, b| {
        let an = a.translated_name.as_deref().unwrap_or(&a.canonical_name).to_lowercase();
        let bn = b.translated_name.as_deref().unwrap_or(&b.canonical_name).to_lowercase();
        an.cmp(&bn)
    });
    Ok(merged)
}

/// Set a genre's translated name across every registered drive. The
/// translation lives in each drive's local `genres` table (TMDB ids
/// are global, so the same `genre_id` on two drives refers to the same
/// concept). For each drive that physically has a folder named after
/// the old display name we merge it into the new one. Drives that
/// don't know about this genre id are silently skipped.
#[tauri::command]
pub async fn update_genre_translation(
    state: State<'_, AppState>,
    genre_id: i64,
    translated: Option<String>,
) -> AppResult<()> {
    let pools = {
        let s = state.read().await;
        s.all_pools()
    };

    for (drive, pool) in pools {
        let genres_before = list_genres(&pool, MediaType::Movie).await?;
        let Some(target) = genres_before.iter().find(|g| g.id == genre_id) else {
            // This drive doesn't have a row for that genre yet (no
            // movies linked under it). Nothing to translate or rename.
            continue;
        };
        let old_display = target.display_name().to_string();

        set_genre_translation(&pool, genre_id, MediaType::Movie, translated.as_deref()).await?;

        let genres_after = list_genres(&pool, MediaType::Movie).await?;
        let Some(updated) = genres_after.iter().find(|g| g.id == genre_id) else {
            continue;
        };
        let new_display = updated.display_name().to_string();
        if old_display == new_display {
            continue;
        }

        let movies_label_value =
            get_setting_or(&pool, KEY_MOVIES_LABEL, "Movies").await?;
        let movies_root = drive.join(sanitize_segment(&movies_label_value));
        let from = movies_root.join(sanitize_segment(&old_display));
        let to = movies_root.join(sanitize_segment(&new_display));

        if from.exists() {
            merge_genre_folders(&from, &to)?;
        }
    }
    Ok(())
}

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

/// Reveal `path` in the OS file manager. Accepts a folder *or* a file;
/// when a file is given the manager opens its containing folder. We
/// shell out per-platform instead of pulling in `tauri-plugin-opener`
/// to keep the dependency set small — this command does one thing.
#[tauri::command]
pub async fn open_in_explorer(path: String) -> AppResult<()> {
    let p = std::path::PathBuf::from(&path);
    if !p.exists() {
        return Err(AppError::NotFound(format!("path does not exist: {path}")));
    }

    #[cfg(target_os = "windows")]
    {
        // Explorer.exe is picky: forward-slash paths (which the
        // frontend produces by joining `${hd_root}/${folder_path}`)
        // make it silently fall back to the Documents folder. We
        // normalize on the *string* form because `PathBuf` on Windows
        // preserves whatever separator was used to construct it —
        // the OS-level filesystem APIs accept either, but explorer
        // does not.
        let normalized = path.replace('/', "\\");
        std::process::Command::new("explorer")
            .arg(&normalized)
            .spawn()?;
        return Ok(());
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open").arg(&p).spawn()?;
        return Ok(());
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        // xdg-open opens a file in its associated app, which is the
        // opposite of what we want for a video file. So when `p` is a
        // file, target its parent.
        let target = if p.is_file() {
            p.parent().map(|x| x.to_path_buf()).unwrap_or(p.clone())
        } else {
            p.clone()
        };
        std::process::Command::new("xdg-open").arg(&target).spawn()?;
        Ok(())
    }
}
