import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import LeftPanel from "@/components/LeftPanel";
import CenterList from "@/components/CenterList";
import RightPanel from "@/components/RightPanel";
import SettingsModal from "@/components/SettingsModal";
import { useLibrary } from "@/stores/library";

export default function App() {
  const { t } = useTranslation();
  const [settingsOpen, setSettingsOpen] = useState(false);
  const { config, loadConfig, refresh } = useLibrary();

  useEffect(() => {
    void (async () => {
      await loadConfig();
      await refresh();
    })();
  }, [loadConfig, refresh]);

  // Re-scan whenever the backend signals a library change.
  useEffect(() => {
    const unlisten = listen("library-changed", () => {
      void refresh();
    });
    return () => {
      void unlisten.then((fn) => fn());
    };
  }, [refresh]);

  const initialized = !!config?.initialized;

  return (
    <div className="flex h-full flex-col bg-neutral-950 text-neutral-100">
      <header className="flex h-12 shrink-0 items-center justify-between border-b border-neutral-800 px-4">
        <h1 className="text-base font-semibold tracking-wide">
          {t("app.title")}
        </h1>
        <button
          onClick={() => setSettingsOpen(true)}
          className="rounded p-2 text-neutral-400 hover:bg-neutral-800 hover:text-white"
          aria-label={t("settings.title")}
        >
          ⚙
        </button>
      </header>

      {!initialized ? (
        <div className="flex flex-1 flex-col items-center justify-center gap-3 p-6 text-center">
          <p className="text-neutral-300">
            {t(
              "first_run.intro",
              "Pick the hard drive that contains your movies and series.",
            )}
          </p>
          <button
            className="rounded bg-accent px-4 py-2 text-sm font-medium text-white hover:bg-accent-hover"
            onClick={() => setSettingsOpen(true)}
          >
            {t("settings.title")}
          </button>
        </div>
      ) : (
        <main className="flex flex-1 overflow-hidden">
          <LeftPanel />
          <CenterList />
          <RightPanel />
        </main>
      )}

      {settingsOpen && <SettingsModal onClose={() => setSettingsOpen(false)} />}
    </div>
  );
}
