//! TMDB-facing commands.

use tauri::State;

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
