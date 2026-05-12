//! Tauri command handlers exposed to the frontend.

pub mod compress_cmds;
pub mod config_cmds;
pub mod library;
pub mod link;
pub mod tmdb_cmds;

// We don't `pub use <module>::*` at this level: `library` and `link`
// both expose submodules with the same name (`genres`, `episodes`),
// so a glob re-export would be ambiguous. `lib.rs` registers each
// command at its full canonical path, so callers don't need a flat
// re-export anyway.
