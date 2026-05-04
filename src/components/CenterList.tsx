import { useTranslation } from "react-i18next";
import { useLibrary } from "@/stores/library";

export default function CenterList() {
  const { t } = useTranslation();
  const {
    leftSelection,
    uncatalogued,
    currentList,
    selection,
    selectItem,
  } = useLibrary();

  if (leftSelection.kind === "uncatalogued") {
    return (
      <div className="flex-1 overflow-y-auto border-r border-neutral-800 bg-neutral-950 p-2">
        {uncatalogued.length === 0 && (
          <div className="p-4 text-sm text-neutral-500">
            {t("panel.uncatalogued")} — {t("media.none", "None")}
          </div>
        )}
        <ul>
          {uncatalogued.map((u) => {
            const active =
              selection?.kind === "uncatalogued" && selection.folder === u.folder;
            return (
              <li key={u.folder}>
                <button
                  onClick={() =>
                    selectItem({
                      kind: "uncatalogued",
                      folder: u.folder,
                      videoFilename: u.video_filename,
                    })
                  }
                  className={`block w-full truncate rounded px-3 py-2 text-left text-sm ${
                    active
                      ? "bg-accent text-white"
                      : "text-neutral-200 hover:bg-neutral-800"
                  }`}
                  title={u.folder}
                >
                  {u.video_filename}
                </button>
              </li>
            );
          })}
        </ul>
      </div>
    );
  }

  return (
    <div className="flex-1 overflow-y-auto border-r border-neutral-800 bg-neutral-950 p-2">
      {currentList.length === 0 && (
        <div className="p-4 text-sm text-neutral-500">
          {t("media.empty", "No items in this category yet.")}
        </div>
      )}
      <ul>
        {currentList.map((m) => {
          const active =
            selection?.kind === "media" && selection.row.id === m.id;
          return (
            <li key={m.id}>
              <button
                onClick={() => selectItem({ kind: "media", row: m })}
                className={`block w-full truncate rounded px-3 py-2 text-left text-sm ${
                  active
                    ? "bg-accent text-white"
                    : "text-neutral-200 hover:bg-neutral-800"
                }`}
                title={m.folder_path}
              >
                {m.title}
              </button>
            </li>
          );
        })}
      </ul>
    </div>
  );
}
