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
    pub hd_root: Option<PathBuf>,
    pub tmdb_api_key: Option<String>,
    pub ui_language: String,
    pub initialized: bool,
    pub compression_codec: Option<String>,
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
    Ok(ConfigDto {
        hd_root: cfg.hd_root,
        tmdb_api_key: cfg.tmdb_api_key,
        ui_language: cfg.ui_language,
        compression_codec: cfg.compression_codec,
        initialized,
    })
}

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
    cfg.hd_root = Some(path.clone());
    cfg.save(&dir)?;

    initialize_with(app.clone(), state.clone(), cfg.clone()).await?;
    Ok(ConfigDto {
        hd_root: cfg.hd_root,
        tmdb_api_key: cfg.tmdb_api_key,
        ui_language: cfg.ui_language,
        compression_codec: cfg.compression_codec,
        initialized: state.read().await.db.is_some(),
    })
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

    Ok(ConfigDto {
        hd_root: cfg.hd_root,
        tmdb_api_key: cfg.tmdb_api_key,
        ui_language: cfg.ui_language,
        compression_codec: cfg.compression_codec,
        initialized: state.read().await.db.is_some(),
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
        s.hd_root = Some(hd_root);
        s.db = Some(db);
        s.tmdb = tmdb;
        s.watcher = Some(handle);
    }
    Ok(())
}
