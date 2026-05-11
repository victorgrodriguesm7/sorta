import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  api,
  type MediaRow,
  type RecatalogPlan,
  type RecatalogResult,
} from "@/lib/tauri";

interface Props {
  /** The catalogued TV row the user clicked Re-Catalog on. */
  media: MediaRow;
  onClose: () => void;
  onDone: (result: RecatalogResult) => void;
}

const POSTER_BASE = "https://image.tmdb.org/t/p/w154";

/**
 * Migration dialog for existing TV rows. The series is already
 * linked, so there's no TMDB picker — we show the locked title,
 * the seasons we discovered under the series folder, and the
 * option toggles. Confirming runs `recatalog_series` which:
 *   - re-fetches TMDB season metadata,
 *   - optionally renames files to S{XX}E{YY}.{Title}.{ext},
 *   - optionally downloads one still per episode,
 *   - upserts the episodes table.
 *
 * Idempotent on the backend, so the user can re-run safely.
 */
export default function RecatalogDialog({ media, onClose, onDone }: Props) {
  const { t } = useTranslation();
  const [plan, setPlan] = useState<RecatalogPlan | null>(null);
  const [loadingPlan, setLoadingPlan] = useState(true);
  const [planError, setPlanError] = useState<string | null>(null);

  const [rename, setRename] = useState(true);
  const [downloadEpisodePosters, setDownloadEpisodePosters] = useState(true);
  // null means "leave the row's is_new flag as-is"; true/false overwrite.
  const [setIsNewOverride, setSetIsNewOverride] = useState<boolean | null>(
    null,
  );

  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    setLoadingPlan(true);
    setPlanError(null);
    void api
      .planRecatalogSeries(media.id)
      .then((p) => setPlan(p))
      .catch((e) => setPlanError((e as Error).message))
      .finally(() => setLoadingPlan(false));
  }, [media.id]);

  const totalEpisodes =
    plan?.seasons.reduce((sum, s) => sum + s.video_filenames.length, 0) ?? 0;

  const confirm = async () => {
    setSubmitting(true);
    setError(null);
    try {
      const result = await api.recatalogSeries({
        mediaId: media.id,
        rename,
        downloadEpisodePosters,
        setIsNew: setIsNewOverride,
      });
      onDone(result);
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setSubmitting(false);
    }
  };

  const posterSrc = media.poster_url ?? null;

  return (
    <div className="fixed inset-0 z-30 flex items-center justify-center bg-black/70 p-6">
      <div className="flex h-full max-h-[88vh] w-full max-w-[70vw] flex-col overflow-hidden rounded-lg border border-neutral-700 bg-neutral-900 shadow-xl">
        <header className="flex items-center justify-between border-b border-neutral-800 p-4">
          <h3 className="text-lg font-semibold text-neutral-100">
            {t("series.recatalog_title", "Re-Catalog series")}
          </h3>
          <button
            onClick={onClose}
            className="rounded p-1 text-neutral-400 hover:text-white"
            aria-label={t("actions.cancel")}
          >
            ✕
          </button>
        </header>

        <div className="grid flex-1 grid-cols-[1.2fr_1fr] overflow-hidden">
          {/* Left: locked series identity. */}
          <section className="flex flex-col overflow-hidden border-r border-neutral-800 p-4">
            <div className="flex items-start gap-3">
              <div className="h-32 w-24 shrink-0 overflow-hidden rounded bg-neutral-800">
                {posterSrc && (
                  <img
                    src={posterSrc.startsWith("http") ? posterSrc : `${POSTER_BASE}${posterSrc}`}
                    alt={media.title}
                    className="h-full w-full object-cover"
                  />
                )}
              </div>
              <div className="min-w-0 flex-1">
                <div className="text-xs uppercase tracking-wide text-neutral-500">
                  {t("series.linked_to", "Linked to TMDB")}
                </div>
                <div className="truncate text-lg font-semibold text-neutral-100">
                  {media.title}
                </div>
                <div className="text-xs text-neutral-500">
                  TMDB #{media.tmdb_id}
                </div>
                <div className="mt-1 break-all text-xs text-neutral-600">
                  {media.folder_path}
                </div>
              </div>
            </div>

            <div className="mt-4 flex-1 overflow-y-auto">
              {loadingPlan && (
                <div className="text-sm text-neutral-500">
                  {t("series.scanning", "Scanning folder…")}
                </div>
              )}
              {planError && (
                <div className="rounded bg-red-900/40 px-3 py-2 text-sm text-red-200">
                  {planError}
                </div>
              )}
              {plan && plan.seasons.length === 0 && !loadingPlan && (
                <div className="text-sm text-neutral-500">
                  {t(
                    "series.no_seasons_found",
                    "No season subfolders found under this series.",
                  )}
                </div>
              )}
              {plan && plan.seasons.length > 0 && (
                <div className="space-y-3">
                  <div className="text-xs uppercase tracking-wide text-neutral-500">
                    {t("series.detected", "Detected")}:{" "}
                    {t("series.seasons_count", "{{count}} seasons", {
                      count: plan.seasons.length,
                    })}{" "}
                    ·{" "}
                    {t("series.episode_count", "{{count}} episodes", {
                      count: totalEpisodes,
                    })}
                  </div>
                  {plan.seasons.map((s) => (
                    <details
                      key={s.season_number}
                      className="rounded border border-neutral-800 bg-neutral-900/40"
                      open={plan.seasons.length === 1}
                    >
                      <summary className="cursor-pointer select-none px-3 py-2 text-sm text-neutral-200">
                        {t("series.season", "Season")} {s.season_number}
                        <span className="ml-2 text-xs text-neutral-500">
                          ({s.video_filenames.length})
                        </span>
                      </summary>
                      <ul className="max-h-40 divide-y divide-neutral-800/60 overflow-y-auto px-3 py-1 text-xs text-neutral-400">
                        {s.video_filenames.map((f) => (
                          <li key={f} className="truncate py-0.5">
                            {f}
                          </li>
                        ))}
                      </ul>
                    </details>
                  ))}
                </div>
              )}
            </div>
          </section>

          {/* Right: options. */}
          <section className="flex flex-col gap-3 overflow-hidden p-4">
            <div className="text-xs uppercase tracking-wide text-neutral-500">
              {t("series.options", "Options")}
            </div>

            <label
              className="flex cursor-pointer items-start gap-2 text-sm text-neutral-200"
              title={t(
                "series.recatalog_rename_help",
                "Rename each file in place to S{XX}E{YY}.{Title}.{ext}. Files already at the target name are left alone.",
              )}
            >
              <input
                type="checkbox"
                checked={rename}
                onChange={(e) => setRename(e.target.checked)}
                className="mt-0.5 h-4 w-4 cursor-pointer accent-accent"
              />
              <div>
                <div>
                  {t(
                    "series.rename_to_standard",
                    "Rename to S{XX}E{YY}.{Title}",
                  )}
                </div>
                <div className="text-xs text-neutral-500">
                  {t(
                    "series.rename_in_place_help",
                    "In-place rename. Files already at the target name are skipped.",
                  )}
                </div>
              </div>
            </label>

            <label
              className="flex cursor-pointer items-start gap-2 text-sm text-neutral-200"
              title={t(
                "series.download_episode_posters_help",
                "Fetch one TMDB still per episode at link time.",
              )}
            >
              <input
                type="checkbox"
                checked={downloadEpisodePosters}
                onChange={(e) => setDownloadEpisodePosters(e.target.checked)}
                className="mt-0.5 h-4 w-4 cursor-pointer accent-accent"
              />
              <div>
                <div>
                  {t(
                    "series.download_episode_posters",
                    "Download episode stills",
                  )}
                </div>
                <div className="text-xs text-neutral-500">
                  {t(
                    "series.recatalog_stills_help",
                    "Saves under <HD>/poster/episodes/. One TMDB request per season.",
                  )}
                </div>
              </div>
            </label>

            <fieldset className="rounded border border-neutral-800 p-2">
              <legend className="px-1 text-xs text-neutral-500">
                {t("media.mark_as_new", "Mark as new")}
              </legend>
              <div className="space-y-1 text-sm text-neutral-200">
                <label className="flex cursor-pointer items-center gap-2">
                  <input
                    type="radio"
                    name="is_new"
                    checked={setIsNewOverride === null}
                    onChange={() => setSetIsNewOverride(null)}
                    className="accent-accent"
                  />
                  {t("series.is_new_keep", "Keep current")}
                  <span className="text-xs text-neutral-500">
                    ({media.is_new ? t("series.is_new_yes", "yes") : t("series.is_new_no", "no")})
                  </span>
                </label>
                <label className="flex cursor-pointer items-center gap-2">
                  <input
                    type="radio"
                    name="is_new"
                    checked={setIsNewOverride === true}
                    onChange={() => setSetIsNewOverride(true)}
                    className="accent-accent"
                  />
                  {t("series.is_new_set_true", "Mark as new")}
                </label>
                <label className="flex cursor-pointer items-center gap-2">
                  <input
                    type="radio"
                    name="is_new"
                    checked={setIsNewOverride === false}
                    onChange={() => setSetIsNewOverride(false)}
                    className="accent-accent"
                  />
                  {t("series.is_new_set_false", "Clear new flag")}
                </label>
              </div>
            </fieldset>

            {error && (
              <div className="rounded bg-red-900/40 px-3 py-2 text-sm text-red-200">
                {error}
              </div>
            )}
          </section>
        </div>

        <footer className="flex items-center justify-between border-t border-neutral-800 p-4">
          <span className="text-xs text-neutral-500">
            {plan
              ? t(
                  "series.recatalog_summary",
                  "{{seasons}} seasons · {{episodes}} files",
                  {
                    seasons: plan.seasons.length,
                    episodes: totalEpisodes,
                  },
                )
              : ""}
          </span>
          <div className="flex gap-2">
            <button
              onClick={onClose}
              className="rounded px-3 py-2 text-sm text-neutral-400 hover:text-white"
            >
              {t("actions.cancel")}
            </button>
            <button
              disabled={!plan || plan.seasons.length === 0 || submitting}
              onClick={confirm}
              className="rounded bg-accent px-3 py-2 text-sm text-white hover:bg-accent-hover disabled:cursor-not-allowed disabled:opacity-40"
            >
              {submitting
                ? t("series.recataloging", "Re-cataloging…")
                : t("series.recatalog_action", "Re-Catalog")}
            </button>
          </div>
        </footer>
      </div>
    </div>
  );
}
