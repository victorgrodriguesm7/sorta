//! Filesystem execution of organizer plans.

use std::path::{Path, PathBuf};

use crate::error::{AppError, AppResult};
use crate::organizer::plan::{LinkPlan, Op};

/// Execute a [`LinkPlan`]. Performs a pre-flight conflict check on the
/// target folder, then runs each operation. On failure, attempts a best-
/// effort rollback of completed `MoveFile` operations.
pub fn execute_link(plan: &LinkPlan) -> AppResult<()> {
    if plan.target_folder.exists() {
        return Err(AppError::Conflict(format!(
            "target folder already exists: {}",
            plan.target_folder.display()
        )));
    }
    execute_ops(&plan.ops)
}

/// Execute a generic op list with rollback. Side-effecting.
pub fn execute_ops(ops: &[Op]) -> AppResult<()> {
    let mut done: Vec<Op> = Vec::with_capacity(ops.len());

    for op in ops {
        match perform(op) {
            Ok(()) => done.push(op.clone()),
            Err(e) => {
                rollback(&done);
                return Err(e);
            }
        }
    }
    Ok(())
}

fn perform(op: &Op) -> AppResult<()> {
    match op {
        Op::CreateDir(p) => {
            std::fs::create_dir_all(p).map_err(AppError::from)?;
            Ok(())
        }
        Op::MoveFile { from, to } => move_path(from, to),
        Op::MoveDir { from, to } => move_path(from, to),
        Op::RemoveEmptyDir(p) => {
            // Best-effort; ignore "not empty" / "not found".
            let _ = std::fs::remove_dir(p);
            Ok(())
        }
    }
}

/// Rename if same volume, else copy+delete. Creates parent dirs.
fn move_path(from: &Path, to: &Path) -> AppResult<()> {
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent).map_err(AppError::from)?;
    }
    if to.exists() {
        return Err(AppError::Conflict(format!(
            "destination exists: {}",
            to.display()
        )));
    }
    match std::fs::rename(from, to) {
        Ok(()) => Ok(()),
        Err(_) => {
            // Cross-volume fallback. Should never happen since we stay
            // within HD root, but be safe.
            if from.is_dir() {
                copy_dir_all(from, to)?;
                std::fs::remove_dir_all(from).map_err(AppError::from)?;
            } else {
                std::fs::copy(from, to).map_err(AppError::from)?;
                std::fs::remove_file(from).map_err(AppError::from)?;
            }
            Ok(())
        }
    }
}

fn copy_dir_all(src: &Path, dst: &Path) -> AppResult<()> {
    std::fs::create_dir_all(dst).map_err(AppError::from)?;
    for entry in std::fs::read_dir(src).map_err(AppError::from)? {
        let entry = entry.map_err(AppError::from)?;
        let kind = entry.file_type().map_err(AppError::from)?;
        let to = dst.join(entry.file_name());
        if kind.is_dir() {
            copy_dir_all(&entry.path(), &to)?;
        } else {
            std::fs::copy(entry.path(), to).map_err(AppError::from)?;
        }
    }
    Ok(())
}

/// Best-effort rollback: reverse the order, swap `from`/`to`.
fn rollback(done: &[Op]) {
    for op in done.iter().rev() {
        match op {
            Op::MoveFile { from, to } | Op::MoveDir { from, to } => {
                let _ = std::fs::rename(to, from);
            }
            Op::CreateDir(p) => {
                // Try to remove if empty.
                let _ = std::fs::remove_dir(p);
            }
            Op::RemoveEmptyDir(_) => {}
        }
    }
}

/// Plan + execute a "merge two genre folders" operation: every immediate
/// child of `from_dir` is moved into `to_dir`. `from_dir` is removed if
/// it ends up empty.
///
/// Conflict policy: if a child already exists in `to_dir`, abort with
/// [`AppError::Conflict`].
pub fn merge_genre_folders(from_dir: &Path, to_dir: &Path) -> AppResult<Vec<PathBuf>> {
    if from_dir == to_dir {
        return Ok(vec![]);
    }
    if !from_dir.exists() {
        return Ok(vec![]);
    }
    std::fs::create_dir_all(to_dir).map_err(AppError::from)?;

    let mut moved = Vec::new();
    let mut ops = Vec::new();
    for entry in std::fs::read_dir(from_dir).map_err(AppError::from)? {
        let entry = entry.map_err(AppError::from)?;
        let dest = to_dir.join(entry.file_name());
        if dest.exists() {
            return Err(AppError::Conflict(format!(
                "merge conflict: {} already exists",
                dest.display()
            )));
        }
        let kind = entry.file_type().map_err(AppError::from)?;
        let from = entry.path();
        moved.push(dest.clone());
        ops.push(if kind.is_dir() {
            Op::MoveDir { from, to: dest }
        } else {
            Op::MoveFile { from, to: dest }
        });
    }
    ops.push(Op::RemoveEmptyDir(from_dir.to_path_buf()));
    execute_ops(&ops)?;
    Ok(moved)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::organizer::plan::{plan_link, LinkPlanInput};
    use std::fs;
    use tempfile::TempDir;

    fn touch(p: &Path) {
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(p, b"x").unwrap();
    }

    #[test]
    fn execute_link_moves_video_and_sidecars() {
        let tmp = TempDir::new().unwrap();
        let hd = tmp.path();
        let src = hd.join("Uncatalogued/raw");
        touch(&src.join("video.mkv"));
        touch(&src.join("video.en.srt"));

        let siblings = vec!["video.mkv".to_string(), "video.en.srt".to_string()];
        let plan = plan_link(&LinkPlanInput {
            hd_root: hd,
            kind_root_label: "Movies",
            genre_folder: Some("Action"),
            current_folder: &src,
            video_filename: "video.mkv",
            siblings: &siblings,
            tmdb_id: 27205,
            title: "Inception",
        });

        execute_link(&plan).unwrap();

        let target = hd.join("Movies/Action/Inception [tmdb-27205]");
        assert!(target.is_dir());
        assert!(target.join("Inception [tmdb-27205].mkv").is_file());
        assert!(target.join("Inception [tmdb-27205].en.srt").is_file());
        assert!(!src.join("video.mkv").exists());
    }

    #[test]
    fn execute_link_aborts_on_conflict() {
        let tmp = TempDir::new().unwrap();
        let hd = tmp.path();
        let src = hd.join("Uncatalogued/raw");
        touch(&src.join("video.mkv"));
        // Pre-create the conflicting target folder.
        fs::create_dir_all(hd.join("Movies/Action/Inception [tmdb-27205]")).unwrap();

        let siblings = vec!["video.mkv".to_string()];
        let plan = plan_link(&LinkPlanInput {
            hd_root: hd,
            kind_root_label: "Movies",
            genre_folder: Some("Action"),
            current_folder: &src,
            video_filename: "video.mkv",
            siblings: &siblings,
            tmdb_id: 27205,
            title: "Inception",
        });

        let err = execute_link(&plan).unwrap_err();
        match err {
            AppError::Conflict(_) => {}
            other => panic!("expected Conflict, got {other:?}"),
        }
        // Source must still be intact (pre-flight check, no ops attempted).
        assert!(src.join("video.mkv").exists());
    }

    #[test]
    fn merge_genre_folders_moves_children_and_removes_source() {
        let tmp = TempDir::new().unwrap();
        let hd = tmp.path();
        let from = hd.join("Movies/Adventure");
        let to = hd.join("Movies/Ação");
        fs::create_dir_all(from.join("Up [tmdb-14160]")).unwrap();
        touch(&from.join("Up [tmdb-14160]/Up [tmdb-14160].mkv"));
        fs::create_dir_all(&to).unwrap();

        let moved = merge_genre_folders(&from, &to).unwrap();
        assert_eq!(moved.len(), 1);
        assert!(to.join("Up [tmdb-14160]/Up [tmdb-14160].mkv").is_file());
        assert!(!from.exists());
    }

    #[test]
    fn merge_genre_folders_conflict_when_child_already_exists() {
        let tmp = TempDir::new().unwrap();
        let hd = tmp.path();
        let from = hd.join("Movies/Adventure");
        let to = hd.join("Movies/Ação");
        fs::create_dir_all(from.join("Up [tmdb-14160]")).unwrap();
        fs::create_dir_all(to.join("Up [tmdb-14160]")).unwrap();

        let err = merge_genre_folders(&from, &to).unwrap_err();
        assert!(matches!(err, AppError::Conflict(_)));
        // From folder must still exist (pre-flight conflict).
        assert!(from.join("Up [tmdb-14160]").exists());
    }
}
