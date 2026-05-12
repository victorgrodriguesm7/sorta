//! `link_as_series` — batch-link several episode files to a TV series,
//! creating the series row if needed.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::State;

use super::{default_true_series, save_episode_still};
use crate::db::episodes::{upsert_episode, NewEpisode};
use crate::db::settings::{get_setting_or, KEY_SEASON_LABEL};
use crate::error::{AppError, AppResult};
use crate::organizer::naming::sanitize_segment;
use crate::state::AppState;
use crate::tmdb::TmdbClient;

mod bootstrap;

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

    let season_label = get_setting_or(&pool, KEY_SEASON_LABEL, "Season").await?;

    // Fetch the season metadata up-front: we need it before renaming
    // files (titles go into the filename) and before inserting rows
    // (titles, overviews, stills go into the episodes table). A failed
    // fetch is non-fatal — we fall back to numeric-only naming.
    let season_details = tmdb.get_season(args.tmdb_id, args.season).await.ok();

    // Either reuse the existing series folder or create a fresh one
    // (TMDB fetch, genre upsert, poster download, media row insert).
    let (media_id, series_folder) =
        bootstrap::get_or_create_series_row(&pool, &hd_root, &tmdb, &args).await?;

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
        if std::fs::rename(&from, &to).is_err() {
            // Cross-volume fallback.
            std::fs::copy(&from, &to).map_err(AppError::from)?;
            std::fs::remove_file(&from).map_err(AppError::from)?;
        }
        moved += 1;

        // Per-episode metadata row + optional still download.
        let still_remote = tmdb_ep
            .and_then(|e| e.still_path.as_deref())
            .map(|p| TmdbClient::still_url(p, "w300"));
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

        let runtime_clean = tmdb_ep.and_then(|e| e.runtime).filter(|m| *m > 0);

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
