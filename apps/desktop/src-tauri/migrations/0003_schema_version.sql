-- Schema version marker. Read by external clients (e.g. the planned
-- TV reader app) so they can refuse to open a DB whose schema is
-- newer than they understand. The value is set/refreshed by the Rust
-- code on every successful migration run; this insert just makes
-- sure the row exists for the very first boot.
INSERT OR IGNORE INTO settings(key, value) VALUES ('schema_version', '3');
-- The row is overwritten by `db::open` to match CURRENT_SCHEMA_VERSION
-- on every boot, so the literal `3` here is only relevant on a *very*
-- first migration run before that overwrite happens. Subsequent
-- migrations bump CURRENT_SCHEMA_VERSION in Rust, not in this file.
