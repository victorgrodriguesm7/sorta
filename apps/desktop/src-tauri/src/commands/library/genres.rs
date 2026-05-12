//! Genre listing + translation commands.

use tauri::State;

use crate::db::genres::{list_genres, set_genre_translation, GenreRow};
use crate::db::media::MediaType;
use crate::db::settings::{get_setting_or, KEY_MOVIES_LABEL};
use crate::error::{AppError, AppResult};
use crate::organizer::execute::merge_genre_folders;
use crate::organizer::naming::sanitize_segment;
use crate::state::AppState;

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

/// Case-insensitive sort by display name (translated > canonical).
fn sort_genres(rows: &mut Vec<GenreRow>) {
    rows.sort_by(|a, b| {
        let an = a
            .translated_name
            .as_deref()
            .unwrap_or(&a.canonical_name)
            .to_lowercase();
        let bn = b
            .translated_name
            .as_deref()
            .unwrap_or(&b.canonical_name)
            .to_lowercase();
        an.cmp(&bn)
    });
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
    sort_genres(&mut merged);
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
    sort_genres(&mut merged);
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
