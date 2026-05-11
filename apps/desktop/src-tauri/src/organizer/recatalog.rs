//! Pure helpers for the "Re-Catalog" flow.
//!
//! Re-cataloging walks an *already-catalogued* series folder, parses
//! each video file's season/episode out of the filename, and feeds
//! the result into the same upsert path that `link_as_series` uses
//! for fresh links. None of the helpers here touch the database or
//! the TMDB client — the surrounding command handler does that.

use once_cell::sync::Lazy;
use regex::Regex;
use std::path::{Path, PathBuf};

use crate::scanner::classify::is_video_file;

/// `S01E02`, `s1e2`, `S001E001`. Mirrors the parser the reader uses
/// on the Android side so a file's identity is the same on both
/// sides of the wire.
static EPISODE_TAG: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)S(\d{1,3})E(\d{1,3})").expect("valid regex"));

/// Parsed `(season, episode)` from a filename. Returns `None` when no
/// `SxxExx` token is present (rare but possible on older user-named
/// files).
pub fn parse_season_episode(filename: &str) -> Option<(i64, i64)> {
    let caps = EPISODE_TAG.captures(filename)?;
    let s = caps.get(1)?.as_str().parse::<i64>().ok()?;
    let e = caps.get(2)?.as_str().parse::<i64>().ok()?;
    Some((s, e))
}

/// Drop the leading `SxxExx` token (and any immediately following
/// `.` / ` ` / `-` separator) from a filename, leaving whatever
/// custom title segment was already there. Used to dedupe when the
/// user re-runs the rename: if the file is `S01E01.Pilot.mkv`,
/// stripping the tag yields `Pilot.mkv` so the new title comparison
/// against TMDB ("Pilot") doesn't double-stamp.
pub fn strip_episode_tag(stem: &str) -> &str {
    if let Some(m) = EPISODE_TAG.find(stem) {
        let rest = &stem[m.end()..];
        rest.trim_start_matches(|c: char| c == '.' || c == ' ' || c == '-' || c == '_')
    } else {
        stem
    }
}

/// One discovered season under a series folder. `season_number` is
/// parsed from the season subfolder name (`<season_label> N`),
/// falling back to scanning episode tags inside the files when the
/// folder name is non-conforming.
#[derive(Debug, Clone, PartialEq)]
pub struct DiscoveredSeason {
    pub season_number: i64,
    pub season_folder: PathBuf,
    pub files: Vec<PathBuf>,
}

/// Walk `series_folder` and return one [`DiscoveredSeason`] per
/// subdirectory whose name starts with `season_label_prefix` (e.g.
/// `"Season"` → matches `Season 1`, `Season 02`, etc.). Returns
/// seasons sorted by `season_number`.
///
/// Sublayouts the walker tolerates:
/// - `<series>/Season 1/S01E01.mkv` — canonical, what `link_as_series`
///   produces. Detected directly.
/// - `<series>/Season 1/some-arbitrary-name.mkv` — the user named
///   episodes themselves; the season number comes from the folder
///   name, the episode number falls out of the file's `SxxExx` tag
///   (if present) at the per-file matching step downstream.
/// - Files dropped loose into the series root (no season folder) are
///   ignored — that layout was never produced by Sorta and probably
///   indicates a manual move we shouldn't second-guess.
pub fn discover_seasons(
    series_folder: &Path,
    season_label_prefix: &str,
) -> std::io::Result<Vec<DiscoveredSeason>> {
    let prefix = format!("{season_label_prefix} ");
    let mut out: Vec<DiscoveredSeason> = Vec::new();

    for entry in std::fs::read_dir(series_folder)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n,
            None => continue,
        };
        let Some(rest) = name.strip_prefix(&prefix) else {
            continue;
        };
        let season_number: i64 = match rest.trim().parse() {
            Ok(n) => n,
            Err(_) => continue,
        };

        // Collect video files inside this season folder.
        let mut files: Vec<PathBuf> = std::fs::read_dir(&path)?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.is_file() && is_video_file(p))
            .collect();
        files.sort();
        out.push(DiscoveredSeason {
            season_number,
            season_folder: path,
            files,
        });
    }
    out.sort_by_key(|s| s.season_number);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn parse_se_handles_common_shapes() {
        assert_eq!(parse_season_episode("S01E01.Pilot.mkv"), Some((1, 1)));
        assert_eq!(parse_season_episode("show.s2e10.mkv"), Some((2, 10)));
        assert_eq!(parse_season_episode("Show - S03E04 - Foo.mkv"), Some((3, 4)));
        assert_eq!(parse_season_episode("S001E001.mkv"), Some((1, 1)));
    }

    #[test]
    fn parse_se_returns_none_without_tag() {
        assert_eq!(parse_season_episode("episode.mkv"), None);
        assert_eq!(parse_season_episode("S01.mkv"), None);
        assert_eq!(parse_season_episode("E01.mkv"), None);
    }

    #[test]
    fn strip_tag_removes_token_and_one_separator() {
        assert_eq!(strip_episode_tag("S01E01.Pilot"), "Pilot");
        assert_eq!(strip_episode_tag("S01E01 - The Pilot"), "The Pilot");
        assert_eq!(strip_episode_tag("S01E01_Pilot"), "Pilot");
        assert_eq!(strip_episode_tag("Pilot"), "Pilot");
    }

    #[test]
    fn discover_seasons_sorts_and_filters_to_video_files() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        // Two season folders, one decoy, plus a stray file in root.
        for sub in ["Season 2", "Season 10", "Season 1", "Specials", "Extras"] {
            std::fs::create_dir(root.join(sub)).unwrap();
        }
        std::fs::write(root.join("loose.mkv"), b"x").unwrap();
        std::fs::write(root.join("Season 1").join("S01E01.Pilot.mkv"), b"x").unwrap();
        std::fs::write(root.join("Season 1").join("readme.txt"), b"x").unwrap();
        std::fs::write(root.join("Season 2").join("S02E01.foo.mp4"), b"x").unwrap();
        std::fs::write(root.join("Season 2").join("S02E02.bar.mkv"), b"x").unwrap();
        std::fs::write(root.join("Season 10").join("S10E01.x.mkv"), b"x").unwrap();
        // "Specials" isn't `Season N` so it must be skipped.
        std::fs::write(root.join("Specials").join("S00E01.x.mkv"), b"x").unwrap();

        let seasons = discover_seasons(root, "Season").unwrap();
        assert_eq!(seasons.len(), 3);
        // Sorted numerically, not lexically (1, 2, 10 — not 1, 10, 2).
        assert_eq!(
            seasons.iter().map(|s| s.season_number).collect::<Vec<_>>(),
            vec![1, 2, 10],
        );
        assert_eq!(seasons[0].files.len(), 1);
        assert_eq!(seasons[1].files.len(), 2);
        assert_eq!(seasons[2].files.len(), 1);
    }

    #[test]
    fn discover_seasons_uses_provided_label_prefix() {
        // Translatable Season label, e.g. Portuguese "Temporada".
        let tmp = TempDir::new().unwrap();
        std::fs::create_dir(tmp.path().join("Temporada 1")).unwrap();
        std::fs::write(
            tmp.path().join("Temporada 1").join("S01E01.mkv"),
            b"x",
        )
        .unwrap();
        let s = discover_seasons(tmp.path(), "Temporada").unwrap();
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].season_number, 1);
    }
}
