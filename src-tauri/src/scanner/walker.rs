//! Walks the HD root and produces a snapshot of catalogued / uncatalogued /
//! skipped folders. Side-effecting (reads from the filesystem) but not
//! mutating.

use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};

use crate::organizer::naming::parse_tmdb_id;
use crate::scanner::entry::{
    is_poster_cache_dir, is_season_folder_name, is_system_dir,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CataloguedHit {
    pub folder: PathBuf,
    pub tmdb_id: i64,
    pub video_filename: String,
}

/// What sort of uncatalogued item the walker thinks a video file is. The
/// frontend uses this hint to suggest whether the user should link as a
/// movie (single-pick) or as a series (multi-select + "Catalog as series").
///
/// The walker assigns `Series` when the file's containing folder holds
/// 2+ direct videos, OR the file lives inside a folder whose siblings
/// look like season folders. Otherwise `Movie`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UncataloguedKind {
    Movie,
    Series,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UncataloguedHit {
    pub folder: PathBuf,
    pub video_filename: String,
    pub kind: UncataloguedKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkippedHit {
    pub folder: PathBuf,
    pub video_count: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScanReport {
    pub catalogued: Vec<CataloguedHit>,
    pub uncatalogued: Vec<UncataloguedHit>,
    pub skipped: Vec<SkippedHit>,
}

/// Recursively walk `root` and classify every folder it finds.
///
/// Folders matching the catalogued convention are NOT recursed into past
/// the immediate level (their inner structure — e.g. season subfolders —
/// is irrelevant for scanning). Folders without videos are recursed.
pub fn scan(root: &Path) -> AppResult<ScanReport> {
    let mut report = ScanReport::default();
    if !root.exists() {
        return Err(AppError::InvalidPath(format!(
            "{} does not exist",
            root.display()
        )));
    }
    walk(root, &mut report)?;
    report.catalogued.sort_by(|a, b| a.folder.cmp(&b.folder));
    report.uncatalogued.sort_by(|a, b| a.folder.cmp(&b.folder));
    report.skipped.sort_by(|a, b| a.folder.cmp(&b.folder));
    Ok(report)
}

/// Best-effort: first video filename found anywhere under `dir`.
fn first_video_recursive(dir: &Path) -> Option<String> {
    use crate::scanner::classify::is_video_file;
    let entries = fs::read_dir(dir).ok()?;
    let mut subdirs = Vec::new();
    for entry in entries.flatten() {
        let kind = match entry.file_type() {
            Ok(k) => k,
            Err(_) => continue,
        };
        let path = entry.path();
        if kind.is_file() && is_video_file(&path) {
            return path.file_name().and_then(|n| n.to_str()).map(String::from);
        }
        if kind.is_dir() {
            subdirs.push(path);
        }
    }
    for sub in subdirs {
        if let Some(v) = first_video_recursive(&sub) {
            return Some(v);
        }
    }
    None
}

fn walk(dir: &Path, out: &mut ScanReport) -> AppResult<()> {
    if is_poster_cache_dir(dir) || is_system_dir(dir) {
        return Ok(());
    }

    // Read entries; collect file names and child directories. Per-folder
    // I/O errors (permission denied, disconnected drive letter, etc.) are
    // treated as non-fatal so that one bad subtree can't kill the whole
    // scan — we just skip what we can't read.
    let read_iter = match fs::read_dir(dir) {
        Ok(it) => it,
        Err(e) => {
            tracing::warn!("scan: cannot read {}: {e}", dir.display());
            return Ok(());
        }
    };
    let mut file_names: Vec<String> = Vec::new();
    let mut child_dirs: Vec<PathBuf> = Vec::new();
    for entry in read_iter {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                tracing::warn!("scan: bad entry in {}: {e}", dir.display());
                continue;
            }
        };
        let kind = match entry.file_type() {
            Ok(k) => k,
            Err(e) => {
                tracing::warn!("scan: bad file_type for {:?}: {e}", entry.path());
                continue;
            }
        };
        let name = entry.file_name().to_string_lossy().to_string();
        if kind.is_dir() {
            child_dirs.push(entry.path());
        } else if kind.is_file() {
            file_names.push(name);
        }
    }

    let basename = dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_string();

    // Special case: a folder whose name matches the catalogued convention
    // is a "container" (typical for TV series with season subfolders).
    // We treat it as catalogued without recursing further so that the
    // individual episodes inside season folders aren't surfaced as
    // "uncatalogued" themselves.
    if let Some(tmdb_id) = parse_tmdb_id(&basename) {
        let video_filename = first_video_recursive(dir).unwrap_or_default();
        out.catalogued.push(CataloguedHit {
            folder: dir.to_path_buf(),
            tmdb_id,
            video_filename,
        });
        return Ok(());
    }

    use crate::scanner::classify::is_video_file;

    // Emit one entry per direct video file. `kind` is Series if there
    // are multiple direct videos here OR the parent folder itself looks
    // like a season folder (so the user can multi-select episodes for
    // bulk "Catalog as series").
    let direct_videos: Vec<&String> = file_names
        .iter()
        .filter(|n| is_video_file(Path::new(n.as_str())))
        .collect();
    let folder_is_season = is_season_folder_name(&basename);
    let kind = if direct_videos.len() >= 2 || folder_is_season {
        UncataloguedKind::Series
    } else {
        UncataloguedKind::Movie
    };
    for v in &direct_videos {
        out.uncatalogued.push(UncataloguedHit {
            folder: dir.to_path_buf(),
            video_filename: (*v).clone(),
            kind,
        });
    }

    // Always recurse into subdirectories so movies nested arbitrarily
    // deep are still discovered. Catalogued containers were short-
    // circuited above.
    for child in child_dirs {
        walk(&child, out)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn touch(p: &Path) {
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(p, b"x").unwrap();
    }

    #[test]
    fn scan_classifies_mixed_tree() {
        let tmp = TempDir::new().unwrap();
        let hd = tmp.path();

        // Catalogued movie.
        touch(&hd.join("Movies/Action/Inception [tmdb-27205]/Inception [tmdb-27205].mkv"));
        // Catalogued series with season subfolder containing episodes; we
        // should NOT descend into Season 1.
        touch(&hd.join("Series/Game of Thrones [tmdb-1399]/Season 1/E1.mkv"));
        // Uncatalogued movie.
        touch(&hd.join("Imports/Some Movie 2019/movie.mp4"));
        // Folder with multiple direct videos — each now surfaced as its
        // own uncatalogued entry tagged Series.
        touch(&hd.join("Imports/Bundle/a.mkv"));
        touch(&hd.join("Imports/Bundle/b.mkv"));
        // Poster cache must be ignored.
        touch(&hd.join("poster/27205.jpg"));

        let report = scan(hd).unwrap();

        assert_eq!(report.catalogued.len(), 2, "{:#?}", report);
        let ids: Vec<i64> = report.catalogued.iter().map(|c| c.tmdb_id).collect();
        assert!(ids.contains(&27205));
        assert!(ids.contains(&1399));

        // GoT folder must NOT be recursed: classify_folder hits "no videos"
        // at root -> recurse into Game of Thrones [tmdb-1399] -> classify
        // sees `Season 1/` only (no immediate video files) -> recurses into
        // Season 1 -> classifies E1.mkv as Uncatalogued. We must NOT count
        // that as another hit. Let's verify the catalogued hit is the GoT
        // folder itself.
        // Wait — inner E1.mkv would actually be uncatalogued by current logic.
        // The walker sees the GoT folder at depth 1 -> its immediate children
        // are [Season 1] (a dir) -> NoVideos -> recurses -> finds E1.mkv as
        // Uncatalogued. We must prevent this: a catalogued folder should be
        // detected at the GoT level using its name, even though it contains
        // no video files directly.
        // The current classify_folder requires a video file. We need to
        // special-case: if the folder name parses as catalogued, treat it
        // as a "container" without descending. See follow-up below.

        // For now, the assertion above accepts 2 catalogued hits assuming
        // the walker handles the convention-folder-without-direct-videos
        // case. We'll update walk() accordingly.

        let unc_paths: Vec<&Path> = report.uncatalogued.iter().map(|u| u.folder.as_path()).collect();
        assert!(unc_paths.contains(&hd.join("Imports/Some Movie 2019").as_path()));
        // Both Bundle videos appear as separate Series entries.
        let bundle_count = report
            .uncatalogued
            .iter()
            .filter(|u| u.folder == hd.join("Imports/Bundle"))
            .count();
        assert_eq!(bundle_count, 2, "{:#?}", report);
        let bundle_kinds: Vec<UncataloguedKind> = report
            .uncatalogued
            .iter()
            .filter(|u| u.folder == hd.join("Imports/Bundle"))
            .map(|u| u.kind)
            .collect();
        assert!(bundle_kinds.iter().all(|k| *k == UncataloguedKind::Series));
    }

    #[test]
    fn scan_errors_on_missing_root() {
        let tmp = TempDir::new().unwrap();
        let missing = tmp.path().join("does-not-exist");
        assert!(matches!(scan(&missing), Err(AppError::InvalidPath(_))));
    }

    #[test]
    fn scan_recurses_into_subfolders_even_when_parent_has_videos() {
        let tmp = TempDir::new().unwrap();
        let hd = tmp.path();
        touch(&hd.join("Library/Top Movie/top.mkv"));
        touch(&hd.join("Library/Top Movie/Inner Movie/inner.mkv"));

        let report = scan(hd).unwrap();
        let folders: Vec<&Path> =
            report.uncatalogued.iter().map(|u| u.folder.as_path()).collect();
        assert!(folders.contains(&hd.join("Library/Top Movie").as_path()));
        assert!(folders.contains(&hd.join("Library/Top Movie/Inner Movie").as_path()));
    }

    #[test]
    fn scan_emits_one_hit_per_video_in_multi_video_folder() {
        // Flat episode dump in a single folder. Every episode must be
        // listed individually so the user can multi-select them in the UI.
        let tmp = TempDir::new().unwrap();
        let hd = tmp.path();
        touch(&hd.join("Lupin/E1.mkv"));
        touch(&hd.join("Lupin/E2.mkv"));
        touch(&hd.join("Lupin/E3.mkv"));

        let report = scan(hd).unwrap();
        let names: Vec<&str> = report
            .uncatalogued
            .iter()
            .filter(|u| u.folder == hd.join("Lupin"))
            .map(|u| u.video_filename.as_str())
            .collect();
        assert_eq!(names.len(), 3, "{:#?}", report);
        for n in ["E1.mkv", "E2.mkv", "E3.mkv"] {
            assert!(names.contains(&n), "{n} missing from {:#?}", names);
        }
        for u in &report.uncatalogued {
            assert_eq!(u.kind, UncataloguedKind::Series);
        }
    }

    #[test]
    fn scan_lists_episodes_under_season_folders_individually() {
        // The user-reported case: M:/serie/9-1-1/Season 1/episodes...
        // Every episode file must show up so they can be multi-selected
        // and bulk-linked via "Catalog as series".
        let tmp = TempDir::new().unwrap();
        let hd = tmp.path();
        touch(&hd.join("serie/9-1-1/Season 1/E1.mkv"));
        touch(&hd.join("serie/9-1-1/Season 1/E2.mkv"));
        touch(&hd.join("serie/9-1-1/Season 2/E1.mkv"));
        touch(&hd.join("serie/Lupin/Season 1/E1.mkv"));

        let report = scan(hd).unwrap();
        // 4 episode files total.
        assert_eq!(report.uncatalogued.len(), 4, "{:#?}", report);
        // Every hit's folder should be a Season folder, never the show
        // root (we no longer pretend the show root is a single linkable
        // entity — the new flow is multi-select episodes).
        for u in &report.uncatalogued {
            let f = u.folder.to_string_lossy().to_string();
            assert!(f.contains("Season "), "{f} should be a Season dir");
        }
        // Episodes inside a Season folder are tagged Series even when
        // there's only one of them (parent-name heuristic).
        for u in &report.uncatalogued {
            assert_eq!(u.kind, UncataloguedKind::Series, "{:?}", u);
        }
    }

    #[test]
    fn scan_tags_single_video_in_plain_folder_as_movie() {
        let tmp = TempDir::new().unwrap();
        let hd = tmp.path();
        touch(&hd.join("Imports/Some Movie/movie.mkv"));

        let report = scan(hd).unwrap();
        assert_eq!(report.uncatalogued.len(), 1);
        assert_eq!(report.uncatalogued[0].kind, UncataloguedKind::Movie);
    }

    #[test]
    fn scan_skips_windows_system_dirs_and_continues() {
        // Repro of the user-reported bug: a Windows-style root with a
        // `$RECYCLE.BIN` sibling next to a real movie folder. The recycle
        // bin must be skipped entirely AND must not abort the scan, so
        // the real movie still shows up.
        let tmp = TempDir::new().unwrap();
        let hd = tmp.path();
        touch(&hd.join("$RECYCLE.BIN/whatever.bin"));
        touch(&hd.join("System Volume Information/track.log"));
        touch(&hd.join("Terror/A Longa Marcha/A Longa Marcha.mp4"));

        let report = scan(hd).unwrap();

        let folders: Vec<&Path> =
            report.uncatalogued.iter().map(|u| u.folder.as_path()).collect();
        assert!(
            folders.contains(&hd.join("Terror/A Longa Marcha").as_path()),
            "expected Terror/A Longa Marcha in {:#?}",
            report
        );
        // System dirs must produce zero hits.
        for u in &report.uncatalogued {
            let s = u.folder.to_string_lossy();
            assert!(!s.contains("$RECYCLE.BIN"), "{s} should be skipped");
            assert!(
                !s.contains("System Volume Information"),
                "{s} should be skipped"
            );
        }
    }

    #[test]
    fn scan_does_not_descend_into_catalogued_containers() {
        // A catalogued folder (TV series with episodes inside season subdirs)
        // should NOT have its inner episodes surfaced as uncatalogued.
        let tmp = TempDir::new().unwrap();
        let hd = tmp.path();
        touch(&hd.join("Series/Game of Thrones [tmdb-1399]/Season 1/E1.mkv"));
        touch(&hd.join("Series/Game of Thrones [tmdb-1399]/Season 1/E2.mkv"));

        let report = scan(hd).unwrap();
        assert_eq!(report.catalogued.len(), 1);
        assert!(report.uncatalogued.is_empty(), "{:#?}", report);
        assert!(report.skipped.is_empty(), "{:#?}", report);
    }
}
