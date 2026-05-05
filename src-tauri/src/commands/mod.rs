//! Tauri command handlers exposed to the frontend.

pub mod compress_cmds;
pub mod config_cmds;
pub mod library;
pub mod link;
pub mod tmdb_cmds;

pub use compress_cmds::*;
pub use config_cmds::*;
pub use library::*;
pub use link::*;
pub use tmdb_cmds::*;
