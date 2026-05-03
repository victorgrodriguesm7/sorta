//! Walks the HD root and produces a snapshot of catalogued / uncatalogued /
//! skipped folders. Side-effecting (reads from the filesystem) but not
//! mutating.

use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{AppError, AppResult};
use crate::organizer::naming::parse_tmdb_id;
use crate::scanner::entry::{classify_folder, is_poster_cache_dir, FolderClassification};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CataloguedHit {
    pub folder: PathBuf,
    pub tmdb_id: i64,
    pub video_filename: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UncataloguedHit {
    pub folder: PathBuf,
    pub video_filename: String,
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
    if is_poster_cache_dir(dir) {
        return Ok(());
    }

    // Read entries; collect file names and child directories.
    let mut file_names: Vec<String> = Vec::new();
    let mut child_dirs: Vec<PathBuf> = Vec::new();
    for entry in fs::read_dir(dir).map_err(AppError::from)? {
        let entry = entry.map_err(AppError::from)?;
        let kind = entry.file_type().map_err(AppError::from)?;
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
    // We treat it as catalogued without recursing further. The first video
    // found anywhere underneath (best-effort) becomes its `video_filename`.
    if let Some(tmdb_id) = parse_tmdb_id(&basename) {
        let video_filename = first_video_recursive(dir).unwrap_or_default();
        out.catalogued.push(CataloguedHit {
            folder: dir.to_path_buf(),
            tmdb_id,
            video_filename,
        });
        return Ok(());
    }

    let classification = classify_folder(&basename, &file_names);

    match classification {
        FolderClassification::Catalogued {
            tmdb_id,
            video_filename,
        } => {
            out.catalogued.push(CataloguedHit {
                folder: dir.to_path_buf(),
                tmdb_id,
                video_filename,
            });
            // Don't recurse into season subfolders etc.
        }
        FolderClassification::Uncatalogued { video_filename } => {
            out.uncatalogued.push(UncataloguedHit {
                folder: dir.to_path_buf(),
                video_filename,
            });
        }
        FolderClassification::SkippedMultipleVideos { count } => {
            out.skipped.push(SkippedHit {
                folder: dir.to_path_buf(),
                video_count: count,
            });
        }
        FolderClassification::NoVideos => {
            for child in child_dirs {
                walk(&child, out)?;
            }
        }
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
        // Skipped folder with multiple videos.
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

        assert_eq!(report.skipped.len(), 1);
        assert_eq!(report.skipped[0].folder, hd.join("Imports/Bundle"));
    }

    #[test]
    fn scan_errors_on_missing_root() {
        let tmp = TempDir::new().unwrap();
        let missing = tmp.path().join("does-not-exist");
        assert!(matches!(scan(&missing), Err(AppError::InvalidPath(_))));
    }
}
