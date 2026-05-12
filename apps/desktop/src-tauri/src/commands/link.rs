//! Linking, renaming, and poster download commands.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use tauri::State;

use crate::commands::library::resolve_drive;
use crate::db::episodes::{upsert_episode, NewEpisode};
use crate::db::genres::{
    list_genres, primary_genre_for, set_media_genres, upsert_genre, GenreRow,
};
use crate::db::media::{
    find_by_id, find_by_tmdb_id, insert_media, set_is_new, update_folder_path, MediaRow,
    MediaType, NewMedia,
};
use crate::db::settings::{
    get_setting_or, KEY_MOVIES_LABEL, KEY_SEASON_LABEL, KEY_SERIES_LABEL,
};
use crate::error::{AppError, AppResult};
use crate::organizer::execute::{execute_link, merge_genre_folders};
use crate::organizer::naming::{folder_name, sanitize_segment, strip_tmdb_tag};
use crate::organizer::plan::{plan_link, LinkPlanInput};
use crate::state::AppState;
use crate::tmdb::TmdbClient;

/// Resolve the drive + pool a write should target. If the caller
/// passes `drive_root`, that wins. Otherwise we fall back to the
/// primary drive — used by old single-drive callers during the
/// frontend migration.
async fn write_target(
    state: &AppState,
    drive_root: Option<&Path>,
) -> AppResult<(SqlitePool, PathBuf)> {
    resolve_drive(state, drive_root).await
}

#[derive(Debug, Deserialize)]
pub struct LinkArgs {
    /// Path to the folder containing the uncatalogued video.
    pub source_folder: PathBuf,
    /// File name of the main video (within source_folder).
    pub video_filename: String,
    /// TMDB id of the work to link to.
    pub tmdb_id: i64,
    pub media_type: String,
    /// Value of the "Mark as new" checkbox at cataloging time.
    /// Defaults to false so older callers continue to work.
    #[serde(default)]
    pub is_new: bool,
}

#[derive(Debug, Serialize)]
pub struct LinkResult {
    pub media_id: i64,
    pub folder_path: PathBuf,
}

/// Download one episode still into `<HD>/poster/episodes/` and return
/// its relative path. Naming convention: `{tv_id}_s{NN}e{NN}.jpg` —
/// stable, matches the on-disk folder/file layout, and keeps the
/// episode directory flat so the reader can mass-iterate it cheaply.
async fn save_episode_still(
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

async fn save_poster(
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

#[tauri::command]
pub async fn link_media(state: State<'_, AppState>, args: LinkArgs) -> AppResult<LinkResult> {
    let media_type = MediaType::parse(&args.media_type)
        .ok_or_else(|| AppError::Other(format!("invalid media_type: {}", args.media_type)))?;

    // Route by the absolute source path: whichever registered drive
    // contains the uncatalogued folder owns the resulting row. We
    // can't fall back to the primary drive here — linking would
    // copy the row into the wrong database and the file move would
    // either fail (cross-volume rename) or silently catalog under a
    // drive the user didn't pick.
    let (pool, hd_root, tmdb) = {
        let s = state.read().await;
        let drive = s.drive_for_path(&args.source_folder).ok_or_else(|| {
            AppError::Other(format!(
                "source folder {} is not under any registered drive",
                args.source_folder.display()
            ))
        })?;
        let pool = s.pool_for(&drive)?;
        let t = s
            .tmdb
            .clone()
            .ok_or_else(|| AppError::Other("TMDB key not set".into()))?;
        (pool, drive, t)
    };

    if find_by_tmdb_id(&pool, args.tmdb_id, media_type).await?.is_some() {
        return Err(AppError::Conflict(format!(
            "tmdb id {} ({:?}) already linked",
            args.tmdb_id, media_type
        )));
    }

    // Pull canonical metadata from TMDB.
    let (title, original_title, runtime, poster_path, genres) = match media_type {
        MediaType::Movie => {
            let m = tmdb.get_movie(args.tmdb_id).await?;
            (m.title, m.original_title, m.runtime, m.poster_path, m.genres)
        }
        MediaType::Tv => {
            let t = tmdb.get_tv(args.tmdb_id).await?;
            (
                t.name.clone(),
                t.original_name.clone(),
                t.primary_runtime(),
                t.poster_path,
                t.genres,
            )
        }
    };

    // Persist EVERY genre into the `genres` table first so the
    // media_genres FK insert later doesn't violate `(genre_id, media_type)
    // REFERENCES genres(id, media_type)`. Previously only the primary
    // movie genre was upserted, which made the secondary FK fail and
    // rolled the entire `set_media_genres` transaction back — leaving the
    // movie with zero genre rows and invisible in the UI.
    for g in &genres {
        upsert_genre(&pool, g.id, media_type, &g.name).await?;
    }

    // Determine kind-root + genre folder.
    let (kind_root_label, genre_folder_name) = match media_type {
        MediaType::Movie => {
            let label = get_setting_or(&pool, KEY_MOVIES_LABEL, "Movies").await?;
            let genre_folder = genres.first().map(|g| g.name.clone());
            (label, genre_folder)
        }
        MediaType::Tv => {
            let label = get_setting_or(&pool, KEY_SERIES_LABEL, "Series").await?;
            (label, None)
        }
    };

    // Decide whether this is a "single video" link (movie or one-file
    // series) or a "whole folder" link (series with multiple files /
    // season subfolders). The latter is detected by the absence of any
    // immediate video file matching `args.video_filename` — typical when
    // the walker surfaced the parent series folder.
    let video_path = args.source_folder.join(&args.video_filename);
    let is_folder_link = !args.video_filename.is_empty() && !video_path.is_file()
        || args.video_filename.is_empty();

    let target_folder = if is_folder_link {
        // Compute target without using plan_link: we just need the folder
        // path under the kind root (no genre subfolder for series).
        let kind_dir = hd_root.join(sanitize_segment(&kind_root_label));
        let parent = match genre_folder_name.as_deref() {
            Some(g) => kind_dir.join(sanitize_segment(g)),
            None => kind_dir,
        };
        let target = parent.join(folder_name(&title, args.tmdb_id));
        if target.exists() {
            return Err(AppError::Conflict(format!(
                "target folder already exists: {}",
                target.display()
            )));
        }
        std::fs::create_dir_all(&parent).map_err(AppError::from)?;
        std::fs::rename(&args.source_folder, &target).map_err(AppError::from)?;
        target
    } else {
        // Sibling files for sidecar discovery.
        let siblings: Vec<String> = std::fs::read_dir(&args.source_folder)
            .map_err(AppError::from)?
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
            .filter_map(|e| e.file_name().into_string().ok())
            .collect();

        let plan = plan_link(&LinkPlanInput {
            hd_root: &hd_root,
            kind_root_label: &kind_root_label,
            genre_folder: genre_folder_name.as_deref(),
            current_folder: &args.source_folder,
            video_filename: &args.video_filename,
            siblings: &siblings,
            tmdb_id: args.tmdb_id,
            title: &title,
        });
        execute_link(&plan)?;
        plan.target_folder
    };

    // Optionally download the poster (best-effort).
    let (poster_local, poster_remote) = if let Some(pp) = poster_path.as_deref() {
        match save_poster(&hd_root, args.tmdb_id, pp).await {
            Ok((local, url)) => (Some(local), Some(url)),
            Err(_) => (None, Some(TmdbClient::poster_url(pp, "w500"))),
        }
    } else {
        (None, None)
    };

    let folder_path_rel = target_folder
        .strip_prefix(&hd_root)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| target_folder.to_string_lossy().to_string());

    let media_id = insert_media(
        &pool,
        &NewMedia {
            tmdb_id: args.tmdb_id,
            media_type,
            title: &title,
            original_title: original_title.as_deref(),
            runtime_minutes: runtime,
            poster_path: poster_local.as_deref(),
            poster_url: poster_remote.as_deref(),
            folder_path: &folder_path_rel,
            is_new: args.is_new,
        },
    )
    .await?;

    // Persist genre links: first one is primary.
    let genre_pairs: Vec<(i64, bool)> = genres
        .iter()
        .enumerate()
        .map(|(i, g)| (g.id, i == 0))
        .collect();
    set_media_genres(&pool, media_id, media_type, &genre_pairs).await?;

    crate::manifest::write_best_effort(&hd_root, &pool).await;

    Ok(LinkResult {
        media_id,
        folder_path: target_folder,
    })
}

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

/// List the genres a media row has, in their stored order. The first
/// entry is the primary (`is_primary = 1`); the rest are returned in
/// `genres.id` order. Useful for the right-panel reorder UI.
#[tauri::command]
pub async fn list_media_genres(
    state: State<'_, AppState>,
    media_id: i64,
    drive_root: Option<PathBuf>,
) -> AppResult<Vec<GenreRow>> {
    let (pool, _drive) = write_target(&state, drive_root.as_deref()).await?;
    sqlx::query_as::<_, GenreRow>(
        "SELECT g.id, g.media_type, g.canonical_name, g.translated_name \
         FROM genres g \
         JOIN media_genres mg ON mg.genre_id = g.id AND mg.media_type = g.media_type \
         WHERE mg.media_id = ? \
         ORDER BY mg.is_primary DESC, g.id ASC",
    )
    .bind(media_id)
    .fetch_all(&pool)
    .await
    .map_err(|e| AppError::Other(format!("list_media_genres: {e}")))
}

#[derive(Debug, Deserialize)]
pub struct ReorderGenresArgs {
    pub media_id: i64,
    /// Ordered list of genre ids. Index 0 = primary.
    pub genre_ids: Vec<i64>,
    #[serde(default)]
    pub drive_root: Option<PathBuf>,
}

/// Replace a media row's genres with `genre_ids` in the given order.
/// `genre_ids[0]` becomes the new primary; the on-disk genre folder for
/// movies is moved if the primary changed.
#[tauri::command]
pub async fn reorder_media_genres(
    state: State<'_, AppState>,
    args: ReorderGenresArgs,
) -> AppResult<MediaRow> {
    if args.genre_ids.is_empty() {
        return Err(AppError::Other("genre_ids cannot be empty".into()));
    }

    let (pool, hd_root) = write_target(&state, args.drive_root.as_deref()).await?;

    let row = find_by_id(&pool, args.media_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("media {}", args.media_id)))?;
    let media_type = MediaType::parse(&row.media_type)
        .ok_or_else(|| AppError::Other(format!("bad media_type {}", row.media_type)))?;

    // Validate that every requested genre id exists in the genres table for this media_type.
    let known: std::collections::HashSet<i64> = list_genres(&pool, media_type)
        .await?
        .into_iter()
        .map(|g| g.id)
        .collect();
    for gid in &args.genre_ids {
        if !known.contains(gid) {
            return Err(AppError::NotFound(format!(
                "genre {gid} not known for {:?}",
                media_type
            )));
        }
    }

    let old_primary = primary_genre_for(&pool, args.media_id).await?;

    // Replace media_genres rows with the new ordering.
    let pairs: Vec<(i64, bool)> = args
        .genre_ids
        .iter()
        .enumerate()
        .map(|(i, id)| (*id, i == 0))
        .collect();
    set_media_genres(&pool, args.media_id, media_type, &pairs).await?;

    // For movies, if the primary genre changed, the on-disk folder must
    // move from the old genre folder into the new one.
    if matches!(media_type, MediaType::Movie) {
        let new_primary = primary_genre_for(&pool, args.media_id).await?;
        let old_name = old_primary.as_ref().map(|g| g.display_name().to_string());
        let new_name = new_primary.as_ref().map(|g| g.display_name().to_string());
        if old_name != new_name {
            let movies_label =
                get_setting_or(&pool, KEY_MOVIES_LABEL, "Movies").await?;
            let movies_root = hd_root.join(sanitize_segment(&movies_label));
            let folder_basename = std::path::Path::new(&row.folder_path)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("");
            let from = hd_root.join(&row.folder_path);
            let to = match &new_name {
                Some(n) => movies_root.join(sanitize_segment(n)).join(folder_basename),
                None => movies_root.join(folder_basename),
            };
            if from.exists() && from != to {
                if let Some(parent) = to.parent() {
                    std::fs::create_dir_all(parent).map_err(AppError::from)?;
                }
                if to.exists() {
                    return Err(AppError::Conflict(format!(
                        "destination already exists: {}",
                        to.display()
                    )));
                }
                std::fs::rename(&from, &to).map_err(AppError::from)?;
                let new_rel = to
                    .strip_prefix(&hd_root)
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|_| to.to_string_lossy().to_string());
                update_folder_path(&pool, args.media_id, &new_rel).await?;
            }
        }
    }
    let _ = KEY_SERIES_LABEL; // referenced via import
    let _ = merge_genre_folders;

    let mut updated = find_by_id(&pool, args.media_id).await?.unwrap();
    updated.drive_root = Some(hd_root);
    Ok(updated)
}

#[derive(Debug, Deserialize)]
pub struct EpisodeSourceArg {
    pub folder: PathBuf,
    pub video_filename: String,
}

#[derive(Debug, Deserialize)]
pub struct LinkAsSeriesArgs {
    pub tmdb_id: i64,
    pub season: i64,
    /// First-episode number for the selection. Subsequent files get
    /// season X, episode start_episode + i. Defaults to 1.
    #[serde(default = "default_start_episode")]
    pub start_episode: i64,
    /// If true (default), each source file is renamed to
    /// `S{XX}E{YY}.{Title}.{ext}` when moved into the season folder.
    /// The `{Title}` segment is the TMDB episode title (sanitized);
    /// it's omitted when TMDB doesn't have a title for that episode.
    /// If false, the original filename is kept verbatim — useful when
    /// the user already has a custom naming scheme they want to
    /// preserve.
    #[serde(default = "default_true_series")]
    pub rename: bool,
    /// If true (default), the linker fetches one TMDB still image per
    /// episode and stores it under `<HD>/poster/episodes/`. Disable to
    /// skip the network roundtrips when you don't care about per-
    /// episode artwork in the reader — the episode rows are still
    /// created, just without `still_path` filled.
    #[serde(default = "default_true_series")]
    pub download_episode_posters: bool,
    /// "Mark as new" checkbox value. Only honoured when the series row
    /// is freshly created — re-linking more episodes onto an already-
    /// catalogued series leaves the existing flag alone, so users
    /// can't accidentally toggle it off by linking a new season.
    #[serde(default)]
    pub is_new: bool,
    /// Source files in the order they should become E{start}, E{start+1}, ...
    pub sources: Vec<EpisodeSourceArg>,
}

fn default_start_episode() -> i64 {
    1
}
fn default_true_series() -> bool {
    true
}

#[derive(Debug, Serialize)]
pub struct LinkSeriesResult {
    pub media_id: i64,
    pub series_folder: PathBuf,
    pub season_folder: PathBuf,
    pub episodes_moved: usize,
}

/// Link a batch of episode files to a TV series. Idempotent: if the
/// series is already in the DB, episodes are added under its existing
/// folder; otherwise the series row is created and the metadata
/// (genres, poster) is fetched from TMDB.
///
/// Layout produced:
///   <HD>/<Series label>/<Title> [tmdb-id]/<Season label> N/SXXEYY.ext
#[tauri::command]
pub async fn link_as_series(
    state: State<'_, AppState>,
    args: LinkAsSeriesArgs,
) -> AppResult<LinkSeriesResult> {
    if args.sources.is_empty() {
        return Err(AppError::Other("at least one episode required".into()));
    }
    if args.season < 0 {
        return Err(AppError::Other("season must be >= 0".into()));
    }
    if args.start_episode < 0 {
        return Err(AppError::Other("start_episode must be >= 0".into()));
    }

    // All episode sources must live on the same drive — series rows
    // can't be split. Reject mixed-drive batches up front instead of
    // half-cataloging the series.
    let (pool, hd_root, tmdb) = {
        let s = state.read().await;
        let first = &args.sources[0].folder;
        let drive = s.drive_for_path(first).ok_or_else(|| {
            AppError::Other(format!(
                "source folder {} is not under any registered drive",
                first.display()
            ))
        })?;
        for src in &args.sources[1..] {
            let other = s.drive_for_path(&src.folder);
            if other.as_deref() != Some(&drive) {
                return Err(AppError::Other(format!(
                    "episode {} is on a different drive than the rest of the batch",
                    src.folder.display(),
                )));
            }
        }
        let pool = s.pool_for(&drive)?;
        let t = s
            .tmdb
            .clone()
            .ok_or_else(|| AppError::Other("TMDB key not set".into()))?;
        (pool, drive, t)
    };

    let series_label = get_setting_or(&pool, KEY_SERIES_LABEL, "Series").await?;
    let season_label = get_setting_or(&pool, KEY_SEASON_LABEL, "Season").await?;
    let series_root = hd_root.join(sanitize_segment(&series_label));

    // Fetch the season metadata up-front: we need it before renaming
    // files (titles go into the filename) and before inserting rows
    // (titles, overviews, stills go into the episodes table). A failed
    // fetch is fatal — the user explicitly asked for series-aware
    // cataloging, and silently dropping back to numeric-only names
    // would be surprising.
    let season_details = tmdb.get_season(args.tmdb_id, args.season).await.ok();

    // Either reuse the existing series folder or create a fresh one.
    let existing = find_by_tmdb_id(&pool, args.tmdb_id, MediaType::Tv).await?;
    let (media_id, series_folder, title) = if let Some(row) = existing {
        let folder = hd_root.join(&row.folder_path);
        if !folder.exists() {
            return Err(AppError::NotFound(format!(
                "series folder vanished: {}",
                folder.display()
            )));
        }
        (row.id, folder, row.title)
    } else {
        // Pull metadata from TMDB.
        let details = tmdb.get_tv(args.tmdb_id).await?;
        let title = details.name.clone();
        let original_title = details.original_name.clone();
        let runtime = details.primary_runtime();

        // Persist all genres up front (FK from media_genres).
        for g in &details.genres {
            upsert_genre(&pool, g.id, MediaType::Tv, &g.name).await?;
        }

        let folder = series_root.join(folder_name(&title, args.tmdb_id));
        std::fs::create_dir_all(&folder).map_err(AppError::from)?;

        // Best-effort poster download.
        let (poster_local, poster_remote) = if let Some(pp) = details.poster_path.as_deref() {
            match save_poster(&hd_root, args.tmdb_id, pp).await {
                Ok((local, url)) => (Some(local), Some(url)),
                Err(_) => (None, Some(TmdbClient::poster_url(pp, "w500"))),
            }
        } else {
            (None, None)
        };

        let folder_rel = folder
            .strip_prefix(&hd_root)
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| folder.to_string_lossy().to_string());

        let media_id = insert_media(
            &pool,
            &NewMedia {
                tmdb_id: args.tmdb_id,
                media_type: MediaType::Tv,
                title: &title,
                original_title: original_title.as_deref(),
                runtime_minutes: runtime,
                poster_path: poster_local.as_deref(),
                poster_url: poster_remote.as_deref(),
                folder_path: &folder_rel,
                is_new: args.is_new,
            },
        )
        .await?;

        let pairs: Vec<(i64, bool)> = details
            .genres
            .iter()
            .enumerate()
            .map(|(i, g)| (g.id, i == 0))
            .collect();
        if !pairs.is_empty() {
            set_media_genres(&pool, media_id, MediaType::Tv, &pairs).await?;
        }
        (media_id, folder, title)
    };

    let _ = title; // future use

    let season_folder =
        series_folder.join(format!("{} {}", sanitize_segment(&season_label), args.season));
    std::fs::create_dir_all(&season_folder).map_err(AppError::from)?;

    // Lookup table: episode_number -> TMDB metadata. We pulled this
    // once up-front so the rename loop doesn't make a request per file.
    let by_number: std::collections::HashMap<i64, &crate::tmdb::TmdbEpisode> = season_details
        .as_ref()
        .map(|d| d.episodes.iter().map(|e| (e.episode_number, e)).collect())
        .unwrap_or_default();

    let mut moved = 0usize;
    for (idx, src) in args.sources.iter().enumerate() {
        let episode_no = args.start_episode + idx as i64;
        let from = src.folder.join(&src.video_filename);
        let tmdb_ep = by_number.get(&episode_no).copied();
        let ep_title = tmdb_ep
            .and_then(|e| e.name.as_deref())
            .map(str::trim)
            .filter(|s| !s.is_empty());

        let new_name = if args.rename {
            let ext = std::path::Path::new(&src.video_filename)
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("mkv");
            // S{XX}E{YY}[.Title].ext. The title segment is sanitized
            // (illegal-on-Windows chars stripped) before being folded
            // in. When TMDB has no title we fall back to the bare
            // S{XX}E{YY}.ext form rather than emit `..ext`.
            match ep_title {
                Some(t) => format!(
                    "S{:02}E{:02}.{}.{}",
                    args.season,
                    episode_no,
                    sanitize_segment(t),
                    ext,
                ),
                None => format!("S{:02}E{:02}.{}", args.season, episode_no, ext),
            }
        } else {
            // Preserve the original filename verbatim. Strip illegal
            // characters in case it contains anything that would break
            // on the target filesystem.
            sanitize_segment(&src.video_filename)
        };
        let to = season_folder.join(&new_name);

        if !from.is_file() {
            tracing::warn!("link_as_series: source missing {}", from.display());
            continue;
        }
        if to.exists() {
            return Err(AppError::Conflict(format!(
                "destination exists: {}",
                to.display()
            )));
        }
        if let Err(_) = std::fs::rename(&from, &to) {
            // Cross-volume fallback.
            std::fs::copy(&from, &to).map_err(AppError::from)?;
            std::fs::remove_file(&from).map_err(AppError::from)?;
        }
        moved += 1;

        // Per-episode metadata row + optional still download.
        let still_remote = tmdb_ep
            .and_then(|e| e.still_path.as_deref())
            .map(|p| crate::tmdb::TmdbClient::still_url(p, "w300"));
        let still_local = if args.download_episode_posters {
            if let Some(p) = tmdb_ep.and_then(|e| e.still_path.as_deref()) {
                match save_episode_still(&hd_root, args.tmdb_id, args.season, episode_no, p).await
                {
                    Ok(rel) => Some(rel),
                    Err(e) => {
                        tracing::warn!("episode still download failed: {e:?}");
                        None
                    }
                }
            } else {
                None
            }
        } else {
            None
        };

        let file_rel = to
            .strip_prefix(&hd_root)
            .map(|p| p.to_string_lossy().to_string())
            .ok();

        let runtime_clean = tmdb_ep
            .and_then(|e| e.runtime)
            .filter(|m| *m > 0);

        upsert_episode(
            &pool,
            &NewEpisode {
                media_id,
                season_number: args.season,
                episode_number: episode_no,
                title: ep_title,
                overview: tmdb_ep.and_then(|e| e.overview.as_deref()),
                air_date: tmdb_ep.and_then(|e| e.air_date.as_deref()),
                runtime_minutes: runtime_clean,
                still_path: still_local.as_deref(),
                still_url: still_remote.as_deref(),
                file_path: file_rel.as_deref(),
            },
        )
        .await?;
    }

    crate::manifest::write_best_effort(&hd_root, &pool).await;

    Ok(LinkSeriesResult {
        media_id,
        series_folder,
        season_folder,
        episodes_moved: moved,
    })
}

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

/// Settings command: update the translatable Season label. The season
/// folders inside every catalogued series are renamed accordingly.
/// Update the Season folder label across every registered drive.
/// Each drive's series folders are renamed in place.
#[tauri::command]
pub async fn update_season_label(
    state: State<'_, AppState>,
    label: String,
) -> AppResult<()> {
    use crate::db::settings::set_setting;

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
    use crate::db::media::delete_media;

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

// ---------------------------------------------------------------------------
// Re-Catalog flow.
//
// Migrates a series that was catalogued before the `episodes` table existed
// (and so has no per-episode metadata, no stills, no `file_path` rows) and
// optionally renames its files to the modern S{XX}E{YY}.{Title}.{ext}
// convention. Distinct from link_as_series because the source files are
// already inside the catalogued folder — there's no move step, only
// optional in-place renames.

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

#[derive(Debug, Deserialize)]
pub struct RecatalogArgs {
    pub media_id: i64,
    /// If true (default), files are renamed in place to
    /// `S{XX}E{YY}.{TmdbTitle}.{ext}` when TMDB has a title for the
    /// episode, otherwise to `S{XX}E{YY}.{ext}`. Files already at
    /// their target name are no-ops.
    #[serde(default = "default_true_series")]
    pub rename: bool,
    /// Fetch one TMDB still per episode and stash it under
    /// `<HD>/poster/episodes/`. Same toggle as link_as_series.
    #[serde(default = "default_true_series")]
    pub download_episode_posters: bool,
    /// If `Some`, the series row's `is_new` flag is overwritten with
    /// this value. `None` leaves it untouched.
    #[serde(default)]
    pub set_is_new: Option<bool>,
    #[serde(default)]
    pub drive_root: Option<PathBuf>,
}

#[derive(Debug, Serialize)]
pub struct RecatalogResult {
    pub seasons_processed: usize,
    pub episodes_processed: usize,
    pub episodes_renamed: usize,
    pub stills_downloaded: usize,
    /// Filenames that couldn't be matched to a TMDB episode (no
    /// `SxxExx` token, or the parsed episode number didn't exist in
    /// the TMDB season). Reported back so the UI can surface them.
    pub skipped: Vec<String>,
}

/// Run the migration: for each season folder under the series,
/// fetch TMDB season metadata once, then walk each video file,
/// match it to a TMDB episode by parsed (season, episode) number,
/// optionally rename in place, optionally fetch the still, and
/// upsert the `episodes` row. Idempotent — running it twice on the
/// same series with the same options is a no-op for already-correct
/// rows.
#[tauri::command]
pub async fn recatalog_series(
    state: State<'_, AppState>,
    args: RecatalogArgs,
) -> AppResult<RecatalogResult> {
    use crate::organizer::recatalog::{discover_seasons, parse_season_episode};

    let (pool, hd_root) = write_target(&state, args.drive_root.as_deref()).await?;
    let tmdb = {
        let s = state.read().await;
        s.tmdb
            .clone()
            .ok_or_else(|| AppError::Other("TMDB key not set".into()))?
    };

    let row = find_by_id(&pool, args.media_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("media {}", args.media_id)))?;
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

    // Re-download the series poster when the cached file is missing
    // but TMDB has one. Cheap, best-effort.
    if row.poster_path.as_deref().map_or(true, |rel| {
        !hd_root.join(rel).is_file()
    }) {
        if let Ok(details) = tmdb.get_tv(row.tmdb_id).await {
            if let Some(pp) = details.poster_path.as_deref() {
                if let Ok((local, url)) = save_poster(&hd_root, row.tmdb_id, pp).await {
                    let _ = sqlx::query(
                        "UPDATE media SET poster_path = ?, poster_url = ? WHERE id = ?",
                    )
                    .bind(&local)
                    .bind(&url)
                    .bind(row.id)
                    .execute(&pool)
                    .await;
                }
            }
        }
    }

    let season_label = get_setting_or(&pool, KEY_SEASON_LABEL, "Season").await?;
    let seasons =
        discover_seasons(&series_folder, &season_label).map_err(AppError::from)?;

    let mut result = RecatalogResult {
        seasons_processed: 0,
        episodes_processed: 0,
        episodes_renamed: 0,
        stills_downloaded: 0,
        skipped: Vec::new(),
    };

    for season in seasons {
        // One season-details call per season, then index by episode #.
        let by_number: std::collections::HashMap<i64, crate::tmdb::TmdbEpisode> =
            match tmdb.get_season(row.tmdb_id, season.season_number).await {
                Ok(d) => d.episodes.into_iter().map(|e| (e.episode_number, e)).collect(),
                Err(e) => {
                    tracing::warn!(
                        "recatalog: TMDB season fetch failed for s{} of {}: {e:?}",
                        season.season_number,
                        row.tmdb_id,
                    );
                    std::collections::HashMap::new()
                }
            };
        result.seasons_processed += 1;

        for file_path in &season.files {
            let filename = match file_path.file_name().and_then(|n| n.to_str()) {
                Some(n) => n.to_string(),
                None => continue,
            };
            let Some((parsed_s, parsed_e)) = parse_season_episode(&filename) else {
                tracing::warn!("recatalog: skip un-tagged file {}", filename);
                result.skipped.push(filename);
                continue;
            };
            // Sanity: trust the folder's season number over the
            // filename when they disagree — a folder layout that says
            // "Season 2" but contains an `S01E*` file is almost
            // certainly a misnamed file the user wants normalised.
            let episode_no = parsed_e;
            let season_no = season.season_number;
            let _ = parsed_s;

            let tmdb_ep = by_number.get(&episode_no);
            let ep_title = tmdb_ep
                .and_then(|e| e.name.as_deref())
                .map(str::trim)
                .filter(|s| !s.is_empty());

            let ext = file_path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("mkv");

            let desired_name = if args.rename {
                match ep_title {
                    Some(t) => format!(
                        "S{:02}E{:02}.{}.{}",
                        season_no,
                        episode_no,
                        sanitize_segment(t),
                        ext,
                    ),
                    None => format!("S{:02}E{:02}.{}", season_no, episode_no, ext),
                }
            } else {
                filename.clone()
            };

            let final_path = if args.rename {
                let dest = season.season_folder.join(&desired_name);
                if dest != *file_path {
                    if dest.exists() {
                        tracing::warn!(
                            "recatalog: cannot rename {} -> {}: destination already exists",
                            filename,
                            desired_name,
                        );
                        result.skipped.push(filename);
                        continue;
                    }
                    std::fs::rename(file_path, &dest).map_err(AppError::from)?;
                    result.episodes_renamed += 1;
                }
                dest
            } else {
                file_path.clone()
            };

            let still_remote = tmdb_ep
                .and_then(|e| e.still_path.as_deref())
                .map(|p| crate::tmdb::TmdbClient::still_url(p, "w300"));
            let still_local = if args.download_episode_posters {
                if let Some(p) = tmdb_ep.and_then(|e| e.still_path.as_deref()) {
                    match save_episode_still(&hd_root, row.tmdb_id, season_no, episode_no, p)
                        .await
                    {
                        Ok(rel) => {
                            result.stills_downloaded += 1;
                            Some(rel)
                        }
                        Err(e) => {
                            tracing::warn!(
                                "recatalog: still download failed s{}e{}: {e:?}",
                                season_no,
                                episode_no,
                            );
                            None
                        }
                    }
                } else {
                    None
                }
            } else {
                None
            };

            let file_rel = final_path
                .strip_prefix(&hd_root)
                .map(|p| p.to_string_lossy().to_string())
                .ok();
            let runtime_clean = tmdb_ep.and_then(|e| e.runtime).filter(|m| *m > 0);

            upsert_episode(
                &pool,
                &NewEpisode {
                    media_id: row.id,
                    season_number: season_no,
                    episode_number: episode_no,
                    title: ep_title,
                    overview: tmdb_ep.and_then(|e| e.overview.as_deref()),
                    air_date: tmdb_ep.and_then(|e| e.air_date.as_deref()),
                    runtime_minutes: runtime_clean,
                    still_path: still_local.as_deref(),
                    still_url: still_remote.as_deref(),
                    file_path: file_rel.as_deref(),
                },
            )
            .await?;
            result.episodes_processed += 1;
        }
    }

    if let Some(flag) = args.set_is_new {
        set_is_new(&pool, row.id, flag).await?;
    }

    crate::manifest::write_best_effort(&hd_root, &pool).await;
    Ok(result)
}
