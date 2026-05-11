-- Per-media "freshness" metadata.
--
-- `catalogued_at` is the UTC timestamp at which the row was inserted
-- (ISO 8601, second precision). Existing rows are backfilled with the
-- current time at migration time — there's no better estimate than
-- "right now, when the user upgraded".
--
-- `is_new` is a user-controlled flag set at cataloging time via the
-- "Mark as new" checkbox. The TV reader uses it to surface a "What's
-- new" row at the top of the browse screen. SQLite has no real bool,
-- so it's stored as 0/1.
ALTER TABLE media ADD COLUMN
    catalogued_at TEXT NOT NULL
    DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'));

ALTER TABLE media ADD COLUMN
    is_new INTEGER NOT NULL DEFAULT 0;

-- Per-episode rows for catalogued TV series. One row per video file
-- moved into the season folder by `link_as_series`. TMDB metadata is
-- pulled from `/tv/{id}/season/{n}` at link time so the reader can
-- display real episode titles + per-episode stills without making any
-- network calls of its own.
CREATE TABLE IF NOT EXISTS episodes (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    media_id        INTEGER NOT NULL REFERENCES media(id) ON DELETE CASCADE,
    season_number   INTEGER NOT NULL,
    episode_number  INTEGER NOT NULL,
    -- TMDB episode title (in the configured UI language). May be NULL
    -- when TMDB doesn't have one yet (e.g. unaired episodes).
    title           TEXT,
    overview        TEXT,
    -- ISO YYYY-MM-DD, NULL if unknown.
    air_date        TEXT,
    runtime_minutes INTEGER,
    -- Local cached still image, relative to HD root. NULL if the user
    -- opted out of per-episode poster download, or download failed.
    still_path      TEXT,
    -- TMDB CDN fallback URL (image.tmdb.org/...).
    still_url       TEXT,
    -- Relative path to the video file inside the HD root, e.g.
    -- "Series/Show [tmdb-1]/Season 1/S01E01.Pilot.mkv". NULL while
    -- the row exists only as TMDB metadata (not currently used).
    file_path       TEXT,
    UNIQUE (media_id, season_number, episode_number)
);

CREATE INDEX IF NOT EXISTS idx_episodes_media ON episodes(media_id);
