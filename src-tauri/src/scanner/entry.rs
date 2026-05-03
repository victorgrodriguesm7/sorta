//! Pure classification of a single directory entry into a discovery state.

use crate::organizer::naming::{is_catalogued_folder, parse_tmdb_id};
use crate::scanner::classify::is_video_file;
use std::path::{Path, PathBuf};

/// What the scanner found inside a single folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FolderClassification {
    /// Folder follows the `Title [tmdb-{id}]` convention and contains exactly
    /// one video file. Ready to be reconciled with the DB.
    Catalogued {
        tmdb_id: i64,
        video_filename: String,
    },
    /// Folder doesn't follow the convention but does contain a single video.
    /// Surfaced in the UI under "Uncatalogued".
    Uncatalogued { video_filename: String },
    /// Folder contains multiple video files — skipped per spec.
    SkippedMultipleVideos { count: usize },
    /// Folder contains no video files (likely a season subfolder, the `poster/`
    /// folder, or an unrelated dir). Scanner should recurse further.
    NoVideos,
}

/// Classify a folder given:
///   - the folder's basename (used for the `Title [tmdb-{id}]` check),
///   - the file names directly inside it (NOT recursive — the caller decides
///     how to recurse).
pub fn classify_folder<S: AsRef<str>>(folder_basename: &str, file_names: &[S]) -> FolderClassification {
    let videos: Vec<String> = file_names
        .iter()
        .map(|s| s.as_ref().to_string())
        .filter(|n| is_video_file(Path::new(n)))
        .collect();

    match videos.len() {
        0 => FolderClassification::NoVideos,
        1 => {
            let video_filename = videos.into_iter().next().unwrap();
            if is_catalogued_folder(folder_basename) {
                if let Some(tmdb_id) = parse_tmdb_id(folder_basename) {
                    return FolderClassification::Catalogued {
                        tmdb_id,
                        video_filename,
                    };
                }
            }
            FolderClassification::Uncatalogued { video_filename }
        }
        n => FolderClassification::SkippedMultipleVideos { count: n },
    }
}

/// Determines whether a directory is the "system" `poster/` cache and should
/// be skipped by the scanner. Comparison is case-insensitive on the basename.
pub fn is_poster_cache_dir(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.eq_ignore_ascii_case("poster"))
        .unwrap_or(false)
}

/// Tiny convenience used by the walker.
pub fn relative_to(root: &Path, full: &Path) -> PathBuf {
    full.strip_prefix(root)
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|_| full.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_catalogued_with_single_video() {
        let files = vec!["movie.mkv", "movie.srt"];
        let got = classify_folder("Inception [tmdb-27205]", &files);
        assert_eq!(
            got,
            FolderClassification::Catalogued {
                tmdb_id: 27205,
                video_filename: "movie.mkv".to_string()
            }
        );
    }

    #[test]
    fn classifies_uncatalogued_when_folder_name_doesnt_match() {
        let files = vec!["movie.mkv"];
        assert_eq!(
            classify_folder("Inception (2010)", &files),
            FolderClassification::Uncatalogued { video_filename: "movie.mkv".to_string() }
        );
    }

    #[test]
    fn skips_folders_with_multiple_videos() {
        let files = vec!["a.mkv", "b.mp4", "notes.txt"];
        assert_eq!(
            classify_folder("Whatever", &files),
            FolderClassification::SkippedMultipleVideos { count: 2 }
        );
    }

    #[test]
    fn no_videos_means_recurse() {
        let files = vec!["readme.txt"];
        assert_eq!(classify_folder("foo", &files), FolderClassification::NoVideos);
    }

    #[test]
    fn detects_poster_cache_dir() {
        assert!(is_poster_cache_dir(Path::new("/hd/poster")));
        assert!(is_poster_cache_dir(Path::new("/hd/Poster")));
        assert!(!is_poster_cache_dir(Path::new("/hd/Movies")));
    }
}
