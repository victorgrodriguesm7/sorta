import { useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useLibrary, checkKey } from "@/stores/library";
import CatalogAsSeriesDialog from "./CatalogAsSeriesDialog";

export default function CenterList() {
  const { t } = useTranslation();
  const {
    leftSelection,
    uncatalogued,
    currentList,
    selection,
    selectItem,
    checked,
    toggleChecked,
    setRangeChecked,
    clearChecked,
    refresh,
  } = useLibrary();
  const [seriesOpen, setSeriesOpen] = useState(false);

  // Anchor for shift-click range select on the uncatalogued list.
  // Holds the key of the last checkbox the user clicked *without*
  // shift held. Stays a ref so updating it doesn't re-render.
  const lastClickedKey = useRef<string | null>(null);

  /** Handle a checkbox click. Shift held + anchor present + anchor
   *  still in the current list → toggle every item in the inclusive
   *  range to the *new* state of the clicked checkbox (Gmail-style).
   *  Otherwise just toggle the single item. */
  const handleCheckboxClick = (
    e: React.MouseEvent<HTMLInputElement>,
    clickedKey: string,
  ) => {
    // Don't preventDefault: we want the native checkbox toggle to
    // proceed so the DOM stays visually in sync if React's re-render
    // hasn't committed yet. Our store update below is what ultimately
    // drives the controlled `checked` prop on the next render.
    e.stopPropagation();

    const keys = uncatalogued.map((u) => checkKey(u.folder, u.video_filename));
    const clickedIdx = keys.indexOf(clickedKey);
    const anchorIdx = lastClickedKey.current
      ? keys.indexOf(lastClickedKey.current)
      : -1;

    if (e.shiftKey && anchorIdx >= 0 && clickedIdx >= 0) {
      const [lo, hi] =
        anchorIdx < clickedIdx ? [anchorIdx, clickedIdx] : [clickedIdx, anchorIdx];
      // The clicked item's new state drives the whole range — if it's
      // currently unchecked the range becomes checked, and vice versa.
      const desired = !checked.has(clickedKey);
      setRangeChecked(keys.slice(lo, hi + 1), desired);
    } else {
      toggleChecked(clickedKey);
    }
    // Anchor moves to wherever the user clicked, with or without
    // shift, so the next shift-click extends from here.
    lastClickedKey.current = clickedKey;
  };

  if (leftSelection.kind === "uncatalogued") {
    const checkedItems = uncatalogued.filter((u) =>
      checked.has(checkKey(u.folder, u.video_filename)),
    );
    return (
      <div className="flex flex-1 flex-col overflow-hidden border-r border-neutral-800 bg-neutral-950">
        {checked.size > 0 && (
          <div className="flex items-center justify-between gap-2 border-b border-neutral-800 bg-neutral-900 px-3 py-2 text-sm">
            <span className="text-neutral-300">
              {t("uncatalogued.selected", "{{count}} selected", {
                count: checked.size,
              })}
            </span>
            <div className="flex gap-2">
              <button
                onClick={clearChecked}
                className="rounded px-2 py-1 text-xs text-neutral-400 hover:text-white"
              >
                {t("actions.clear", "Clear")}
              </button>
              <button
                onClick={() => setSeriesOpen(true)}
                className="rounded bg-accent px-2 py-1 text-xs text-white hover:bg-accent-hover"
              >
                {t("series.catalog_as", "Catalog as series")}
              </button>
            </div>
          </div>
        )}
        <div className="flex-1 overflow-y-auto p-2">
          {uncatalogued.length === 0 && (
            <div className="p-4 text-sm text-neutral-500">
              {t("panel.uncatalogued")} — {t("media.none", "None")}
            </div>
          )}
          <ul>
            {uncatalogued.map((u) => {
              const k = checkKey(u.folder, u.video_filename);
              const isChecked = checked.has(k);
              const active =
                selection?.kind === "uncatalogued" &&
                selection.folder === u.folder &&
                selection.videoFilename === u.video_filename;
              return (
                <li
                  key={k}
                  className={`group flex items-center gap-2 rounded px-2 py-1.5 text-sm ${
                    active
                      ? "bg-accent text-white"
                      : "text-neutral-200 hover:bg-neutral-800"
                  }`}
                >
                  <input
                    type="checkbox"
                    checked={isChecked}
                    // The whole click → state mapping happens in
                    // handleCheckboxClick (shift-range, anchor track).
                    // onChange is required by React's controlled-input
                    // contract; the actual mutation is in onClick.
                    onChange={() => {}}
                    onClick={(e) => handleCheckboxClick(e, k)}
                    className="h-4 w-4 shrink-0 accent-accent"
                    aria-label={t("uncatalogued.toggle", "Toggle selection")}
                  />
                  <button
                    onClick={() =>
                      selectItem({
                        kind: "uncatalogued",
                        folder: u.folder,
                        videoFilename: u.video_filename,
                      })
                    }
                    className="flex-1 truncate text-left"
                    title={`${u.folder}\n${u.video_filename}`}
                  >
                    <div className="truncate">{u.video_filename}</div>
                    <div className="truncate text-[10px] opacity-60">
                      {u.folder}
                    </div>
                  </button>
                  {u.kind === "series" && (
                    <span className="rounded bg-neutral-800 px-1.5 py-0.5 text-[10px] uppercase tracking-wide text-neutral-400 group-hover:bg-neutral-700">
                      {t("uncatalogued.series_hint", "Series")}
                    </span>
                  )}
                </li>
              );
            })}
          </ul>
        </div>
        {seriesOpen && (
          <CatalogAsSeriesDialog
            episodes={checkedItems}
            onClose={() => setSeriesOpen(false)}
            onLinked={async () => {
              setSeriesOpen(false);
              clearChecked();
              await refresh();
            }}
          />
        )}
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
