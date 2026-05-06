//! Compression commands (ffmpeg-backed).

use std::path::PathBuf;
use std::sync::Arc;

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};
use tokio::process::Command;

use crate::compress::ffmpeg::{
    parse_hwaccels_from_encoders, parse_version_output, probe_binaries, Codec, FfmpegStatus,
};
use crate::compress::job::{
    cleanup_originals, new_job_id, run_job, CancelFlag, JobProgress, JobReport, JobReporter,
    JobSettings,
};
use crate::compress::preview::{
    encode_preview, folder_video_bytes, measure_original_segment, probe_duration, PreviewClip,
};
use crate::db::media::find_by_id;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

#[tauri::command]
pub async fn ffmpeg_status() -> AppResult<FfmpegStatus> {
    let mut status = probe_binaries();
    if let Some(ff) = status.ffmpeg_path.clone() {
        if let Ok(out) = Command::new(&ff).arg("-version").output().await {
            let text = String::from_utf8_lossy(&out.stdout).to_string();
            status.ffmpeg_version = parse_version_output(&text);
        }
        if let Ok(out) = Command::new(&ff).arg("-encoders").output().await {
            let text = String::from_utf8_lossy(&out.stdout).to_string();
            status.hwaccels = parse_hwaccels_from_encoders(&text);
        }
    }
    Ok(status)
}

#[tauri::command]
pub async fn media_total_bytes(state: State<'_, AppState>, media_id: i64) -> AppResult<u64> {
    let (pool, hd_root) = {
        let s = state.read().await;
        (
            s.db.clone()
                .ok_or_else(|| AppError::Other("DB not initialized".into()))?,
            s.hd_root
                .clone()
                .ok_or_else(|| AppError::Other("HD not set".into()))?,
        )
    };
    let row = find_by_id(&pool, media_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("media {media_id}")))?;
    let folder = hd_root.join(&row.folder_path);
    Ok(folder_video_bytes(&folder))
}

#[derive(Debug, Deserialize)]
pub struct PreviewArgs {
    pub media_id: i64,
    pub crfs: Vec<i32>,
    pub codec: Option<Codec>,
    pub downscale_720p: bool,
    /// Override start position; otherwise default = 25% of duration.
    pub start_seconds: Option<f64>,
    /// Defaults to 15 s.
    pub duration_seconds: Option<f64>,
}

#[tauri::command]
pub async fn generate_compression_preview(
    state: State<'_, AppState>,
    args: PreviewArgs,
) -> AppResult<PreviewBundleDto> {
    let (pool, hd_root) = {
        let s = state.read().await;
        (
            s.db.clone()
                .ok_or_else(|| AppError::Other("DB not initialized".into()))?,
            s.hd_root
                .clone()
                .ok_or_else(|| AppError::Other("HD not set".into()))?,
        )
    };
    let row = find_by_id(&pool, args.media_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("media {}", args.media_id)))?;
    let folder = hd_root.join(&row.folder_path);

    let status = ffmpeg_status().await?;
    if !status.is_ready() {
        return Err(AppError::Other(
            "ffmpeg/ffprobe not found on PATH".into(),
        ));
    }
    let ffmpeg = status.ffmpeg_path.unwrap();
    let ffprobe = status.ffprobe_path.unwrap();

    let codec = args
        .codec
        .unwrap_or(Codec::Hevc); // sensible default if caller omits

    // Pick the first video file in the folder (typically the movie itself
    // or S01E01 for series).
    let videos = crate::compress::preview::collect_video_files(&folder);
    let videos: Vec<PathBuf> = videos
        .into_iter()
        .filter(|p| {
            let s = p.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            !s.ends_with(".original") && !s.ends_with(".compressing")
        })
        .collect();
    let source = videos
        .into_iter()
        .next()
        .ok_or_else(|| AppError::NotFound(format!("no video in {}", folder.display())))?;

    let total_duration = probe_duration(&ffprobe, &source).await?;
    let duration = args.duration_seconds.unwrap_or(15.0).clamp(2.0, 120.0);
    let start = args
        .start_seconds
        .unwrap_or((total_duration * 0.25).max(0.0))
        .clamp(0.0, (total_duration - duration).max(0.0));

    let source_size = std::fs::metadata(&source).map(|m| m.len()).unwrap_or(0);

    // Total bytes the *whole* media folder currently uses — used to
    // extrapolate an estimated-final-size for each preview clip.
    let total_media_bytes = folder_video_bytes(&folder);

    // Temp directory under the system temp.
    let job_id = new_job_id();
    let tmp_dir = std::env::temp_dir().join(format!("sorta-preview-{job_id}"));
    std::fs::create_dir_all(&tmp_dir).map_err(AppError::from)?;

    // Step 1: extract the segment ONCE via stream-copy, into a working
    // source file. The previous "measure_original_segment" did the same
    // thing but the previews transcoded from the FULL source — so the
    // 'original size' shown to the user wasn't comparable to the encoded
    // sizes. Encoding everything from this working source guarantees an
    // apples-to-apples comparison.
    let working_source = tmp_dir.join("source.mkv");
    let working_source_bytes =
        measure_original_segment(&ffmpeg, &source, &working_source, start, duration).await?;

    let mut clips = Vec::new();
    for crf in args.crfs.iter().copied() {
        let out = tmp_dir.join(format!("crf-{crf}.mkv"));
        // Encode the WORKING SOURCE (not the full media) — start = 0,
        // duration = full segment length.
        let bytes = encode_preview(
            &ffmpeg,
            &working_source,
            &out,
            codec,
            crf,
            args.downscale_720p,
            0.0,
            duration,
        )
        .await?;
        let ratio = if working_source_bytes > 0 {
            1.0 - (bytes as f64 / working_source_bytes as f64)
        } else {
            0.0
        };
        clips.push(PreviewClip {
            crf,
            path: out,
            size_bytes: bytes,
            source_size_bytes: source_size,
            original_segment_size_bytes: working_source_bytes,
            ratio,
        });
    }

    // Read each clip into a base64 data URL for the UI <video> tag,
    // and extrapolate an estimated FINAL size by scaling the
    // (preview / source) ratio across the entire media folder.
    let mut dto_clips = Vec::with_capacity(clips.len());
    for c in clips.iter() {
        let bytes = std::fs::read(&c.path).map_err(AppError::from)?;
        let mime = "video/x-matroska";
        let data_url = format!("data:{mime};base64,{}", B64.encode(&bytes));
        let estimated_final_bytes = if working_source_bytes > 0 {
            ((total_media_bytes as f64) * (c.size_bytes as f64)
                / (working_source_bytes as f64))
                .round() as u64
        } else {
            0
        };
        dto_clips.push(PreviewClipDto {
            crf: c.crf,
            size_bytes: c.size_bytes,
            ratio: c.ratio,
            data_url,
            estimated_final_bytes,
        });
    }
    let original_bytes = std::fs::read(&working_source).map_err(AppError::from)?;
    let original_data_url = format!(
        "data:video/x-matroska;base64,{}",
        B64.encode(&original_bytes)
    );

    Ok(PreviewBundleDto {
        source_path: source,
        source_duration_seconds: total_duration,
        start_seconds: start,
        duration_seconds: duration,
        original_segment_size_bytes: working_source_bytes,
        total_media_bytes,
        original_data_url,
        clips: dto_clips,
        tmp_dir,
    })
}

#[derive(Debug, Serialize)]
pub struct PreviewClipDto {
    pub crf: i32,
    pub size_bytes: u64,
    pub ratio: f64,
    pub data_url: String,
    /// Extrapolated final size for the WHOLE media folder if this CRF
    /// were applied: total_media_bytes * (preview_size / source_size).
    pub estimated_final_bytes: u64,
}

#[derive(Debug, Serialize)]
pub struct PreviewBundleDto {
    pub source_path: PathBuf,
    pub source_duration_seconds: f64,
    pub start_seconds: f64,
    pub duration_seconds: f64,
    pub original_segment_size_bytes: u64,
    pub total_media_bytes: u64,
    pub original_data_url: String,
    pub clips: Vec<PreviewClipDto>,
    pub tmp_dir: PathBuf,
}

#[derive(Debug, Deserialize)]
pub struct StartCompressionArgs {
    pub media_id: i64,
    pub codec: Codec,
    pub crf: i32,
    pub downscale_720p: bool,
    pub exhaustive_verify: bool,
}

#[derive(Debug, Serialize)]
pub struct StartCompressionResult {
    pub job_id: String,
}

struct AppHandleReporter {
    app: AppHandle,
}

impl JobReporter for AppHandleReporter {
    fn progress(&self, p: &JobProgress) {
        let _ = self.app.emit("compression-progress", p);
    }
    fn report(&self, r: &JobReport) {
        let _ = self.app.emit("compression-report", r);
    }
}

#[tauri::command]
pub async fn start_compression(
    app: AppHandle,
    state: State<'_, AppState>,
    args: StartCompressionArgs,
) -> AppResult<StartCompressionResult> {
    let (pool, hd_root, jobs) = {
        let s = state.read().await;
        (
            s.db.clone()
                .ok_or_else(|| AppError::Other("DB not initialized".into()))?,
            s.hd_root
                .clone()
                .ok_or_else(|| AppError::Other("HD not set".into()))?,
            s.jobs.clone(),
        )
    };
    let row = find_by_id(&pool, args.media_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("media {}", args.media_id)))?;
    let folder = hd_root.join(&row.folder_path);

    let status = ffmpeg_status().await?;
    if !status.is_ready() {
        return Err(AppError::Other(
            "ffmpeg/ffprobe not found on PATH".into(),
        ));
    }
    let ffmpeg = status.ffmpeg_path.unwrap();
    let ffprobe = status.ffprobe_path.unwrap();

    let job_id = new_job_id();
    let cancel = CancelFlag::default();
    jobs.register(&job_id, cancel.clone()).await;

    let reporter: Arc<dyn JobReporter> = Arc::new(AppHandleReporter { app });
    let job_id_for_task = job_id.clone();
    let jobs_for_task = jobs.clone();
    tokio::spawn(async move {
        let _ = run_job(
            ffmpeg,
            ffprobe,
            folder,
            JobSettings {
                codec: args.codec,
                crf: args.crf,
                downscale_720p: args.downscale_720p,
                exhaustive_verify: args.exhaustive_verify,
            },
            cancel,
            reporter,
            job_id_for_task.clone(),
        )
        .await;
        jobs_for_task.unregister(&job_id_for_task).await;
    });

    Ok(StartCompressionResult { job_id })
}

#[tauri::command]
pub async fn cancel_compression(
    state: State<'_, AppState>,
    job_id: String,
) -> AppResult<bool> {
    let jobs = {
        let s = state.read().await;
        s.jobs.clone()
    };
    Ok(jobs.cancel(&job_id).await)
}

#[derive(Debug, Serialize)]
pub struct CleanupResult {
    pub files_removed: usize,
    pub bytes_freed: u64,
}

#[tauri::command]
pub async fn cleanup_originals_for(
    state: State<'_, AppState>,
    media_id: i64,
) -> AppResult<CleanupResult> {
    let (pool, hd_root) = {
        let s = state.read().await;
        (
            s.db.clone()
                .ok_or_else(|| AppError::Other("DB not initialized".into()))?,
            s.hd_root
                .clone()
                .ok_or_else(|| AppError::Other("HD not set".into()))?,
        )
    };
    let row = find_by_id(&pool, media_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("media {media_id}")))?;
    let (n, b) = cleanup_originals(&hd_root.join(&row.folder_path));
    Ok(CleanupResult {
        files_removed: n,
        bytes_freed: b,
    })
}

#[tauri::command]
pub async fn has_original_backups(
    state: State<'_, AppState>,
    media_id: i64,
) -> AppResult<bool> {
    let (pool, hd_root) = {
        let s = state.read().await;
        (
            s.db.clone()
                .ok_or_else(|| AppError::Other("DB not initialized".into()))?,
            s.hd_root
                .clone()
                .ok_or_else(|| AppError::Other("HD not set".into()))?,
        )
    };
    let row = find_by_id(&pool, media_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("media {media_id}")))?;
    let folder = hd_root.join(&row.folder_path);
    let any = crate::compress::preview::collect_video_files(&folder)
        .iter()
        .any(|p| crate::compress::job::is_original_backup(p.as_path()));
    Ok(any)
}

/// Remove the temp preview directory once the UI is done with it.
#[tauri::command]
pub async fn discard_preview_dir(tmp_dir: PathBuf) -> AppResult<()> {
    if tmp_dir
        .file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.starts_with("sorta-preview-"))
        .unwrap_or(false)
    {
        let _ = std::fs::remove_dir_all(&tmp_dir);
    }
    Ok(())
}
