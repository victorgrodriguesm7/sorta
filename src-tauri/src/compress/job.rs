//! Long-running compression job: walk every video file in a media
//! folder, encode → verify → swap, with progress events and
//! cancellation.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::compress::ffmpeg::{
    apply_progress_line, build_encode_argv, estimate_eta_seconds, Codec, EncodeArgs, ProgressTick,
};
use crate::compress::preview::{
    collect_video_files, probe_duration, verify_exhaustive, verify_structural,
};

/// Settings for a single compression job.
#[derive(Debug, Clone, Deserialize)]
pub struct JobSettings {
    pub codec: Codec,
    pub crf: i32,
    pub downscale_720p: bool,
    pub exhaustive_verify: bool,
}

/// Live status of an in-flight job (snapshot, sent to the UI).
#[derive(Debug, Clone, Serialize)]
pub struct JobProgress {
    pub job_id: String,
    pub current_file_index: usize,
    pub total_files: usize,
    pub current_file_name: String,
    pub current_file_duration_seconds: f64,
    pub current_file_position_seconds: f64,
    pub current_file_speed: Option<f64>,
    pub eta_current_file_seconds: Option<f64>,
    pub eta_total_seconds: Option<f64>,
    pub state: JobState,
    pub bytes_saved: i64,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum JobState {
    Encoding,
    Verifying,
    Swapping,
    Done,
    Cancelled,
    Failed,
}

/// Shared cancellation flag stored in the app state.
#[derive(Debug, Clone, Default)]
pub struct CancelFlag(pub Arc<std::sync::atomic::AtomicBool>);

impl CancelFlag {
    pub fn cancel(&self) {
        self.0
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(std::sync::atomic::Ordering::SeqCst)
    }
}

/// In-memory registry of running jobs, keyed by job id. Held inside
/// the Tauri AppState. The Mutex protects the HashMap; the per-job
/// CancelFlag uses an internal AtomicBool, so the mutex isn't held
/// across the actual ffmpeg run.
#[derive(Debug, Default, Clone)]
pub struct JobRegistry {
    inner: Arc<Mutex<std::collections::HashMap<String, CancelFlag>>>,
}

impl JobRegistry {
    pub async fn register(&self, id: &str, flag: CancelFlag) {
        self.inner.lock().await.insert(id.to_string(), flag);
    }
    pub async fn cancel(&self, id: &str) -> bool {
        if let Some(f) = self.inner.lock().await.get(id) {
            f.cancel();
            true
        } else {
            false
        }
    }
    pub async fn unregister(&self, id: &str) {
        self.inner.lock().await.remove(id);
    }
}

pub fn new_job_id() -> String {
    Uuid::new_v4().to_string()
}

/// Result of a single file's compression attempt.
#[derive(Debug, Clone, Serialize)]
pub struct PerFileOutcome {
    pub file: PathBuf,
    pub original_bytes: u64,
    pub compressed_bytes: Option<u64>,
    pub error: Option<String>,
    pub skipped: bool,
}

/// Final result emitted when the job ends.
#[derive(Debug, Clone, Serialize)]
pub struct JobReport {
    pub job_id: String,
    pub state: JobState,
    pub outcomes: Vec<PerFileOutcome>,
    pub total_original_bytes: u64,
    pub total_compressed_bytes: u64,
}

/// Trait the runner uses to publish progress + final reports. Tauri
/// wires this to `AppHandle::emit`; tests can use a recording impl.
pub trait JobReporter: Send + Sync {
    fn progress(&self, p: &JobProgress);
    fn report(&self, r: &JobReport);
}

/// Compute the path of the renamed-original sibling: `<name>.original.<ext>`.
pub fn original_sidecar_path(file: &Path) -> PathBuf {
    let stem = file
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let ext = file
        .extension()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let parent = file.parent().unwrap_or_else(|| Path::new(""));
    if ext.is_empty() {
        parent.join(format!("{stem}.original"))
    } else {
        parent.join(format!("{stem}.original.{ext}"))
    }
}

/// Compute the path of the temporary in-progress encode: `<name>.compressing.<ext>`.
pub fn temp_encode_path(file: &Path) -> PathBuf {
    let stem = file
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let ext = file
        .extension()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let parent = file.parent().unwrap_or_else(|| Path::new(""));
    if ext.is_empty() {
        parent.join(format!("{stem}.compressing"))
    } else {
        parent.join(format!("{stem}.compressing.{ext}"))
    }
}

/// True if `file` already ends in `.original.<ext>` — i.e. it's a leftover
/// backup we should skip during a fresh encode pass.
pub fn is_original_backup(file: &Path) -> bool {
    file.file_stem()
        .and_then(|s| s.to_str())
        .map(|s| s.ends_with(".original"))
        .unwrap_or(false)
}

/// True if `file` ends in `.compressing.<ext>` (an interrupted prior run).
pub fn is_temp_encode(file: &Path) -> bool {
    file.file_stem()
        .and_then(|s| s.to_str())
        .map(|s| s.ends_with(".compressing"))
        .unwrap_or(false)
}

/// Run the full compression pass on every video under `media_folder`.
pub async fn run_job(
    ffmpeg: PathBuf,
    ffprobe: PathBuf,
    media_folder: PathBuf,
    settings: JobSettings,
    cancel: CancelFlag,
    reporter: Arc<dyn JobReporter>,
    job_id: String,
) -> JobReport {
    let mut all_files = collect_video_files(&media_folder);
    // Ignore .original.* and .compressing.* leftovers.
    all_files.retain(|p| !is_original_backup(p) && !is_temp_encode(p));

    let total_files = all_files.len();
    let mut outcomes: Vec<PerFileOutcome> = Vec::with_capacity(total_files);
    let mut total_original_bytes: u64 = 0;
    let mut total_compressed_bytes: u64 = 0;
    let mut state = JobState::Done;

    for (idx, file) in all_files.iter().enumerate() {
        if cancel.is_cancelled() {
            state = JobState::Cancelled;
            break;
        }

        let file_name = file
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        let original_bytes = std::fs::metadata(file).map(|m| m.len()).unwrap_or(0);
        total_original_bytes += original_bytes;

        let mut outcome = PerFileOutcome {
            file: file.clone(),
            original_bytes,
            compressed_bytes: None,
            error: None,
            skipped: false,
        };

        let duration = match probe_duration(&ffprobe, file).await {
            Ok(d) => d,
            Err(e) => {
                outcome.error = Some(format!("ffprobe duration: {e}"));
                outcomes.push(outcome);
                continue;
            }
        };

        // Initial progress tick for this file.
        reporter.progress(&JobProgress {
            job_id: job_id.clone(),
            current_file_index: idx,
            total_files,
            current_file_name: file_name.clone(),
            current_file_duration_seconds: duration,
            current_file_position_seconds: 0.0,
            current_file_speed: None,
            eta_current_file_seconds: None,
            eta_total_seconds: None,
            state: JobState::Encoding,
            bytes_saved: total_original_bytes as i64 - total_compressed_bytes as i64,
        });

        let temp = temp_encode_path(file);
        // Best-effort cleanup of any prior leftover.
        let _ = std::fs::remove_file(&temp);

        let argv = build_encode_argv(&EncodeArgs {
            input: file,
            output: &temp,
            codec: settings.codec,
            crf: settings.crf,
            downscale_720p: settings.downscale_720p,
            start_seconds: None,
            duration_seconds: None,
            progress_to_stderr: true,
        });

        let mut child = match Command::new(&ffmpeg)
            .args(&argv)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                outcome.error = Some(format!("spawn ffmpeg: {e}"));
                outcomes.push(outcome);
                continue;
            }
        };

        let stderr = child.stderr.take().expect("piped stderr");
        let mut reader = BufReader::new(stderr).lines();
        let mut tick = ProgressTick::default();

        loop {
            tokio::select! {
                line = reader.next_line() => {
                    match line {
                        Ok(Some(l)) => {
                            apply_progress_line(&l, &mut tick);
                            if let (Some(pos), Some(speed)) = (tick.time_position_seconds, tick.speed) {
                                let eta_file = estimate_eta_seconds(pos, duration, speed);
                                reporter.progress(&JobProgress {
                                    job_id: job_id.clone(),
                                    current_file_index: idx,
                                    total_files,
                                    current_file_name: file_name.clone(),
                                    current_file_duration_seconds: duration,
                                    current_file_position_seconds: pos,
                                    current_file_speed: Some(speed),
                                    eta_current_file_seconds: eta_file,
                                    // crude total-ETA estimate: assume remaining
                                    // files take roughly the same wall time as the
                                    // current one.
                                    eta_total_seconds: eta_file.map(|s| {
                                        let remaining = total_files.saturating_sub(idx + 1) as f64;
                                        s * (1.0 + remaining)
                                    }),
                                    state: JobState::Encoding,
                                    bytes_saved: total_original_bytes as i64 - total_compressed_bytes as i64,
                                });
                            }
                        }
                        Ok(None) => break,
                        Err(_) => break,
                    }
                }
                _ = tokio::time::sleep(std::time::Duration::from_millis(500)) => {
                    if cancel.is_cancelled() {
                        let _ = child.start_kill();
                        break;
                    }
                }
            }
        }
        let status = child.wait().await;

        if cancel.is_cancelled() {
            let _ = std::fs::remove_file(&temp);
            state = JobState::Cancelled;
            outcome.error = Some("cancelled".into());
            outcomes.push(outcome);
            break;
        }

        match status {
            Ok(s) if s.success() => {}
            Ok(s) => {
                outcome.error = Some(format!("ffmpeg exit {:?}", s.code()));
                let _ = std::fs::remove_file(&temp);
                outcomes.push(outcome);
                continue;
            }
            Err(e) => {
                outcome.error = Some(format!("ffmpeg wait: {e}"));
                let _ = std::fs::remove_file(&temp);
                outcomes.push(outcome);
                continue;
            }
        }

        // Verify.
        reporter.progress(&JobProgress {
            job_id: job_id.clone(),
            current_file_index: idx,
            total_files,
            current_file_name: file_name.clone(),
            current_file_duration_seconds: duration,
            current_file_position_seconds: duration,
            current_file_speed: None,
            eta_current_file_seconds: Some(0.0),
            eta_total_seconds: None,
            state: JobState::Verifying,
            bytes_saved: total_original_bytes as i64 - total_compressed_bytes as i64,
        });
        let verify_res = if settings.exhaustive_verify {
            verify_exhaustive(&ffmpeg, &temp).await
        } else {
            verify_structural(&ffprobe, &temp).await
        };
        if let Err(e) = verify_res {
            outcome.error = Some(format!("verify: {e}"));
            let _ = std::fs::remove_file(&temp);
            outcomes.push(outcome);
            continue;
        }

        // Swap: rename original → .original.ext, then temp → original name.
        reporter.progress(&JobProgress {
            job_id: job_id.clone(),
            current_file_index: idx,
            total_files,
            current_file_name: file_name.clone(),
            current_file_duration_seconds: duration,
            current_file_position_seconds: duration,
            current_file_speed: None,
            eta_current_file_seconds: Some(0.0),
            eta_total_seconds: None,
            state: JobState::Swapping,
            bytes_saved: total_original_bytes as i64 - total_compressed_bytes as i64,
        });
        let backup = original_sidecar_path(file);
        if let Err(e) = std::fs::rename(file, &backup) {
            outcome.error = Some(format!("rename original: {e}"));
            let _ = std::fs::remove_file(&temp);
            outcomes.push(outcome);
            continue;
        }
        if let Err(e) = std::fs::rename(&temp, file) {
            outcome.error = Some(format!("rename compressed: {e}"));
            // try to restore the original
            let _ = std::fs::rename(&backup, file);
            outcomes.push(outcome);
            continue;
        }

        let compressed_bytes = std::fs::metadata(file).map(|m| m.len()).unwrap_or(0);
        total_compressed_bytes += compressed_bytes;
        outcome.compressed_bytes = Some(compressed_bytes);
        outcomes.push(outcome);
    }

    let report = JobReport {
        job_id: job_id.clone(),
        state,
        outcomes,
        total_original_bytes,
        total_compressed_bytes,
    };
    reporter.report(&report);
    report
}

/// Sweep a media folder removing every `.original.<ext>` backup.
/// Returns (files removed, bytes freed).
pub fn cleanup_originals(folder: &Path) -> (usize, u64) {
    let files = collect_video_files(folder);
    let mut count = 0;
    let mut bytes = 0u64;
    for f in files {
        if !is_original_backup(&f) {
            continue;
        }
        if let Ok(md) = std::fs::metadata(&f) {
            bytes += md.len();
        }
        if std::fs::remove_file(&f).is_ok() {
            count += 1;
        }
    }
    (count, bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn original_sidecar_path_appends_dot_original() {
        assert_eq!(
            original_sidecar_path(&PathBuf::from("/x/Movie.mkv")),
            PathBuf::from("/x/Movie.original.mkv")
        );
        assert_eq!(
            original_sidecar_path(&PathBuf::from("/x/Movie")),
            PathBuf::from("/x/Movie.original")
        );
    }

    #[test]
    fn temp_encode_path_appends_compressing() {
        assert_eq!(
            temp_encode_path(&PathBuf::from("/x/Movie.mkv")),
            PathBuf::from("/x/Movie.compressing.mkv")
        );
    }

    #[test]
    fn detects_backup_and_temp_files() {
        assert!(is_original_backup(&PathBuf::from("/x/Movie.original.mkv")));
        assert!(!is_original_backup(&PathBuf::from("/x/Movie.mkv")));
        assert!(is_temp_encode(&PathBuf::from("/x/Movie.compressing.mkv")));
        assert!(!is_temp_encode(&PathBuf::from("/x/Movie.mkv")));
    }
}
