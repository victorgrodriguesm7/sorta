import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { api, type SearchResult, type UncataloguedItem } from "@/lib/tauri";

interface Props {
  episodes: UncataloguedItem[];
  onClose: () => void;
  onLinked: () => void;
}

const POSTER_BASE = "https://image.tmdb.org/t/p/w154";

/** Multi-episode bulk linker. Pick a TMDB show + season number; the
 *  backend renames each file to S{XX}E{YY}.ext and drops them under
 *  <Series Label>/<Show Title> [tmdb-id]/<Season Label> N/. */
export default function CatalogAsSeriesDialog({
  episodes,
  onClose,
  onLinked,
}: Props) {
  const { t } = useTranslation();
  const [query, setQuery] = useState(
    episodes[0]?.folder.split(/[\\/]/).reverse().find(
      (s) =>
        s &&
        !/^season\b|^temporada\b|^s\d+$/i.test(s),
    ) ?? "",
  );
  const [results, setResults] = useState<SearchResult[]>([]);
  const [picked, setPicked] = useState<SearchResult | null>(null);
  const [season, setSeason] = useState<number>(1);
  const [startEpisode, setStartEpisode] = useState<number>(1);
  // Episodes in apply order. Initial order = walker order.
  const [order, setOrder] = useState<UncataloguedItem[]>(episodes);
  const [loading, setLoading] = useState(false);
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    setOrder(episodes);
  }, [episodes]);

  const runSearch = async (q: string) => {
    if (!q.trim()) return;
    setLoading(true);
    setError(null);
    try {
      // Bias to TV results.
      const all = await api.tmdbSearch(q.trim());
      setResults(all.filter((r) => r.media_type === "tv"));
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    void runSearch(query);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const swap = (i: number, j: number) => {
    if (j < 0 || j >= order.length) return;
    const next = order.slice();
    [next[i], next[j]] = [next[j], next[i]];
    setOrder(next);
  };

  const confirm = async () => {
    if (!picked) {
      setError(t("series.pick_show", "Pick a show first."));
      return;
    }
    setSubmitting(true);
    setError(null);
    try {
      await api.linkAsSeries({
        tmdbId: picked.id,
        season,
        startEpisode,
        sources: order.map((e) => ({
          folder: e.folder,
          videoFilename: e.video_filename,
        })),
      });
      onLinked();
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <div className="fixed inset-0 z-30 flex items-center justify-center bg-black/70 p-6">
      <div className="flex h-full max-h-[88vh] w-full max-w-4xl flex-col overflow-hidden rounded-lg border border-neutral-700 bg-neutral-900 shadow-xl">
        <header className="flex items-center justify-between border-b border-neutral-800 p-4">
          <h3 className="text-lg font-semibold text-neutral-100">
            {t("series.catalog_as", "Catalog as series")}
          </h3>
          <button
            onClick={onClose}
            className="rounded p-1 text-neutral-400 hover:text-white"
            aria-label={t("actions.cancel")}
          >
            ✕
          </button>
        </header>

        <div className="grid flex-1 grid-cols-[1.4fr_1fr] overflow-hidden">
          {/* Show search */}
          <section className="flex flex-col overflow-hidden border-r border-neutral-800">
            <div className="flex items-center gap-2 border-b border-neutral-800 p-3">
              <input
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                onKeyDown={(e) => e.key === "Enter" && runSearch(query)}
                placeholder={t("series.search_show", "Search TV shows…")}
                className="flex-1 rounded bg-neutral-800 px-3 py-2 text-sm text-white outline-none focus:ring-2 focus:ring-accent"
              />
              <button
                onClick={() => runSearch(query)}
                className="rounded bg-accent px-3 py-2 text-sm text-white hover:bg-accent-hover"
              >
                {t("actions.search")}
              </button>
            </div>
            <div className="flex-1 overflow-y-auto">
              {loading && (
                <div className="p-3 text-sm text-neutral-500">…</div>
              )}
              {!loading && results.length === 0 && (
                <div className="p-3 text-sm text-neutral-500">
                  {t("series.no_results", "No TV results.")}
                </div>
              )}
              <ul>
                {results.map((r) => {
                  const active =
                    picked?.id === r.id && picked.media_type === r.media_type;
                  return (
                    <li key={`${r.media_type}-${r.id}`}>
                      <button
                        onClick={() => setPicked(r)}
                        className={`flex w-full items-start gap-3 border-b border-neutral-800 p-3 text-left transition ${
                          active ? "bg-accent/20" : "hover:bg-neutral-800"
                        }`}
                      >
                        <div className="h-20 w-14 shrink-0 overflow-hidden rounded bg-neutral-800">
                          {r.poster_path && (
                            <img
                              src={`${POSTER_BASE}${r.poster_path}`}
                              alt={r.title}
                              className="h-full w-full object-cover"
                            />
                          )}
                        </div>
                        <div className="flex-1">
                          <div className="font-medium text-neutral-100">
                            {r.title}
                          </div>
                          <div className="text-xs text-neutral-500">
                            {r.year ?? "—"} · TMDB #{r.id}
                          </div>
                        </div>
                      </button>
                    </li>
                  );
                })}
              </ul>
            </div>
          </section>

          {/* Episode order + season */}
          <section className="flex flex-col overflow-hidden">
            <div className="flex items-end gap-2 border-b border-neutral-800 p-3">
              <label className="flex flex-col text-xs text-neutral-500">
                {t("series.season", "Season")}
                <input
                  type="number"
                  min={0}
                  value={season}
                  onChange={(e) => setSeason(Number(e.target.value))}
                  className="mt-1 w-20 rounded bg-neutral-800 px-2 py-1 text-sm text-white outline-none focus:ring-2 focus:ring-accent"
                />
              </label>
              <label className="flex flex-col text-xs text-neutral-500">
                {t("series.start_episode", "First episode #")}
                <input
                  type="number"
                  min={1}
                  value={startEpisode}
                  onChange={(e) => setStartEpisode(Number(e.target.value))}
                  className="mt-1 w-20 rounded bg-neutral-800 px-2 py-1 text-sm text-white outline-none focus:ring-2 focus:ring-accent"
                />
              </label>
            </div>
            <div className="flex-1 overflow-y-auto p-2">
              <ul className="space-y-1">
                {order.map((ep, i) => {
                  const epNo = startEpisode + i;
                  return (
                    <li
                      key={`${ep.folder}|${ep.video_filename}`}
                      className="flex items-center gap-2 rounded bg-neutral-800/50 px-2 py-1 text-sm"
                    >
                      <span className="w-16 shrink-0 font-mono text-xs text-accent">
                        S{String(season).padStart(2, "0")}E
                        {String(epNo).padStart(2, "0")}
                      </span>
                      <span
                        className="flex-1 truncate text-neutral-200"
                        title={`${ep.folder}/${ep.video_filename}`}
                      >
                        {ep.video_filename}
                      </span>
                      <button
                        onClick={() => swap(i, i - 1)}
                        disabled={i === 0}
                        aria-label={t("actions.move_up", "Move up")}
                        className="rounded px-1 text-neutral-400 hover:text-white disabled:opacity-30"
                      >
                        ▲
                      </button>
                      <button
                        onClick={() => swap(i, i + 1)}
                        disabled={i === order.length - 1}
                        aria-label={t("actions.move_down", "Move down")}
                        className="rounded px-1 text-neutral-400 hover:text-white disabled:opacity-30"
                      >
                        ▼
                      </button>
                    </li>
                  );
                })}
              </ul>
            </div>
          </section>
        </div>

        {error && (
          <div className="border-t border-red-900/40 bg-red-900/20 px-4 py-2 text-sm text-red-200">
            {error}
          </div>
        )}

        <footer className="flex items-center justify-between border-t border-neutral-800 p-4">
          <span className="text-xs text-neutral-500">
            {t("series.episode_count", "{{count}} episodes", {
              count: order.length,
            })}
          </span>
          <div className="flex gap-2">
            <button
              onClick={onClose}
              className="rounded px-3 py-2 text-sm text-neutral-400 hover:text-white"
            >
              {t("actions.cancel")}
            </button>
            <button
              onClick={confirm}
              disabled={!picked || submitting}
              className="rounded bg-accent px-3 py-2 text-sm text-white hover:bg-accent-hover disabled:cursor-not-allowed disabled:opacity-40"
            >
              {t("actions.confirm")}
            </button>
          </div>
        </footer>
      </div>
    </div>
  );
}
