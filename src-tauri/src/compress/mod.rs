//! Video re-encoding via ffmpeg.
//!
//! Layout:
//! - [`ffmpeg`] — locate the binaries on PATH, detect hardware encoders,
//!   build argv vectors (pure), parse `-progress` output (pure).
//! - [`preview`] — generate short comparison clips at multiple CRF values.
//! - [`job`] — full-folder encode job with progress + cancellation.

pub mod ffmpeg;
pub mod job;
pub mod preview;

pub use ffmpeg::*;
pub use job::*;
pub use preview::*;
