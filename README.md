# Sorta

A desktop app that organizes a hard drive of movies and TV series using TMDB
metadata. Designed around the workflow of "manage on the PC, then unplug the
drive and watch on a TV box / NAS / phone" — every piece of state lives on the
hard drive itself, so the desktop app is just an editor.

Built with Tauri 2 + React + TypeScript + Tailwind on the front, Rust + sqlx +
reqwest + notify on the back, and ffmpeg for the optional compression pipeline.

---

## Features

### Cataloging

- Recursive scan of the chosen HD root that ignores OS junk
  (`$RECYCLE.BIN`, `System Volume Information`, `.Trash`, etc.) and tolerates
  inaccessible subtrees instead of aborting the whole walk.
- Surfaces every uncatalogued video file individually, tagged as
  *movie* or *series* candidate based on the parent folder shape.
- TMDB multi-search dialog with poster previews; one click links a file
  to a movie. Idempotent re-link safe.
- **Catalog as series** bulk linker: pick a TMDB show, season number, and
  starting episode #; multi-select episodes (or drag-and-drop / `Ctrl+↑↓`
  to reorder); files are renamed to `S{XX}E{YY}.{ext}` and placed under
  `Series/<Title> [tmdb-id]/Season N/`. Optional rename-off mode keeps
  the original filenames.
- **Unlink** rolls back a link: the on-disk folder is renamed back to a
  bare name (drops the `[tmdb-id]` tag), the DB row is removed, and the
  cached poster is deleted. Files themselves are never deleted.

### Genres

- Per-row genre editor: reorder (first = primary, which determines the
  on-disk folder for movies), add genres from the full TMDB catalogue,
  remove, save.
- User-translated genre names. Renaming "Action" → "Ação" renames the
  folder on disk; translating two TMDB genres to the same display name
  physically merges their folders.
- Movies grouped by primary translated genre in the left panel; only
  genres with at least one linked movie are surfaced.

### Compression

- ffmpeg-backed pipeline. Detects libx265, libx264, NVENC, QSV, and AMF
  hardware encoders at runtime and remembers the user's choice across
  sessions.
- **Side-by-side preview**: extracts a 15 s segment once via stream-copy,
  encodes it at three CRFs, then displays the four clips (Original + 3)
  in a 2×2 grid with a master Play/Seek/Mute control bar so they play
  in lockstep. Each tile shows the projected total final size for the
  whole movie / series, and a fullscreen toggle.
- Per-file flow: encode → ffprobe verify → swap (original is renamed to
  `<name>.original.<ext>`, never deleted). The "Clean up originals"
  button on the right panel removes the backups when the user is happy.
- Live progress + ETA emitted via Tauri events; **Cancel actually kills
  ffmpeg** within ~200 ms.
- 720p downscale toggle + exhaustive (full re-decode) verify toggle.

### Other

- Translatable Movies / Series / Season folder labels — renaming a label
  renames the folders on disk in place.
- Local backup of the SQLite DB via `VACUUM INTO` — Settings →
  "Backup database" → save-file dialog.
- Live filesystem watcher debounces and re-scans on disk changes.
- All UI strings flow through `react-i18next` (en-US ships; pt-BR planned).

---

## On-disk format

The HD root is the source of truth. A complete spec lives at
[`docs/disk-format.md`](docs/disk-format.md) and is the one document an
external reader (e.g. a TV-box client) needs in order to be built
independently of this codebase.

Quick summary:

```
<HD root>/
├── sorta.db                      # SQLite — full catalog state
├── manifest.json                 # quick health check, schema_version
├── poster/<tmdb_id>.jpg
├── <Movies label>/
│   └── <Genre>/<Title> [tmdb-{id}]/
│       └── <Title> [tmdb-{id}].mkv
└── <Series label>/
    └── <Title> [tmdb-{id}]/
        └── <Season label> N/
            └── S01E01.mkv
```

`schema_version` is bumped only when a change to `sorta.db` would break
a reader that didn't know about it. Adding columns / indexes / new
tables is non-breaking; renaming or repurposing existing columns is.

---

## Stack

- **Backend** — Rust, Tauri 2, sqlx (SQLite), reqwest (TMDB),
  notify-debouncer-full, base64, chrono.
- **Frontend** — React 18, TypeScript, Tailwind CSS, Zustand, i18next,
  Vite.
- **Tests** — `cargo test` with `wiremock` (TMDB) and `tempfile` (FS) on
  the Rust side; Vitest + React Testing Library on the TS side. Currently
  87 Rust + 8 TS tests.

---

## Prerequisites

- Rust stable (≥ 1.77)
- Node 20+ and pnpm 10+
- A TMDB API key (v3) — create one at <https://www.themoviedb.org/settings/api>
- **For compression only:** `ffmpeg` and `ffprobe` on `PATH` (any recent
  build with `libx265`; hardware encoders are optional and auto-detected)

---

## Development

All commands run from the **repo root**. The pnpm workspace
(`pnpm-workspace.yaml`) routes scripts through `apps/desktop`.

```bash
pnpm install              # one install at the workspace root
pnpm tauri:dev            # full Tauri dev cycle (Vite + Rust)
```

First boot opens a setup screen. Settings → pick the hard drive root,
paste the TMDB API key. The scan starts automatically.

```bash
# Tests
pnpm test                 # 8 Vitest tests (frontend)
pnpm test:rust            # 87 cargo tests (backend)

# TypeScript typecheck only
pnpm typecheck

# Production build (Vite + cargo build)
pnpm build
```

If you need to drop into the desktop package directly:

```bash
cd apps/desktop && pnpm tauri dev
```

---

## Project layout

Monorepo. The desktop app is the only published artefact today; a TV
client (`apps/tv-android/`, native Kotlin + Leanback) is the next
planned addition. Both apps would share only the on-disk contract
spelled out in [`docs/disk-format.md`](docs/disk-format.md), so they
can evolve independently.

```
sorta/
├── apps/
│   └── desktop/
│       ├── src/                       # React frontend
│       │   ├── components/
│       │   ├── lib/                   # tauri command wrappers, formatters
│       │   ├── stores/                # zustand
│       │   └── i18n/locales/
│       ├── src-tauri/                 # Rust backend
│       │   ├── src/
│       │   │   ├── commands/          # #[tauri::command] handlers
│       │   │   ├── compress/          # ffmpeg helpers + job runner
│       │   │   ├── db/                # sqlx migrations + repos
│       │   │   ├── organizer/         # pure naming/plan/sidecar logic
│       │   │   ├── scanner/           # walker + watcher
│       │   │   ├── tmdb/              # http client
│       │   │   ├── manifest.rs        # <HD>/manifest.json writer
│       │   │   └── ...
│       │   └── migrations/            # 0001_init, 0002_season_label, 0003_schema_version
│       └── package.json               # @sorta/desktop
├── docs/
│   └── disk-format.md                 # external-reader contract
├── package.json                       # workspace root, passthrough scripts
├── pnpm-workspace.yaml
├── PLAN.md
└── README.md
```

The two toolchains (`pnpm`/`vite` for the desktop frontend, `cargo`
for the backend) keep their own build systems — the workspace just
routes pnpm commands and shares a single `node_modules` symlink
forest. No Nx/Turborepo/Bazel needed.

---

## Contributing

Atomic commits per feature; tests come before implementation. See
[`PLAN.md`](PLAN.md) for the original spec.
