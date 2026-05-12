//! `recatalog_series` — the actual migration: per-season TMDB fetch,
//! optional in-place rename, optional still download, and one
//! `upsert_episode` per file.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use tauri::State;

use crate::commands::link::{default_true_series, save_episode_still, save_poster, write_target};
use crate::db::episodes::{upsert_episode, NewEpisode};
use crate::db::media::{find_by_id, set_is_new, MediaRow};
use crate::db::settings::{get_setting_or, KEY_SEASON_LABEL};
use crate::error::{AppError, AppResult};
use crate::organizer::naming::sanitize_segment;
use crate::organizer::recatalog::DiscoveredSeason;
use crate::state::AppState;
use crate::tmdb::TmdbEpisode;

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
    use crate::organizer::recatalog::discover_seasons;

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
    if row
        .poster_path
        .as_deref()
        .map_or(true, |rel| !hd_root.join(rel).is_file())
    {
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
    let seasons = discover_seasons(&series_folder, &season_label).map_err(AppError::from)?;

    let mut result = RecatalogResult {
        seasons_processed: 0,
        episodes_processed: 0,
        episodes_renamed: 0,
        stills_downloaded: 0,
        skipped: Vec::new(),
    };

    for season in seasons {
        // One season-details call per season, then index by episode #.
        let by_number: std::collections::HashMap<i64, TmdbEpisode> =
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
            process_recatalog_file(
                &pool,
                &hd_root,
                &row,
                &args,
                &by_number,
                &season,
                file_path,
                &mut result,
            )
            .await?;
        }
    }

    if let Some(flag) = args.set_is_new {
        set_is_new(&pool, row.id, flag).await?;
    }

    crate::manifest::write_best_effort(&hd_root, &pool).await;
    Ok(result)
}

/// Handle one file inside a season folder: parse its SxxExx, rename it
/// to the canonical form (when `args.rename`), optionally fetch the
/// still image, and upsert the `episodes` row. Mutates `result`'s
/// counters so the caller sees the running tallies.
#[allow(clippy::too_many_arguments)]
async fn process_recatalog_file(
    pool: &SqlitePool,
    hd_root: &Path,
    row: &MediaRow,
    args: &RecatalogArgs,
    by_number: &std::collections::HashMap<i64, TmdbEpisode>,
    season: &DiscoveredSeason,
    file_path: &Path,
    result: &mut RecatalogResult,
) -> AppResult<()> {
    use crate::organizer::recatalog::parse_season_episode;

    let filename = match file_path.file_name().and_then(|n| n.to_str()) {
        Some(n) => n.to_string(),
        None => return Ok(()),
    };
    let Some((_parsed_s, parsed_e)) = parse_season_episode(&filename) else {
        tracing::warn!("recatalog: skip un-tagged file {}", filename);
        result.skipped.push(filename);
        return Ok(());
    };
    // Sanity: trust the folder's season number over the filename when
    // they disagree — a folder layout that says "Season 2" but contains
    // an `S01E*` file is almost certainly a misnamed file the user
    // wants normalised.
    let episode_no = parsed_e;
    let season_no = season.season_number;

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
                return Ok(());
            }
            std::fs::rename(file_path, &dest).map_err(AppError::from)?;
            result.episodes_renamed += 1;
        }
        dest
    } else {
        file_path.to_path_buf()
    };

    let still_remote = tmdb_ep
        .and_then(|e| e.still_path.as_deref())
        .map(|p| crate::tmdb::TmdbClient::still_url(p, "w300"));
    let still_local = if args.download_episode_posters {
        if let Some(p) = tmdb_ep.and_then(|e| e.still_path.as_deref()) {
            match save_episode_still(hd_root, row.tmdb_id, season_no, episode_no, p).await {
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
        .strip_prefix(hd_root)
        .map(|p| p.to_string_lossy().to_string())
        .ok();
    let runtime_clean = tmdb_ep.and_then(|e| e.runtime).filter(|m| *m > 0);

    upsert_episode(
        pool,
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
    Ok(())
}
