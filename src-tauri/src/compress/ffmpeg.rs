//! ffmpeg invocation primitives.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Result of probing the system for ffmpeg/ffprobe + GPU encoders.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FfmpegStatus {
    pub ffmpeg_path: Option<PathBuf>,
    pub ffprobe_path: Option<PathBuf>,
    pub ffmpeg_version: Option<String>,
    pub hwaccels: Vec<String>, // e.g. ["nvenc", "qsv", "amf"]
}

impl FfmpegStatus {
    pub fn is_ready(&self) -> bool {
        self.ffmpeg_path.is_some() && self.ffprobe_path.is_some()
    }
}

/// Encoder family the user (or auto-detect) chose.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Codec {
    /// libx265 (HEVC) — software, slow, smallest output.
    Hevc,
    /// libx264 (H.264) — software, fast, universal.
    H264,
    /// hevc_nvenc — NVIDIA hardware HEVC.
    HevcNvenc,
    /// hevc_qsv — Intel QuickSync HEVC.
    HevcQsv,
    /// hevc_amf — AMD VCE HEVC.
    HevcAmf,
}

impl Codec {
    pub fn ffmpeg_name(self) -> &'static str {
        match self {
            Codec::Hevc => "libx265",
            Codec::H264 => "libx264",
            Codec::HevcNvenc => "hevc_nvenc",
            Codec::HevcQsv => "hevc_qsv",
            Codec::HevcAmf => "hevc_amf",
        }
    }

    /// Auto-pick the best available HEVC encoder given a probe.
    pub fn auto_default(status: &FfmpegStatus) -> Codec {
        for h in &status.hwaccels {
            match h.as_str() {
                "nvenc" => return Codec::HevcNvenc,
                "qsv" => return Codec::HevcQsv,
                "amf" => return Codec::HevcAmf,
                _ => {}
            }
        }
        Codec::Hevc
    }

    pub fn is_hardware(self) -> bool {
        matches!(self, Codec::HevcNvenc | Codec::HevcQsv | Codec::HevcAmf)
    }
}

/// Build the ffmpeg argv for an encode (preview or full).
///
/// `start` and `duration` are optional — when both are set, only that
/// segment is encoded (preview path). When neither is set, the entire
/// input is encoded (full path).
#[derive(Debug, Clone)]
pub struct EncodeArgs<'a> {
    pub input: &'a Path,
    pub output: &'a Path,
    pub codec: Codec,
    pub crf: i32,
    pub downscale_720p: bool,
    pub start_seconds: Option<f64>,
    pub duration_seconds: Option<f64>,
    pub progress_to_stderr: bool,
}

pub fn build_encode_argv(args: &EncodeArgs<'_>) -> Vec<String> {
    let mut argv: Vec<String> = Vec::new();
    argv.push("-y".into());
    argv.push("-hide_banner".into());

    // -ss before -i seeks via index = much faster, may snap to keyframe.
    if let Some(start) = args.start_seconds {
        argv.push("-ss".into());
        argv.push(format!("{start:.3}"));
    }
    argv.push("-i".into());
    argv.push(args.input.to_string_lossy().to_string());

    if let Some(dur) = args.duration_seconds {
        argv.push("-t".into());
        argv.push(format!("{dur:.3}"));
    }

    // Video filter: optional 720p downscale (preserves aspect ratio).
    if args.downscale_720p {
        argv.push("-vf".into());
        argv.push("scale=-2:720".into());
    }

    // Codec.
    argv.push("-c:v".into());
    argv.push(args.codec.ffmpeg_name().into());

    // Quality knob — name varies between encoders.
    let quality_flag = match args.codec {
        Codec::Hevc | Codec::H264 => "-crf",
        Codec::HevcNvenc | Codec::HevcAmf => "-cq",
        Codec::HevcQsv => "-global_quality",
    };
    argv.push(quality_flag.into());
    argv.push(args.crf.to_string());

    // Software HEVC: medium preset is the "best size for time" sweet spot.
    if matches!(args.codec, Codec::Hevc) {
        argv.push("-preset".into());
        argv.push("medium".into());
    }
    if matches!(args.codec, Codec::H264) {
        argv.push("-preset".into());
        argv.push("medium".into());
    }

    // Audio: copy through.
    argv.push("-c:a".into());
    argv.push("copy".into());

    // Subtitles: copy through too — preserves embedded subtitle tracks.
    argv.push("-c:s".into());
    argv.push("copy".into());

    if args.progress_to_stderr {
        argv.push("-progress".into());
        argv.push("pipe:2".into());
        argv.push("-nostats".into());
    }

    argv.push(args.output.to_string_lossy().to_string());
    argv
}

/// Parsed line from ffmpeg's `-progress pipe:2` output.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ProgressTick {
    pub time_position_seconds: Option<f64>,
    pub speed: Option<f64>,
    pub fps: Option<f64>,
    pub total_size_bytes: Option<u64>,
    pub finished: bool,
}

/// Apply one `key=value` line to a running progress accumulator.
/// Lines with `progress=end` mark the run as finished.
pub fn apply_progress_line(line: &str, into: &mut ProgressTick) {
    let line = line.trim();
    let Some((k, v)) = line.split_once('=') else {
        return;
    };
    let v = v.trim();
    match k.trim() {
        "out_time_us" | "out_time_ms" => {
            // out_time_us is microseconds; out_time_ms is also documented as
            // microseconds (an ffmpeg historical naming bug). Either way the
            // number we get is in microseconds.
            if let Ok(us) = v.parse::<i64>() {
                if us >= 0 {
                    into.time_position_seconds = Some(us as f64 / 1_000_000.0);
                }
            }
        }
        "out_time" => {
            // "HH:MM:SS.mmm" form.
            if let Some(secs) = parse_hms(v) {
                into.time_position_seconds = Some(secs);
            }
        }
        "speed" => {
            // "1.23x" or "N/A"
            if let Some(num) = v.strip_suffix('x') {
                if let Ok(s) = num.trim().parse::<f64>() {
                    into.speed = Some(s);
                }
            }
        }
        "fps" => {
            if let Ok(f) = v.parse::<f64>() {
                if f.is_finite() {
                    into.fps = Some(f);
                }
            }
        }
        "total_size" => {
            if let Ok(b) = v.parse::<u64>() {
                into.total_size_bytes = Some(b);
            }
        }
        "progress" => {
            if v == "end" {
                into.finished = true;
            }
        }
        _ => {}
    }
}

/// Parse `HH:MM:SS(.mmm)?` into seconds.
pub fn parse_hms(s: &str) -> Option<f64> {
    let mut parts = s.split(':').rev();
    let secs = parts.next()?.parse::<f64>().ok()?;
    let mins = parts.next().map(|m| m.parse::<f64>().ok()).flatten().unwrap_or(0.0);
    let hours = parts.next().map(|h| h.parse::<f64>().ok()).flatten().unwrap_or(0.0);
    Some(hours * 3600.0 + mins * 60.0 + secs)
}

/// ETA in seconds, given how far we've progressed so far.
/// `current` and `total` are the encode position in seconds; `speed` is
/// ffmpeg's "Nx realtime" multiplier. None if we don't know enough yet.
pub fn estimate_eta_seconds(current: f64, total: f64, speed: f64) -> Option<f64> {
    if speed <= 0.0 || total <= 0.0 || !speed.is_finite() {
        return None;
    }
    let remaining = (total - current).max(0.0);
    Some(remaining / speed)
}

/// Locate ffmpeg + ffprobe on PATH (sync).
pub fn probe_binaries() -> FfmpegStatus {
    let ffmpeg_path = which::which("ffmpeg").ok();
    let ffprobe_path = which::which("ffprobe").ok();
    FfmpegStatus {
        ffmpeg_path,
        ffprobe_path,
        ffmpeg_version: None,
        hwaccels: Vec::new(),
    }
}

/// Read ffmpeg's `-version` output and pick a short version string.
pub fn parse_version_output(text: &str) -> Option<String> {
    let first_line = text.lines().next()?;
    // Typical form: "ffmpeg version n7.1 Copyright (c) ..."
    let mut tokens = first_line.split_whitespace();
    if tokens.next()? != "ffmpeg" {
        return None;
    }
    if tokens.next()? != "version" {
        return None;
    }
    tokens.next().map(|s| s.to_string())
}

/// Detect available hardware HEVC encoders by parsing `ffmpeg -encoders`.
pub fn parse_hwaccels_from_encoders(encoders_output: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in encoders_output.lines() {
        let trimmed = line.trim();
        // Lines of interest look like:
        //   V....D hevc_nvenc           NVIDIA NVENC hevc encoder ...
        //   V....D hevc_qsv             ...
        //   V....D hevc_amf             ...
        //   V....D hevc_videotoolbox    ...
        for tag in ["nvenc", "qsv", "amf", "videotoolbox", "vaapi"] {
            let needle = format!("hevc_{tag}");
            if trimmed.contains(&needle) && !out.contains(&tag.to_string()) {
                out.push(tag.to_string());
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    #[test]
    fn build_argv_for_software_hevc_full_encode() {
        let args = EncodeArgs {
            input: &p("/in.mkv"),
            output: &p("/out.mkv"),
            codec: Codec::Hevc,
            crf: 26,
            downscale_720p: false,
            start_seconds: None,
            duration_seconds: None,
            progress_to_stderr: true,
        };
        let argv = build_encode_argv(&args);
        // Must include codec, crf, audio copy, progress pipe.
        assert!(argv.contains(&"libx265".to_string()));
        assert!(argv.contains(&"-crf".to_string()));
        assert!(argv.contains(&"26".to_string()));
        assert!(argv.contains(&"copy".to_string()));
        assert!(argv.contains(&"-progress".to_string()));
        // No -ss / -t when not previewing.
        assert!(!argv.contains(&"-ss".to_string()));
        assert!(!argv.contains(&"-t".to_string()));
    }

    #[test]
    fn build_argv_for_preview_segment() {
        let args = EncodeArgs {
            input: &p("/in.mkv"),
            output: &p("/out.mkv"),
            codec: Codec::Hevc,
            crf: 22,
            downscale_720p: true,
            start_seconds: Some(120.0),
            duration_seconds: Some(15.0),
            progress_to_stderr: false,
        };
        let argv = build_encode_argv(&args);
        assert!(argv.contains(&"-ss".to_string()));
        assert!(argv.contains(&"120.000".to_string()));
        assert!(argv.contains(&"-t".to_string()));
        assert!(argv.contains(&"15.000".to_string()));
        assert!(argv.contains(&"scale=-2:720".to_string()));
        assert!(!argv.contains(&"-progress".to_string()));
    }

    #[test]
    fn nvenc_uses_cq_not_crf() {
        let args = EncodeArgs {
            input: &p("/in.mkv"),
            output: &p("/out.mkv"),
            codec: Codec::HevcNvenc,
            crf: 24,
            downscale_720p: false,
            start_seconds: None,
            duration_seconds: None,
            progress_to_stderr: true,
        };
        let argv = build_encode_argv(&args);
        assert!(argv.contains(&"hevc_nvenc".to_string()));
        assert!(argv.contains(&"-cq".to_string()));
        assert!(!argv.contains(&"-crf".to_string()));
    }

    #[test]
    fn qsv_uses_global_quality() {
        let args = EncodeArgs {
            input: &p("/in.mkv"),
            output: &p("/out.mkv"),
            codec: Codec::HevcQsv,
            crf: 24,
            downscale_720p: false,
            start_seconds: None,
            duration_seconds: None,
            progress_to_stderr: false,
        };
        let argv = build_encode_argv(&args);
        assert!(argv.contains(&"-global_quality".to_string()));
    }

    #[test]
    fn parse_progress_lines_accumulate() {
        let mut tick = ProgressTick::default();
        for l in [
            "frame=12",
            "fps=30",
            "out_time_us=4500000",
            "speed=2.50x",
            "total_size=1048576",
            "progress=continue",
        ] {
            apply_progress_line(l, &mut tick);
        }
        assert!((tick.time_position_seconds.unwrap() - 4.5).abs() < 1e-6);
        assert_eq!(tick.speed, Some(2.5));
        assert_eq!(tick.fps, Some(30.0));
        assert_eq!(tick.total_size_bytes, Some(1048576));
        assert!(!tick.finished);

        apply_progress_line("progress=end", &mut tick);
        assert!(tick.finished);
    }

    #[test]
    fn parse_hms_handles_decimals_and_partial_forms() {
        assert!((parse_hms("00:01:30.500").unwrap() - 90.5).abs() < 1e-6);
        assert!((parse_hms("01:00:00").unwrap() - 3600.0).abs() < 1e-6);
        assert!((parse_hms("90.5").unwrap() - 90.5).abs() < 1e-6);
    }

    #[test]
    fn eta_math_is_remaining_over_speed() {
        // 1000 s total, at 100 s with speed 2x → (1000-100)/2 = 450 s.
        assert!(
            (estimate_eta_seconds(100.0, 1000.0, 2.0).unwrap() - 450.0).abs() < 1e-6
        );
        assert_eq!(estimate_eta_seconds(0.0, 0.0, 2.0), None);
        assert_eq!(estimate_eta_seconds(0.0, 100.0, 0.0), None);
    }

    #[test]
    fn auto_codec_prefers_nvenc_over_software() {
        let s = FfmpegStatus {
            ffmpeg_path: None,
            ffprobe_path: None,
            ffmpeg_version: None,
            hwaccels: vec!["nvenc".into(), "qsv".into()],
        };
        assert_eq!(Codec::auto_default(&s), Codec::HevcNvenc);

        let s = FfmpegStatus {
            ffmpeg_path: None,
            ffprobe_path: None,
            ffmpeg_version: None,
            hwaccels: vec!["amf".into()],
        };
        assert_eq!(Codec::auto_default(&s), Codec::HevcAmf);

        let s = FfmpegStatus {
            ffmpeg_path: None,
            ffprobe_path: None,
            ffmpeg_version: None,
            hwaccels: vec![],
        };
        assert_eq!(Codec::auto_default(&s), Codec::Hevc);
    }

    #[test]
    fn version_output_is_parsed() {
        let v = parse_version_output(
            "ffmpeg version n7.1 Copyright (c) 2000-2024 the FFmpeg developers\n  built with ...",
        );
        assert_eq!(v.as_deref(), Some("n7.1"));
    }

    #[test]
    fn hwaccels_extracted_from_encoders_listing() {
        let sample = "
 V....D h264                 ...
 V....D libx264              ...
 V....D libx265              ...
 V....D hevc_nvenc           NVIDIA NVENC hevc encoder
 V....D hevc_qsv             ...
        ";
        let mut got = parse_hwaccels_from_encoders(sample);
        got.sort();
        assert_eq!(got, vec!["nvenc".to_string(), "qsv".to_string()]);
    }
}
