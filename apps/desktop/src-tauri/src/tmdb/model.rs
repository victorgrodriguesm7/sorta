//! TMDB DTOs.

use serde::{Deserialize, Serialize};

use crate::db::media::MediaType;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Genre {
    pub id: i64,
    pub name: String,
}

/// Raw element from `/search/multi` (mixed shape).
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct RawSearchResult {
    pub media_type: String,
    pub id: i64,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub original_title: Option<String>,
    #[serde(default)]
    pub original_name: Option<String>,
    #[serde(default)]
    pub release_date: Option<String>,
    #[serde(default)]
    pub first_air_date: Option<String>,
    #[serde(default)]
    pub poster_path: Option<String>,
    #[serde(default)]
    pub genre_ids: Vec<i64>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct SearchResponse {
    pub results: Vec<RawSearchResult>,
}

/// Normalized search result the rest of the app consumes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub media_type: MediaType,
    pub id: i64,
    pub title: String,
    pub original_title: Option<String>,
    pub year: Option<String>,
    pub poster_path: Option<String>,
    pub genre_ids: Vec<i64>,
}

impl SearchResult {
    pub(crate) fn from_raw(r: RawSearchResult) -> Option<Self> {
        let media_type = match r.media_type.as_str() {
            "movie" => MediaType::Movie,
            "tv" => MediaType::Tv,
            _ => return None, // ignore "person" and unknown
        };
        let title = match media_type {
            MediaType::Movie => r.title.or(r.original_title.clone()),
            MediaType::Tv => r.name.or(r.original_name.clone()),
        };
        let original_title = match media_type {
            MediaType::Movie => r.original_title,
            MediaType::Tv => r.original_name,
        };
        let date = match media_type {
            MediaType::Movie => r.release_date,
            MediaType::Tv => r.first_air_date,
        };
        Some(Self {
            media_type,
            id: r.id,
            title: title.unwrap_or_else(|| format!("Untitled #{}", r.id)),
            original_title,
            year: date.and_then(|d| d.split('-').next().map(|s| s.to_string())),
            poster_path: r.poster_path,
            genre_ids: r.genre_ids,
        })
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MovieDetails {
    pub id: i64,
    pub title: String,
    pub original_title: Option<String>,
    pub release_date: Option<String>,
    pub runtime: Option<i64>,
    pub poster_path: Option<String>,
    #[serde(default)]
    pub genres: Vec<Genre>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TvDetails {
    pub id: i64,
    pub name: String,
    pub original_name: Option<String>,
    pub first_air_date: Option<String>,
    #[serde(default)]
    pub episode_run_time: Vec<i64>,
    pub poster_path: Option<String>,
    #[serde(default)]
    pub genres: Vec<Genre>,
}

impl TvDetails {
    /// First episode runtime as a single value, useful for the UI.
    pub fn primary_runtime(&self) -> Option<i64> {
        self.episode_run_time.first().copied()
    }
}

/// One element of `/tv/{id}/season/{n}` -> `episodes`. Only the fields
/// the linker actually persists are pulled out; TMDB returns much more
/// that we deliberately drop on the floor (vote_average, crew, …).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TmdbEpisode {
    pub episode_number: i64,
    pub season_number: i64,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub overview: Option<String>,
    #[serde(default)]
    pub air_date: Option<String>,
    /// In minutes. TMDB occasionally returns 0 for unaired episodes —
    /// callers should treat 0 as "unknown" and store NULL.
    #[serde(default)]
    pub runtime: Option<i64>,
    /// Relative path, e.g. `/abc.jpg`. NULL on episodes TMDB has no
    /// still for (very common on recent or obscure seasons).
    #[serde(default)]
    pub still_path: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SeasonDetails {
    pub season_number: i64,
    #[serde(default)]
    pub episodes: Vec<TmdbEpisode>,
}
