# Sorta on-disk format

Authoritative spec for everything Sorta writes to the user's hard drive.
External readers (e.g. the planned TV-side Android client) should be
able to build *only* from this document, without reading the desktop
app's source.

Versioning policy is at the [end](#schema-versioning).

---

## Top-level layout

Everything lives under a single user-chosen **HD root** (e.g.
`M:/`, `/media/movies`). The desktop never writes outside it.

```
<HD root>/
├── sorta.db                 # SQLite — the source of truth
├── manifest.json            # quick health-check companion (see below)
├── poster/                  # cached poster images, one per linked media
│   ├── 27205.jpg
│   ├── 1399.jpg
│   └── ...
├── <Movies label>/          # default "Movies" — user-translatable
│   └── <Genre label>/       # default canonical English — user-translatable
│       └── <Title> [tmdb-{id}]/
│           ├── <Title> [tmdb-{id}].mkv
│           ├── <Title> [tmdb-{id}].en.srt        # sidecars (optional)
│           └── <Title> [tmdb-{id}].original.mkv  # compression backup (optional)
└── <Series label>/          # default "Series" — user-translatable
    └── <Title> [tmdb-{id}]/
        └── <Season label> N/   # default "Season" — user-translatable
            ├── S01E01.mkv
            ├── S01E02.mkv
            └── ...
```

The user-facing labels (Movies, Series, Season, and per-genre
display names) are stored in the SQLite `settings` and `genres`
tables — the **physical folder names on disk follow whatever's
currently set there**. Renaming a label causes the desktop to rename
the corresponding folder; readers should never assume the English
defaults.

### Folder convention: `Title [tmdb-{id}]`

The trailing `[tmdb-{id}]` is required for any folder Sorta considers
catalogued. The title is the Brazilian-Portuguese title from TMDB,
falling back to the original title. Illegal Windows filename
characters (`<>:"/\\|?*` plus control codes) are replaced with
spaces and trimmed. Trailing dots and whitespace are stripped.

Regex (case-sensitive, one capture group = numeric id, trailing
whitespace tolerated):

```
.+?\s\[tmdb-(\d+)\]\s*$
```

### Episode filenames

Inside a series' `Season N/` folder, episodes follow `S{XX}E{YY}.{ext}`
where the **extension matches the source file's extension** (Sorta
never changes a file's container format on link).

The user can opt out of renaming when bulk-linking — in that case the
original filename is kept verbatim (lightly sanitised for Windows-
illegal characters). External readers must not assume the `S{XX}E{YY}`
shape and should fall back to the file's basename for display.

### Sidecars

Files that share the main video's basename are considered sidecars
and follow it through every rename. Recognized extensions:

- Subtitles: `.srt .ass .ssa .sub .vtt`
- Metadata:  `.nfo`

Plex-style language tags between the basename and the extension are
preserved across renames: `Movie.mkv` + `Movie.en.srt` →
`Inception [tmdb-27205].mkv` + `Inception [tmdb-27205].en.srt`.

### Special files readers should ignore

- `*.original.<ext>` — pre-compression backup, kept until the user
  clicks "Clean up originals". Readers should hide these.
- `*.compressing.<ext>` — in-flight encode, present only while a
  compression job is running. Hide.
- `<HD>/poster/` — internal cache directory. Don't recurse.
- `<HD>/$RECYCLE.BIN`, `System Volume Information`, `.Trash`,
  `.Trashes`, `.Spotlight-V100`, `.fseventsd`, `lost+found`,
  any folder starting with `$` — OS junk; skip.

---

## `manifest.json`

Written next to `sorta.db` after every catalog mutation (link,
unlink, link-as-series, root/genre/season label change) and on app
boot. Intentionally tiny — a quick health check that doesn't require
opening SQLite.

```json
{
  "schema_version": 3,
  "app_version": "0.1.0",
  "generated_at": "2026-05-15T12:34:56Z",
  "counts": {
    "media_total": 142,
    "movies": 98,
    "series": 44
  }
}
```

Field guarantees:

- `schema_version` — non-negative integer; **mirrors** the
  `settings.schema_version` row. Readers compare it against their
  own compile-time constant (see [versioning](#schema-versioning)).
- `app_version` — semver-ish string from the writer's
  `CARGO_PKG_VERSION`. Informational only.
- `generated_at` — ISO 8601 UTC, second precision, always ends in
  `Z`. Useful for staleness warnings.
- `counts` — trivial stats; informational only.

Readers may encounter additional fields in future versions. They
**must** ignore unknown keys rather than rejecting the file.
Manifest writes are atomic (`*.tmp` → rename). A missing
`manifest.json` is **not** an error — the file is regenerated on
the next desktop boot or mutation.

---

## SQLite schema (`sorta.db`)

Open with `OPEN_READONLY` from external clients. The desktop is the
only writer.

### `media`

One row per linked work (movie OR series — *not* one row per episode).

| column            | type              | notes                                                |
| ----------------- | ----------------- | ---------------------------------------------------- |
| `id`              | INTEGER PK auto   | local row id                                         |
| `tmdb_id`         | INTEGER NOT NULL  | TMDB id; unique per `media_type`                     |
| `media_type`      | TEXT NOT NULL     | `'movie'` or `'tv'` (CHECK constrained)              |
| `title`           | TEXT NOT NULL     | display title (pt-BR with original fallback)         |
| `original_title`  | TEXT              |                                                      |
| `runtime_minutes` | INTEGER           | for TV: first episode runtime if reported            |
| `poster_path`     | TEXT              | path relative to HD root, e.g. `poster/27205.jpg`    |
| `poster_url`      | TEXT              | TMDB CDN fallback if local file is missing           |
| `folder_path`     | TEXT NOT NULL     | path relative to HD root, e.g. `Movies/Action/X [tmdb-1]` |

Index on `folder_path`. Unique on `(tmdb_id, media_type)`.

### `genres`

| column            | type              | notes                                              |
| ----------------- | ----------------- | -------------------------------------------------- |
| `id`              | INTEGER NOT NULL  | TMDB genre id                                      |
| `media_type`      | TEXT NOT NULL     | `'movie'` or `'tv'`                                |
| `canonical_name`  | TEXT NOT NULL     | English from TMDB                                  |
| `translated_name` | TEXT              | user override; if set, this is the display name    |

Primary key `(id, media_type)`. The on-disk genre folder name is
`COALESCE(translated_name, canonical_name)` (sanitised).

### `media_genres`

Many-to-many bridge.

| column       | type             | notes                                              |
| ------------ | ---------------- | -------------------------------------------------- |
| `media_id`   | INTEGER NOT NULL | FK media(id) ON DELETE CASCADE                     |
| `genre_id`   | INTEGER NOT NULL | FK (genre_id, media_type) → genres(id, media_type) |
| `media_type` | TEXT NOT NULL    | redundant for the FK; matches media.media_type     |
| `is_primary` | INTEGER NOT NULL | 1 = the genre that determines the folder location  |

Primary key `(media_id, genre_id)`. Index on `(genre_id, media_type)`.
For movies, exactly **one** row per media should have `is_primary = 1`
— that's the genre folder the file lives under. For TV, no folder is
chosen by genre, but the same `is_primary` rule applies for sorting.

### `settings`

Generic key/value table. Known keys:

| key                    | meaning                                          | default      |
| ---------------------- | ------------------------------------------------ | ------------ |
| `movies_folder_label`  | top-level folder name for movies                 | `Movies`     |
| `series_folder_label`  | top-level folder name for series                 | `Series`     |
| `season_label`         | per-series `Season N` subfolder prefix           | `Season`     |
| `schema_version`       | integer, mirrors `manifest.json#schema_version`  | (see below)  |

External readers may store their own keys with a vendor prefix
(`tv_*`, `kodi_*`, …). The desktop ignores unknown keys.

### `_sqlx_migrations`

Internal sqlx bookkeeping. Don't read or write.

---

## Reading a single linked file

To play a specific media row from an external client:

```text
abs_path = <HD root> / media.folder_path / <video file>
```

For **movies**, the video file inside `folder_path` is named
`<basename>.<ext>` where `<basename>` matches the folder basename
(`Title [tmdb-{id}]`). There is **at most one** `is_video_file()`
file directly inside the folder. Look for the first file whose
extension is in:

```
mkv mp4 avi mov wmv m4v webm
```

For **series**, walk into each `<Season label> N/` subfolder and
list every video file. Filenames are `S{XX}E{YY}.{ext}` if the user
let Sorta rename, otherwise arbitrary. Skip `*.original.*` and
`*.compressing.*`.

To launch playback on Android:

```kotlin
val intent = Intent(Intent.ACTION_VIEW)
    .setDataAndType(Uri.fromFile(file), "video/*")
startActivity(intent)
```

---

## Schema versioning

`schema_version` is bumped only when a change to `sorta.db` would
break a reader that didn't know about it. Adding a row, an index, or
an entirely new table is **not** a breaking change. Removing/renaming
a column or changing the meaning of an existing field is.

Compatibility rule for external readers:

```text
on_disk = settings.schema_version  (or manifest.json.schema_version)
known_to_reader = <constant baked at build time>

if on_disk > known_to_reader:
    refuse to open; tell the user to update the reader
else:
    open OPEN_READONLY and proceed
```

The desktop overwrites `settings.schema_version` with its own
constant on every successful migration run. So if a user downgrades
the desktop, the older binary will rewrite the version *down* to
match its constant — readers should be tolerant of that case too
(it's the same compatibility direction: on-disk ≤ known).

History:

| version | added in commit         | notes                          |
| ------- | ----------------------- | ------------------------------ |
| 1       | initial release         | media/genres/settings tables   |
| 2       | season_label migration  | `season_label` settings key    |
| 3       | schema_version + manifest | this document; no schema break |
