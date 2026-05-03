//! Sorta — backend library.
//!
//! Modules are organized as follows:
//! - [`organizer`]: pure logic for folder/file naming, classification, plan generation
//! - [`scanner`]: filesystem walking & classification (uses [`organizer`])
//! - [`db`]: SQLite (sqlx) models + migrations
//! - [`tmdb`]: TMDB HTTP client
//! - [`config`]: user-level config persistence
//! - [`error`]: shared error type
//!
//! Pure modules are unit-tested; integration tests live alongside the
//! impure ones with `tempfile` / `wiremock`.

pub mod db;
pub mod error;
pub mod organizer;
pub mod scanner;
pub mod tmdb;

#[cfg_attr(not(test), allow(dead_code))]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .invoke_handler(tauri::generate_handler![])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
