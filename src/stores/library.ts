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

  loadConfig: () => Promise<void>;
  refresh: () => Promise<void>;
  selectLeft: (sel: LeftSelection) => Promise<void>;
  selectItem: (sel: Selection | null) => void;
  toggleChecked: (key: string) => void;
  clearChecked: () => void;
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

  toggleChecked: (key: string) => {
    const cur = new Set(get().checked);
    if (cur.has(key)) cur.delete(key);
    else cur.add(key);
    set({ checked: cur });
  },
  clearChecked: () => set({ checked: new Set<string>() }),

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
    set({ leftSelection: sel, selection: null, checked: new Set<string>() });
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
