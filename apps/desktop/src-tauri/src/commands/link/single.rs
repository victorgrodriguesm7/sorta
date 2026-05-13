//! `link_media` — link a single uncatalogued video (movie, or a
//! one-file / one-folder series) to TMDB.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::State;

use super::save_poster;
use crate::db::genres::{set_media_genres, upsert_genre};
use crate::db::media::{find_by_tmdb_id, insert_media, MediaType, NewMedia};
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
