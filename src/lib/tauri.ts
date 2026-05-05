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
  listMoviesByGenres: (genreIds: number[]) =>
    invoke<MediaRow[]>("list_movies_by_genres", { genreIds }),
  getPosterUrl: (mediaId: number) =>
    invoke<string | null>("get_poster_url", { mediaId }),
  listSeries: () => invoke<MediaRow[]>("list_series"),
  listMovieGenres: () => invoke<GenreRow[]>("list_movie_genres"),
  listMovieGenresInUse: () =>
    invoke<GenreRow[]>("list_movie_genres_in_use"),
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
  linkAsSeries: (args: {
    tmdbId: number;
    season: number;
    startEpisode?: number;
    rename?: boolean;
    sources: { folder: string; videoFilename: string }[];
  }) =>
    invoke<{
      media_id: number;
      series_folder: string;
      season_folder: string;
      episodes_moved: number;
    }>("link_as_series", {
      args: {
        tmdb_id: args.tmdbId,
        season: args.season,
        start_episode: args.startEpisode ?? 1,
        rename: args.rename ?? true,
        sources: args.sources.map((s) => ({
          folder: s.folder,
          video_filename: s.videoFilename,
        })),
      },
    }),
  updateSeasonLabel: (label: string) =>
    invoke<void>("update_season_label", { label }),
  unlinkMedia: (mediaId: number, renameBack = true) =>
    invoke<{
      removed_media_id: number;
      new_folder_path: string | null;
      poster_deleted: boolean;
    }>("unlink_media", {
      args: { media_id: mediaId, rename_back: renameBack },
    }),
  ffmpegStatus: () =>
    invoke<{
      ffmpeg_path: string | null;
      ffprobe_path: string | null;
      ffmpeg_version: string | null;
      hwaccels: string[];
    }>("ffmpeg_status"),
  mediaTotalBytes: (mediaId: number) =>
    invoke<number>("media_total_bytes", { mediaId }),
  generateCompressionPreview: (args: {
    mediaId: number;
    crfs: number[];
    codec: Codec | null;
    downscale720p: boolean;
    startSeconds?: number | null;
    durationSeconds?: number | null;
  }) =>
    invoke<{
      source_path: string;
      source_duration_seconds: number;
      start_seconds: number;
      duration_seconds: number;
      original_segment_size_bytes: number;
      original_data_url: string;
      clips: { crf: number; size_bytes: number; ratio: number; data_url: string }[];
      tmp_dir: string;
    }>("generate_compression_preview", {
      args: {
        media_id: args.mediaId,
        crfs: args.crfs,
        codec: args.codec,
        downscale_720p: args.downscale720p,
        start_seconds: args.startSeconds ?? null,
        duration_seconds: args.durationSeconds ?? null,
      },
    }),
  startCompression: (args: {
    mediaId: number;
    codec: Codec;
    crf: number;
    downscale720p: boolean;
    exhaustiveVerify: boolean;
  }) =>
    invoke<{ job_id: string }>("start_compression", {
      args: {
        media_id: args.mediaId,
        codec: args.codec,
        crf: args.crf,
        downscale_720p: args.downscale720p,
        exhaustive_verify: args.exhaustiveVerify,
      },
    }),
  cancelCompression: (jobId: string) =>
    invoke<boolean>("cancel_compression", { jobId }),
  cleanupOriginalsFor: (mediaId: number) =>
    invoke<{ files_removed: number; bytes_freed: number }>(
      "cleanup_originals_for",
      { mediaId },
    ),
  hasOriginalBackups: (mediaId: number) =>
    invoke<boolean>("has_original_backups", { mediaId }),
  discardPreviewDir: (tmpDir: string) =>
    invoke<void>("discard_preview_dir", { tmpDir }),
};

export type Codec =
  | "hevc"
  | "h264"
  | "hevc_nvenc"
  | "hevc_qsv"
  | "hevc_amf";

export type CompressionState =
  | "encoding"
  | "verifying"
  | "swapping"
  | "done"
  | "cancelled"
  | "failed";

export interface CompressionProgress {
  job_id: string;
  current_file_index: number;
  total_files: number;
  current_file_name: string;
  current_file_duration_seconds: number;
  current_file_position_seconds: number;
  current_file_speed: number | null;
  eta_current_file_seconds: number | null;
  eta_total_seconds: number | null;
  state: CompressionState;
  bytes_saved: number;
}

export interface CompressionReport {
  job_id: string;
  state: CompressionState;
  total_original_bytes: number;
  total_compressed_bytes: number;
  outcomes: {
    file: string;
    original_bytes: number;
    compressed_bytes: number | null;
    error: string | null;
    skipped: boolean;
  }[];
}
