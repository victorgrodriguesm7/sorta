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
  /** Every drive the user has registered. Empty before initial setup. */
  hd_roots: string[];
  /** Primary / "active" drive — equal to `hd_roots[0]` after the
   *  backend's normalize pass. Kept for code paths that still assume
   *  a single library while the multi-drive refactor lands. */
  hd_root: string | null;
  tmdb_api_key: string | null;
  ui_language: string;
  initialized: boolean;
  compression_codec: string | null;
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
  /** ISO 8601 UTC timestamp set by the DB when the row was inserted. */
  catalogued_at: string;
  /** "Mark as new" flag set at cataloging time. */
  is_new: boolean;
  /** Drive this row was fetched from. Stamped by the backend during
   *  the fan-out read. Needed because `id` is per-pool and would
   *  collide across drives. */
  drive_root: string | null;
}

export interface RecatalogPlanSeason {
  season_number: number;
  season_folder: string;
  video_filenames: string[];
}

export interface RecatalogPlan {
  media_id: number;
  tmdb_id: number;
  title: string;
  poster_path: string | null;
  poster_url: string | null;
  series_folder: string;
  seasons: RecatalogPlanSeason[];
}

export interface RecatalogResult {
  seasons_processed: number;
  episodes_processed: number;
  episodes_renamed: number;
  stills_downloaded: number;
  skipped: string[];
}

export interface EpisodeRow {
  id: number;
  media_id: number;
  season_number: number;
  episode_number: number;
  title: string | null;
  overview: string | null;
  air_date: string | null;
  runtime_minutes: number | null;
  /** Cached still image, relative to the HD root. */
  still_path: string | null;
  /** TMDB CDN fallback URL. */
  still_url: string | null;
  /** Video file, relative to the HD root. */
  file_path: string | null;
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
  /** Drive this item was discovered on. */
  drive_root: string;
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
  removeHdRoot: (path: string) =>
    invoke<ConfigDto>("remove_hd_root", { path }),
  setApiKey: (apiKey: string) =>
    invoke<ConfigDto>("set_api_key", { apiKey }),
  setUiLanguage: (language: string) =>
    invoke<void>("set_ui_language", { language }),
  setCompressionCodec: (codec: Codec) =>
    invoke<void>("set_compression_codec", { codec }),
  backupDatabase: (destination: string) =>
    invoke<{ destination: string; bytes_written: number }>(
      "backup_database",
      { destination },
    ),
  scanNow: () => invoke<ScanResult>("scan_now"),
  listMoviesByGenre: (genreId: number) =>
    invoke<MediaRow[]>("list_movies_by_genre", { genreId }),
  listMoviesByGenres: (genreIds: number[]) =>
    invoke<MediaRow[]>("list_movies_by_genres", { genreIds }),
  getPosterUrl: (mediaId: number, driveRoot?: string | null) =>
    invoke<string | null>("get_poster_url", {
      mediaId,
      driveRoot: driveRoot ?? null,
    }),
  listSeries: () => invoke<MediaRow[]>("list_series"),
  listMovieGenres: () => invoke<GenreRow[]>("list_movie_genres"),
  listMovieGenresInUse: () =>
    invoke<GenreRow[]>("list_movie_genres_in_use"),
  updateGenreTranslation: (genreId: number, translated: string | null) =>
    invoke<void>("update_genre_translation", { genreId, translated }),
  updateRootLabel: (kind: MediaType, label: string) =>
    invoke<void>("update_root_label", { kind, label }),
  openInExplorer: (path: string) =>
    invoke<void>("open_in_explorer", { path }),
  tmdbSearch: (query: string) =>
    invoke<SearchResult[]>("tmdb_search", { query }),
  linkMedia: (args: {
    sourceFolder: string;
    videoFilename: string;
    tmdbId: number;
    mediaType: MediaType;
    isNew?: boolean;
  }) =>
    invoke<{ media_id: number; folder_path: string }>("link_media", {
      args: {
        source_folder: args.sourceFolder,
        video_filename: args.videoFilename,
        tmdb_id: args.tmdbId,
        media_type: args.mediaType,
        is_new: args.isNew ?? false,
      },
    }),
  renameMedia: (mediaId: number, newTitle: string, driveRoot?: string | null) =>
    invoke<MediaRow>("rename_media", {
      args: {
        media_id: mediaId,
        new_title: newTitle,
        drive_root: driveRoot ?? null,
      },
    }),
  listMediaGenres: (mediaId: number, driveRoot?: string | null) =>
    invoke<GenreRow[]>("list_media_genres", {
      mediaId,
      driveRoot: driveRoot ?? null,
    }),
  tmdbSyncGenres: (mediaType: MediaType) =>
    invoke<GenreRow[]>("tmdb_sync_genres", { mediaType }),
  reorderMediaGenres: (
    mediaId: number,
    genreIds: number[],
    driveRoot?: string | null,
  ) =>
    invoke<MediaRow>("reorder_media_genres", {
      args: {
        media_id: mediaId,
        genre_ids: genreIds,
        drive_root: driveRoot ?? null,
      },
    }),
  linkAsSeries: (args: {
    tmdbId: number;
    season: number;
    startEpisode?: number;
    rename?: boolean;
    downloadEpisodePosters?: boolean;
    isNew?: boolean;
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
        download_episode_posters: args.downloadEpisodePosters ?? true,
        is_new: args.isNew ?? false,
        sources: args.sources.map((s) => ({
          folder: s.folder,
          video_filename: s.videoFilename,
        })),
      },
    }),
  listEpisodes: (mediaId: number, driveRoot?: string | null) =>
    invoke<EpisodeRow[]>("list_episodes", {
      mediaId,
      driveRoot: driveRoot ?? null,
    }),
  setMediaIsNew: (mediaId: number, isNew: boolean, driveRoot?: string | null) =>
    invoke<void>("set_media_is_new", {
      mediaId,
      isNew,
      driveRoot: driveRoot ?? null,
    }),
  planRecatalogSeries: (mediaId: number, driveRoot?: string | null) =>
    invoke<RecatalogPlan>("plan_recatalog_series", {
      mediaId,
      driveRoot: driveRoot ?? null,
    }),
  recatalogSeries: (args: {
    mediaId: number;
    rename?: boolean;
    downloadEpisodePosters?: boolean;
    setIsNew?: boolean | null;
    driveRoot?: string | null;
  }) =>
    invoke<RecatalogResult>("recatalog_series", {
      args: {
        media_id: args.mediaId,
        rename: args.rename ?? true,
        download_episode_posters: args.downloadEpisodePosters ?? true,
        set_is_new: args.setIsNew ?? null,
        drive_root: args.driveRoot ?? null,
      },
    }),
  updateSeasonLabel: (label: string) =>
    invoke<void>("update_season_label", { label }),
  unlinkMedia: (
    mediaId: number,
    renameBack = true,
    driveRoot?: string | null,
  ) =>
    invoke<{
      removed_media_id: number;
      new_folder_path: string | null;
      poster_deleted: boolean;
    }>("unlink_media", {
      args: {
        media_id: mediaId,
        rename_back: renameBack,
        drive_root: driveRoot ?? null,
      },
    }),
  ffmpegStatus: () =>
    invoke<{
      ffmpeg_path: string | null;
      ffprobe_path: string | null;
      ffmpeg_version: string | null;
      hwaccels: string[];
    }>("ffmpeg_status"),
  mediaTotalBytes: (mediaId: number, driveRoot?: string | null) =>
    invoke<number>("media_total_bytes", {
      mediaId,
      driveRoot: driveRoot ?? null,
    }),
  generateCompressionPreview: (args: {
    mediaId: number;
    crfs: number[];
    codec: Codec | null;
    downscale720p: boolean;
    startSeconds?: number | null;
    durationSeconds?: number | null;
    driveRoot?: string | null;
  }) =>
    invoke<{
      source_path: string;
      source_duration_seconds: number;
      start_seconds: number;
      duration_seconds: number;
      original_segment_size_bytes: number;
      total_media_bytes: number;
      original_data_url: string;
      clips: {
        crf: number;
        size_bytes: number;
        ratio: number;
        data_url: string;
        estimated_final_bytes: number;
      }[];
      tmp_dir: string;
    }>("generate_compression_preview", {
      args: {
        media_id: args.mediaId,
        crfs: args.crfs,
        codec: args.codec,
        downscale_720p: args.downscale720p,
        start_seconds: args.startSeconds ?? null,
        duration_seconds: args.durationSeconds ?? null,
        drive_root: args.driveRoot ?? null,
      },
    }),
  startCompression: (args: {
    mediaId: number;
    codec: Codec;
    crf: number;
    downscale720p: boolean;
    exhaustiveVerify: boolean;
    driveRoot?: string | null;
  }) =>
    invoke<{ job_id: string }>("start_compression", {
      args: {
        media_id: args.mediaId,
        codec: args.codec,
        crf: args.crf,
        downscale_720p: args.downscale720p,
        exhaustive_verify: args.exhaustiveVerify,
        drive_root: args.driveRoot ?? null,
      },
    }),
  cancelCompression: (jobId: string) =>
    invoke<boolean>("cancel_compression", { jobId }),
  cleanupOriginalsFor: (mediaId: number, driveRoot?: string | null) =>
    invoke<{ files_removed: number; bytes_freed: number }>(
      "cleanup_originals_for",
      { mediaId, driveRoot: driveRoot ?? null },
    ),
  hasOriginalBackups: (mediaId: number, driveRoot?: string | null) =>
    invoke<boolean>("has_original_backups", {
      mediaId,
      driveRoot: driveRoot ?? null,
    }),
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
