import { create } from "zustand";
import {
  api,
  type ConfigDto,
  type GenreRow,
  type MediaRow,
  type UncataloguedItem,
} from "@/lib/tauri";

export type Selection =
  | { kind: "uncatalogued"; folder: string; videoFilename: string }
  | { kind: "media"; row: MediaRow };

export type LeftSelection =
  | { kind: "uncatalogued" }
  | { kind: "movieGenre"; group: GenreRow[] }
  | { kind: "series" };

interface LibraryState {
  config: ConfigDto | null;
  uncatalogued: UncataloguedItem[];
  /** Every movie genre known locally — used by SettingsModal so the
   *  user can translate genres they may not yet have movies for. */
  movieGenres: GenreRow[];
  /** Subset of movieGenres that is the primary genre of at least one
   *  linked movie — used by LeftPanel to avoid empty buckets. */
  movieGenresInUse: GenreRow[];
  series: MediaRow[];
  currentList: MediaRow[];
  leftSelection: LeftSelection;
  selection: Selection | null;
  /** Set of "folder|video_filename" keys for multi-select on uncatalogued. */
  checked: Set<string>;
  loading: boolean;
  error: string | null;
  /** Compression dialog (hoisted out of RightPanel so the watcher
   *  refresh can't unmount it mid-encode). */
  compression: { media: MediaRow; totalBytes: number } | null;
  /** Bumped after a compression job completes; the RightPanel uses
   *  it to know when to re-fetch totalBytes / hasOriginalBackups. */
  compressionDoneTick: number;

  loadConfig: () => Promise<void>;
  refresh: () => Promise<void>;
  selectLeft: (sel: LeftSelection) => Promise<void>;
  selectItem: (sel: Selection | null) => void;
  toggleChecked: (key: string) => void;
  clearChecked: () => void;
  openCompression: (media: MediaRow, totalBytes: number) => void;
  closeCompression: (didFinish?: boolean) => void;
}

function sameLeftSelection(a: LeftSelection, b: LeftSelection): boolean {
  if (a.kind !== b.kind) return false;
  if (a.kind === "movieGenre" && b.kind === "movieGenre") {
    if (a.group.length !== b.group.length) return false;
    const aIds = new Set(a.group.map((g) => g.id));
    return b.group.every((g) => aIds.has(g.id));
  }
  return true;
}

export const checkKey = (folder: string, file: string) => `${folder}|${file}`;

export const useLibrary = create<LibraryState>((set, get) => ({
  config: null,
  uncatalogued: [],
  movieGenres: [],
  movieGenresInUse: [],
  series: [],
  currentList: [],
  leftSelection: { kind: "uncatalogued" },
  selection: null,
  checked: new Set<string>(),
  loading: false,
  error: null,
  compression: null,
  compressionDoneTick: 0,

  toggleChecked: (key: string) => {
    const cur = new Set(get().checked);
    if (cur.has(key)) cur.delete(key);
    else cur.add(key);
    set({ checked: cur });
  },
  clearChecked: () => set({ checked: new Set<string>() }),
  openCompression: (media, totalBytes) =>
    set({ compression: { media, totalBytes } }),
  closeCompression: (didFinish?: boolean) =>
    set((s) => ({
      compression: null,
      compressionDoneTick: didFinish
        ? s.compressionDoneTick + 1
        : s.compressionDoneTick,
    })),

  async loadConfig() {
    try {
      const cfg = await api.getConfig();
      set({ config: cfg });
    } catch (e) {
      set({ error: (e as Error).message });
    }
  },

  async refresh() {
    if (!get().config?.initialized) return;
    set({ loading: true, error: null });
    try {
      const [scan, movieGenres, movieGenresInUse, series] = await Promise.all([
        api.scanNow(),
        api.listMovieGenres(),
        api.listMovieGenresInUse(),
        api.listSeries(),
      ]);
      set({
        uncatalogued: scan.uncatalogued,
        movieGenres,
        movieGenresInUse,
        series,
      });
      // Re-fetch the current list view.
      await get().selectLeft(get().leftSelection);
    } catch (e) {
      set({ error: (e as Error).message });
    } finally {
      set({ loading: false });
    }
  },

  async selectLeft(sel) {
    // Preserve the current right-panel selection (and the multi-select
    // checkboxes) when this is a no-op nav — e.g. the watcher fires
    // a refresh and we re-call selectLeft with the SAME bucket. Without
    // this guard, every background refresh would clear whatever the
    // user was looking at.
    const prev = get().leftSelection;
    const same = sameLeftSelection(prev, sel);
    set({
      leftSelection: sel,
      selection: same ? get().selection : null,
      checked: same ? get().checked : new Set<string>(),
    });
    try {
      if (sel.kind === "movieGenre") {
        const list = await api.listMoviesByGenres(sel.group.map((g) => g.id));
        set({ currentList: list });
      } else if (sel.kind === "series") {
        set({ currentList: get().series });
      } else {
        set({ currentList: [] });
      }
    } catch (e) {
      set({ error: (e as Error).message });
    }
  },

  selectItem(sel) {
    set({ selection: sel });
  },
}));
