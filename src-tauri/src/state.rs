//! Shared application state, held inside Tauri's `State<>`.

use std::path::PathBuf;
use std::sync::Arc;

use sqlx::SqlitePool;
use tokio::sync::RwLock;

use crate::scanner::watcher::WatcherHandle;
use crate::tmdb::TmdbClient;

/// Mutable state shared across all command invocations.
#[derive(Default)]
pub struct AppStateInner {
    /// Resolved HD root, if the user has chosen one.
    pub hd_root: Option<PathBuf>,
    /// Open DB pool against `<hd_root>/sorta.db`.
    pub db: Option<SqlitePool>,
    /// TMDB client (constructed lazily once api_key + hd_root are set).
    pub tmdb: Option<TmdbClient>,
    /// Live filesystem watcher.
    pub watcher: Option<WatcherHandle>,
}

pub type AppState = Arc<RwLock<AppStateInner>>;

pub fn new_state() -> AppState {
    Arc::new(RwLock::new(AppStateInner::default()))
}
