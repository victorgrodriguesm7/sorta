import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { useLibrary } from "@/stores/library";
import { api } from "@/lib/tauri";
import SearchDialog from "./SearchDialog";

export default function RightPanel() {
  const { t } = useTranslation();
  const { selection, refresh, config } = useLibrary();
  const [searchOpen, setSearchOpen] = useState(false);
  const [renaming, setRenaming] = useState(false);
  const [newTitle, setNewTitle] = useState("");
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    setRenaming(false);
    setError(null);
    if (selection?.kind === "media") setNewTitle(selection.row.title);
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
  const posterSrc = row.poster_url ?? undefined;
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
          <dt className="text-neutral-500">Path</dt>
          <dd className="break-all text-xs">
            {baseUrl}
            {baseUrl ? "/" : ""}
            {row.folder_path}
          </dd>
        </dl>

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
        </div>

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
