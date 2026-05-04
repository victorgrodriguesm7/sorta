# Sorta

A small desktop app that organizes a hard drive of movies and TV series using
TMDB metadata. Built with Tauri 2 + React + TypeScript + Tailwind, with a Rust
backend that uses SQLite (sqlx) for local storage and `notify` for live
filesystem watching.

## Features

- Scans a chosen hard drive root and surfaces "uncatalogued" video files.
- Searches TMDB and links a video to a movie/series.
- Renames the folder + the main video to the Plex/Jellyfin
  `Title [tmdb-{id}]` convention; moves matching subtitles/`.nfo` sidecars
  alongside.
- Movies are grouped by primary genre into subfolders under the configurable
  "Movies" root. TV series are stored flat under the configurable "Series"
  root (season subfolders inside are preserved untouched).
- All metadata lives at `<HD root>/sorta.db`; posters live in
  `<HD root>/poster/`. The user-level config (HD path, TMDB API key, UI
  language) lives in the Tauri app config dir.
- Live re-scan on filesystem changes (debounced).
- Genre translations: rename a genre and the on-disk folder is renamed
  too. Renaming two genres to the same name physically merges the folders.

## Stack

- **Backend**: Rust, Tauri 2, sqlx (SQLite), reqwest (TMDB), notify-debouncer-full
- **Frontend**: React 18, TypeScript, Tailwind CSS, Zustand, i18next, Vite
- **Tests**: `cargo test` + `wiremock` + `tempfile` on the Rust side; Vitest +
  React Testing Library on the TS side.

## Prerequisites

- Rust stable (≥ 1.77)
- Node 20+ and pnpm 10+
- A TMDB API key (v3) — create one at <https://www.themoviedb.org/settings/api>

## Development

```bash
pnpm install
pnpm tauri dev
```

The app boots into a "first run" screen. Click *Settings* and:
1. Pick the hard drive root that contains your library.
2. Paste your TMDB API key.

The first scan starts automatically once the HD root is set.

## Tests

```bash
cd src-tauri && cargo test --lib       # ~50+ Rust tests
pnpm test                              # Vitest
```

## Layout produced on disk

```
<HD root>/
├── sorta.db
├── poster/
│   └── 27205.jpg
├── Movies/                          # name configurable
│   ├── Action/                      # primary-genre folders
│   │   └── Inception [tmdb-27205]/
│   │       ├── Inception [tmdb-27205].mkv
│   │       └── Inception [tmdb-27205].en.srt
│   └── Drama/
└── Series/                          # name configurable
    └── Game of Thrones [tmdb-1399]/
        └── Season 1/
            └── (untouched episode files)
```

## Contributing

Atomic commits per feature; tests come before implementation. See `PLAN.md`
for the full breakdown.
