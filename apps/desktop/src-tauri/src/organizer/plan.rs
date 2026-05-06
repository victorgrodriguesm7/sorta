//! Pure planning of rename/move operations.
//!
//! Given an HD root, the source folder/video, and the target media metadata,
//! compute the sequence of `Op`s the executor should perform. The planner
//! never touches the filesystem — that's the executor's job in Phase 4.

use std::path::{Path, PathBuf};

use crate::organizer::naming::{folder_name, sanitize_segment};
use crate::organizer::sidecars::{find_sidecars, rename_sidecar};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Op {
    /// Create a directory (and any missing parents).
    CreateDir(PathBuf),
    /// Move (and possibly rename) a file from `from` to `to`.
    MoveFile { from: PathBuf, to: PathBuf },
    /// Move a directory tree from `from` to `to` (used for genre merges).
    MoveDir { from: PathBuf, to: PathBuf },
    /// Remove an empty directory (best-effort).
    RemoveEmptyDir(PathBuf),
}

/// All inputs needed to plan a "link" operation.
#[derive(Debug, Clone)]
pub struct LinkPlanInput<'a> {
    /// HD root, e.g. `D:/Movies`.
    pub hd_root: &'a Path,
    /// Translated label of the kind-root folder, e.g. `"Movies"` or `"Filmes"`.
    pub kind_root_label: &'a str,
    /// For movies: translated primary genre folder, e.g. `"Action"` / `"Ação"`.
    /// For TV: pass `None` — series go directly under the kind root.
    pub genre_folder: Option<&'a str>,
    /// Folder that currently contains the video, relative to `hd_root` or absolute.
    pub current_folder: &'a Path,
    /// Filename of the main video, relative to `current_folder`.
    pub video_filename: &'a str,
    /// Other files in `current_folder` (just file names, not full paths).
    pub siblings: &'a [String],
    /// TMDB id of the linked work.
    pub tmdb_id: i64,
    /// Title to use for the new folder (already in pt-BR by caller).
    pub title: &'a str,
}

/// The output of [`plan_link`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkPlan {
    /// New absolute folder path (under `hd_root`).
    pub target_folder: PathBuf,
    /// New filename for the main video (without path).
    pub new_video_filename: String,
    /// Ordered operations to perform.
    pub ops: Vec<Op>,
}

/// Compute a [`LinkPlan`] for a "link" action. Pure: no filesystem access.
///
/// Note: the executor is responsible for verifying that `target_folder` does
/// not already exist (the conflict check). The planner only computes paths.
pub fn plan_link(input: &LinkPlanInput<'_>) -> LinkPlan {
    let kind_label = sanitize_segment(input.kind_root_label);
    let kind_dir = input.hd_root.join(kind_label);

    let parent_dir = match input.genre_folder {
        Some(genre) => kind_dir.join(sanitize_segment(genre)),
        None => kind_dir,
    };

    let new_folder = folder_name(input.title, input.tmdb_id);
    let target_folder = parent_dir.join(&new_folder);

    let video_ext = Path::new(input.video_filename)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    let new_video_filename = if video_ext.is_empty() {
        new_folder.clone()
    } else {
        format!("{new_folder}.{video_ext}")
    };

    let old_stem = Path::new(input.video_filename)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("");

    let mut ops = Vec::new();
    ops.push(Op::CreateDir(target_folder.clone()));

    // Main video.
    ops.push(Op::MoveFile {
        from: input.current_folder.join(input.video_filename),
        to: target_folder.join(&new_video_filename),
    });

    // Sidecars.
    for sidecar in find_sidecars(input.video_filename, input.siblings) {
        let new_name = rename_sidecar(&sidecar, old_stem, &new_folder)
            .unwrap_or_else(|| sidecar.clone());
        ops.push(Op::MoveFile {
            from: input.current_folder.join(&sidecar),
            to: target_folder.join(new_name),
        });
    }

    LinkPlan {
        target_folder,
        new_video_filename,
        ops,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    #[test]
    fn plans_movie_link_with_genre() {
        let siblings = vec!["video.mkv".to_string(), "video.srt".to_string()];
        let input = LinkPlanInput {
            hd_root: Path::new("/hd"),
            kind_root_label: "Movies",
            genre_folder: Some("Action"),
            current_folder: Path::new("/hd/Uncatalogued/whatever"),
            video_filename: "video.mkv",
            siblings: &siblings,
            tmdb_id: 27205,
            title: "Inception",
        };
        let plan = plan_link(&input);
        assert_eq!(plan.target_folder, p("/hd/Movies/Action/Inception [tmdb-27205]"));
        assert_eq!(plan.new_video_filename, "Inception [tmdb-27205].mkv");
        assert_eq!(plan.ops.len(), 3); // create + video + sidecar
        assert_eq!(plan.ops[0], Op::CreateDir(p("/hd/Movies/Action/Inception [tmdb-27205]")));
        assert_eq!(
            plan.ops[1],
            Op::MoveFile {
                from: p("/hd/Uncatalogued/whatever/video.mkv"),
                to: p("/hd/Movies/Action/Inception [tmdb-27205]/Inception [tmdb-27205].mkv"),
            }
        );
        assert_eq!(
            plan.ops[2],
            Op::MoveFile {
                from: p("/hd/Uncatalogued/whatever/video.srt"),
                to: p("/hd/Movies/Action/Inception [tmdb-27205]/Inception [tmdb-27205].srt"),
            }
        );
    }

    #[test]
    fn plans_tv_link_without_genre() {
        let siblings = vec!["pilot.mkv".to_string()];
        let input = LinkPlanInput {
            hd_root: Path::new("/hd"),
            kind_root_label: "Series",
            genre_folder: None,
            current_folder: Path::new("/hd/Series/RawDump"),
            video_filename: "pilot.mkv",
            siblings: &siblings,
            tmdb_id: 1399,
            title: "Game of Thrones",
        };
        let plan = plan_link(&input);
        assert_eq!(plan.target_folder, p("/hd/Series/Game of Thrones [tmdb-1399]"));
        assert_eq!(plan.new_video_filename, "Game of Thrones [tmdb-1399].mkv");
    }

    #[test]
    fn plans_with_translated_labels() {
        let input = LinkPlanInput {
            hd_root: Path::new("/hd"),
            kind_root_label: "Filmes",
            genre_folder: Some("Ação"),
            current_folder: Path::new("/hd/x"),
            video_filename: "v.mp4",
            siblings: &[],
            tmdb_id: 1,
            title: "Matrix",
        };
        let plan = plan_link(&input);
        assert_eq!(plan.target_folder, p("/hd/Filmes/Ação/Matrix [tmdb-1]"));
    }

    #[test]
    fn plans_preserves_tagged_sidecar_names() {
        let siblings = vec![
            "video.mkv".to_string(),
            "video.en.srt".to_string(),
            "video.pt-BR.srt".to_string(),
        ];
        let input = LinkPlanInput {
            hd_root: Path::new("/hd"),
            kind_root_label: "Movies",
            genre_folder: Some("Drama"),
            current_folder: Path::new("/hd/x"),
            video_filename: "video.mkv",
            siblings: &siblings,
            tmdb_id: 9,
            title: "T",
        };
        let plan = plan_link(&input);
        let new_names: Vec<_> = plan
            .ops
            .iter()
            .filter_map(|op| match op {
                Op::MoveFile { to, .. } => to.file_name().and_then(|n| n.to_str()),
                _ => None,
            })
            .collect();
        assert!(new_names.iter().any(|n| n.ends_with(".en.srt")));
        assert!(new_names.iter().any(|n| n.ends_with(".pt-BR.srt")));
    }
}
