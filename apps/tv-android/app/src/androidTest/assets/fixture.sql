-- Fixture sorta.db generator for MediaRepository instrumented tests.
--
-- Mirrors what the desktop writes after a few links + a recatalog
-- pass on schema_version = 4. Schema is copied from the desktop's
-- migrations 0001..0004 so external readers don't have to grep Rust
-- source to stay in sync.
--
-- Regenerate with:
--     sqlite3 sorta.db < fixture.sql

CREATE TABLE IF NOT EXISTS media (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    tmdb_id         INTEGER NOT NULL,
    media_type      TEXT NOT NULL CHECK (media_type IN ('movie', 'tv')),
    title           TEXT NOT NULL,
    original_title  TEXT,
    runtime_minutes INTEGER,
    poster_path     TEXT,
    poster_url      TEXT,
    folder_path     TEXT NOT NULL,
    catalogued_at   TEXT NOT NULL DEFAULT '',
    is_new          INTEGER NOT NULL DEFAULT 0,
    UNIQUE (tmdb_id, media_type)
);

CREATE INDEX IF NOT EXISTS idx_media_folder ON media(folder_path);

CREATE TABLE IF NOT EXISTS genres (
    id              INTEGER NOT NULL,
    media_type      TEXT NOT NULL CHECK (media_type IN ('movie', 'tv')),
    canonical_name  TEXT NOT NULL,
    translated_name TEXT,
    PRIMARY KEY (id, media_type)
);

CREATE TABLE IF NOT EXISTS media_genres (
    media_id   INTEGER NOT NULL REFERENCES media(id) ON DELETE CASCADE,
    genre_id   INTEGER NOT NULL,
    media_type TEXT NOT NULL,
    is_primary INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (media_id, genre_id),
    FOREIGN KEY (genre_id, media_type) REFERENCES genres(id, media_type)
);

CREATE INDEX IF NOT EXISTS idx_media_genres_genre ON media_genres(genre_id, media_type);

CREATE TABLE IF NOT EXISTS episodes (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    media_id        INTEGER NOT NULL REFERENCES media(id) ON DELETE CASCADE,
    season_number   INTEGER NOT NULL,
    episode_number  INTEGER NOT NULL,
    title           TEXT,
    overview        TEXT,
    air_date        TEXT,
    runtime_minutes INTEGER,
    still_path      TEXT,
    still_url       TEXT,
    file_path       TEXT,
    UNIQUE (media_id, season_number, episode_number)
);

CREATE INDEX IF NOT EXISTS idx_episodes_media ON episodes(media_id);

CREATE TABLE IF NOT EXISTS settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

INSERT INTO settings(key, value) VALUES
    ('movies_folder_label', 'Movies'),
    ('series_folder_label', 'Series'),
    ('season_label', 'Season'),
    ('schema_version', '4');

-- Genres. TMDB ids; movie and tv share the same id space but the
-- canonical_name column is independent per media_type.
INSERT INTO genres(id, media_type, canonical_name, translated_name) VALUES
    (28, 'movie', 'Action',   'Ação'),
    (80, 'movie', 'Crime',    'Crime'),
    (18, 'movie', 'Drama',    'Drama'),
    (18, 'tv',    'Drama',    'Drama');

-- Movie 1: Inception. is_new=1, catalogued today → must surface in
-- Recently Added.
INSERT INTO media(id, tmdb_id, media_type, title, original_title, runtime_minutes,
                  poster_path, poster_url, folder_path, catalogued_at, is_new)
VALUES (1, 27205, 'movie', 'A Origem', 'Inception', 148,
        'poster/27205.jpg', 'https://image.tmdb.org/t/p/w500/27205.jpg',
        'Movies/Ação/A Origem [tmdb-27205]',
        strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), 1);

INSERT INTO media_genres(media_id, genre_id, media_type, is_primary) VALUES
    (1, 28, 'movie', 1);

-- Movie 2: Cidade de Deus. is_new=0 (an old favourite, not "new"),
-- catalogued well over a year ago → must NOT surface in Recently
-- Added even though it's a real movie.
INSERT INTO media(id, tmdb_id, media_type, title, original_title, runtime_minutes,
                  poster_path, poster_url, folder_path, catalogued_at, is_new)
VALUES (2, 598, 'movie', 'Cidade de Deus', 'City of God', 130,
        'poster/598.jpg', 'https://image.tmdb.org/t/p/w500/598.jpg',
        'Movies/Crime/Cidade de Deus [tmdb-598]',
        '2024-01-15T10:00:00Z', 0);

INSERT INTO media_genres(media_id, genre_id, media_type, is_primary) VALUES
    (2, 80, 'movie', 1),
    (2, 18, 'movie', 0);

-- Movie 3: Stale-but-flagged. is_new=1 but catalogued 30 days ago
-- (outside the 14-day window) → still skipped. Locks in that BOTH
-- conditions must hold, not just `is_new`.
INSERT INTO media(id, tmdb_id, media_type, title, original_title, runtime_minutes,
                  poster_path, poster_url, folder_path, catalogued_at, is_new)
VALUES (4, 555, 'movie', 'Stale Flag', 'Stale Flag', 100,
        NULL, NULL,
        'Movies/Ação/Stale Flag [tmdb-555]',
        strftime('%Y-%m-%dT%H:%M:%SZ', 'now', '-30 days'), 1);

INSERT INTO media_genres(media_id, genre_id, media_type, is_primary) VALUES
    (4, 28, 'movie', 1);

-- Series: Game of Thrones (Drama). is_new=0.
INSERT INTO media(id, tmdb_id, media_type, title, original_title, runtime_minutes,
                  poster_path, poster_url, folder_path, catalogued_at, is_new)
VALUES (3, 1399, 'tv', 'Game of Thrones', 'Game of Thrones', 60,
        'poster/1399.jpg', 'https://image.tmdb.org/t/p/w500/1399.jpg',
        'Series/Game of Thrones [tmdb-1399]',
        '2024-06-01T12:00:00Z', 0);

INSERT INTO media_genres(media_id, genre_id, media_type, is_primary) VALUES
    (3, 18, 'tv', 1);

-- Two episodes of Game of Thrones, inserted out of natural order to
-- prove `listEpisodes` sorts by (season_number, episode_number).
INSERT INTO episodes(media_id, season_number, episode_number, title, overview,
                     air_date, runtime_minutes, still_path, still_url, file_path)
VALUES
    (3, 1, 2, 'The Kingsroad',
     'Bran faces ' || char(13) || 'consequences.',
     '2011-04-24', 56,
     'poster/episodes/1399_s01e02.jpg',
     'https://image.tmdb.org/t/p/w300/kingsroad.jpg',
     'Series/Game of Thrones [tmdb-1399]/Season 1/S01E02.The Kingsroad.mkv'),
    (3, 1, 1, 'Winter Is Coming',
     'Eddard Stark is torn between his family and an old friend when asked to serve at the side of King Robert Baratheon; Viserys plans to wed his sister to a nomadic warlord in exchange for an army.',
     '2011-04-17', 62,
     NULL,
     'https://image.tmdb.org/t/p/w300/winter.jpg',
     'Series/Game of Thrones [tmdb-1399]/Season 1/S01E01.Winter Is Coming.mkv'),
    (3, 2, 1, 'The North Remembers',
     NULL,
     '2012-04-01', 53,
     NULL, NULL,
     'Series/Game of Thrones [tmdb-1399]/Season 2/S02E01.The North Remembers.mkv');
