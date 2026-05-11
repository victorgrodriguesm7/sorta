//! Config + initial-setup commands.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
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

    if was_primary {
        // The active pool now points at a drive we just unregistered;
        // tear it down and re-initialise against the new primary (if
        // any). Doing this synchronously means the next command sees
        // a consistent state.
        {
            let mut s = state.write().await;
            s.hd_root = None;
            s.db = None;
            s.watcher = None;
        }
        if cfg.hd_root.is_some() {
            initialize_with(app.clone(), state.clone(), cfg.clone()).await?;
        }
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

/// Try to initialize app state (db + watcher + tmdb client) from a config.
/// Called on startup and whenever HD root changes. Returns `Ok(())` even
/// if nothing could be initialized (e.g. no HD root set yet).
pub async fn initialize_with(
    app: AppHandle,
    state: State<'_, AppState>,
    cfg: UserConfig,
) -> AppResult<()> {
    let Some(hd_root) = cfg.hd_root.clone() else {
        return Ok(());
    };
    if !hd_root.exists() {
        return Err(AppError::InvalidPath(format!(
            "configured HD root {} no longer exists",
            hd_root.display()
        )));
    }

    let db = db::open(&hd_root.join("sorta.db")).await?;

    // Wire the filesystem watcher → emit "library-changed" Tauri event.
    let (tx, mut rx) = mpsc::unbounded_channel::<ChangeEvent>();
    let handle = watch(&hd_root, tx)?;
    let app_for_events = app.clone();
    tokio::spawn(async move {
        while let Some(_ev) = rx.recv().await {
            let _ = app_for_events.emit("library-changed", ());
        }
    });

    let tmdb = cfg.tmdb_api_key.as_ref().map(|k| TmdbClient::new(k.clone()));

    {
        let mut s = state.write().await;
        s.hd_root = Some(hd_root.clone());
        s.db = Some(db.clone());
        s.tmdb = tmdb;
        s.watcher = Some(handle);
    }

    // Refresh the manifest companion file alongside sorta.db so any
    // external reader (TV-side client) sees a current snapshot.
    crate::manifest::write_best_effort(&hd_root, &db).await;

    Ok(())
}
