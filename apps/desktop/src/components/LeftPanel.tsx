import { useTranslation } from "react-i18next";
import { useLibrary, type LeftSelection } from "@/stores/library";
import type { GenreRow } from "@/lib/tauri";

function isSameSelection(a: LeftSelection, b: LeftSelection): boolean {
  if (a.kind !== b.kind) return false;
  if (a.kind === "movieGenre" && b.kind === "movieGenre") {
    if (a.group.length !== b.group.length) return false;
    const aIds = new Set(a.group.map((g) => g.id));
    return b.group.every((g) => aIds.has(g.id));
  }
  return true;
}

export default function LeftPanel() {
  const { t } = useTranslation();
  const { uncatalogued, movieGenresInUse, leftSelection, selectLeft } =
    useLibrary();

  // Only show genres that are the primary for at least one linked
  // movie (the backend pre-filters), then visually merge ones that
  // share the same display name.
  const merged: { displayName: string; genres: GenreRow[] }[] = [];
  for (const g of movieGenresInUse) {
    const display = g.translated_name ?? g.canonical_name;
    const existing = merged.find(
      (m) => m.displayName.toLowerCase() === display.toLowerCase(),
    );
    if (existing) existing.genres.push(g);
    else merged.push({ displayName: display, genres: [g] });
  }

  const item = (
    sel: LeftSelection,
    label: string,
    badge?: string | number,
  ) => {
    const active = isSameSelection(leftSelection, sel);
    return (
      <button
        key={`${sel.kind}-${
          sel.kind === "movieGenre"
            ? sel.group.map((g) => g.id).join(",")
            : sel.kind
        }`}
        onClick={() => selectLeft(sel)}
        className={`flex w-full items-center justify-between rounded px-3 py-1.5 text-left text-sm transition ${
          active
            ? "bg-accent text-white"
            : "text-neutral-300 hover:bg-neutral-800"
        }`}
      >
        <span className="truncate">{label}</span>
        {badge !== undefined && (
          <span className="ml-2 shrink-0 text-xs opacity-75">{badge}</span>
        )}
      </button>
    );
  };

  return (
    <aside className="flex h-full w-64 shrink-0 flex-col gap-4 overflow-y-auto border-r border-neutral-800 bg-neutral-950 p-3">
      <div>
        {item(
          { kind: "uncatalogued" },
          t("panel.uncatalogued"),
          uncatalogued.length || undefined,
        )}
      </div>

      <div className="space-y-1">
        <div className="px-3 text-xs font-semibold uppercase tracking-wide text-neutral-500">
          {t("panel.movies")}
        </div>
        {merged.length === 0 && (
          <div className="px-3 py-2 text-sm text-neutral-500">—</div>
        )}
        {merged.map((m) =>
          item(
            { kind: "movieGenre", group: m.genres },
            m.displayName,
          ),
        )}
      </div>

      <div className="space-y-1">
        <div className="px-3 text-xs font-semibold uppercase tracking-wide text-neutral-500">
          {t("panel.series")}
        </div>
        {item({ kind: "series" }, t("panel.series"))}
      </div>
    </aside>
  );
}
