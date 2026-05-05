import { describe, it, expect, beforeEach, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import LeftPanel from "./LeftPanel";
import { useLibrary } from "@/stores/library";
import "@/i18n";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(async () => null),
}));

beforeEach(() => {
  useLibrary.setState({
    config: {
      hd_root: "D:/Movies",
      tmdb_api_key: "k",
      ui_language: "en-US",
      initialized: true,
      compression_codec: null,
    },
    uncatalogued: [
      { folder: "/x", video_filename: "a.mkv", kind: "movie" },
      { folder: "/y", video_filename: "b.mkv", kind: "movie" },
    ],
    movieGenres: [],
    movieGenresInUse: [
      {
        id: 28,
        media_type: "movie",
        canonical_name: "Action",
        translated_name: "Aventura",
      },
      {
        id: 12,
        media_type: "movie",
        canonical_name: "Adventure",
        translated_name: "Aventura",
      },
      {
        id: 35,
        media_type: "movie",
        canonical_name: "Comedy",
        translated_name: null,
      },
    ],
    series: [],
    currentList: [],
    leftSelection: { kind: "uncatalogued" },
    selection: null,
    loading: false,
    error: null,
    compression: null,
    compressionDoneTick: 0,
  });
});

describe("LeftPanel", () => {
  it("shows the uncatalogued count badge", () => {
    render(<LeftPanel />);
    expect(screen.getByText("Uncatalogued")).toBeInTheDocument();
    expect(screen.getByText("2")).toBeInTheDocument();
  });

  it("merges genres with the same translated name", () => {
    render(<LeftPanel />);
    // 'Aventura' should appear once even though two genres translate to it.
    expect(screen.getAllByText("Aventura")).toHaveLength(1);
    expect(screen.getByText("Comedy")).toBeInTheDocument();
  });
});
