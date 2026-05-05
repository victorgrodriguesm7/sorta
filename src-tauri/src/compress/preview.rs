//! Preview-segment generation: find a video file, pick a sample window,
//! re-encode at multiple CRFs side-by-side for comparison.

use std::path::{Path, PathBuf};
use std::process::Stdio;

use serde::{Deserialize, Serialize};
use tokio::process::Command;

use crate::compress::ffmpeg::{build_encode_argv, parse_hms, Codec, EncodeArgs};
use crate::error::{AppError, AppResult};
use crate::scanner::classify::is_video_file;

/// One produced preview clip + the metrics needed for the comparison UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreviewClip {
    pub crf: i32,
    pub path: PathBuf,
    pub size_bytes: u64,
    pub source_size_bytes: u64,
    pub original_segment_size_bytes: u64,
    pub ratio: f64, // 1.0 - (size / original_segment_size)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreviewBundle {
    pub source_path: PathBuf,
    pub source_duration_seconds: f64,
    pub start_seconds: f64,
    pub duration_seconds: f64,
    pub original_segment_size_bytes: u64,
    pub clips: Vec<PreviewClip>,
}

/// Recursively collect every video file under `folder`. Sorted for
/// deterministic ordering (alphabetical, full path).
pub fn collect_video_files(folder: &Path) -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(rd) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in rd.flatten() {
            let p = entry.path();
            if p.is_dir() {
                walk(&p, out);
            } else if is_video_file(&p) {
                out.push(p);
            }
        }
    }
    let mut out = Vec::new();
    walk(folder, &mut out);
    out.sort();
    out
}

/// Total size in bytes of every video file under `folder`.
pub fn folder_video_bytes(folder: &Path) -> u64 {
    collect_video_files(folder)
        .iter()
        .filter_map(|p| std::fs::metadata(p).ok().map(|m| m.len()))
        .sum()
}

/// Run `ffprobe` to get the duration (seconds) of a media file.
pub async fn probe_duration(ffprobe: &Path, file: &Path) -> AppResult<f64> {
    let out = Command::new(ffprobe)
        .args(["-v", "error", "-show_entries", "format=duration", "-of", "default=nw=1:nk=1"])
        .arg(file)
        .output()
        .await
        .map_err(AppError::from)?;
    if !out.status.success() {
        return Err(AppError::Other(format!(
            "ffprobe duration failed: {}",
            String::from_utf8_lossy(&out.stderr)
        )));
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    s.parse::<f64>()
        .ok()
        .or_else(|| parse_hms(&s))
        .ok_or_else(|| AppError::Other(format!("could not parse duration {s:?}")))
}

/// Verify a file's structural integrity. Returns Ok(()) iff ffprobe
/// finds no errors. Cheap: a few hundred ms even for big files.
pub async fn verify_structural(ffprobe: &Path, file: &Path) -> AppResult<()> {
    let out = Command::new(ffprobe)
        .args(["-v", "error", "-i"])
        .arg(file)
        .args(["-f", "null", "-"])
        .stdout(Stdio::null())
        .output()
        .await
        .map_err(AppError::from)?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(AppError::Other(format!("structural verify failed: {err}")));
    }
    let stderr = String::from_utf8_lossy(&out.stderr);
    if !stderr.trim().is_empty() {
        // ffprobe with `-v error` only writes stderr when errors occur.
        return Err(AppError::Other(format!("verify reported: {stderr}")));
    }
    Ok(())
}

/// Exhaustive verify — ffmpeg decodes every frame to /dev/null and
/// reports any decode errors. Slow (linear with file length) but
/// catches content corruption, not just structural issues.
pub async fn verify_exhaustive(ffmpeg: &Path, file: &Path) -> AppResult<()> {
    let out = Command::new(ffmpeg)
        .args(["-v", "error", "-i"])
        .arg(file)
        .args(["-f", "null", "-"])
        .stdout(Stdio::null())
        .output()
        .await
        .map_err(AppError::from)?;
    if !out.status.success() {
        return Err(AppError::Other(format!(
            "exhaustive verify failed: {}",
            String::from_utf8_lossy(&out.stderr)
        )));
    }
    let stderr = String::from_utf8_lossy(&out.stderr);
    if !stderr.trim().is_empty() {
        return Err(AppError::Other(format!("verify reported: {stderr}")));
    }
    Ok(())
}

/// Encode one preview segment. Returns the bytes written.
pub async fn encode_preview(
    ffmpeg: &Path,
    input: &Path,
    output: &Path,
    codec: Codec,
    crf: i32,
    downscale_720p: bool,
    start: f64,
    duration: f64,
) -> AppResult<u64> {
    let args = EncodeArgs {
        input,
        output,
        codec,
        crf,
        downscale_720p,
        start_seconds: Some(start),
        duration_seconds: Some(duration),
        progress_to_stderr: false,
    };
    let argv = build_encode_argv(&args);
    let status = Command::new(ffmpeg)
        .args(&argv)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await
        .map_err(AppError::from)?;
    if !status.success() {
        return Err(AppError::Other(format!(
            "ffmpeg preview encode failed (exit {:?})",
            status.code()
        )));
    }
    Ok(std::fs::metadata(output).map_err(AppError::from)?.len())
}

/// Best-effort: copy a passthrough segment to measure the *original*
/// bytes that the picked window would account for. Avoids using a
/// fraction-of-runtime estimate which can be wildly off for VBR files.
pub async fn measure_original_segment(
    ffmpeg: &Path,
    input: &Path,
    output: &Path,
    start: f64,
    duration: f64,
) -> AppResult<u64> {
    let argv = vec![
        "-y".to_string(),
        "-hide_banner".into(),
        "-ss".into(),
        format!("{start:.3}"),
        "-i".into(),
        input.to_string_lossy().to_string(),
        "-t".into(),
        format!("{duration:.3}"),
        "-c".into(),
        "copy".into(),
        output.to_string_lossy().to_string(),
    ];
    let status = Command::new(ffmpeg)
        .args(&argv)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await
        .map_err(AppError::from)?;
    if !status.success() {
        return Err(AppError::Other("segment-copy failed".into()));
    }
    Ok(std::fs::metadata(output).map_err(AppError::from)?.len())
}
