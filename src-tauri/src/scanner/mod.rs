//! Filesystem scanning + classification.
//!
//! Only the pure classification helpers live here for now. The actual
//! `walkdir`-driven scanner will be added in Phase 5 alongside the watcher.

pub mod classify;
pub mod entry;
pub mod walker;
pub mod watcher;

pub use classify::*;
pub use entry::*;
pub use walker::*;
pub use watcher::*;
