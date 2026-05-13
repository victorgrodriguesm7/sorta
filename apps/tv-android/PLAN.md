# Sorta TV — implementation plan

Companion to the desktop Sorta app. Read-only catalog viewer that runs
on a TV box the user has plugged the catalogued hard drive into.
Native Kotlin, AndroidX Leanback, classic XML layouts.

This is a scoped plan (matches the style of the root `PLAN.md`). Read
[`docs/disk-format.md`](../../docs/disk-format.md) before any
data-layer work — that's the contract this app reads against.

---

## 1. Hardware + build target

| | |
|--|--|
| Box | Amlogic-class Android TV box |
| OS  | Android 7.1.1 (API 25), 32-bit userspace, AArch64 kernel |
| GPU | Mali-450 MP (GLES 2.0 only, no Vulkan, no compute) |
| RAM | 2 GB total, ~500 MB free at runtime |
| ABI | `armeabi-v7a` only (single APK) |
| Build | `compileSdk = 35`, `targetSdk = 25`, `minSdk = 21` |
| ADB | Network: `adb connect 192.168.0.7` (no native "Network Debugging" toggle on this box; relies on whichever workaround we land on — see hardware notes in chat history) |

**Why API 25 as target:** keeps us out of forced Scoped Storage
(API 30+) without needing `requestLegacyExternalStorage`. Direct
`/storage/usb*/` paths just work. Lint warnings about old target
are suppressed in `app/build.gradle.kts`.

**Why no Compose, no Flutter:** see the framework discussion summary
in chat history. Mali-450 + 500 MB free means classic XML +
Leanback `BrowseSupportFragment` is the only option that won't
stutter. Compose targets API 26+ and assumes far beefier GPU.

---

## 2. Stack

| Concern | Choice | Reason |
|---|---|---|
| Language | Kotlin 1.9.x | Modern, coroutines, less boilerplate than Java |
| UI framework | AndroidX Leanback (`BrowseSupportFragment`, `SearchSupportFragment`) | Built for TV remotes, free D-pad focus, row-of-cards UI |
| Image loading | Glide 4 | Mature, low-overhead, handles `file://` + content URIs natively |
| DB access | `android.database.sqlite.SQLiteDatabase` directly (no Room) | Tiny, no codegen, opens external DB read-only trivially |
| Async | Kotlin coroutines + `lifecycleScope` | Standard for AndroidX |
| Tests | JUnit 4 (`./gradlew test`) for pure JVM logic; AndroidX Test + Espresso later for instrumented | TDD inner loop must run in seconds, not minutes |
| Build | Gradle 8.10.2 + AGP 8.7.x via wrapper | Pinned for reproducibility |

No DI framework, no architecture library (no Hilt/Dagger, no Room,
no MVVM scaffolding). The app is small enough that a single
`Repository` instance + manual lifecycle scoping wins.

---

## 3. Data flow

```
USB drive plugged into TV box
        │
        ├─ /storage/usb<XXXX>/        ← UsbDriveLocator finds the mount
        │     │
        │     ├─ manifest.json        ← read first, sanity check
        │     ├─ sorta.db             ← SQLiteDatabase.OPEN_READONLY
        │     ├─ poster/<id>.jpg      ← Glide loads via file://
        │     ├─ Movies/<Genre>/<Title> [tmdb-id]/
        │     │     └─ <Title> [tmdb-id].mkv
        │     └─ Series/<Title> [tmdb-id]/
        │           └─ Season N/SXXEYY.mkv
        │
        ↓
   MediaRepository (caches in-memory: genres, media list)
        │
        ↓
   BrowseFragment / SearchFragment (Leanback)
        │
        ↓ on click
   PlaybackIntent.build(file)  →  Intent.ACTION_VIEW (video/*) → external player
```

Single direction, no writes. The only mutable state on disk is the
**SAF tree URI permission grant** (only used as a fallback when
direct path access fails on a non-stock Android image), persisted
via `SharedPreferences`.

---

## 4. Module structure

```
app/src/main/java/dev/sorta/tv/
├── MainActivity.kt              ← thin: routes to BrowseActivity or first-run
├── data/
│   ├── TmdbTagParser.kt         ← pure: parse "[tmdb-id]" from folder names
│   ├── Manifest.kt              ← pure: data class + parse manifest.json
│   ├── SchemaCompat.kt          ← pure: compare on-disk vs known schema_version
│   ├── DiskFormat.kt            ← pure: helpers for "is video file", "is hidden"
│   ├── MediaRow.kt              ← data class mirroring `media` table
│   ├── GenreRow.kt              ← data class mirroring `genres` table
│   └── MediaRepository.kt       ← opens DB read-only, exposes typed queries
├── usb/
│   └── UsbDriveLocator.kt       ← walks /storage/ for a directory containing sorta.db
├── playback/
│   └── PlaybackIntent.kt        ← pure: builds Intent.ACTION_VIEW
└── ui/
    ├── BrowseActivity.kt
    ├── BrowseFragment.kt        ← Leanback rows: Series row + one row per genre
    ├── CardPresenter.kt         ← Glide-loaded poster card
    ├── SearchActivity.kt
    └── SearchFragment.kt        ← Leanback search

app/src/test/java/dev/sorta/tv/
└── …                             ← JVM unit tests for everything in `data/`,
                                     `playback/`, and any pure helper logic

app/src/androidTest/java/dev/sorta/tv/
└── …                             ← instrumented tests: MediaRepository against
                                     a fixture sorta.db, UsbDriveLocator
```

---

## 5. TDD plan

Inner loop: every commit on a pure-Kotlin file ships with a JUnit
test under `app/src/test/`. Run via `./gradlew :app:test` — sub-5s
on a warm daemon.

Pure-JVM testable (no Android dependencies, no instrumentation):

- `TmdbTagParser` — extract numeric id from `Title [tmdb-{id}]`,
  strip the tag back off (mirrors the desktop's
  `strip_tmdb_tag` / `parse_tmdb_id`).
- `Manifest` — `Manifest.fromJson(text: String): Manifest` plus
  field accessors. Tolerates unknown fields, rejects malformed.
- `SchemaCompat` — `isCompatible(onDisk: Int, known: Int): Result`,
  surfaces `OK` / `OnDiskNewer` / `OnDiskOlderTolerated`.
- `DiskFormat` — predicates: `isVideoFile(name)`, `isHiddenForReader(name)`
  (matches `.original.*`, `.compressing.*`, OS junk).
- `PlaybackIntent` — given a `File` + an output sink (we mock the
  Android `Intent` constructor via a thin abstraction), returns the
  expected URI + MIME + flags.

Instrumented (real device / emulator, slower):

- `MediaRepository` — opens a fixture `sorta.db` shipped under
  `androidTest/assets/`, runs canned queries, asserts row shapes.
- `UsbDriveLocator` — needs real `/storage/` walk; can run on
  emulator with a synthetic mount, or be deferred to manual testing
  on the actual box.
- UI smoke (Espresso + Leanback) — only if it pays for itself.

Failure mode for instrumented tests: cycling APK to the box for
every red-green is slow. Keep these to a minimum; rely on JVM tests
for the interesting logic.

---

## 6. Phased roadmap (atomic commits)

Each line below is one commit unless noted. Tests precede impl
where it makes sense.

### Phase 0 — scaffolding
1. ✅ `chore(tv): gradle skeleton (AGP 8.7, Kotlin 1.9, Leanback 1.0)`
   *(uncommitted at handoff time — see "Handover state" below)*

### Phase 1 — pure data layer (JVM tests only)
2. `test+feat(tv): TmdbTagParser pair`
3. `test+feat(tv): Manifest pair (parse + tolerate unknown fields)`
4. `test+feat(tv): SchemaCompat (on-disk vs known)`
5. `test+feat(tv): DiskFormat predicates (is_video, is_hidden)`
6. `test+feat(tv): PlaybackIntent builder`

### Phase 2 — DB layer (instrumented test against fixture)
7. `chore(tv): commit a fixture sorta.db under androidTest/assets/`
   (built by running the desktop app once against an empty HD root,
   linking 2 movies + 1 series)
8. `feat(tv): MediaRepository — open OPEN_READONLY, list genres,
   list movies by genre, list series`

### Phase 3 — USB drive discovery
9. `feat(tv): UsbDriveLocator — walk /storage/usb*, find sorta.db,
   surface a chosen drive root`
10. `feat(tv): SAF fallback — when /storage/ access is denied, prompt
    ACTION_OPEN_DOCUMENT_TREE, copy sorta.db to internal cache,
    persist the tree URI grant`

### Phase 4 — Leanback UI
11. `feat(tv): BrowseActivity + BrowseFragment skeleton (empty rows)`
12. `feat(tv): CardPresenter — Glide-loaded poster card with focus
    highlight`
13. `feat(tv): wire MediaRepository to BrowseFragment — one row per
    translated genre, plus a Series row`
14. `feat(tv): playback intent on click → external player picker`

### Phase 5 — search + polish
15. `feat(tv): SearchActivity (Leanback) over title/original_title`
16. `feat(tv): missing-DB / schema-mismatch error screens`
17. `feat(tv): "no posters" placeholder card art`
18. `chore(tv): release build + manual install instructions`

### Phase 6 — only-if-needed
19. `feat(tv): remember last-opened drive in SharedPreferences`
20. `feat(tv): refresh manifest on resume; warn if stale`
21. `feat(tv): D-pad shortcut — long-press menu = jump-to-letter`

---

## 7. Conventions

- **Atomic commits** — same as the desktop. Test + impl can be split
  across two commits (`test:` then `feat:`) if they're individually
  large.
- **No mutation of the HD.** Read-only SQLite open. Never write a
  file under the user's drive.
- **Hide internal markers** — `.original.*`, `.compressing.*`, the
  `poster/` directory, the OS-junk dirs (see `docs/disk-format.md`).
- **Schema compatibility** — if `manifest.json#schema_version` >
  this app's compile-time constant, refuse to open and tell the user
  to update the TV app. Older on-disk versions are tolerated.
- **No Internet usage** — the TV app must work fully offline. No
  TMDB calls, no analytics. The desktop is the only thing that talks
  to TMDB.

---

## 8. Open questions / risks

- **USB drive permission on the actual Amlogic box.** Confirmed
  Android 7.1.1, so `/storage/usb*/` should work without SAF, but
  vendor builds vary. Plan SAF fallback (commit #10) defensively.
- **Filesystem on the HD.** exFAT is the only safe RW format across
  Windows / Android. Document this in the desktop README too.
- **Player intent compatibility.** `Intent.ACTION_VIEW` with
  `file://` works on API 25 but is officially discouraged from API
  24 onward. We may need a `FileProvider` for stricter players. VLC
  and MX Player accept `file://` directly, so first cut targets them.
- **Glide on Mali-450.** Bitmap pool defaults assume a phone; cap it
  explicitly. Load TMDB w185 for grid cards (≈ 90 KB), w500 only
  for the focused/detail view.
- **APK size.** Stay under 5 MB for fast sideload over `adb install`.
- **No emulator.** The Android emulator's Mali-G doesn't reflect
  Mali-450 behaviour. Test on the real box from commit #11 onward.

---

## 9. Handover state (commit 1, uncommitted as of plan write)

Files staged but **not yet committed** in the working tree:

- `apps/tv-android/build.gradle.kts`
- `apps/tv-android/settings.gradle.kts`
- `apps/tv-android/gradle.properties` (incl. `-Djava.net.preferIPv4Stack=true`
  to work around a JDK 21 + Windows loopback issue)
- `apps/tv-android/gradle/libs.versions.toml`
- `apps/tv-android/gradle/wrapper/gradle-wrapper.properties` (Gradle 8.10.2)
- `apps/tv-android/gradle/wrapper/gradle-wrapper.jar` (downloaded from
  the official `gradle/gradle@v8.10.2` tag)
- `apps/tv-android/gradlew`, `apps/tv-android/gradlew.bat`
- `apps/tv-android/.gitignore`
- `apps/tv-android/app/build.gradle.kts` (compileSdk 35, targetSdk 25,
  abiFilter armeabi-v7a, lint suppressions for `OldTargetApi` /
  `ExpiredTargetSdkVersion`)
- `apps/tv-android/app/proguard-rules.pro` (empty; minify off)
- `apps/tv-android/app/src/main/AndroidManifest.xml`
- `apps/tv-android/app/src/main/java/dev/sorta/tv/MainActivity.kt`
  (placeholder TextView, real UI lands in commit #11)
- `apps/tv-android/app/src/main/res/`:
  - `values/strings.xml`, `values/colors.xml`, `values/themes.xml`,
    `values/ic_launcher_background.xml`
  - `drawable/banner_tv.xml` (vector banner)
  - `drawable/ic_launcher_foreground.xml` (vector mark)
  - `mipmap-anydpi-v26/ic_launcher.xml`, `ic_launcher_round.xml`
  - `mipmap-{m,h,xh,xxh,xxxh}dpi/ic_launcher.png` +
    `ic_launcher_round.png` (PowerShell-generated indigo "S" tiles)
- `apps/tv-android/README.md`

Verified working before plan write:

- `./gradlew --version` reports Gradle 8.10.2 + JDK 21
- Wrapper jar checksum matches a real ZIP archive

Pending verification (left to next session before committing):

- `./gradlew :app:test` — first run will download AGP + Kotlin +
  Leanback transitively. The IPv4 stack JVM arg should fix the
  "Unable to establish loopback connection" error we hit pre-fix.
- `./gradlew :app:assembleDebug` — first APK build.
- `adb install app-debug.apk` to the box.

After verifying, commit the skeleton as **commit 1** of the phased
roadmap above. Then proceed to Phase 1 / commit 2 (`TmdbTagParser`)
TDD-first.
