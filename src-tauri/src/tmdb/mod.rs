//! TMDB API client.
//!
//! Built on `reqwest` with a configurable base URL so tests can inject a
//! `wiremock` server. We use TMDB v3 endpoints with `Bearer` auth so users
//! can supply either a v4 read access token or a v3 API key (passed as
//! `api_key` query param). For simplicity we use the API key style.

pub mod model;

pub use model::*;

use serde::Deserialize;

use crate::db::media::MediaType;
use crate::error::{AppError, AppResult};

const DEFAULT_BASE_URL: &str = "https://api.themoviedb.org/3";
pub const IMG_BASE_URL: &str = "https://image.tmdb.org/t/p";

#[derive(Debug, Clone)]
pub struct TmdbClient {
    base_url: String,
    api_key: String,
    language: String,
    http: reqwest::Client,
}

impl TmdbClient {
    pub fn new(api_key: impl Into<String>) -> Self {
        Self::with_base_url(api_key, DEFAULT_BASE_URL)
    }

    pub fn with_base_url(api_key: impl Into<String>, base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            api_key: api_key.into(),
            language: "pt-BR".to_string(),
            http: reqwest::Client::builder()
                .user_agent("Sorta/0.1")
                .build()
                .expect("reqwest client"),
        }
    }

    pub fn with_language(mut self, lang: impl Into<String>) -> Self {
        self.language = lang.into();
        self
    }

    /// Multi-search across movies and TV. Person results are filtered out.
    pub async fn search_multi(&self, query: &str) -> AppResult<Vec<SearchResult>> {
        let url = format!("{}/search/multi", self.base_url);
        let raw: SearchResponse = self
            .http
            .get(&url)
            .query(&[
                ("api_key", self.api_key.as_str()),
                ("language", self.language.as_str()),
                ("query", query),
                ("include_adult", "false"),
            ])
            .send()
            .await
            .map_err(|e| AppError::Other(format!("tmdb search: {e}")))?
            .error_for_status()
            .map_err(|e| AppError::Other(format!("tmdb search status: {e}")))?
            .json::<SearchResponse>()
            .await
            .map_err(|e| AppError::Other(format!("tmdb search decode: {e}")))?;
        Ok(raw
            .results
            .into_iter()
            .filter_map(SearchResult::from_raw)
            .collect())
    }

    /// Fetch full details (with genres, runtime) for a movie.
    pub async fn get_movie(&self, id: i64) -> AppResult<MovieDetails> {
        let url = format!("{}/movie/{}", self.base_url, id);
        self.http
            .get(&url)
            .query(&[
                ("api_key", self.api_key.as_str()),
                ("language", self.language.as_str()),
            ])
            .send()
            .await
            .map_err(|e| AppError::Other(format!("tmdb get_movie: {e}")))?
            .error_for_status()
            .map_err(|e| AppError::Other(format!("tmdb get_movie status: {e}")))?
            .json::<MovieDetails>()
            .await
            .map_err(|e| AppError::Other(format!("tmdb get_movie decode: {e}")))
    }

    /// Fetch full details for a TV series.
    pub async fn get_tv(&self, id: i64) -> AppResult<TvDetails> {
        let url = format!("{}/tv/{}", self.base_url, id);
        self.http
            .get(&url)
            .query(&[
                ("api_key", self.api_key.as_str()),
                ("language", self.language.as_str()),
            ])
            .send()
            .await
            .map_err(|e| AppError::Other(format!("tmdb get_tv: {e}")))?
            .error_for_status()
            .map_err(|e| AppError::Other(format!("tmdb get_tv status: {e}")))?
            .json::<TvDetails>()
            .await
            .map_err(|e| AppError::Other(format!("tmdb get_tv decode: {e}")))
    }

    /// Fetch the full genre list for a media type. The result is a stable
    /// ordering by id; callers can cache it locally.
    pub async fn list_genres(&self, media_type: MediaType) -> AppResult<Vec<Genre>> {
        let endpoint = match media_type {
            MediaType::Movie => "/genre/movie/list",
            MediaType::Tv => "/genre/tv/list",
        };
        #[derive(Deserialize)]
        struct Resp {
            genres: Vec<Genre>,
        }
        let resp: Resp = self
            .http
            .get(format!("{}{}", self.base_url, endpoint))
            .query(&[
                ("api_key", self.api_key.as_str()),
                ("language", self.language.as_str()),
            ])
            .send()
            .await
            .map_err(|e| AppError::Other(format!("tmdb genres: {e}")))?
            .error_for_status()
            .map_err(|e| AppError::Other(format!("tmdb genres status: {e}")))?
            .json::<Resp>()
            .await
            .map_err(|e| AppError::Other(format!("tmdb genres decode: {e}")))?;
        Ok(resp.genres)
    }

    /// Build a poster URL from a TMDB poster_path. `size` is e.g. `"w500"`.
    pub fn poster_url(poster_path: &str, size: &str) -> String {
        let trimmed = poster_path.trim_start_matches('/');
        format!("{IMG_BASE_URL}/{size}/{trimmed}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn mock_search_body() -> serde_json::Value {
        serde_json::json!({
            "page": 1,
            "results": [
                {
                    "media_type": "movie",
                    "id": 27205,
                    "title": "A Origem",
                    "original_title": "Inception",
                    "release_date": "2010-07-15",
                    "poster_path": "/p.jpg",
                    "genre_ids": [28, 878]
                },
                {
                    "media_type": "tv",
                    "id": 1399,
                    "name": "Game of Thrones",
                    "original_name": "Game of Thrones",
                    "first_air_date": "2011-04-17",
                    "poster_path": "/got.jpg",
                    "genre_ids": [10765, 18]
                },
                {
                    "media_type": "person",
                    "id": 500,
                    "name": "Tom Cruise"
                }
            ],
            "total_pages": 1,
            "total_results": 3
        })
    }

    #[tokio::test]
    async fn search_multi_filters_persons_and_normalizes() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/search/multi"))
            .and(query_param("query", "incep"))
            .respond_with(ResponseTemplate::new(200).set_body_json(mock_search_body()))
            .mount(&server)
            .await;

        let client = TmdbClient::with_base_url("k", server.uri());
        let results = client.search_multi("incep").await.unwrap();
        assert_eq!(results.len(), 2);
        let movie = results
            .iter()
            .find(|r| matches!(r.media_type, MediaType::Movie))
            .unwrap();
        assert_eq!(movie.id, 27205);
        assert_eq!(movie.title, "A Origem");
        assert_eq!(movie.year.as_deref(), Some("2010"));

        let tv = results
            .iter()
            .find(|r| matches!(r.media_type, MediaType::Tv))
            .unwrap();
        assert_eq!(tv.id, 1399);
        assert_eq!(tv.title, "Game of Thrones");
        assert_eq!(tv.year.as_deref(), Some("2011"));
    }

    #[tokio::test]
    async fn get_movie_parses_genres_and_runtime() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/movie/27205"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": 27205,
                "title": "A Origem",
                "original_title": "Inception",
                "release_date": "2010-07-15",
                "runtime": 148,
                "poster_path": "/p.jpg",
                "genres": [
                    {"id": 28, "name": "Ação"},
                    {"id": 878, "name": "Ficção científica"}
                ]
            })))
            .mount(&server)
            .await;

        let client = TmdbClient::with_base_url("k", server.uri());
        let movie = client.get_movie(27205).await.unwrap();
        assert_eq!(movie.runtime, Some(148));
        assert_eq!(movie.genres.len(), 2);
        assert_eq!(movie.genres[0].name, "Ação");
    }

    #[tokio::test]
    async fn get_tv_parses_episode_runtime() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/tv/1399"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": 1399,
                "name": "Game of Thrones",
                "original_name": "Game of Thrones",
                "first_air_date": "2011-04-17",
                "episode_run_time": [60],
                "poster_path": "/got.jpg",
                "genres": [{"id": 18, "name": "Drama"}]
            })))
            .mount(&server)
            .await;

        let client = TmdbClient::with_base_url("k", server.uri());
        let tv = client.get_tv(1399).await.unwrap();
        assert_eq!(tv.episode_run_time, vec![60]);
        assert_eq!(tv.name, "Game of Thrones");
    }

    #[tokio::test]
    async fn list_genres_returns_full_list() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/genre/movie/list"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "genres": [
                    {"id": 28, "name": "Ação"},
                    {"id": 12, "name": "Aventura"}
                ]
            })))
            .mount(&server)
            .await;

        let client = TmdbClient::with_base_url("k", server.uri());
        let genres = client.list_genres(MediaType::Movie).await.unwrap();
        assert_eq!(genres.len(), 2);
        assert_eq!(genres[0].name, "Ação");
    }

    #[test]
    fn poster_url_handles_leading_slash() {
        assert_eq!(
            TmdbClient::poster_url("/abc.jpg", "w500"),
            format!("{IMG_BASE_URL}/w500/abc.jpg")
        );
        assert_eq!(
            TmdbClient::poster_url("abc.jpg", "w500"),
            format!("{IMG_BASE_URL}/w500/abc.jpg")
        );
    }
}
