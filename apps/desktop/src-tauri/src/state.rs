//! Shared application state, held inside Tauri's `State<>`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use sqlx::SqlitePool;
use tokio::sync::RwLock;

use crate::compress::job::JobRegistry;
use crate::error::{AppError, AppResult};
use crate::scanner::watcher::WatcherHandle;
use crate::tmdb::TmdbClient;

/// Per-drive runtime handles. One entry per registered HD root.
pub struct DriveHandles {
    pub db: SqlitePool,
    pub watcher: WatcherHandle,
}

/// Mutable state shared across all command invocations.
#[derive(Default)]
pub struct AppStateInner {
    /// Primary / "active" drive — the first entry of `cfg.hd_roots`.
    /// Single-drive code paths still read this; the multi-drive
    /// refactor (phase C.2+) is incremental.
    pub hd_root: Option<PathBuf>,
    /// Open pool for the primary drive. Kept as a separate field so
    /// legacy callers (`s.db.clone()`) don't have to learn about the
    /// drive map yet.
    pub db: Option<SqlitePool>,
    /// Watcher tied to the primary drive.
    pub watcher: Option<WatcherHandle>,

    /// Every registered drive, including the primary. Read commands
    /// fan out across this map; write commands look up the row's
    /// `drive_root` to pick the right pool.
    pub drives: HashMap<PathBuf, DriveHandles>,

    /// TMDB client (constructed lazily once api_key + at least one
    /// drive are set). One client serves all drives — TMDB itself is
    /// per-account, not per-library.
    pub tmdb: Option<TmdbClient>,
    /// Active compression jobs (cancellation flags).
    pub jobs: JobRegistry,
}

impl AppStateInner {
    /// Pool for [drive_root]. Returns `NotFound` when the drive isn't
    /// registered. Caller-friendly wrapper around the drive map so
    /// command handlers don't have to construct error messages.
    pub fn pool_for(&self, drive_root: &Path) -> AppResult<SqlitePool> {
        self.drives
            .get(drive_root)
            .map(|d| d.db.clone())
            .ok_or_else(|| {
                AppError::NotFound(format!("drive {} not registered", drive_root.display()))
            })
    }

    /// Snapshot of every registered drive's (root, pool). Used by
    /// read commands that fan out and merge results.
    pub fn all_pools(&self) -> Vec<(PathBuf, SqlitePool)> {
        self.drives
            .iter()
            .map(|(root, handles)| (root.clone(), handles.db.clone()))
            .collect()
    }

    /// Find which registered drive contains `abs_path` by prefix match.
    /// Used by write commands that take an absolute filesystem path
    /// (e.g. `link_media.source_folder`) and need to route to the
    /// owning pool. Longest-match wins so a drive registered as
    /// `D:\Movies\Sub` beats `D:\Movies` for paths inside the subdir.
    pub fn drive_for_path(&self, abs_path: &Path) -> Option<PathBuf> {
        self.drives
            .keys()
            .filter(|root| abs_path.starts_with(root))
            .max_by_key(|root| root.as_os_str().len())
            .cloned()
    }
}

pub type AppState = Arc<RwLock<AppStateInner>>;

pub fn new_state() -> AppState {
    Arc::new(RwLock::new(AppStateInner::default()))
}
