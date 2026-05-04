import { invoke as rawInvoke } from "@tauri-apps/api/core";

/**
 * Wrapper around `invoke` that re-types the response and converts the
 * structured `AppError` payload into a JS Error with `.kind` + `.message`.
 */
export async function invoke<T>(
  cmd: string,
  args?: Record<string, unknown>,
): Promise<T> {
  try {
    return (await rawInvoke<T>(cmd, args)) as T;
  } catch (err) {
    if (
      err &&
      typeof err === "object" &&
      "kind" in err &&
      "message" in err
    ) {
      const e = err as { kind: string; message: string };
      const wrapped = new Error(e.message);
      (wrapped as Error & { kind?: string }).kind = e.kind;
      throw wrapped;
    }
    throw err;
  }
}

export type MediaType = "movie" | "tv";

export interface ConfigDto {
  hd_root: string | null;
  tmdb_api_key: string | null;
  ui_language: string;
  initialized: boolean;
}

export interface MediaRow {
  id: number;
  tmdb_id: number;
  media_type: string;
  title: string;
  original_title: string | null;
  runtime_minutes: number | null;
  poster_path: string | null;
  poster_url: string | null;
  folder_path: string;
}

export interface GenreRow {
  id: number;
  media_type: string;
  canonical_name: string;
  translated_name: string | null;
}

export type UncataloguedKind = "movie" | "series";

export interface UncataloguedItem {
  folder: string;
  video_filename: string;
  kind: UncataloguedKind;
}

export interface ScanResult {
  uncatalogued: UncataloguedItem[];
  catalogued_count: number;
  skipped_count: number;
}

export interface SearchResult {
  media_type: MediaType;
  id: number;
  title: string;
  original_title: string | null;
  year: string | null;
  poster_path: string | null;
  genre_ids: number[];
}

export const api = {
  getConfig: () => invoke<ConfigDto>("get_config"),
  setHdRoot: (path: string) => invoke<ConfigDto>("set_hd_root", { path }),
  setApiKey: (apiKey: string) =>
    invoke<ConfigDto>("set_api_key", { apiKey }),
  setUiLanguage: (language: string) =>
    invoke<void>("set_ui_language", { language }),
  scanNow: () => invoke<ScanResult>("scan_now"),
  listMoviesByGenre: (genreId: number) =>
    invoke<MediaRow[]>("list_movies_by_genre", { genreId }),
  listSeries: () => invoke<MediaRow[]>("list_series"),
  listMovieGenres: () => invoke<GenreRow[]>("list_movie_genres"),
  updateGenreTranslation: (genreId: number, translated: string | null) =>
    invoke<void>("update_genre_translation", { genreId, translated }),
  updateRootLabel: (kind: MediaType, label: string) =>
    invoke<void>("update_root_label", { kind, label }),
  tmdbSearch: (query: string) =>
    invoke<SearchResult[]>("tmdb_search", { query }),
  linkMedia: (args: {
    sourceFolder: string;
    videoFilename: string;
    tmdbId: number;
    mediaType: MediaType;
  }) =>
    invoke<{ media_id: number; folder_path: string }>("link_media", {
      args: {
        source_folder: args.sourceFolder,
        video_filename: args.videoFilename,
        tmdb_id: args.tmdbId,
        media_type: args.mediaType,
      },
    }),
  renameMedia: (mediaId: number, newTitle: string) =>
    invoke<MediaRow>("rename_media", {
      args: { media_id: mediaId, new_title: newTitle },
    }),
  listMediaGenres: (mediaId: number) =>
    invoke<GenreRow[]>("list_media_genres", { mediaId }),
  tmdbSyncGenres: (mediaType: MediaType) =>
    invoke<GenreRow[]>("tmdb_sync_genres", { mediaType }),
  reorderMediaGenres: (mediaId: number, genreIds: number[]) =>
    invoke<MediaRow>("reorder_media_genres", {
      args: { media_id: mediaId, genre_ids: genreIds },
    }),
};
