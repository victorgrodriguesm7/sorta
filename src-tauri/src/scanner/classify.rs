//! Pure classification helpers used by the scanner.

use std::path::Path;

/// Recognized video file extensions (lowercase, no leading dot).
pub const VIDEO_EXTENSIONS: &[&str] = &[
    "mkv", "mp4", "avi", "mov", "wmv", "m4v", "webm",
];

/// Recognized sidecar extensions that should be moved/renamed alongside
/// the main video file (subtitles, metadata).
pub const SIDECAR_EXTENSIONS: &[&str] = &[
    "srt", "ass", "ssa", "sub", "vtt", "nfo",
];

/// Returns true if the given path looks like a video file we care about.
pub fn is_video_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| {
            let lower = e.to_ascii_lowercase();
            VIDEO_EXTENSIONS.contains(&lower.as_str())
        })
        .unwrap_or(false)
}

/// Returns true if the path is a sidecar we should move with the video.
pub fn is_sidecar_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| {
            let lower = e.to_ascii_lowercase();
            SIDECAR_EXTENSIONS.contains(&lower.as_str())
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn detects_common_video_extensions() {
        for ext in ["mkv", "mp4", "avi", "mov", "wmv", "m4v", "webm"] {
            let p = PathBuf::from(format!("movie.{ext}"));
            assert!(is_video_file(&p), "should detect .{ext}");
        }
    }

    #[test]
    fn ignores_non_video_extensions() {
        for name in ["readme.txt", "cover.jpg", "subs.srt", "noext"] {
            assert!(!is_video_file(Path::new(name)), "should ignore {name}");
        }
    }

    #[test]
    fn video_detection_is_case_insensitive() {
        assert!(is_video_file(Path::new("MOVIE.MKV")));
        assert!(is_video_file(Path::new("Movie.Mp4")));
    }

    #[test]
    fn detects_sidecar_files() {
        for ext in ["srt", "ass", "ssa", "sub", "vtt", "nfo"] {
            assert!(is_sidecar_file(&PathBuf::from(format!("a.{ext}"))));
        }
        assert!(!is_sidecar_file(Path::new("movie.mkv")));
    }
}
