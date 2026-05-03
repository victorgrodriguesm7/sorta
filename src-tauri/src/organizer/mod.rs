//! Pure organization logic: folder/file naming, classification, planning.
//!
//! Everything in this module is intentionally side-effect-free so it can
//! be unit-tested without touching the filesystem.

pub mod naming;

pub use naming::*;
