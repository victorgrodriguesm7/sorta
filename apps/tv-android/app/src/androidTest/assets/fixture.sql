-- Fixture sorta.db generator for MediaRepository instrumented tests.
--
-- Mirrors what the desktop would write after linking 2 movies + 1
-- series. Schema is copied verbatim from
-- apps/desktop/src-tauri/migrations/0001_init.sql so external readers
-- aren't reading Rust source to stay in sync.
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

CREATE TABLE IF NOT EXISTS settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

INSERT INTO settings(key, value) VALUES
    ('movies_folder_label', 'Movies'),
    ('series_folder_label', 'Series'),
    ('season_label', 'Season'),
    ('schema_version', '3');

-- Genres. TMDB ids; movie and tv share the same id space but the
-- canonical_name column is independent per media_type.
INSERT INTO genres(id, media_type, canonical_name, translated_name) VALUES
    (28, 'movie', 'Action',   'Ação'),
    (80, 'movie', 'Crime',    'Crime'),
    (18, 'movie', 'Drama',    'Drama'),
    (18, 'tv',    'Drama',    'Drama');

-- Movie 1: Inception (Action / primary).
INSERT INTO media(id, tmdb_id, media_type, title, original_title, runtime_minutes, poster_path, poster_url, folder_path)
VALUES (1, 27205, 'movie', 'A Origem', 'Inception', 148,
        'poster/27205.jpg', 'https://image.tmdb.org/t/p/w500/27205.jpg',
        'Movies/Ação/A Origem [tmdb-27205]');

INSERT INTO media_genres(media_id, genre_id, media_type, is_primary) VALUES
    (1, 28, 'movie', 1);

-- Movie 2: Cidade de Deus (Crime / primary; also Drama secondary).
INSERT INTO media(id, tmdb_id, media_type, title, original_title, runtime_minutes, poster_path, poster_url, folder_path)
VALUES (2, 598, 'movie', 'Cidade de Deus', 'City of God', 130,
        'poster/598.jpg', 'https://image.tmdb.org/t/p/w500/598.jpg',
        'Movies/Crime/Cidade de Deus [tmdb-598]');

INSERT INTO media_genres(media_id, genre_id, media_type, is_primary) VALUES
    (2, 80, 'movie', 1),
    (2, 18, 'movie', 0);

-- Series: Game of Thrones (Drama).
INSERT INTO media(id, tmdb_id, media_type, title, original_title, runtime_minutes, poster_path, poster_url, folder_path)
VALUES (3, 1399, 'tv', 'Game of Thrones', 'Game of Thrones', 60,
        'poster/1399.jpg', 'https://image.tmdb.org/t/p/w500/1399.jpg',
        'Series/Game of Thrones [tmdb-1399]');

INSERT INTO media_genres(media_id, genre_id, media_type, is_primary) VALUES
    (3, 18, 'tv', 1);
