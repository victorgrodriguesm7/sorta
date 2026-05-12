//! Re-Catalog flow: migrate a series catalogued before the `episodes`
//! table existed and (optionally) rename its files to the modern
//! `S{XX}E{YY}.{Title}.{ext}` convention. Distinct from
//! `link_as_series` because the source files are already inside the
//! catalogued folder — there's no move step, only optional in-place
//! renames + TMDB metadata fetch.
//!
//! Split into two submodules to keep each piece readable:
//!   - `plan` — discovery + `plan_recatalog_series` command
//!   - `run`  — the `recatalog_series` command + per-file helper

pub mod plan;
pub mod run;
