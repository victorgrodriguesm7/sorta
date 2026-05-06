import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen } from "@testing-library/react";
import App from "./App";
import "./i18n";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(async (cmd: string) => {
    if (cmd === "get_config") {
      return {
        hd_root: null,
        tmdb_api_key: null,
        ui_language: "en-US",
        initialized: false,
        compression_codec: null,
      };
    }
    return null;
  }),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));

beforeEach(() => {
  vi.clearAllMocks();
});

describe("App", () => {
  it("renders the localized title in the header", () => {
    render(<App />);
    expect(
      screen.getByRole("heading", { name: /sorta/i }),
    ).toBeInTheDocument();
  });

  it("shows first-run prompt when not initialized", async () => {
    render(<App />);
    expect(
      await screen.findByText(/pick the hard drive/i),
    ).toBeInTheDocument();
  });
});
