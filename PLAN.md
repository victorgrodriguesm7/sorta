# Sorta — Implementation Plan

A Tauri 2 + React + TypeScript + Tailwind desktop app that organizes a hard drive of movies and TV shows by genre, using TMDB metadata. Backend in Rust with SQLite (sqlx).

---

## 1. Stack & tooling

| Concern        | Choice                                               |
| -------------- | ---------------------------------------------------- |
| Shell          | Tauri 2                                              |
| Backend        | Rust (stable)                                        |
| DB             | SQLite via `sqlx` (async, compile-time-checked SQL)  |
| HTTP           | `reqwest` (TMDB API)                                 |
| FS watching    | `notify` crate                                       |
| Frontend       | React 18 + TypeScript + Vite                         |
| Styling        | Tailwind CSS                                         |
| State          | Zustand (lightweight) + Tauri events                 |
| i18n           | `i18next` + `react-i18next`                          |
| Package mgr    | pnpm                                                 |
| Tests (Rust)   | built-in `cargo test`, `tokio-test` for async        |
| Tests (TS)     | Vitest + React Testing Library                       |
| Lint/format    | rustfmt + clippy; eslint + prettier                  |

---

## 2. Repository layout

```
sorta/
├── PLAN.md
├── IDEA.md
├── README.md
├── package.json                # pnpm workspace root
├── pnpm-workspace.yaml
├── src/                        # React frontend
│   ├── main.tsx
│   ├── App.tsx
│   ├── components/
│   │   ├── LeftPanel.tsx
│   │   ├── RightPanel.tsx
│   │   ├── SettingsModal.tsx
│   │   └── SearchDialog.tsx
│   ├── hooks/
│   ├── stores/                 # zustand stores
│   ├── lib/                    # tauri command wrappers
│   ├── i18n/
│   │   ├── index.ts
│   │   └── locales/
│   │       └── en-US.json
│   └── styles/
├── src-tauri/
│   ├── Cargo.toml
│   ├── tauri.conf.json
│   ├── build.rs
│   └── src/
│       ├── main.rs
│       ├── lib.rs
│       ├── commands/           # #[tauri::command] handlers
│       ├── db/                 # sqlx models, migrations
│       ├── tmdb/               # TMDB client
│       ├── scanner/            # filesystem scan + watcher
│       ├── organizer/          # rename/move logic
│       ├── config/             # user config persistence
│       └── error.rs
│   └── migrations/             # sqlx migrations
├── tailwind.config.ts
├── tsconfig.json
└── vite.config.ts
```

---

## 3. Domain rules (decided)

- **Media kinds**: `movie` and `tv`. Separate top-level folders per translated label (default `Movies/`, `Series/`).
- **TV series**: organized as a flat list under the `Series/` root — **not** by genre. One folder per series: `Series Title [tmdb-{id}]`. Season subfolders inside are allowed and preserved as-is; we never rename or move episode files in v1.
- **Video extensions**: `.mkv .mp4 .avi .mov .wmv .m4v .webm` (configurable later).
- **Folder format**: `Title [tmdb-{id}]` (Plex/Jellyfin convention). Title is `pt-BR` from TMDB, falling back to original.
- **Inner file rule**: the main video file is renamed to match the folder name (extension preserved).
- **Sidecars**: subtitles (`.srt .ass .ssa .sub .vtt`), `.nfo`, and any file sharing the video's basename are moved/renamed alongside.
- **Multi-video folders**: skip — log and surface in UI as "skipped (multiple videos)".
- **Uncatalogued**: any video file whose parent folder doesn't match `Title [tmdb-{id}]` AND/OR whose `tmdb_id` is not in the DB. **Do not move** uncatalogued files; surface them in the UI under "Uncatalogued".
- **Genre folder**: created from the **primary** genre (TMDB returns genres in order; first = primary). Movie is moved into it on link.
- **Conflict on link**: if target folder name already exists, **abort with error** shown to user.
- **Genre merging via translation**: if user renames two genres to the same label, the underlying folders are physically merged into one. Files moved accordingly.
- **DB location**: `<HD root>/sorta.db`.
- **Posters**: stored in `<HD root>/poster/` as `{tmdb_id}.jpg`. DB stores both local path and the TMDB URL (URL is fallback if local file missing).
- **Auto-scan**: on app start + `notify`-based watcher on the HD root.
- **TMDB API key**: stored in user config (Tauri's app config dir), entered via Settings.

---

## 4. Database schema (sqlx migrations)

```sql
-- 0001_init.sql
CREATE TABLE media (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    tmdb_id         INTEGER NOT NULL,
    media_type      TEXT NOT NULL CHECK (media_type IN ('movie','tv')),
    title           TEXT NOT NULL,
    original_title  TEXT,
    runtime_minutes INTEGER,
    poster_path     TEXT,           -- local relative path inside HD root
    poster_url      TEXT,           -- TMDB URL fallback
    folder_path     TEXT NOT NULL,  -- relative to HD root
    UNIQUE (tmdb_id, media_type)
);

CREATE TABLE genres (
    id              INTEGER PRIMARY KEY,        -- TMDB genre id
    canonical_name  TEXT NOT NULL,              -- English from TMDB
    translated_name TEXT,                       -- user-chosen
    media_type      TEXT NOT NULL CHECK (media_type IN ('movie','tv'))
);

CREATE TABLE media_genres (
    media_id    INTEGER NOT NULL REFERENCES media(id) ON DELETE CASCADE,
    genre_id    INTEGER NOT NULL REFERENCES genres(id),
    is_primary  INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (media_id, genre_id)
);

CREATE TABLE settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
-- seeded keys: 'movies_folder_label', 'series_folder_label'
```

User-level config (NOT in HD DB; lives in Tauri app config dir as `config.json`):

```json
{
  "hd_root": "D:/Movies",
  "tmdb_api_key": "...",
  "ui_language": "en-US"
}
```

---

## 5. Rust modules — responsibilities

- **`config`** — load/save `config.json`; on startup if missing or path invalid → emit event so UI prompts user.
- **`db`** — open/create `sorta.db` at HD root; run migrations; typed query helpers.
- **`tmdb`** — `search_multi`, `get_movie`, `get_tv`, `get_genres(movie|tv)`, image URL builder. Caches genre list.
- **`scanner`** — recursive walk of HD root; classify entries (catalogued / uncatalogued / skipped); reconcile with DB.
- **`organizer`** — pure functions to compute target folder/file names; `link_media` action that performs DB write + rename + move + poster download atomically (with rollback on failure).
- **`watcher`** — `notify` watcher → debounced events → re-scan affected paths → emit Tauri events to frontend.
- **`commands`** — thin Tauri command wrappers exposed to JS:
  - `get_config`, `set_hd_root`, `set_api_key`
  - `scan_now`
  - `list_uncatalogued`, `list_genres`, `list_media_by_genre(genre_id)`
  - `tmdb_search(query)`
  - `link_media(local_path, tmdb_id, media_type)`
  - `rename_media(media_id, new_title)`
  - `update_genre_translation(genre_id, name)`
  - `update_root_label(kind: 'movie'|'tv', label)`

All commands return `Result<T, AppError>` where `AppError` serializes to a typed JSON error.

---

## 6. Frontend layout

- **AppShell** — top bar with Settings cog; two-pane below.
- **LeftPanel** — virtual list:
  - "Uncatalogued (N)" (covers both movies and series)
  - Two collapsible sections: **Movies** (containing genre buckets) and **Series** (a flat list of series — no genre subdivision).
- **RightPanel** — selected media detail: poster, title, TMDB ID, runtime, primary + secondary genres, action buttons (Rename, Link, Search TMDB).
- **SearchDialog** — TMDB search results: poster, title, year, genres, "Confirm" button.
- **SettingsModal** — HD root path, TMDB API key, UI language, root labels (Movies/Series), genre translations grouped by media_type.

Minimalist styling: neutral grays, single accent color, generous spacing, no chrome.

---

## 7. TDD approach — test order (each = atomic commit)

Tests come **before** implementation for each unit.

### Phase 0 — scaffolding
1. `chore: bootstrap tauri 2 + react + ts + tailwind + pnpm`
2. `chore: add cargo workspace, sqlx, reqwest, notify, tokio deps`
3. `chore: configure vitest + rtl`

### Phase 1 — pure logic (Rust)
4. `test(organizer): folder name formatting from title + tmdb id` → impl
5. `test(organizer): sanitize illegal filename chars across windows/linux` → impl
6. `test(organizer): detect uncatalogued from path pattern` → impl
7. `test(organizer): compute sidecar files for a video` → impl
8. `test(organizer): plan rename/move operations (pure, no fs)` → impl
9. `test(scanner): classify directory entries` → impl

### Phase 2 — DB
10. `test(db): migrations run; insert + query media` → impl
11. `test(db): genre translation update + merge detection` → impl
12. `test(db): settings get/set` → impl

### Phase 3 — TMDB client (with mock server via `wiremock`)
13. `test(tmdb): search_multi parses movie + tv results` → impl
14. `test(tmdb): get_movie + get_tv shape` → impl
15. `test(tmdb): genres cached` → impl

### Phase 4 — organizer integration (tempdir-based)
16. `test(organizer): link_media performs rename+move+db write` → impl
17. `test(organizer): link conflict aborts cleanly` → impl
18. `test(organizer): genre rename merges folders` → impl

### Phase 5 — scanner + watcher
19. `test(scanner): full HD walk produces expected catalogued/uncatalogued lists` → impl
20. `feat(watcher): debounced notify integration` (manual test, basic unit)

### Phase 6 — Tauri commands
21. `feat(commands): wire all commands; smoke test via tauri test harness`

### Phase 7 — Frontend
22. `test(ui): LeftPanel renders sections + selection` → impl
23. `test(ui): RightPanel renders media details` → impl
24. `test(ui): SearchDialog flow` → impl
25. `test(ui): SettingsModal validates inputs` → impl
26. `feat(ui): wire to tauri commands; loading/error states`
27. `feat(i18n): en-US locale + scaffolding for additional locales`

### Phase 8 — polish
28. `feat: first-run HD root picker`
29. `feat: poster download + caching`
30. `docs: README with setup + dev instructions`

Each numbered item = one commit. Where a test+impl pair grows large, they may split into two commits (`test: …` then `feat: …`) but stay atomic.

---

## 8. Open items / assumptions to confirm

- **TV folder layout** (confirmed): one folder per series, `Series Title [tmdb-{id}]`. Season subfolders allowed and untouched. Episodes never renamed/moved. TV is **not** subdivided by genre.
- **Scanner performance**: large drives may have thousands of folders. Walk is single-pass + cached; no thumbnail extraction.
- **Atomicity**: rename + DB write wrapped in best-effort rollback; true cross-volume atomicity isn't possible. We never operate cross-volume because everything lives under one HD root.
- **No write outside HD root** — hard rule enforced in organizer.

If the TV assumption (one folder per series, episodes untouched) is wrong, tell me and I'll adjust before Phase 1.
