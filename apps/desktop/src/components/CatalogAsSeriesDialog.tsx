import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { api, type SearchResult, type UncataloguedItem } from "@/lib/tauri";

interface Props {
  episodes: UncataloguedItem[];
  onClose: () => void;
  onLinked: () => void;
}

const POSTER_BASE = "https://image.tmdb.org/t/p/w154";

const epKey = (e: UncataloguedItem) => `${e.folder}|${e.video_filename}`;

/** Move every item whose key is in `selected` one slot up. Items whose
 *  predecessor is also selected stay put — so contiguous blocks shift
 *  up as one unit and non-contiguous selections each shift by one. */
function moveSelectionUp<T>(list: T[], isSelected: (t: T) => boolean): T[] {
  const out = list.slice();
  for (let i = 1; i < out.length; i++) {
    if (isSelected(out[i]) && !isSelected(out[i - 1])) {
      [out[i - 1], out[i]] = [out[i], out[i - 1]];
    }
  }
  return out;
}

function moveSelectionDown<T>(list: T[], isSelected: (t: T) => boolean): T[] {
  const out = list.slice();
  for (let i = out.length - 2; i >= 0; i--) {
    if (isSelected(out[i]) && !isSelected(out[i + 1])) {
      [out[i + 1], out[i]] = [out[i], out[i + 1]];
    }
  }
  return out;
}

/** Drop the items with keys in `dragKeys` immediately before the item at
 *  `targetIndex` in the original list. Preserves the relative order of
 *  the dragged group. If the dragged group includes the target, no-op. */
function reorderByDrop(
  list: UncataloguedItem[],
  dragKeys: Set<string>,
  targetIndex: number,
): UncataloguedItem[] {
  const moving = list.filter((e) => dragKeys.has(epKey(e)));
  if (moving.length === 0) return list;
  const remaining = list.filter((e) => !dragKeys.has(epKey(e)));
  // Translate targetIndex from the full list into the remaining list:
  // count how many surviving items lie at indices < targetIndex.
  let insertAt = 0;
  for (let i = 0; i < targetIndex && i < list.length; i++) {
    if (!dragKeys.has(epKey(list[i]))) insertAt++;
  }
  return [
    ...remaining.slice(0, insertAt),
    ...moving,
    ...remaining.slice(insertAt),
  ];
}

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
  const [rename, setRename] = useState<boolean>(true);
  const [downloadEpisodePosters, setDownloadEpisodePosters] =
    useState<boolean>(true);
  const [isNew, setIsNew] = useState<boolean>(false);
  // Episodes in apply order. Initial order = walker order.
  const [order, setOrder] = useState<UncataloguedItem[]>(episodes);
  // Per-row checkboxes for multi-select reorder (separate from the
  // outer LeftPanel selection — these only live for this dialog's
  // lifetime).
  const [pickedRows, setPickedRows] = useState<Set<string>>(new Set());
  // Drag state — which keys are currently being dragged.
  const [dragKeys, setDragKeys] = useState<Set<string>>(new Set());
  const [dragOverIndex, setDragOverIndex] = useState<number | null>(null);
  const [loading, setLoading] = useState(false);
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    setOrder(episodes);
    setPickedRows(new Set());
  }, [episodes]);

  const togglePicked = (key: string) => {
    setPickedRows((prev) => {
      const next = new Set(prev);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
  };
  const isPicked = (e: UncataloguedItem) => pickedRows.has(epKey(e));

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

  // Ctrl+ArrowUp / Ctrl+ArrowDown mirror the Move Up / Move Down buttons.
  // When 1+ rows are checked, the entire selection moves; otherwise the
  // shortcut is a no-op (there's no "focused" row to move).
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (!e.ctrlKey || (e.key !== "ArrowUp" && e.key !== "ArrowDown")) return;
      if (pickedRows.size === 0) return;
      e.preventDefault();
      setOrder((cur) =>
        e.key === "ArrowUp"
          ? moveSelectionUp(cur, (it) => pickedRows.has(epKey(it)))
          : moveSelectionDown(cur, (it) => pickedRows.has(epKey(it))),
      );
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [pickedRows]);

  // Single-row swap — used as a fallback when nothing is checked.
  const swap = (i: number, j: number) => {
    if (j < 0 || j >= order.length) return;
    const next = order.slice();
    [next[i], next[j]] = [next[j], next[i]];
    setOrder(next);
  };

  const moveUp = (clickedIndex: number) => {
    if (pickedRows.size === 0) {
      swap(clickedIndex, clickedIndex - 1);
      return;
    }
    setOrder((cur) => moveSelectionUp(cur, isPicked));
  };

  const moveDown = (clickedIndex: number) => {
    if (pickedRows.size === 0) {
      swap(clickedIndex, clickedIndex + 1);
      return;
    }
    setOrder((cur) => moveSelectionDown(cur, isPicked));
  };

  // Drag handlers — if the user starts dragging a checked row, the
  // entire selection moves; otherwise just that single row.
  const onRowDragStart = (e: React.DragEvent, key: string) => {
    const movingSet = pickedRows.has(key)
      ? new Set(pickedRows)
      : new Set([key]);
    setDragKeys(movingSet);
    e.dataTransfer.effectAllowed = "move";
    // Required by Firefox to actually start the drag.
    e.dataTransfer.setData("text/plain", key);
  };
  const onRowDragOver = (e: React.DragEvent, index: number) => {
    if (dragKeys.size === 0) return;
    e.preventDefault();
    e.dataTransfer.dropEffect = "move";
    setDragOverIndex(index);
  };
  const onRowDrop = (e: React.DragEvent, targetIndex: number) => {
    e.preventDefault();
    if (dragKeys.size === 0) return;
    setOrder((cur) => reorderByDrop(cur, dragKeys, targetIndex));
    setDragKeys(new Set());
    setDragOverIndex(null);
  };
  const onRowDragEnd = () => {
    setDragKeys(new Set());
    setDragOverIndex(null);
  };
  const onListDropAtEnd = (e: React.DragEvent) => {
    e.preventDefault();
    if (dragKeys.size === 0) return;
    setOrder((cur) => reorderByDrop(cur, dragKeys, cur.length));
    setDragKeys(new Set());
    setDragOverIndex(null);
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
        rename,
        downloadEpisodePosters,
        isNew,
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
      <div className="flex h-full max-h-[88vh] w-full max-w-[70vw] flex-col overflow-hidden rounded-lg border border-neutral-700 bg-neutral-900 shadow-xl">
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
            <div className="flex flex-wrap items-end gap-3 border-b border-neutral-800 p-3">
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
                  min={0}
                  value={startEpisode}
                  onChange={(e) => setStartEpisode(Number(e.target.value))}
                  className="mt-1 w-20 rounded bg-neutral-800 px-2 py-1 text-sm text-white outline-none focus:ring-2 focus:ring-accent"
                />
              </label>
              <label
                className="flex cursor-pointer items-center gap-2 text-xs text-neutral-300"
                title={t(
                  "series.rename_help",
                  "If unchecked, the original filenames are kept when files are moved.",
                )}
              >
                <input
                  type="checkbox"
                  checked={rename}
                  onChange={(e) => setRename(e.target.checked)}
                  className="h-4 w-4 cursor-pointer accent-accent"
                />
                {t(
                  "series.rename_to_standard",
                  "Rename to S{XX}E{YY}.{Title}",
                )}
              </label>
              <label
                className="flex cursor-pointer items-center gap-2 text-xs text-neutral-300"
                title={t(
                  "series.download_episode_posters_help",
                  "Fetch one TMDB still per episode at link time.",
                )}
              >
                <input
                  type="checkbox"
                  checked={downloadEpisodePosters}
                  onChange={(e) =>
                    setDownloadEpisodePosters(e.target.checked)
                  }
                  className="h-4 w-4 cursor-pointer accent-accent"
                />
                {t(
                  "series.download_episode_posters",
                  "Download episode stills",
                )}
              </label>
              <label
                className="flex cursor-pointer items-center gap-2 text-xs text-neutral-300"
                title={t(
                  "media.mark_as_new_help",
                  "Flag this item as new so the TV reader highlights it.",
                )}
              >
                <input
                  type="checkbox"
                  checked={isNew}
                  onChange={(e) => setIsNew(e.target.checked)}
                  className="h-4 w-4 cursor-pointer accent-accent"
                />
                {t("media.mark_as_new", "Mark as new")}
              </label>
            </div>
            <div
              className="flex-1 overflow-y-auto p-2"
              onDragOver={(e) => {
                if (dragKeys.size > 0) e.preventDefault();
              }}
              onDrop={onListDropAtEnd}
            >
              {pickedRows.size > 0 && (
                <div className="mb-2 flex items-center justify-between rounded bg-neutral-800/60 px-2 py-1 text-xs text-neutral-300">
                  <span>
                    {t("series.row_selected", "{{count}} selected", {
                      count: pickedRows.size,
                    })}
                  </span>
                  <button
                    onClick={() => setPickedRows(new Set())}
                    className="rounded px-2 py-0.5 text-neutral-400 hover:text-white"
                  >
                    {t("actions.clear", "Clear")}
                  </button>
                </div>
              )}
              <ul className="space-y-1">
                {order.map((ep, i) => {
                  const epNo = startEpisode + i;
                  const key = epKey(ep);
                  const checked = pickedRows.has(key);
                  const beingDragged = dragKeys.has(key);
                  const showDropIndicator =
                    dragOverIndex === i && !dragKeys.has(key);
                  return (
                    <li
                      key={key}
                      draggable
                      onDragStart={(e) => onRowDragStart(e, key)}
                      onDragOver={(e) => onRowDragOver(e, i)}
                      onDrop={(e) => onRowDrop(e, i)}
                      onDragEnd={onRowDragEnd}
                      onDragLeave={() =>
                        setDragOverIndex((cur) => (cur === i ? null : cur))
                      }
                      className={`flex items-center gap-2 rounded px-2 py-1 text-sm transition ${
                        checked
                          ? "bg-accent/30 text-white"
                          : "bg-neutral-800/50 text-neutral-200"
                      } ${beingDragged ? "opacity-40" : ""} ${
                        showDropIndicator ? "border-t-2 border-accent" : ""
                      }`}
                    >
                      <input
                        type="checkbox"
                        checked={checked}
                        onChange={() => togglePicked(key)}
                        className="h-4 w-4 shrink-0 cursor-pointer accent-accent"
                        aria-label={t("actions.select", "Select")}
                      />
                      <span
                        className="cursor-grab select-none text-neutral-500 hover:text-neutral-300 active:cursor-grabbing"
                        title={t("actions.drag", "Drag to reorder")}
                      >
                        ⋮⋮
                      </span>
                      <span
                        className={`w-16 shrink-0 font-mono text-xs ${
                          rename ? "text-accent" : "text-neutral-600 line-through"
                        }`}
                        title={
                          rename
                            ? t("series.target_name", "New filename")
                            : t(
                                "series.original_kept",
                                "Original filename will be kept",
                              )
                        }
                      >
                        S{String(season).padStart(2, "0")}E
                        {String(epNo).padStart(2, "0")}
                      </span>
                      <span
                        className="flex-1 truncate"
                        title={`${ep.folder}/${ep.video_filename}`}
                      >
                        {ep.video_filename}
                      </span>
                      <button
                        onClick={() => moveUp(i)}
                        disabled={i === 0 && pickedRows.size === 0}
                        aria-label={t("actions.move_up", "Move up")}
                        className="rounded px-1 text-neutral-400 hover:text-white disabled:opacity-30"
                      >
                        ▲
                      </button>
                      <button
                        onClick={() => moveDown(i)}
                        disabled={i === order.length - 1 && pickedRows.size === 0}
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
