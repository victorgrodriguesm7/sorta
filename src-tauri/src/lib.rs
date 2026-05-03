//! Sorta — backend library.
//!
//! Modules are organized as follows:
//! - [`organizer`]: pure logic for folder/file naming, classification, plan generation, plus an executor
//! - [`scanner`]: filesystem walking, classification, and a debounced watcher
//! - [`db`]: SQLite (sqlx) models + migrations
//! - [`tmdb`]: TMDB HTTP client
//! - [`config`]: user-level config persistence
//! - [`state`]: shared mutable state held in `tauri::State`
//! - [`commands`]: `#[tauri::command]` handlers exposed to the frontend
//! - [`error`]: shared error type

pub mod commands;
pub mod config;
pub mod db;
pub mod error;
pub mod organizer;
pub mod scanner;
pub mod state;
pub mod tmdb;

use tauri::Manager;

pub fn run() {
    let state = state::new_state();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .manage(state.clone())
        .setup(move |app| {
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                if let Ok(dir) = handle.path().app_config_dir() {
                    if let Ok(cfg) = config::UserConfig::load(&dir) {
                        let st: tauri::State<'_, state::AppState> = handle.state();
                        if let Err(e) =
                            commands::config_cmds::initialize_with(handle.clone(), st, cfg).await
                        {
                            tracing::warn!("startup init failed: {e:?}");
                        }
                    }
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::config_cmds::get_config,
            commands::config_cmds::set_hd_root,
            commands::config_cmds::set_api_key,
            commands::config_cmds::set_ui_language,
            commands::library::scan_now,
            commands::library::list_movies_by_genre,
            commands::library::list_series,
            commands::library::list_movie_genres,
            commands::library::update_genre_translation,
            commands::library::update_root_label,
            commands::tmdb_cmds::tmdb_search,
            commands::tmdb_cmds::tmdb_get_movie,
            commands::tmdb_cmds::tmdb_get_tv,
            commands::tmdb_cmds::tmdb_list_genres,
            commands::link::link_media,
            commands::link::rename_media,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
