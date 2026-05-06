import { useState } from "react";
import { useTranslation } from "react-i18next";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import { useLibrary } from "@/stores/library";
import { api } from "@/lib/tauri";
import { formatBytes } from "@/lib/format";

interface Props {
  onClose: () => void;
}

export default function SettingsModal({ onClose }: Props) {
  const { t } = useTranslation();
  const { config, movieGenres, refresh, loadConfig } = useLibrary();
  const [apiKey, setApiKey] = useState(config?.tmdb_api_key ?? "");
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [translations, setTranslations] = useState<Record<number, string>>(
    () => Object.fromEntries(movieGenres.map((g) => [g.id, g.translated_name ?? ""])),
  );
  const [seasonLabel, setSeasonLabel] = useState("Season");
  const [seasonSaving, setSeasonSaving] = useState(false);
  const [backingUp, setBackingUp] = useState(false);
  const [backupNote, setBackupNote] = useState<string | null>(null);

  const runBackup = async () => {
    setError(null);
    setBackupNote(null);
    const stamp = new Date().toISOString().replace(/[:.]/g, "-").slice(0, 19);
    const dest = await saveDialog({
      title: "Backup database",
      defaultPath: `sorta-backup-${stamp}.db`,
      filters: [{ name: "SQLite", extensions: ["db"] }],
    });
    if (typeof dest !== "string") return;
    setBackingUp(true);
    try {
      const r = await api.backupDatabase(dest);
      setBackupNote(
        `${r.destination} (${formatBytes(r.bytes_written)})`,
      );
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setBackingUp(false);
    }
  };

  const pickHd = async () => {
    setError(null);
    try {
      const selected = await openDialog({ directory: true, multiple: false });
      if (typeof selected === "string") {
        await api.setHdRoot(selected);
        await loadConfig();
        await refresh();
      }
    } catch (e) {
      setError((e as Error).message);
    }
  };

  const saveApiKey = async () => {
    setSaving(true);
    setError(null);
    try {
      await api.setApiKey(apiKey);
      await loadConfig();
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setSaving(false);
    }
  };

  const saveTranslations = async () => {
    setSaving(true);
    setError(null);
    try {
      for (const g of movieGenres) {
        const next = (translations[g.id] ?? "").trim();
        const cur = g.translated_name ?? "";
        if (next !== cur) {
          await api.updateGenreTranslation(g.id, next.length ? next : null);
        }
      }
      await refresh();
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="fixed inset-0 z-30 flex items-center justify-center bg-black/70 p-6">
      <div className="flex h-full max-h-[80vh] w-full max-w-2xl flex-col overflow-hidden rounded-lg border border-neutral-700 bg-neutral-900 shadow-xl">
        <header className="flex items-center justify-between border-b border-neutral-800 p-4">
          <h3 className="text-lg font-semibold text-neutral-100">
            {t("settings.title")}
          </h3>
          <button
            onClick={onClose}
            className="rounded p-1 text-neutral-400 hover:text-white"
            aria-label={t("actions.cancel")}
          >
            ✕
          </button>
        </header>

        <div className="flex-1 overflow-y-auto p-5 text-neutral-200">
          {error && (
            <div className="mb-4 rounded bg-red-900/40 px-3 py-2 text-sm text-red-200">
              {error}
            </div>
          )}

          <section className="mb-6 space-y-2">
            <label className="block text-xs uppercase tracking-wide text-neutral-500">
              {t("settings.hd_root")}
            </label>
            <div className="flex gap-2">
              <input
                readOnly
                value={config?.hd_root ?? ""}
                placeholder="—"
                className="flex-1 rounded bg-neutral-800 px-3 py-2 text-sm"
              />
              <button
                onClick={pickHd}
                className="rounded bg-accent px-3 py-2 text-sm text-white hover:bg-accent-hover"
              >
                …
              </button>
            </div>
          </section>

          <section className="mb-6 space-y-2">
            <label className="block text-xs uppercase tracking-wide text-neutral-500">
              {t("settings.tmdb_api_key")}
            </label>
            <div className="flex gap-2">
              <input
                value={apiKey}
                onChange={(e) => setApiKey(e.target.value)}
                placeholder="…"
                type="password"
                className="flex-1 rounded bg-neutral-800 px-3 py-2 text-sm"
              />
              <button
                onClick={saveApiKey}
                disabled={saving}
                className="rounded bg-accent px-3 py-2 text-sm text-white hover:bg-accent-hover disabled:opacity-40"
              >
                {t("actions.save")}
              </button>
            </div>
          </section>

          <section className="mb-6 space-y-2">
            <label className="block text-xs uppercase tracking-wide text-neutral-500">
              {t("settings.season_label", "Season folder name")}
            </label>
            <div className="flex gap-2">
              <input
                value={seasonLabel}
                onChange={(e) => setSeasonLabel(e.target.value)}
                placeholder="Season"
                className="flex-1 rounded bg-neutral-800 px-3 py-2 text-sm"
              />
              <button
                onClick={async () => {
                  setSeasonSaving(true);
                  setError(null);
                  try {
                    await api.updateSeasonLabel(seasonLabel.trim() || "Season");
                    await refresh();
                  } catch (e) {
                    setError((e as Error).message);
                  } finally {
                    setSeasonSaving(false);
                  }
                }}
                disabled={seasonSaving}
                className="rounded bg-accent px-3 py-2 text-sm text-white hover:bg-accent-hover disabled:opacity-40"
              >
                {t("actions.save")}
              </button>
            </div>
          </section>

          <section className="mb-6 space-y-2">
            <div className="text-xs uppercase tracking-wide text-neutral-500">
              {t("settings.backup", "Database backup")}
            </div>
            <p className="text-xs text-neutral-500">
              {t(
                "settings.backup_blurb",
                "Saves a clean copy of <HD>/sorta.db (genres, links, translations, posters metadata) anywhere you choose. Movie files themselves are not included.",
              )}
            </p>
            <button
              onClick={runBackup}
              disabled={backingUp}
              className="rounded bg-accent px-3 py-2 text-sm text-white hover:bg-accent-hover disabled:opacity-40"
            >
              {backingUp
                ? t("settings.backup_running", "Backing up…")
                : t("settings.backup_now", "Backup database…")}
            </button>
            {backupNote && (
              <div className="break-all rounded bg-neutral-800/60 px-2 py-1 text-xs text-neutral-300">
                ✔ {backupNote}
              </div>
            )}
          </section>

          <section className="space-y-2">
            <div className="text-xs uppercase tracking-wide text-neutral-500">
              {t("settings.genre_translations")}
            </div>
            {movieGenres.length === 0 && (
              <div className="text-sm text-neutral-500">—</div>
            )}
            <ul className="space-y-2">
              {movieGenres.map((g) => (
                <li
                  key={g.id}
                  className="flex items-center gap-2 rounded bg-neutral-800/40 p-2"
                >
                  <span className="w-32 shrink-0 truncate text-sm text-neutral-400">
                    {g.canonical_name}
                  </span>
                  <input
                    value={translations[g.id] ?? ""}
                    onChange={(e) =>
                      setTranslations((prev) => ({
                        ...prev,
                        [g.id]: e.target.value,
                      }))
                    }
                    placeholder={g.canonical_name}
                    className="flex-1 rounded bg-neutral-900 px-2 py-1 text-sm"
                  />
                </li>
              ))}
            </ul>
            {movieGenres.length > 0 && (
              <button
                onClick={saveTranslations}
                disabled={saving}
                className="mt-2 rounded bg-accent px-3 py-2 text-sm text-white hover:bg-accent-hover disabled:opacity-40"
              >
                {t("actions.save")}
              </button>
            )}
          </section>
        </div>
      </div>
    </div>
  );
}
