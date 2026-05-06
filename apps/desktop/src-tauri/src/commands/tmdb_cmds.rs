//! TMDB-facing commands.

use tauri::State;

use crate::db::genres::{list_genres, upsert_genre, GenreRow};
use crate::db::media::MediaType;
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::tmdb::{MovieDetails, SearchResult, TvDetails};

async fn require_client(state: &AppState) -> AppResult<crate::tmdb::TmdbClient> {
    let s = state.read().await;
    s.tmdb
        .clone()
        .ok_or_else(|| AppError::Other("TMDB API key not configured".into()))
}

#[tauri::command]
pub async fn tmdb_search(
    state: State<'_, AppState>,
    query: String,
) -> AppResult<Vec<SearchResult>> {
    let client = require_client(&state).await?;
    client.search_multi(&query).await
}

#[tauri::command]
pub async fn tmdb_get_movie(
    state: State<'_, AppState>,
    id: i64,
) -> AppResult<MovieDetails> {
    let client = require_client(&state).await?;
    client.get_movie(id).await
}

#[tauri::command]
pub async fn tmdb_get_tv(
    state: State<'_, AppState>,
    id: i64,
) -> AppResult<TvDetails> {
    let client = require_client(&state).await?;
    client.get_tv(id).await
}

#[tauri::command]
pub async fn tmdb_list_genres(
    state: State<'_, AppState>,
    media_type: String,
) -> AppResult<Vec<crate::tmdb::Genre>> {
    let mt = MediaType::parse(&media_type)
        .ok_or_else(|| AppError::Other(format!("invalid media_type: {media_type}")))?;
    let client = require_client(&state).await?;
    client.list_genres(mt).await
}

/// Pull TMDB's full genre catalogue for `media_type`, upsert every entry
/// into the local `genres` table (preserving existing translations), and
/// return the merged local list. Used by the genre editor so the user can
/// pick from any TMDB genre — not just the ones already attached to some
/// linked media.
#[tauri::command]
pub async fn tmdb_sync_genres(
    state: State<'_, AppState>,
    media_type: String,
) -> AppResult<Vec<GenreRow>> {
    let mt = MediaType::parse(&media_type)
        .ok_or_else(|| AppError::Other(format!("invalid media_type: {media_type}")))?;

    let (pool, client) = {
        let s = state.read().await;
        let pool = s
            .db
            .clone()
            .ok_or_else(|| AppError::Other("DB not initialized".into()))?;
        let client = s
            .tmdb
            .clone()
            .ok_or_else(|| AppError::Other("TMDB API key not configured".into()))?;
        (pool, client)
    };

    let remote = client.list_genres(mt).await?;
    for g in &remote {
        upsert_genre(&pool, g.id, mt, &g.name).await?;
    }
    list_genres(&pool, mt).await
}
