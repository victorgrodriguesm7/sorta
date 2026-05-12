import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { api, type GenreRow, type MediaType } from "@/lib/tauri";

interface Props {
  mediaId: number;
  mediaType: MediaType;
  /** Drive the row lives on. Passed through to `reorder_media_genres`
   *  so the backend can dispatch the write to the correct pool. */
  driveRoot: string | null;
  initialGenres: GenreRow[];
  onClose: () => void;
  onSaved: () => void;
}

function displayName(g: GenreRow) {
  return g.translated_name ?? g.canonical_name;
}

/** Reorder + add + remove editor for a media row's genres. First entry = primary. */
export default function GenreEditor({
  mediaId,
  mediaType,
  driveRoot,
  initialGenres,
  onClose,
  onSaved,
}: Props) {
  const { t } = useTranslation();
  const [list, setList] = useState<GenreRow[]>(initialGenres);
  const [available, setAvailable] = useState<GenreRow[]>([]);
  const [pickerOpen, setPickerOpen] = useState(false);
  const [pickerQuery, setPickerQuery] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [loadingAvailable, setLoadingAvailable] = useState(false);

  useEffect(() => {
    setList(initialGenres);
  }, [initialGenres]);

  // Lazy-load the full TMDB genre catalogue the first time the user
  // opens the picker, so we don't hit the network on every render.
  const ensureAvailableLoaded = async () => {
    if (available.length > 0) return;
    setLoadingAvailable(true);
    setError(null);
    try {
      const all = await api.tmdbSyncGenres(mediaType);
      setAvailable(all);
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setLoadingAvailable(false);
    }
  };

  const candidates = useMemo(() => {
    const onRow = new Set(list.map((g) => g.id));
    const q = pickerQuery.trim().toLowerCase();
    return available
      .filter((g) => !onRow.has(g.id))
      .filter((g) => (q ? displayName(g).toLowerCase().includes(q) : true));
  }, [available, list, pickerQuery]);

  const swap = (i: number, j: number) => {
    if (j < 0 || j >= list.length) return;
    const next = list.slice();
    [next[i], next[j]] = [next[j], next[i]];
    setList(next);
  };

  const remove = (id: number) => {
    setList((prev) => prev.filter((g) => g.id !== id));
  };

  const add = (g: GenreRow) => {
    setList((prev) => [...prev, g]);
    setPickerQuery("");
    setPickerOpen(false);
  };

  const save = async () => {
    if (list.length === 0) {
      setError(t("media.need_one_genre", "At least one genre is required."));
      return;
    }
    setSaving(true);
    setError(null);
    try {
      await api.reorderMediaGenres(
        mediaId,
        list.map((g) => g.id),
        driveRoot,
      );
      onSaved();
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="rounded border border-neutral-700 bg-neutral-900/80 p-3">
      <div className="mb-2 flex items-center justify-between">
        <span className="text-xs uppercase tracking-wide text-neutral-500">
          {t("media.genres", "Genres (first = primary)")}
        </span>
        <div className="flex gap-2">
          <button
            onClick={onClose}
            className="rounded px-2 py-1 text-xs text-neutral-400 hover:text-white"
          >
            {t("actions.cancel")}
          </button>
          <button
            onClick={save}
            disabled={saving}
            className="rounded bg-accent px-2 py-1 text-xs text-white hover:bg-accent-hover disabled:opacity-40"
          >
            {t("actions.save")}
          </button>
        </div>
      </div>
      {error && (
        <div className="mb-2 rounded bg-red-900/40 px-2 py-1 text-xs text-red-200">
          {error}
        </div>
      )}

      <ul className="space-y-1">
        {list.map((g, i) => (
          <li
            key={`${g.media_type}-${g.id}`}
            className={`flex items-center gap-2 rounded px-2 py-1 text-sm ${
              i === 0
                ? "bg-accent/20 text-white"
                : "bg-neutral-800/60 text-neutral-200"
            }`}
          >
            <span className="w-6 text-right text-xs text-neutral-500">{i + 1}</span>
            <span className="flex-1 truncate">{displayName(g)}</span>
            {i === 0 && (
              <span className="rounded bg-accent px-1.5 py-0.5 text-[10px] uppercase tracking-wide text-white">
                {t("media.primary", "Primary")}
              </span>
            )}
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
              disabled={i === list.length - 1}
              aria-label={t("actions.move_down", "Move down")}
              className="rounded px-1 text-neutral-400 hover:text-white disabled:opacity-30"
            >
              ▼
            </button>
            <button
              onClick={() => remove(g.id)}
              aria-label={t("actions.remove", "Remove")}
              className="rounded px-1 text-neutral-500 hover:text-red-300"
            >
              ✕
            </button>
          </li>
        ))}
      </ul>

      <div className="mt-3 border-t border-neutral-800 pt-3">
        {!pickerOpen ? (
          <button
            onClick={async () => {
              setPickerOpen(true);
              await ensureAvailableLoaded();
            }}
            className="w-full rounded border border-dashed border-neutral-700 px-2 py-1.5 text-xs text-neutral-400 hover:border-neutral-500 hover:text-white"
          >
            + {t("actions.add_genre", "Add genre")}
          </button>
        ) : (
          <div className="space-y-2">
            <div className="flex items-center gap-2">
              <input
                autoFocus
                value={pickerQuery}
                onChange={(e) => setPickerQuery(e.target.value)}
                placeholder={t("media.search_genre", "Search genre…")}
                className="flex-1 rounded bg-neutral-800 px-2 py-1 text-sm text-white outline-none focus:ring-2 focus:ring-accent"
              />
              <button
                onClick={() => {
                  setPickerOpen(false);
                  setPickerQuery("");
                }}
                className="rounded px-2 py-1 text-xs text-neutral-400 hover:text-white"
              >
                {t("actions.cancel")}
              </button>
            </div>
            {loadingAvailable && (
              <div className="text-xs text-neutral-500">…</div>
            )}
            {!loadingAvailable && candidates.length === 0 && (
              <div className="text-xs text-neutral-500">
                {t("media.no_more_genres", "No more genres to add.")}
              </div>
            )}
            <ul className="max-h-48 overflow-y-auto rounded bg-neutral-950 p-1">
              {candidates.map((g) => (
                <li key={`pick-${g.media_type}-${g.id}`}>
                  <button
                    onClick={() => add(g)}
                    className="block w-full truncate rounded px-2 py-1 text-left text-xs text-neutral-200 hover:bg-neutral-800 hover:text-white"
                  >
                    {displayName(g)}
                  </button>
                </li>
              ))}
            </ul>
          </div>
        )}
      </div>
    </div>
  );
}
