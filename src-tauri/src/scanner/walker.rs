//! Walks the HD root and produces a snapshot of catalogued / uncatalogued /
//! skipped folders. Side-effecting (reads from the filesystem) but not
//! mutating.

use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};

use crate::organizer::naming::parse_tmdb_id;
use crate::scanner::entry::{
    classify_folder, is_poster_cache_dir, is_season_folder_name, is_system_dir,
    FolderClassification,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CataloguedHit {
    pub folder: PathBuf,
    pub tmdb_id: i64,
    pub video_filename: String,
}

/// What sort of uncatalogued item the walker thinks a folder is. The link
/// flow uses this hint to decide whether to rename a single video file
/// (Movie) or the whole folder tree (Series).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UncataloguedKind {
    /// One video file directly inside `folder`.
    Movie,
    /// Either multiple video files directly inside `folder` (flat episode
    /// dump) or `folder` contains "Season N"-shaped subdirectories.
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

    // Detect "this folder is a TV series root" — its immediate children
    // look like season folders ("Season 1", "Temporada 2", "S01", ...).
    // If so, surface IT as the uncatalogued series and don't descend
    // (otherwise each season would be reported individually).
    let has_season_children = child_dirs.iter().any(|p| {
        p.file_name()
            .and_then(|n| n.to_str())
            .map(is_season_folder_name)
            .unwrap_or(false)
    });
    let direct_video_count = file_names
        .iter()
        .filter(|n| crate::scanner::classify::is_video_file(Path::new(n.as_str())))
        .count();

    if direct_video_count == 0 && has_season_children {
        let video_filename = first_video_recursive(dir).unwrap_or_default();
        out.uncatalogued.push(UncataloguedHit {
            folder: dir.to_path_buf(),
            video_filename,
            kind: UncataloguedKind::Series,
        });
        return Ok(());
    }

    // Classify the videos in THIS folder (if any) — without preventing
    // recursion into subdirectories. A folder can simultaneously hold an
    // uncatalogued movie AND nest more movies in subfolders.
    match classify_folder(&basename, &file_names) {
        FolderClassification::Catalogued {
            tmdb_id,
            video_filename,
        } => {
            out.catalogued.push(CataloguedHit {
                folder: dir.to_path_buf(),
                tmdb_id,
                video_filename,
            });
        }
        FolderClassification::Uncatalogued { video_filename } => {
            out.uncatalogued.push(UncataloguedHit {
                folder: dir.to_path_buf(),
                video_filename,
                kind: UncataloguedKind::Movie,
            });
        }
        FolderClassification::SkippedMultipleVideos { count } => {
            // A folder with >=2 direct videos is almost always a flat
            // series dump (Lupin/E1.mkv, E2.mkv, ...). Surface it as an
            // uncatalogued series candidate so the user can link it.
            // Don't recurse: those videos are episodes.
            let video_filename = file_names
                .iter()
                .find(|n| crate::scanner::classify::is_video_file(Path::new(n.as_str())))
                .cloned()
                .unwrap_or_default();
            out.uncatalogued.push(UncataloguedHit {
                folder: dir.to_path_buf(),
                video_filename,
                kind: UncataloguedKind::Series,
            });
            // Track for diagnostics; not user-actionable.
            out.skipped.push(SkippedHit {
                folder: dir.to_path_buf(),
                video_count: count,
            });
            return Ok(());
        }
        FolderClassification::NoVideos => {}
    }

    // Always recurse into subdirectories so movies nested arbitrarily
    // deep are still discovered.
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
        // Folder with multiple direct videos — now surfaced as an
        // uncatalogued *series* candidate (and also recorded in `skipped`
        // for diagnostics).
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
        // Bundle is now an uncatalogued series candidate too.
        assert!(unc_paths.contains(&hd.join("Imports/Bundle").as_path()));

        assert_eq!(report.skipped.len(), 1);
        assert_eq!(report.skipped[0].folder, hd.join("Imports/Bundle"));
    }

    #[test]
    fn scan_errors_on_missing_root() {
        let tmp = TempDir::new().unwrap();
        let missing = tmp.path().join("does-not-exist");
        assert!(matches!(scan(&missing), Err(AppError::InvalidPath(_))));
    }

    #[test]
    fn scan_recurses_into_subfolders_even_when_parent_has_videos() {
        // Parent folder has its own video file AND a subfolder containing
        // another movie. We must surface BOTH as uncatalogued movies.
        let tmp = TempDir::new().unwrap();
        let hd = tmp.path();
        touch(&hd.join("Library/Top Movie/top.mkv"));
        touch(&hd.join("Library/Top Movie/Inner Movie/inner.mkv"));

        let report = scan(hd).unwrap();
        let folders: Vec<&Path> =
            report.uncatalogued.iter().map(|u| u.folder.as_path()).collect();
        assert!(
            folders.contains(&hd.join("Library/Top Movie").as_path()),
            "{:#?}",
            report
        );
        assert!(
            folders.contains(&hd.join("Library/Top Movie/Inner Movie").as_path()),
            "{:#?}",
            report
        );
        for u in &report.uncatalogued {
            assert_eq!(u.kind, UncataloguedKind::Movie);
        }
    }

    #[test]
    fn scan_treats_multi_video_folder_as_uncatalogued_series() {
        // Flat dump of episodes inside a single folder (no Season subdir).
        let tmp = TempDir::new().unwrap();
        let hd = tmp.path();
        touch(&hd.join("Lupin/E1.mkv"));
        touch(&hd.join("Lupin/E2.mkv"));
        touch(&hd.join("Lupin/E3.mkv"));

        let report = scan(hd).unwrap();
        let folders: Vec<&Path> =
            report.uncatalogued.iter().map(|u| u.folder.as_path()).collect();
        assert!(
            folders.contains(&hd.join("Lupin").as_path()),
            "{:#?}",
            report
        );
        let lupin = report
            .uncatalogued
            .iter()
            .find(|u| u.folder == hd.join("Lupin"))
            .unwrap();
        assert_eq!(lupin.kind, UncataloguedKind::Series);
    }

    #[test]
    fn scan_surfaces_series_root_when_subdirs_look_like_seasons() {
        // The user-reported case: M:/serie/9-1-1/Season 1/episodes...
        // The walker must surface "9-1-1" (and "Lupin"), not "Season X".
        let tmp = TempDir::new().unwrap();
        let hd = tmp.path();
        touch(&hd.join("serie/9-1-1/Season 1/E1.mkv"));
        touch(&hd.join("serie/9-1-1/Season 1/E2.mkv"));
        touch(&hd.join("serie/9-1-1/Season 2/E1.mkv"));
        touch(&hd.join("serie/Lupin/Season 1/E1.mkv"));

        let report = scan(hd).unwrap();
        let folders: Vec<&Path> =
            report.uncatalogued.iter().map(|u| u.folder.as_path()).collect();
        assert!(
            folders.contains(&hd.join("serie/9-1-1").as_path()),
            "9-1-1 not in {:#?}",
            report
        );
        assert!(
            folders.contains(&hd.join("serie/Lupin").as_path()),
            "Lupin not in {:#?}",
            report
        );
        // The Season subfolders themselves must NOT be surfaced.
        for u in &report.uncatalogued {
            assert!(
                !u.folder.to_string_lossy().contains("Season "),
                "Season folder leaked into uncatalogued: {:?}",
                u
            );
        }
        // Both must be tagged as Series.
        for u in &report.uncatalogued {
            assert_eq!(u.kind, UncataloguedKind::Series, "{:?}", u);
        }
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
