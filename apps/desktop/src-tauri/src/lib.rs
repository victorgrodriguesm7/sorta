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
pub mod compress;
pub mod config;
pub mod db;
pub mod error;
pub mod manifest;
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
            commands::config_cmds::remove_hd_root,
            commands::config_cmds::set_api_key,
            commands::config_cmds::set_ui_language,
            commands::config_cmds::set_compression_codec,
            commands::config_cmds::backup_database,
            // The `library` module is split into submodules (listing,
            // genres, poster, labels, explorer). `tauri::generate_handler!`
            // needs the canonical module path of each command — the hidden
            // `__cmd__<name>` items the macro generates live next to the
            // function definition, so `pub use` re-exports at the
            // `library::mod` level aren't enough.
            commands::library::listing::scan_now,
            commands::library::listing::list_movies_by_genre,
            commands::library::listing::list_movies_by_genres,
            commands::library::poster::get_poster_url,
            commands::library::listing::list_series,
            commands::library::genres::list_movie_genres,
            commands::library::genres::list_movie_genres_in_use,
            commands::library::genres::update_genre_translation,
            commands::library::labels::update_root_label,
            commands::library::explorer::open_in_explorer,
            commands::tmdb_cmds::tmdb_search,
            commands::tmdb_cmds::tmdb_get_movie,
            commands::tmdb_cmds::tmdb_get_tv,
            commands::tmdb_cmds::tmdb_list_genres,
            commands::tmdb_cmds::tmdb_sync_genres,
            commands::link::link_media,
            commands::link::rename_media,
            commands::link::list_media_genres,
            commands::link::reorder_media_genres,
            commands::link::link_as_series,
            commands::link::list_episodes,
            commands::link::plan_recatalog_series,
            commands::link::recatalog_series,
            commands::link::set_media_is_new,
            commands::link::update_season_label,
            commands::link::unlink_media,
            commands::compress_cmds::ffmpeg_status,
            commands::compress_cmds::media_total_bytes,
            commands::compress_cmds::generate_compression_preview,
            commands::compress_cmds::start_compression,
            commands::compress_cmds::cancel_compression,
            commands::compress_cmds::cleanup_originals_for,
            commands::compress_cmds::has_original_backups,
            commands::compress_cmds::discard_preview_dir,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
