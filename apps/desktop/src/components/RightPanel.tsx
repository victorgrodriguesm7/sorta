import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { useLibrary } from "@/stores/library";
import { api, type EpisodeRow, type GenreRow } from "@/lib/tauri";
import SearchDialog from "./SearchDialog";
import RecatalogDialog from "./RecatalogDialog";
import GenreEditor from "./GenreEditor";
import { formatBytes } from "@/lib/format";

export default function RightPanel() {
  const { t } = useTranslation();
  const {
    selection,
    refresh,
    config,
    selectItem,
    openCompression,
    compressionDoneTick,
  } = useLibrary();
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
  // Mirrors `row.is_new` but updated optimistically so toggling the
  // chip is instant.
  const [isNew, setIsNew] = useState(false);
  const [episodes, setEpisodes] = useState<EpisodeRow[]>([]);
  const [recatalogOpen, setRecatalogOpen] = useState(false);
  const [recatalogReport, setRecatalogReport] = useState<string | null>(null);

  useEffect(() => {
    setRenaming(false);
    setEditingGenres(false);
    setUnlinkConfirm(false);
    setRecatalogOpen(false);
    setRecatalogReport(null);
    setError(null);
    if (selection?.kind === "media") {
      setNewTitle(selection.row.title);
      setIsNew(selection.row.is_new);
      setEpisodes([]);
      const id = selection.row.id;
      const drive = selection.row.drive_root;
      if (selection.row.media_type === "tv") {
        void api
          .listEpisodes(id, drive)
          .then(setEpisodes)
          .catch(() => setEpisodes([]));
      }
      void api
        .listMediaGenres(id, drive)
        .then(setGenres)
        .catch((e) => setError((e as Error).message));
      // Prefer the locally cached poster (returned as a data: URL by the
      // backend); fall back to whatever poster_url the row stored
      // (typically the TMDB CDN).
      setPosterSrc(selection.row.poster_url ?? null);
      void api
        .getPosterUrl(id, drive)
        .then((src) => {
          if (src) setPosterSrc(src);
        })
        .catch(() => {
          /* non-fatal: keep the fallback */
        });
    } else {
      setGenres([]);
      setPosterSrc(null);
      setTotalBytes(null);
      setHasOriginals(false);
    }
  }, [selection]);

  // Folder size + originals presence (best-effort). Refetched whenever
  // the selection changes OR a compression job just finished.
  useEffect(() => {
    if (selection?.kind !== "media") return;
    const id = selection.row.id;
    const drive = selection.row.drive_root;
    setTotalBytes(null);
    setHasOriginals(false);
    void api
      .mediaTotalBytes(id, drive)
      .then(setTotalBytes)
      .catch(() => setTotalBytes(0));
    void api
      .hasOriginalBackups(id, drive)
      .then(setHasOriginals)
      .catch(() => setHasOriginals(false));
  }, [selection, compressionDoneTick]);

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
  // Resolve absolute paths against the row's own drive — falling back
  // to `config.hd_root` (the primary drive) would point the file
  // manager at the wrong disk when the row lives elsewhere.
  const baseUrl = row.drive_root ?? config?.hd_root ?? "";

  const handleRename = async () => {
    setError(null);
    try {
      // The backend returns the *updated* row (new folder_path, same id).
      // Without re-seating selection, the panel keeps rendering the
      // pre-rename row and the input snaps back to the old title on
      // the next selection effect — looks like nothing happened.
      const updated = await api.renameMedia(row.id, newTitle, row.drive_root);
      setRenaming(false);
      selectItem({ kind: "media", row: updated });
      await refresh();
    } catch (e) {
      setError((e as Error).message);
    }
  };

  const handleOpenInExplorer = async () => {
    setError(null);
    try {
      // `folder_path` is relative to the HD root; the backend resolves
      // and reveals it. Pass the absolute path so the command stays
      // ignorant of which drive a row lives on.
      const abs = baseUrl
        ? `${baseUrl}/${row.folder_path}`
        : row.folder_path;
      await api.openInExplorer(abs);
    } catch (e) {
      setError((e as Error).message);
    }
  };

  /** Group episodes by season for the accordion view. Sorted by
   *  season number, with episodes inside each season sorted by
   *  episode number. */
  const seasonGroups = (() => {
    if (row.media_type !== "tv") return [];
    const map = new Map<number, EpisodeRow[]>();
    for (const ep of episodes) {
      const arr = map.get(ep.season_number) ?? [];
      arr.push(ep);
      map.set(ep.season_number, arr);
    }
    return Array.from(map.entries())
      .sort((a, b) => a[0] - b[0])
      .map(([season, eps]) => ({
        season,
        episodes: eps.sort((a, b) => a.episode_number - b.episode_number),
      }));
  })();

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
          {row.catalogued_at && (
            <>
              <dt className="text-neutral-500">
                {t("media.catalogued_at", "Catalogued")}
              </dt>
              <dd
                className="text-xs"
                title={row.catalogued_at}
              >
                {row.catalogued_at.slice(0, 10)}
              </dd>
            </>
          )}
          <dt className="text-neutral-500">Path</dt>
          <dd className="break-all text-xs">
            {baseUrl}
            {baseUrl ? "/" : ""}
            {row.folder_path}
          </dd>
        </dl>

        <label className="flex w-fit cursor-pointer items-center gap-2 text-xs text-neutral-300">
          <input
            type="checkbox"
            checked={isNew}
            onChange={async (e) => {
              const next = e.target.checked;
              setIsNew(next);
              try {
                await api.setMediaIsNew(row.id, next, row.drive_root);
                await refresh();
              } catch (err) {
                setIsNew(!next);
                setError((err as Error).message);
              }
            }}
            className="h-4 w-4 cursor-pointer accent-accent"
          />
          {t("media.mark_as_new", "Mark as new")}
        </label>

        <div>
          {editingGenres ? (
            <GenreEditor
              mediaId={row.id}
              mediaType={row.media_type === "tv" ? "tv" : "movie"}
              driveRoot={row.drive_root}
              initialGenres={genres}
              onClose={() => setEditingGenres(false)}
              onSaved={async () => {
                setEditingGenres(false);
                await refresh();
                const next = await api.listMediaGenres(row.id, row.drive_root);
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

        {row.media_type === "tv" && seasonGroups.length > 0 && (
          <div className="space-y-1">
            {seasonGroups.map(({ season, episodes: eps }) => (
              <details
                key={season}
                className="rounded border border-neutral-800 bg-neutral-900/40"
              >
                <summary className="flex cursor-pointer select-none items-center justify-between gap-2 px-3 py-2 text-xs uppercase tracking-wide text-neutral-400">
                  <span>
                    {t("media.season", "Season")} {season}
                  </span>
                  <span className="text-neutral-500">
                    {t("media.episode_count", "{{count}} ep", {
                      count: eps.length,
                    })}
                  </span>
                </summary>
                <ul className="max-h-64 divide-y divide-neutral-800/60 overflow-y-auto text-xs">
                  {eps.map((ep) => (
                    <li
                      key={ep.id}
                      className="flex items-baseline gap-2 px-3 py-1.5"
                    >
                      <span className="font-mono text-accent">
                        S{String(ep.season_number).padStart(2, "0")}E
                        {String(ep.episode_number).padStart(2, "0")}
                      </span>
                      <span className="flex-1 truncate text-neutral-200">
                        {ep.title ?? "—"}
                      </span>
                      {ep.air_date && (
                        <span className="text-neutral-500">{ep.air_date}</span>
                      )}
                    </li>
                  ))}
                </ul>
              </details>
            ))}
          </div>
        )}

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
            className="rounded border border-neutral-700 px-3 py-1.5 text-sm text-neutral-200 hover:bg-neutral-800"
            onClick={handleOpenInExplorer}
          >
            {t("actions.open_in_explorer", "Open in file explorer")}
          </button>
          {row.media_type === "tv" && (
            <button
              className="rounded border border-neutral-700 px-3 py-1.5 text-sm text-neutral-200 hover:bg-neutral-800"
              onClick={() => setRecatalogOpen(true)}
              title={t(
                "actions.recatalog_help",
                "Re-fetch TMDB metadata for this series. Useful for older catalog rows that pre-date per-episode storage.",
              )}
            >
              {t("actions.recatalog", "Re-Catalog")}
            </button>
          )}
          <button
            className="rounded border border-red-900/60 px-3 py-1.5 text-sm text-red-300 hover:bg-red-900/30"
            onClick={() => setUnlinkConfirm(true)}
          >
            {t("actions.unlink", "Unlink")}
          </button>
          <button
            className="rounded border border-neutral-700 px-3 py-1.5 text-sm text-neutral-200 hover:bg-neutral-800"
            onClick={() => openCompression(row, totalBytes ?? 0)}
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
                  await api.cleanupOriginalsFor(row.id, row.drive_root);
                  const fresh = await api.mediaTotalBytes(row.id, row.drive_root);
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


        {recatalogReport && (
          <div className="rounded border border-emerald-900/60 bg-emerald-900/20 px-3 py-2 text-sm text-emerald-100">
            {recatalogReport}
          </div>
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
                    await api.unlinkMedia(row.id, true, row.drive_root);
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

        {recatalogOpen && (
          <RecatalogDialog
            media={row}
            onClose={() => setRecatalogOpen(false)}
            onDone={async (result) => {
              setRecatalogOpen(false);
              setRecatalogReport(
                t(
                  "series.recatalog_done",
                  "Re-cataloged {{seasons}} season(s), {{episodes}} episode(s). Renamed {{renamed}}, stills downloaded {{stills}}.{{skippedHint}}",
                  {
                    seasons: result.seasons_processed,
                    episodes: result.episodes_processed,
                    renamed: result.episodes_renamed,
                    stills: result.stills_downloaded,
                    skippedHint:
                      result.skipped.length > 0
                        ? t(
                            "series.recatalog_skipped",
                            " Skipped {{count}} file(s): {{names}}",
                            {
                              count: result.skipped.length,
                              names: result.skipped.slice(0, 3).join(", ")
                                + (result.skipped.length > 3 ? "…" : ""),
                            },
                          )
                        : "",
                  },
                ),
              );
              await refresh();
              // Refresh the episodes section without changing selection.
              try {
                const eps = await api.listEpisodes(row.id, row.drive_root);
                setEpisodes(eps);
              } catch {
                /* non-fatal */
              }
            }}
          />
        )}
      </div>
    </div>
  );
}
