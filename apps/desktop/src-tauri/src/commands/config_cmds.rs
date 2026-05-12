//! Config + initial-setup commands.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::mpsc;

use crate::config::UserConfig;
use crate::db;
use crate::error::{AppError, AppResult};
use crate::scanner::watcher::{watch, ChangeEvent};
use crate::state::AppState;
use crate::tmdb::TmdbClient;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigDto {
    /// Every drive the user has registered. Frontend uses this for
    /// the drive manager in Settings and to know whether the library
    /// is multi-drive (≥ 2) for header affordances.
    #[serde(default)]
    pub hd_roots: Vec<PathBuf>,
    /// Primary / "active" drive — `hd_roots[0]`. Kept populated for
    /// callers (initial-setup flow, single-drive code paths) that
    /// pre-date the multi-drive refactor.
    pub hd_root: Option<PathBuf>,
    pub tmdb_api_key: Option<String>,
    pub ui_language: String,
    pub initialized: bool,
    pub compression_codec: Option<String>,
}

fn to_dto(cfg: &UserConfig, initialized: bool) -> ConfigDto {
    ConfigDto {
        hd_roots: cfg.hd_roots.clone(),
        hd_root: cfg.hd_root.clone(),
        tmdb_api_key: cfg.tmdb_api_key.clone(),
        ui_language: cfg.ui_language.clone(),
        compression_codec: cfg.compression_codec.clone(),
        initialized,
    }
}

fn config_dir(app: &AppHandle) -> AppResult<PathBuf> {
    app.path()
        .app_config_dir()
        .map_err(|e| AppError::Other(format!("config dir: {e}")))
}

#[tauri::command]
pub async fn get_config(app: AppHandle, state: State<'_, AppState>) -> AppResult<ConfigDto> {
    let cfg = UserConfig::load(&config_dir(&app)?)?;
    let initialized = state.read().await.db.is_some();
    Ok(to_dto(&cfg, initialized))
}

/// Register [path] as a library drive. Adds it to `hd_roots` if not
/// already present; the first drive added becomes the primary one
/// `initialize_with` opens. Calling this with a path that's already
/// registered is a no-op (the config stays untouched).
///
/// Pre–multi-drive callers (the initial-setup wizard) still hit this
/// command — for them the very first call registers the only drive,
/// matching the old single-drive behaviour exactly.
#[tauri::command]
pub async fn set_hd_root(
    app: AppHandle,
    state: State<'_, AppState>,
    path: PathBuf,
) -> AppResult<ConfigDto> {
    if !path.exists() || !path.is_dir() {
        return Err(AppError::InvalidPath(format!(
            "{} is not a directory",
            path.display()
        )));
    }

    let dir = config_dir(&app)?;
    let mut cfg = UserConfig::load(&dir)?;
    cfg.add_hd_root(path.clone());
    cfg.save(&dir)?;

    initialize_with(app.clone(), state.clone(), cfg.clone()).await?;
    Ok(to_dto(&cfg, state.read().await.db.is_some()))
}

/// Remove [path] from the registered drive list. If the removed drive
/// was the active primary one, the next drive in `hd_roots` (if any)
/// takes over after re-initialization. No-op when [path] isn't
/// registered.
#[tauri::command]
pub async fn remove_hd_root(
    app: AppHandle,
    state: State<'_, AppState>,
    path: PathBuf,
) -> AppResult<ConfigDto> {
    let dir = config_dir(&app)?;
    let mut cfg = UserConfig::load(&dir)?;
    let was_primary = cfg.hd_root.as_deref() == Some(path.as_path());
    cfg.remove_hd_root(&path);
    cfg.save(&dir)?;

    // Drop the unregistered drive's pool + watcher from the runtime
    // map. If it was the active primary, also clear the legacy
    // single-drive fields and reinitialise against the new primary
    // (if any) so subsequent commands see a consistent state.
    {
        let mut s = state.write().await;
        s.drives.remove(&path);
        if was_primary {
            s.hd_root = None;
            s.db = None;
            s.watcher = None;
        }
    }
    if was_primary && cfg.hd_root.is_some() {
        initialize_with(app.clone(), state.clone(), cfg.clone()).await?;
    }
    Ok(to_dto(&cfg, state.read().await.db.is_some()))
}

#[tauri::command]
pub async fn set_api_key(
    app: AppHandle,
    state: State<'_, AppState>,
    api_key: String,
) -> AppResult<ConfigDto> {
    let dir = config_dir(&app)?;
    let mut cfg = UserConfig::load(&dir)?;
    cfg.tmdb_api_key = Some(api_key.clone());
    cfg.save(&dir)?;

    {
        let mut s = state.write().await;
        s.tmdb = Some(TmdbClient::new(api_key));
    }

    Ok(to_dto(&cfg, state.read().await.db.is_some()))
}

#[derive(Debug, serde::Serialize)]
pub struct BackupResult {
    pub destination: PathBuf,
    pub bytes_written: u64,
}

/// Create a clean, compacted backup of the live SQLite DB at
/// `destination`. Uses `VACUUM INTO`, which:
///   - takes a brief shared lock so concurrent reads aren't blocked
///   - produces a single-file copy that's already defragmented
///   - is safe to run while the pool is open
///
/// The destination is created/overwritten atomically (SQLite refuses
/// to write to an existing file, so we delete first if present).
#[tauri::command]
pub async fn backup_database(
    state: State<'_, AppState>,
    destination: PathBuf,
) -> AppResult<BackupResult> {
    let pool = {
        let s = state.read().await;
        s.db.clone()
            .ok_or_else(|| AppError::Other("DB not initialized".into()))?
    };
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent).map_err(AppError::from)?;
    }
    if destination.exists() {
        std::fs::remove_file(&destination).map_err(AppError::from)?;
    }

    // VACUUM INTO doesn't accept a bound parameter on most SQLite
    // versions — the path must be a string literal. We escape the
    // user-supplied path defensively.
    let dest_str = destination.to_string_lossy().replace('\'', "''");
    let sql = format!("VACUUM INTO '{dest_str}'");
    sqlx::query(&sql)
        .execute(&pool)
        .await
        .map_err(|e| AppError::Other(format!("vacuum into: {e}")))?;

    let bytes = std::fs::metadata(&destination).map(|m| m.len()).unwrap_or(0);
    Ok(BackupResult {
        destination,
        bytes_written: bytes,
    })
}

/// Persist the user's preferred compression encoder so the dialog
/// doesn't auto-pick a different one (e.g. NVENC) on the next launch.
#[tauri::command]
pub async fn set_compression_codec(
    app: AppHandle,
    _state: State<'_, AppState>,
    codec: String,
) -> AppResult<()> {
    let dir = config_dir(&app)?;
    let mut cfg = UserConfig::load(&dir)?;
    cfg.compression_codec = Some(codec);
    cfg.save(&dir)?;
    Ok(())
}

#[tauri::command]
pub async fn set_ui_language(
    app: AppHandle,
    _state: State<'_, AppState>,
    language: String,
) -> AppResult<()> {
    let dir = config_dir(&app)?;
    let mut cfg = UserConfig::load(&dir)?;
    cfg.ui_language = language;
    cfg.save(&dir)?;
    Ok(())
}

/// Try to initialize app state (db + watcher + tmdb client) for every
/// drive in [cfg.hd_roots]. Called on startup and whenever the drive
/// list changes. Returns `Ok(())` even if no drives are configured
/// yet — the frontend's initial-setup flow handles that case.
///
/// Each drive that opens successfully gets a pool + filesystem
/// watcher in `state.drives`. The first registered drive also lights
/// up the legacy `state.hd_root` / `state.db` / `state.watcher`
/// fields so single-drive command paths keep working.
pub async fn initialize_with(
    app: AppHandle,
    state: State<'_, AppState>,
    cfg: UserConfig,
) -> AppResult<()> {
    if cfg.hd_roots.is_empty() {
        return Ok(());
    }

    // Open every drive. A missing drive (USB unplugged, network share
    // disconnected) is skipped with a warning rather than aborting the
    // whole init — otherwise one stale entry in the registered list
    // would lock the user out of the settings UI where they could
    // remove it. Drives that exist but fail to open (corrupt DB, etc.)
    // still bubble up.
    let mut handles: Vec<(PathBuf, crate::state::DriveHandles, SqlitePool)> = Vec::new();
    for root in &cfg.hd_roots {
        if !root.exists() {
            tracing::warn!(
                "skipping registered HD root {} — not currently mounted",
                root.display()
            );
            continue;
        }
        let db = db::open(&root.join("sorta.db")).await?;
        let (tx, mut rx) = mpsc::unbounded_channel::<ChangeEvent>();
        let watcher = watch(root, tx)?;
        // One emitter task per drive. The "library-changed" event is
        // un-tagged today — phase C.4 may add the drive root once the
        // frontend wants per-drive refresh.
        let app_for_events = app.clone();
        tokio::spawn(async move {
            while let Some(_ev) = rx.recv().await {
                let _ = app_for_events.emit("library-changed", ());
            }
        });
        handles.push((root.clone(), crate::state::DriveHandles { db: db.clone(), watcher }, db));
    }

    let tmdb = cfg.tmdb_api_key.as_ref().map(|k| TmdbClient::new(k.clone()));

    // Pick the first drive in `cfg.hd_roots` that actually opened —
    // not necessarily `hd_roots[0]`, because that one may have been
    // skipped above. If nothing opened (every registered drive is
    // currently unmounted), leave the runtime state empty so the UI
    // can render its "no drive" screen.
    let primary = handles.first().map(|(root, _, db)| (root.clone(), db.clone()));

    {
        let mut s = state.write().await;
        s.drives.clear();
        for (root, drive_handles, _db) in handles {
            s.drives.insert(root, drive_handles);
        }
        match primary {
            Some((root, db)) => {
                s.hd_root = Some(root);
                s.db = Some(db);
            }
            None => {
                s.hd_root = None;
                s.db = None;
            }
        }
        s.watcher = None; // Lives inside `drives[primary]` now.
        s.tmdb = tmdb;
    }

    // Refresh the manifest companion file alongside sorta.db on every
    // drive that opened so external readers see a current snapshot.
    let mounted_roots: Vec<PathBuf> = {
        let s = state.read().await;
        s.drives.keys().cloned().collect()
    };
    for root in mounted_roots {
        if let Ok(pool) = state.read().await.pool_for(&root) {
            crate::manifest::write_best_effort(&root, &pool).await;
        }
    }

    Ok(())
}
