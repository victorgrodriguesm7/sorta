import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import {
  api,
  type Codec,
  type CompressionProgress,
  type CompressionReport,
  type MediaRow,
} from "@/lib/tauri";
import { formatBytes, formatDurationSeconds } from "@/lib/format";

interface Props {
  media: MediaRow;
  totalBytes: number;
  onClose: () => void;
  onDone: () => void;
}

const DEFAULT_PREVIEW_CRFS = [22, 26, 28];

type Phase = "configure" | "previewing" | "compare" | "running" | "complete";

export default function CompressionDialog({
  media,
  totalBytes,
  onClose,
  onDone,
}: Props) {
  const { t } = useTranslation();
  const [phase, setPhase] = useState<Phase>("configure");
  const [error, setError] = useState<string | null>(null);

  // Settings
  const [codec, setCodec] = useState<Codec>("hevc");
  const [downscale720p, setDownscale720p] = useState(false);
  const [exhaustiveVerify, setExhaustiveVerify] = useState(false);
  const [previewCrfs, setPreviewCrfs] = useState<number[]>(DEFAULT_PREVIEW_CRFS);
  const [hwaccels, setHwaccels] = useState<string[]>([]);
  const [ffmpegMissing, setFfmpegMissing] = useState(false);

  // Preview state
  const [preview, setPreview] = useState<{
    original_data_url: string;
    original_segment_size_bytes: number;
    duration_seconds: number;
    start_seconds: number;
    clips: { crf: number; size_bytes: number; ratio: number; data_url: string }[];
    tmp_dir: string;
  } | null>(null);
  const [chosenCrf, setChosenCrf] = useState<number | null>(null);

  // Job state
  const [jobId, setJobId] = useState<string | null>(null);
  const [progress, setProgress] = useState<CompressionProgress | null>(null);
  const [report, setReport] = useState<CompressionReport | null>(null);

  // ----- ffmpeg detection on mount -----
  useEffect(() => {
    void (async () => {
      try {
        const status = await api.ffmpegStatus();
        if (!status.ffmpeg_path || !status.ffprobe_path) {
          setFfmpegMissing(true);
          return;
        }
        setHwaccels(status.hwaccels);
        // Auto-pick a hardware HEVC encoder if available.
        if (status.hwaccels.includes("nvenc")) setCodec("hevc_nvenc");
        else if (status.hwaccels.includes("qsv")) setCodec("hevc_qsv");
        else if (status.hwaccels.includes("amf")) setCodec("hevc_amf");
      } catch (e) {
        setError((e as Error).message);
      }
    })();
  }, []);

  // ----- subscribe to progress + report events while a job runs -----
  useEffect(() => {
    if (!jobId) return;
    const unsubP = listen<CompressionProgress>(
      "compression-progress",
      (ev) => {
        if (ev.payload.job_id === jobId) setProgress(ev.payload);
      },
    );
    const unsubR = listen<CompressionReport>("compression-report", (ev) => {
      if (ev.payload.job_id === jobId) {
        setReport(ev.payload);
        setPhase("complete");
      }
    });
    return () => {
      void unsubP.then((fn) => fn());
      void unsubR.then((fn) => fn());
    };
  }, [jobId]);

  // ----- generate previews -----
  const runPreview = async () => {
    setError(null);
    setPhase("previewing");
    try {
      const bundle = await api.generateCompressionPreview({
        mediaId: media.id,
        crfs: previewCrfs,
        codec,
        downscale720p,
      });
      setPreview(bundle);
      setChosenCrf(bundle.clips[Math.floor(bundle.clips.length / 2)].crf);
      setPhase("compare");
    } catch (e) {
      setError((e as Error).message);
      setPhase("configure");
    }
  };

  // Discard preview tmp dir on close.
  const cleanupPreview = useRef<() => void>(() => {});
  useEffect(() => {
    if (preview) {
      cleanupPreview.current = () => {
        void api.discardPreviewDir(preview.tmp_dir).catch(() => {});
      };
    }
  }, [preview]);
  useEffect(() => {
    return () => cleanupPreview.current();
  }, []);

  const startFull = async () => {
    if (chosenCrf == null) return;
    setError(null);
    try {
      const r = await api.startCompression({
        mediaId: media.id,
        codec,
        crf: chosenCrf,
        downscale720p,
        exhaustiveVerify,
      });
      setJobId(r.job_id);
      setPhase("running");
    } catch (e) {
      setError((e as Error).message);
    }
  };

  const cancelJob = async () => {
    if (!jobId) return;
    try {
      await api.cancelCompression(jobId);
    } catch (e) {
      setError((e as Error).message);
    }
  };

  const overallRatio = useMemo(() => {
    if (!report || report.total_original_bytes === 0) return 0;
    return 1 - report.total_compressed_bytes / report.total_original_bytes;
  }, [report]);

  return (
    <div className="fixed inset-0 z-30 flex items-center justify-center bg-black/70 p-6">
      <div className="flex h-full max-h-[92vh] w-full max-w-[80vw] flex-col overflow-hidden rounded-lg border border-neutral-700 bg-neutral-900 shadow-xl">
        <header className="flex items-center justify-between border-b border-neutral-800 p-4">
          <h3 className="text-lg font-semibold text-neutral-100">
            {t("compress.title", "Compress")} — {media.title}
          </h3>
          <button
            onClick={onClose}
            className="rounded p-1 text-neutral-400 hover:text-white"
          >
            ✕
          </button>
        </header>

        {ffmpegMissing && (
          <div className="border-b border-yellow-900/40 bg-yellow-900/30 px-4 py-3 text-sm text-yellow-100">
            {t(
              "compress.ffmpeg_missing",
              "ffmpeg / ffprobe not found on PATH. Install ffmpeg and restart the app.",
            )}
          </div>
        )}
        {error && (
          <div className="border-b border-red-900/40 bg-red-900/20 px-4 py-2 text-sm text-red-200">
            {error}
          </div>
        )}

        <div className="flex-1 overflow-y-auto p-4">
          {phase === "configure" && (
            <ConfigurePanel
              codec={codec}
              setCodec={setCodec}
              hwaccels={hwaccels}
              downscale720p={downscale720p}
              setDownscale720p={setDownscale720p}
              exhaustiveVerify={exhaustiveVerify}
              setExhaustiveVerify={setExhaustiveVerify}
              previewCrfs={previewCrfs}
              setPreviewCrfs={setPreviewCrfs}
              totalBytes={totalBytes}
            />
          )}

          {phase === "previewing" && (
            <div className="flex flex-col items-center justify-center gap-3 p-12 text-center text-neutral-300">
              <div className="text-2xl">🎞️</div>
              <p>
                {t(
                  "compress.generating_previews",
                  "Generating preview clips at {{crfs}} (CRF). This takes a minute…",
                  { crfs: previewCrfs.join(", ") },
                )}
              </p>
            </div>
          )}

          {phase === "compare" && preview && (
            <ComparePanel
              preview={preview}
              chosenCrf={chosenCrf}
              setChosenCrf={setChosenCrf}
            />
          )}

          {phase === "running" && (
            <RunningPanel
              progress={progress}
              chosenCrf={chosenCrf}
              codec={codec}
              downscale720p={downscale720p}
            />
          )}

          {phase === "complete" && report && (
            <CompletePanel report={report} ratio={overallRatio} />
          )}
        </div>

        <footer className="flex items-center justify-end gap-2 border-t border-neutral-800 p-4">
          {phase === "configure" && (
            <>
              <button
                onClick={onClose}
                className="rounded px-3 py-2 text-sm text-neutral-400 hover:text-white"
              >
                {t("actions.cancel")}
              </button>
              <button
                onClick={runPreview}
                disabled={ffmpegMissing}
                className="rounded bg-accent px-3 py-2 text-sm text-white hover:bg-accent-hover disabled:opacity-40"
              >
                {t("compress.generate_preview", "Generate preview")}
              </button>
            </>
          )}
          {phase === "compare" && (
            <>
              <button
                onClick={() => setPhase("configure")}
                className="rounded px-3 py-2 text-sm text-neutral-400 hover:text-white"
              >
                {t("actions.back", "Back")}
              </button>
              <button
                onClick={startFull}
                disabled={chosenCrf == null}
                className="rounded bg-accent px-3 py-2 text-sm text-white hover:bg-accent-hover disabled:opacity-40"
              >
                {t("compress.use_this", "Use this — compress all files")}
              </button>
            </>
          )}
          {phase === "running" && (
            <button
              onClick={cancelJob}
              className="rounded border border-red-900/60 px-3 py-2 text-sm text-red-300 hover:bg-red-900/30"
            >
              {t("actions.cancel")}
            </button>
          )}
          {phase === "complete" && (
            <button
              onClick={() => {
                onDone();
                onClose();
              }}
              className="rounded bg-accent px-3 py-2 text-sm text-white hover:bg-accent-hover"
            >
              {t("actions.close", "Close")}
            </button>
          )}
        </footer>
      </div>
    </div>
  );
}

// ===== Sub-panels =====

function ConfigurePanel(props: {
  codec: Codec;
  setCodec: (c: Codec) => void;
  hwaccels: string[];
  downscale720p: boolean;
  setDownscale720p: (b: boolean) => void;
  exhaustiveVerify: boolean;
  setExhaustiveVerify: (b: boolean) => void;
  previewCrfs: number[];
  setPreviewCrfs: (n: number[]) => void;
  totalBytes: number;
}) {
  const { t } = useTranslation();
  return (
    <div className="space-y-5 text-sm text-neutral-200">
      <div className="rounded bg-neutral-800/60 p-3">
        <div className="text-xs uppercase tracking-wide text-neutral-500">
          {t("compress.current_size", "Current size")}
        </div>
        <div className="text-lg font-semibold">
          {formatBytes(props.totalBytes)}
        </div>
      </div>

      <label className="flex flex-col gap-1">
        <span className="text-xs uppercase tracking-wide text-neutral-500">
          {t("compress.codec", "Encoder")}
        </span>
        <select
          value={props.codec}
          onChange={(e) => props.setCodec(e.target.value as Codec)}
          className="rounded bg-neutral-800 px-3 py-2 outline-none focus:ring-2 focus:ring-accent"
        >
          <option value="hevc">libx265 (H.265 software, smallest)</option>
          <option value="h264">libx264 (H.264 software, fastest)</option>
          {props.hwaccels.includes("nvenc") && (
            <option value="hevc_nvenc">hevc_nvenc (NVIDIA GPU)</option>
          )}
          {props.hwaccels.includes("qsv") && (
            <option value="hevc_qsv">hevc_qsv (Intel QuickSync)</option>
          )}
          {props.hwaccels.includes("amf") && (
            <option value="hevc_amf">hevc_amf (AMD)</option>
          )}
        </select>
      </label>

      <label className="flex flex-col gap-1">
        <span className="text-xs uppercase tracking-wide text-neutral-500">
          {t("compress.preview_crfs", "Preview CRF values (comma-separated)")}
        </span>
        <input
          value={props.previewCrfs.join(",")}
          onChange={(e) => {
            const list = e.target.value
              .split(",")
              .map((s) => parseInt(s.trim(), 10))
              .filter((n) => Number.isFinite(n) && n > 0 && n < 60);
            if (list.length > 0) props.setPreviewCrfs(list);
          }}
          className="rounded bg-neutral-800 px-3 py-2 outline-none focus:ring-2 focus:ring-accent"
        />
      </label>

      <label className="flex items-center gap-2 text-sm">
        <input
          type="checkbox"
          checked={props.downscale720p}
          onChange={(e) => props.setDownscale720p(e.target.checked)}
          className="h-4 w-4 accent-accent"
        />
        {t("compress.downscale_720p", "Scale to ≤ 720p")}
      </label>

      <label className="flex items-center gap-2 text-sm">
        <input
          type="checkbox"
          checked={props.exhaustiveVerify}
          onChange={(e) => props.setExhaustiveVerify(e.target.checked)}
          className="h-4 w-4 accent-accent"
        />
        {t(
          "compress.exhaustive_verify",
          "Exhaustive verify (full re-decode — slow but catches content corruption)",
        )}
      </label>
    </div>
  );
}

function ComparePanel(props: {
  preview: {
    original_data_url: string;
    original_segment_size_bytes: number;
    duration_seconds: number;
    start_seconds: number;
    clips: { crf: number; size_bytes: number; ratio: number; data_url: string }[];
    tmp_dir: string;
  };
  chosenCrf: number | null;
  setChosenCrf: (n: number) => void;
}) {
  const { t } = useTranslation();
  const { preview } = props;
  const cols = 1 + preview.clips.length;
  return (
    <div className="space-y-3">
      <p className="text-xs text-neutral-500">
        {t(
          "compress.compare_blurb",
          "Each clip is the same {{dur}}s slice of the source, encoded at a different quality.",
          { dur: Math.round(preview.duration_seconds) },
        )}
      </p>
      <div
        className="grid gap-3"
        style={{
          gridTemplateColumns: `repeat(${cols}, minmax(0, 1fr))`,
        }}
      >
        <Tile
          label={t("compress.original", "Original")}
          src={preview.original_data_url}
          sizeBytes={preview.original_segment_size_bytes}
        />
        {preview.clips.map((c) => (
          <Tile
            key={c.crf}
            label={`CRF ${c.crf}`}
            src={c.data_url}
            sizeBytes={c.size_bytes}
            ratio={c.ratio}
            chosen={props.chosenCrf === c.crf}
            onChoose={() => props.setChosenCrf(c.crf)}
          />
        ))}
      </div>
    </div>
  );
}

function Tile(props: {
  label: string;
  src: string;
  sizeBytes: number;
  ratio?: number;
  chosen?: boolean;
  onChoose?: () => void;
}) {
  const { t } = useTranslation();
  return (
    <div
      className={`flex flex-col gap-2 rounded border p-2 ${
        props.chosen ? "border-accent" : "border-neutral-800"
      }`}
    >
      <div className="flex items-baseline justify-between">
        <span className="text-sm font-semibold">{props.label}</span>
        <span className="text-xs text-neutral-500">
          {formatBytes(props.sizeBytes)}
        </span>
      </div>
      <video
        src={props.src}
        controls
        muted
        loop
        className="aspect-video w-full rounded bg-black"
      />
      {props.ratio !== undefined && (
        <div className="text-xs text-neutral-400">
          {t("compress.savings", "{{pct}}% smaller", {
            pct: Math.round(props.ratio * 100),
          })}
        </div>
      )}
      {props.onChoose && (
        <button
          onClick={props.onChoose}
          className={`rounded px-2 py-1 text-xs ${
            props.chosen
              ? "bg-accent text-white"
              : "border border-neutral-700 text-neutral-300 hover:bg-neutral-800"
          }`}
        >
          {props.chosen
            ? t("compress.chosen", "Selected")
            : t("compress.choose", "Use this")}
        </button>
      )}
    </div>
  );
}

function RunningPanel(props: {
  progress: CompressionProgress | null;
  chosenCrf: number | null;
  codec: Codec;
  downscale720p: boolean;
}) {
  const { t } = useTranslation();
  const p = props.progress;
  const fileFrac =
    p && p.current_file_duration_seconds > 0
      ? p.current_file_position_seconds / p.current_file_duration_seconds
      : 0;
  const overallFrac = p
    ? (p.current_file_index + fileFrac) / Math.max(1, p.total_files)
    : 0;

  return (
    <div className="space-y-4 text-sm text-neutral-200">
      <div className="rounded bg-neutral-800/50 p-3 text-xs text-neutral-400">
        <span className="font-mono">
          {props.codec} · CRF {props.chosenCrf}
          {props.downscale720p ? " · 720p" : ""}
        </span>
      </div>

      {!p && (
        <div className="text-neutral-500">
          {t("compress.starting", "Starting…")}
        </div>
      )}
      {p && (
        <>
          <div>
            <div className="mb-1 flex justify-between text-xs text-neutral-500">
              <span>
                {t("compress.file_n_of_m", "File {{n}} of {{m}}", {
                  n: p.current_file_index + 1,
                  m: p.total_files,
                })}
              </span>
              <span>{p.current_file_name}</span>
            </div>
            <ProgressBar value={fileFrac} />
            <div className="mt-1 flex justify-between text-xs text-neutral-500">
              <span>
                {t("compress.state", "State")}: {p.state}
                {p.current_file_speed
                  ? ` · ${p.current_file_speed.toFixed(2)}×`
                  : ""}
              </span>
              <span>
                ETA{" "}
                {p.eta_current_file_seconds != null
                  ? formatDurationSeconds(p.eta_current_file_seconds)
                  : "—"}
              </span>
            </div>
          </div>

          <div>
            <div className="mb-1 flex justify-between text-xs text-neutral-500">
              <span>{t("compress.overall", "Overall")}</span>
              <span>
                ETA{" "}
                {p.eta_total_seconds != null
                  ? formatDurationSeconds(p.eta_total_seconds)
                  : "—"}
              </span>
            </div>
            <ProgressBar value={overallFrac} />
          </div>
        </>
      )}
    </div>
  );
}

function ProgressBar({ value }: { value: number }) {
  const pct = Math.max(0, Math.min(1, value)) * 100;
  return (
    <div className="h-2 w-full overflow-hidden rounded bg-neutral-800">
      <div
        className="h-full bg-accent transition-[width] duration-300"
        style={{ width: `${pct}%` }}
      />
    </div>
  );
}

function CompletePanel(props: {
  report: CompressionReport;
  ratio: number;
}) {
  const { t } = useTranslation();
  const r = props.report;
  return (
    <div className="space-y-4 text-sm text-neutral-200">
      <div className="rounded bg-neutral-800/60 p-3">
        <div className="text-xs uppercase tracking-wide text-neutral-500">
          {t("compress.result", "Result")}: {r.state}
        </div>
        <div className="mt-1">
          {formatBytes(r.total_original_bytes)} →{" "}
          {formatBytes(r.total_compressed_bytes)} (
          {Math.round(props.ratio * 100)}% smaller)
        </div>
        <div className="mt-1 text-xs text-neutral-500">
          {t(
            "compress.originals_kept",
            "Original files renamed to *.original.<ext>. Use 'Clean up originals' on the right panel when you're satisfied.",
          )}
        </div>
      </div>
      <details className="rounded border border-neutral-800 p-2 text-xs">
        <summary className="cursor-pointer text-neutral-400">
          {t("compress.per_file", "Per-file outcomes ({{count}})", {
            count: r.outcomes.length,
          })}
        </summary>
        <ul className="mt-2 space-y-1">
          {r.outcomes.map((o) => (
            <li key={o.file} className="font-mono text-[11px]">
              <span className="text-neutral-500">
                {formatBytes(o.original_bytes)} →{" "}
                {o.compressed_bytes != null
                  ? formatBytes(o.compressed_bytes)
                  : "—"}
              </span>{" "}
              {o.error ? (
                <span className="text-red-300">{o.error}</span>
              ) : null}{" "}
              {o.file}
            </li>
          ))}
        </ul>
      </details>
    </div>
  );
}
