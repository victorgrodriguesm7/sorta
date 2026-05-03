//! Sidecar discovery — given a video file path and a list of sibling
//! filenames, return the subset that should be moved/renamed alongside
//! the video.
//!
//! A sidecar is a sibling whose filename starts with the video's basename
//! (without extension) AND whose extension is in [`SIDECAR_EXTENSIONS`].
//! This catches Plex/Jellyfin patterns like:
//!
//! - `Movie.srt`           (exact basename match)
//! - `Movie.en.srt`        (basename + language tag)
//! - `Movie.forced.srt`    (basename + flag)

use crate::scanner::classify::is_sidecar_file;
use std::path::{Path, PathBuf};

/// Given the video filename (just the file name, not the full path) and
/// a list of sibling file names, return which siblings are sidecars for
/// the video. Comparison is case-insensitive on the basename portion.
pub fn find_sidecars<S: AsRef<str>>(video_filename: &str, siblings: &[S]) -> Vec<String> {
    let video_path = Path::new(video_filename);
    let Some(stem) = video_path.file_stem().and_then(|s| s.to_str()) else {
        return vec![];
    };
    let stem_lower = stem.to_ascii_lowercase();

    siblings
        .iter()
        .map(|s| s.as_ref())
        .filter(|name| *name != video_filename)
        .filter(|name| is_sidecar_file(Path::new(name)))
        .filter(|name| {
            let p = Path::new(name);
            let s = match p.file_stem().and_then(|s| s.to_str()) {
                Some(s) => s.to_ascii_lowercase(),
                None => return false,
            };
            // Either the stem matches exactly or it begins with `<stem>.` (Plex tag form).
            s == stem_lower || s.starts_with(&format!("{stem_lower}."))
        })
        .map(|s| s.to_string())
        .collect()
}

/// Compute the new sidecar filename when the video is renamed.
///
/// Preserves any tag suffix between the video stem and the sidecar
/// extension. e.g.:
///   video `Old.mkv` → `New.mkv`
///   sidecar `Old.en.srt` → `New.en.srt`
pub fn rename_sidecar(
    sidecar_filename: &str,
    old_video_stem: &str,
    new_video_stem: &str,
) -> Option<String> {
    let p = Path::new(sidecar_filename);
    let stem = p.file_stem().and_then(|s| s.to_str())?;
    let ext = p.extension().and_then(|s| s.to_str())?;

    let old_lower = old_video_stem.to_ascii_lowercase();
    let stem_lower = stem.to_ascii_lowercase();

    let suffix = if stem_lower == old_lower {
        String::new()
    } else if let Some(rest) = stem_lower.strip_prefix(&format!("{old_lower}.")) {
        // Preserve the original casing of the tag — slice from the original stem.
        let tag = &stem[old_video_stem.len() + 1..stem.len().min(old_video_stem.len() + 1 + rest.len())];
        format!(".{tag}")
    } else {
        return None;
    };

    Some(format!("{new_video_stem}{suffix}.{ext}"))
}

/// Convenience: given a folder path, video filename, and sibling names,
/// return absolute-ish PathBufs for each sidecar (joined to the folder).
pub fn sidecar_paths<S: AsRef<str>>(
    folder: &Path,
    video_filename: &str,
    siblings: &[S],
) -> Vec<PathBuf> {
    find_sidecars(video_filename, siblings)
        .into_iter()
        .map(|name| folder.join(name))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_exact_basename_sidecars() {
        let siblings = vec!["Movie.mkv", "Movie.srt", "Movie.nfo", "Other.srt"];
        let mut got = find_sidecars("Movie.mkv", &siblings);
        got.sort();
        assert_eq!(got, vec!["Movie.nfo", "Movie.srt"]);
    }

    #[test]
    fn finds_tagged_sidecars() {
        let siblings = vec![
            "Movie.mkv",
            "Movie.en.srt",
            "Movie.forced.srt",
            "Movie.pt-BR.srt",
        ];
        let mut got = find_sidecars("Movie.mkv", &siblings);
        got.sort();
        assert_eq!(
            got,
            vec!["Movie.en.srt", "Movie.forced.srt", "Movie.pt-BR.srt"]
        );
    }

    #[test]
    fn ignores_non_sidecar_extensions() {
        let siblings = vec!["Movie.mkv", "Movie.txt", "Movie.jpg"];
        assert!(find_sidecars("Movie.mkv", &siblings).is_empty());
    }

    #[test]
    fn ignores_unrelated_basenames() {
        let siblings = vec!["Movie.mkv", "Trailer.srt", "Sample.srt"];
        assert!(find_sidecars("Movie.mkv", &siblings).is_empty());
    }

    #[test]
    fn basename_match_is_case_insensitive() {
        let siblings = vec!["Movie.MKV", "MOVIE.SRT"];
        let got = find_sidecars("Movie.MKV", &siblings);
        assert_eq!(got, vec!["MOVIE.SRT"]);
    }

    #[test]
    fn renames_exact_sidecar() {
        assert_eq!(
            rename_sidecar("Old.srt", "Old", "New").as_deref(),
            Some("New.srt")
        );
    }

    #[test]
    fn renames_tagged_sidecar_preserving_tag() {
        assert_eq!(
            rename_sidecar("Old.en.srt", "Old", "New").as_deref(),
            Some("New.en.srt")
        );
        assert_eq!(
            rename_sidecar("Old.pt-BR.srt", "Old", "New").as_deref(),
            Some("New.pt-BR.srt")
        );
    }

    #[test]
    fn rename_returns_none_when_unrelated() {
        assert_eq!(rename_sidecar("Other.srt", "Old", "New"), None);
    }
}
