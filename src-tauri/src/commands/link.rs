//! Linking, renaming, and poster download commands.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::db::genres::{primary_genre_for, set_media_genres, upsert_genre, GenreRow};
use crate::db::media::{
    find_by_id, find_by_tmdb_id, insert_media, update_folder_path, MediaRow, MediaType, NewMedia,
};
use crate::db::settings::{get_setting_or, KEY_MOVIES_LABEL, KEY_SERIES_LABEL};
use crate::error::{AppError, AppResult};
use crate::organizer::execute::execute_link;
use crate::organizer::naming::{folder_name, sanitize_segment};
use crate::organizer::plan::{plan_link, LinkPlanInput};
use crate::state::AppState;
use crate::tmdb::TmdbClient;

#[derive(Debug, Deserialize)]
pub struct LinkArgs {
    /// Path to the folder containing the uncatalogued video.
    pub source_folder: PathBuf,
    /// File name of the main video (within source_folder).
    pub video_filename: String,
    /// TMDB id of the work to link to.
    pub tmdb_id: i64,
    pub media_type: String,
}

#[derive(Debug, Serialize)]
pub struct LinkResult {
    pub media_id: i64,
    pub folder_path: PathBuf,
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

    let (pool, hd_root, tmdb) = {
        let s = state.read().await;
        let pool = s.db.clone().ok_or_else(|| AppError::Other("DB not initialized".into()))?;
        let hd = s.hd_root.clone().ok_or_else(|| AppError::Other("HD not set".into()))?;
        let t = s.tmdb.clone().ok_or_else(|| AppError::Other("TMDB key not set".into()))?;
        (pool, hd, t)
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

    // Optionally download the poster (best-effort).
    let (poster_local, poster_remote) = if let Some(pp) = poster_path.as_deref() {
        match save_poster(&hd_root, args.tmdb_id, pp).await {
            Ok((local, url)) => (Some(local), Some(url)),
            Err(_) => (None, Some(TmdbClient::poster_url(pp, "w500"))),
        }
    } else {
        (None, None)
    };

    let folder_path_rel = plan
        .target_folder
        .strip_prefix(&hd_root)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| plan.target_folder.to_string_lossy().to_string());

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

    Ok(LinkResult {
        media_id,
        folder_path: plan.target_folder,
    })
}

#[derive(Debug, Deserialize)]
pub struct RenameArgs {
    pub media_id: i64,
    pub new_title: String,
}

#[tauri::command]
pub async fn rename_media(state: State<'_, AppState>, args: RenameArgs) -> AppResult<MediaRow> {
    let (pool, hd_root) = {
        let s = state.read().await;
        let pool = s.db.clone().ok_or_else(|| AppError::Other("DB not initialized".into()))?;
        let hd = s.hd_root.clone().ok_or_else(|| AppError::Other("HD not set".into()))?;
        (pool, hd)
    };

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

    let updated = find_by_id(&pool, row.id).await?.unwrap();
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
