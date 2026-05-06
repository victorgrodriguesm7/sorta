import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { api, type SearchResult } from "@/lib/tauri";

interface Props {
  initialQuery: string;
  sourceFolder: string;
  videoFilename: string;
  onClose: () => void;
  onLinked: () => void;
}

const POSTER_BASE = "https://image.tmdb.org/t/p/w154";

export default function SearchDialog({
  initialQuery,
  sourceFolder,
  videoFilename,
  onClose,
  onLinked,
}: Props) {
  const { t } = useTranslation();
  const [query, setQuery] = useState(initialQuery);
  const [results, setResults] = useState<SearchResult[]>([]);
  const [picked, setPicked] = useState<SearchResult | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const runSearch = async (q: string) => {
    if (!q.trim()) return;
    setLoading(true);
    setError(null);
    try {
      const res = await api.tmdbSearch(q.trim());
      setResults(res);
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    void runSearch(initialQuery);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const confirm = async () => {
    if (!picked) return;
    setError(null);
    try {
      await api.linkMedia({
        sourceFolder,
        videoFilename,
        tmdbId: picked.id,
        mediaType: picked.media_type,
      });
      onLinked();
    } catch (e) {
      setError((e as Error).message);
    }
  };

  return (
    <div className="fixed inset-0 z-30 flex items-center justify-center bg-black/70 p-6">
      <div className="flex h-full max-h-[80vh] w-full max-w-3xl flex-col overflow-hidden rounded-lg border border-neutral-700 bg-neutral-900 shadow-xl">
        <header className="flex items-center justify-between border-b border-neutral-800 p-4">
          <h3 className="text-lg font-semibold text-neutral-100">
            {t("actions.search")}
          </h3>
          <button
            onClick={onClose}
            className="rounded p-1 text-neutral-400 hover:text-white"
            aria-label={t("actions.cancel")}
          >
            ✕
          </button>
        </header>

        <div className="flex items-center gap-2 border-b border-neutral-800 p-4">
          <input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && runSearch(query)}
            placeholder={t("actions.search")}
            className="flex-1 rounded bg-neutral-800 px-3 py-2 text-sm text-white outline-none focus:ring-2 focus:ring-accent"
          />
          <button
            onClick={() => runSearch(query)}
            className="rounded bg-accent px-3 py-2 text-sm text-white hover:bg-accent-hover"
          >
            {t("actions.search")}
          </button>
        </div>

        {error && (
          <div className="border-b border-red-900/40 bg-red-900/20 px-4 py-2 text-sm text-red-200">
            {error}
          </div>
        )}

        <div className="flex-1 overflow-y-auto">
          {loading && (
            <div className="p-4 text-sm text-neutral-500">…</div>
          )}
          <ul>
            {results.map((r) => {
              const active = picked?.id === r.id && picked.media_type === r.media_type;
              return (
                <li key={`${r.media_type}-${r.id}`}>
                  <button
                    onClick={() => setPicked(r)}
                    className={`flex w-full items-start gap-3 border-b border-neutral-800 p-3 text-left transition ${
                      active ? "bg-accent/20" : "hover:bg-neutral-800"
                    }`}
                  >
                    <div className="h-24 w-16 shrink-0 overflow-hidden rounded bg-neutral-800">
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
                        {r.title}{" "}
                        <span className="text-xs text-neutral-500">
                          ({r.media_type})
                        </span>
                      </div>
                      <div className="text-xs text-neutral-500">
                        {r.year ?? "—"} · TMDB #{r.id}
                      </div>
                      {r.original_title && r.original_title !== r.title && (
                        <div className="text-xs italic text-neutral-500">
                          {r.original_title}
                        </div>
                      )}
                    </div>
                  </button>
                </li>
              );
            })}
          </ul>
        </div>

        <footer className="flex items-center justify-end gap-2 border-t border-neutral-800 p-4">
          <button
            onClick={onClose}
            className="rounded px-3 py-2 text-sm text-neutral-400 hover:text-white"
          >
            {t("actions.cancel")}
          </button>
          <button
            disabled={!picked}
            onClick={confirm}
            className="rounded bg-accent px-3 py-2 text-sm text-white hover:bg-accent-hover disabled:cursor-not-allowed disabled:opacity-40"
          >
            {t("actions.confirm")}
          </button>
        </footer>
      </div>
    </div>
  );
}
