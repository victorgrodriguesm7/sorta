import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { useLibrary } from "@/stores/library";
import { api, type GenreRow } from "@/lib/tauri";
import SearchDialog from "./SearchDialog";
import GenreEditor from "./GenreEditor";
import CompressionDialog from "./CompressionDialog";
import { formatBytes } from "@/lib/format";

export default function RightPanel() {
  const { t } = useTranslation();
  const { selection, refresh, config, selectItem } = useLibrary();
  const [searchOpen, setSearchOpen] = useState(false);
  const [renaming, setRenaming] = useState(false);
  const [newTitle, setNewTitle] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [genres, setGenres] = useState<GenreRow[]>([]);
  const [editingGenres, setEditingGenres] = useState(false);
  const [posterSrc, setPosterSrc] = useState<string | null>(null);
  const [unlinkConfirm, setUnlinkConfirm] = useState(false);
  const [unlinking, setUnlinking] = useState(false);
  const [totalBytes, setTotalBytes] = useState<number | null>(null);
  const [hasOriginals, setHasOriginals] = useState(false);
  const [cleaningUp, setCleaningUp] = useState(false);
  const [compressOpen, setCompressOpen] = useState(false);

  useEffect(() => {
    setRenaming(false);
    setEditingGenres(false);
    setUnlinkConfirm(false);
    setError(null);
    if (selection?.kind === "media") {
      setNewTitle(selection.row.title);
      const id = selection.row.id;
      void api
        .listMediaGenres(id)
        .then(setGenres)
        .catch((e) => setError((e as Error).message));
      // Prefer the locally cached poster (returned as a data: URL by the
      // backend); fall back to whatever poster_url the row stored
      // (typically the TMDB CDN).
      setPosterSrc(selection.row.poster_url ?? null);
      void api
        .getPosterUrl(id)
        .then((src) => {
          if (src) setPosterSrc(src);
        })
        .catch(() => {
          /* non-fatal: keep the fallback */
        });
      // Folder size + originals presence (best-effort).
      setTotalBytes(null);
      setHasOriginals(false);
      void api
        .mediaTotalBytes(id)
        .then(setTotalBytes)
        .catch(() => setTotalBytes(0));
      void api
        .hasOriginalBackups(id)
        .then(setHasOriginals)
        .catch(() => setHasOriginals(false));
    } else {
      setGenres([]);
      setPosterSrc(null);
      setTotalBytes(null);
      setHasOriginals(false);
    }
  }, [selection]);

  if (!selection) {
    return (
      <div className="flex flex-1 items-center justify-center bg-neutral-925 text-sm text-neutral-500">
        {t("media.no_selection", "Nothing selected")}
      </div>
    );
  }

  if (selection.kind === "uncatalogued") {
    return (
      <div className="flex flex-1 flex-col gap-4 p-6">
        <div>
          <div className="text-xs uppercase tracking-wide text-neutral-500">
            {t("panel.uncatalogued")}
          </div>
          <div className="mt-1 break-all text-lg font-medium text-neutral-100">
            {selection.videoFilename}
          </div>
          <div className="mt-1 break-all text-xs text-neutral-500">
            {selection.folder}
          </div>
        </div>
        <button
          className="self-start rounded bg-accent px-4 py-2 text-sm font-medium text-white hover:bg-accent-hover"
          onClick={() => setSearchOpen(true)}
        >
          {t("actions.search")}
        </button>
        {searchOpen && (
          <SearchDialog
            initialQuery={selection.videoFilename.replace(/\.[^.]+$/, "")}
            sourceFolder={selection.folder}
            videoFilename={selection.videoFilename}
            onClose={() => setSearchOpen(false)}
            onLinked={async () => {
              setSearchOpen(false);
              await refresh();
            }}
          />
        )}
      </div>
    );
  }

  const { row } = selection;
  const baseUrl = config?.hd_root ?? "";

  const handleRename = async () => {
    setError(null);
    try {
      await api.renameMedia(row.id, newTitle);
      setRenaming(false);
      await refresh();
    } catch (e) {
      setError((e as Error).message);
    }
  };

  return (
    <div className="flex flex-1 gap-6 overflow-y-auto p-6">
      <div className="w-48 shrink-0">
        {posterSrc ? (
          <img
            src={posterSrc}
            alt={row.title}
            className="aspect-[2/3] w-full rounded object-cover shadow"
          />
        ) : (
          <div className="aspect-[2/3] w-full rounded bg-neutral-800" />
        )}
      </div>
      <div className="flex-1 space-y-3">
        {renaming ? (
          <div className="flex items-center gap-2">
            <input
              className="flex-1 rounded bg-neutral-800 px-3 py-2 text-lg text-white outline-none focus:ring-2 focus:ring-accent"
              value={newTitle}
              onChange={(e) => setNewTitle(e.target.value)}
              autoFocus
            />
            <button
              className="rounded bg-accent px-3 py-2 text-sm text-white"
              onClick={handleRename}
            >
              {t("actions.save")}
            </button>
            <button
              className="rounded px-3 py-2 text-sm text-neutral-400 hover:text-white"
              onClick={() => setRenaming(false)}
            >
              {t("actions.cancel")}
            </button>
          </div>
        ) : (
          <h2 className="text-2xl font-semibold text-neutral-100">{row.title}</h2>
        )}

        <dl className="grid grid-cols-[max-content_1fr] gap-x-4 gap-y-1 text-sm text-neutral-300">
          <dt className="text-neutral-500">{t("media.tmdb_id")}</dt>
          <dd>{row.tmdb_id}</dd>
          {row.runtime_minutes !== null && (
            <>
              <dt className="text-neutral-500">{t("media.runtime")}</dt>
              <dd>{row.runtime_minutes} min</dd>
            </>
          )}
          <dt className="text-neutral-500">{t("media.size", "Size")}</dt>
          <dd>
            {totalBytes != null ? formatBytes(totalBytes) : "…"}
          </dd>
          <dt className="text-neutral-500">Path</dt>
          <dd className="break-all text-xs">
            {baseUrl}
            {baseUrl ? "/" : ""}
            {row.folder_path}
          </dd>
        </dl>

        <div>
          {editingGenres ? (
            <GenreEditor
              mediaId={row.id}
              mediaType={row.media_type === "tv" ? "tv" : "movie"}
              initialGenres={genres}
              onClose={() => setEditingGenres(false)}
              onSaved={async () => {
                setEditingGenres(false);
                await refresh();
                const next = await api.listMediaGenres(row.id);
                setGenres(next);
              }}
            />
          ) : (
            <div className="flex items-start gap-2">
              <div className="flex-1">
                <div className="text-xs uppercase tracking-wide text-neutral-500">
                  {t("media.genres", "Genres")}
                </div>
                {genres.length === 0 ? (
                  <div className="text-sm text-neutral-500">—</div>
                ) : (
                  <ul className="flex flex-wrap gap-1">
                    {genres.map((g, i) => (
                      <li
                        key={`${g.media_type}-${g.id}`}
                        className={`rounded px-2 py-0.5 text-xs ${
                          i === 0
                            ? "bg-accent text-white"
                            : "bg-neutral-800 text-neutral-300"
                        }`}
                        title={i === 0 ? t("media.primary", "Primary") : undefined}
                      >
                        {g.translated_name ?? g.canonical_name}
                      </li>
                    ))}
                  </ul>
                )}
              </div>
              <button
                onClick={() => setEditingGenres(true)}
                aria-label={t("actions.edit_genres", "Edit genres")}
                className="rounded p-1 text-neutral-400 hover:bg-neutral-800 hover:text-white"
              >
                ✎
              </button>
            </div>
          )}
        </div>

        {error && (
          <div className="rounded bg-red-900/40 px-3 py-2 text-sm text-red-200">
            {error}
          </div>
        )}

        <div className="flex flex-wrap gap-2 pt-2">
          <button
            className="rounded border border-neutral-700 px-3 py-1.5 text-sm text-neutral-200 hover:bg-neutral-800"
            onClick={() => {
              setNewTitle(row.title);
              setRenaming(true);
            }}
          >
            {t("actions.rename")}
          </button>
          <button
            className="rounded border border-neutral-700 px-3 py-1.5 text-sm text-neutral-200 hover:bg-neutral-800"
            onClick={() => setSearchOpen(true)}
          >
            {t("actions.search")}
          </button>
          <button
            className="rounded border border-red-900/60 px-3 py-1.5 text-sm text-red-300 hover:bg-red-900/30"
            onClick={() => setUnlinkConfirm(true)}
          >
            {t("actions.unlink", "Unlink")}
          </button>
          <button
            className="rounded border border-neutral-700 px-3 py-1.5 text-sm text-neutral-200 hover:bg-neutral-800"
            onClick={() => setCompressOpen(true)}
          >
            {t("actions.compress", "Compress")}
          </button>
          {hasOriginals && (
            <button
              disabled={cleaningUp}
              className="rounded border border-yellow-800/60 px-3 py-1.5 text-sm text-yellow-200 hover:bg-yellow-900/30 disabled:opacity-40"
              onClick={async () => {
                setCleaningUp(true);
                setError(null);
                try {
                  await api.cleanupOriginalsFor(row.id);
                  const fresh = await api.mediaTotalBytes(row.id);
                  setTotalBytes(fresh);
                  setHasOriginals(false);
                } catch (e) {
                  setError((e as Error).message);
                } finally {
                  setCleaningUp(false);
                }
              }}
            >
              {t("actions.clean_up_originals", "Clean up originals")}
            </button>
          )}
        </div>

        {compressOpen && (
          <CompressionDialog
            media={row}
            totalBytes={totalBytes ?? 0}
            onClose={() => setCompressOpen(false)}
            onDone={async () => {
              const fresh = await api.mediaTotalBytes(row.id);
              setTotalBytes(fresh);
              const has = await api.hasOriginalBackups(row.id);
              setHasOriginals(has);
              await refresh();
            }}
          />
        )}

        {unlinkConfirm && (
          <div className="rounded border border-red-900/60 bg-red-900/20 p-3 text-sm text-red-100">
            <p className="mb-2">
              {t(
                "actions.unlink_confirm",
                "Unlink this from TMDB? The folder will be renamed back so it shows up under Uncatalogued. The cached poster will be deleted. Files will not be deleted.",
              )}
            </p>
            <div className="flex justify-end gap-2">
              <button
                onClick={() => setUnlinkConfirm(false)}
                className="rounded px-2 py-1 text-xs text-neutral-300 hover:text-white"
              >
                {t("actions.cancel")}
              </button>
              <button
                disabled={unlinking}
                onClick={async () => {
                  setUnlinking(true);
                  setError(null);
                  try {
                    await api.unlinkMedia(row.id, true);
                    selectItem(null);
                    await refresh();
                  } catch (e) {
                    setError((e as Error).message);
                  } finally {
                    setUnlinking(false);
                    setUnlinkConfirm(false);
                  }
                }}
                className="rounded bg-red-700 px-3 py-1 text-xs text-white hover:bg-red-600 disabled:opacity-40"
              >
                {t("actions.unlink", "Unlink")}
              </button>
            </div>
          </div>
        )}

        {searchOpen && (
          <SearchDialog
            initialQuery={row.title}
            sourceFolder={`${baseUrl}/${row.folder_path}`}
            videoFilename={`${row.title}.mkv`}
            onClose={() => setSearchOpen(false)}
            onLinked={async () => {
              setSearchOpen(false);
              await refresh();
            }}
          />
        )}
      </div>
    </div>
  );
}
