import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { api, type GenreRow } from "@/lib/tauri";

interface Props {
  mediaId: number;
  initialGenres: GenreRow[];
  onClose: () => void;
  onSaved: () => void;
}

function displayName(g: GenreRow) {
  return g.translated_name ?? g.canonical_name;
}

/** Reorder editor: drag-free, button-based row reorder. First entry = primary. */
export default function GenreEditor({
  mediaId,
  initialGenres,
  onClose,
  onSaved,
}: Props) {
  const { t } = useTranslation();
  const [list, setList] = useState<GenreRow[]>(initialGenres);
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    setList(initialGenres);
  }, [initialGenres]);

  const swap = (i: number, j: number) => {
    if (j < 0 || j >= list.length) return;
    const next = list.slice();
    [next[i], next[j]] = [next[j], next[i]];
    setList(next);
  };

  const save = async () => {
    setSaving(true);
    setError(null);
    try {
      await api.reorderMediaGenres(
        mediaId,
        list.map((g) => g.id),
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
              aria-label="Move up"
              className="rounded px-1 text-neutral-400 hover:text-white disabled:opacity-30"
            >
              ▲
            </button>
            <button
              onClick={() => swap(i, i + 1)}
              disabled={i === list.length - 1}
              aria-label="Move down"
              className="rounded px-1 text-neutral-400 hover:text-white disabled:opacity-30"
            >
              ▼
            </button>
          </li>
        ))}
      </ul>
    </div>
  );
}
