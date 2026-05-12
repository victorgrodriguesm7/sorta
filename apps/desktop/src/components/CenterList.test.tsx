import { describe, it, expect, beforeEach, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import CenterList from "./CenterList";
import { useLibrary, checkKey } from "@/stores/library";
import "@/i18n";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(async () => null),
}));

const items = [
  { folder: "/x", video_filename: "a.mkv", kind: "movie" as const },
  { folder: "/x", video_filename: "b.mkv", kind: "movie" as const },
  { folder: "/x", video_filename: "c.mkv", kind: "movie" as const },
  { folder: "/x", video_filename: "d.mkv", kind: "movie" as const },
];

beforeEach(() => {
  useLibrary.setState({
    config: {
      hd_roots: ["D:/Movies"],
      hd_root: "D:/Movies",
      tmdb_api_key: "k",
      ui_language: "en-US",
      initialized: true,
      compression_codec: null,
    },
    uncatalogued: items,
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
  });
});

/** Read the current checked keys out of the store — order-independent. */
const checkedKeys = () => Array.from(useLibrary.getState().checked).sort();

/** Find a checkbox by the filename of the item it belongs to. */
const findCheckbox = (filename: string): HTMLInputElement => {
  // Each row has a button labelled with the filename next to its
  // checkbox. The row's <li> is their common parent.
  const button = screen.getByTitle(new RegExp(filename));
  const li = button.closest("li");
  if (!li) throw new Error(`no <li> for ${filename}`);
  const cb = li.querySelector('input[type="checkbox"]');
  if (!cb) throw new Error(`no checkbox for ${filename}`);
  return cb as HTMLInputElement;
};

describe("CenterList — uncatalogued checkbox selection", () => {
  it("plain click toggles a single item and updates the store", async () => {
    const user = userEvent.setup();
    render(<CenterList />);

    await user.click(findCheckbox("a.mkv"));
    expect(checkedKeys()).toEqual([checkKey("/x", "a.mkv")]);

    // The visible input must reflect the store on the next render.
    expect(findCheckbox("a.mkv").checked).toBe(true);
  });

  it("a second plain click on the same item un-checks it", async () => {
    const user = userEvent.setup();
    render(<CenterList />);

    await user.click(findCheckbox("a.mkv"));
    await user.click(findCheckbox("a.mkv"));
    expect(checkedKeys()).toEqual([]);
    expect(findCheckbox("a.mkv").checked).toBe(false);
  });

  it("plain clicks on different items accumulate independently", async () => {
    const user = userEvent.setup();
    render(<CenterList />);

    await user.click(findCheckbox("a.mkv"));
    await user.click(findCheckbox("c.mkv"));
    expect(checkedKeys()).toEqual(
      [checkKey("/x", "a.mkv"), checkKey("/x", "c.mkv")].sort(),
    );
  });

  it("shift-click extends the selection from the last anchor", async () => {
    const user = userEvent.setup();
    render(<CenterList />);

    // Set anchor at a.mkv with a plain click, then shift-click c.mkv.
    // Expected: a, b, c all checked.
    await user.click(findCheckbox("a.mkv"));
    await user.keyboard("{Shift>}");
    await user.click(findCheckbox("c.mkv"));
    await user.keyboard("{/Shift}");

    expect(checkedKeys()).toEqual(
      [
        checkKey("/x", "a.mkv"),
        checkKey("/x", "b.mkv"),
        checkKey("/x", "c.mkv"),
      ].sort(),
    );
  });

  it("shift-click works in the reverse direction", async () => {
    const user = userEvent.setup();
    render(<CenterList />);

    // Anchor at d.mkv, shift-click b.mkv. Range: b, c, d.
    await user.click(findCheckbox("d.mkv"));
    await user.keyboard("{Shift>}");
    await user.click(findCheckbox("b.mkv"));
    await user.keyboard("{/Shift}");

    expect(checkedKeys()).toEqual(
      [
        checkKey("/x", "b.mkv"),
        checkKey("/x", "c.mkv"),
        checkKey("/x", "d.mkv"),
      ].sort(),
    );
  });

  it("shift-click on a checked item un-checks the whole range", async () => {
    const user = userEvent.setup();
    render(<CenterList />);

    // Build up a 4-wide selection so the shift-click has something
    // to clear.
    await user.click(findCheckbox("a.mkv"));
    await user.keyboard("{Shift>}");
    await user.click(findCheckbox("d.mkv"));
    await user.keyboard("{/Shift}");
    expect(checkedKeys()).toHaveLength(4);

    // Now plain-click b.mkv to move the anchor there (b.mkv toggles
    // off in the process), then shift-click d.mkv. The clicked item
    // (d.mkv) is currently checked, so its new state is unchecked —
    // and the whole range becomes unchecked.
    await user.click(findCheckbox("b.mkv"));
    await user.keyboard("{Shift>}");
    await user.click(findCheckbox("d.mkv"));
    await user.keyboard("{/Shift}");

    // After the sequence: a stays checked (untouched), b/c/d are
    // unchecked by the range op (and b also by the plain click
    // before it).
    expect(checkedKeys()).toEqual([checkKey("/x", "a.mkv")]);
  });

  it("shift-click without a prior anchor falls back to a single toggle", async () => {
    const user = userEvent.setup();
    render(<CenterList />);

    // First interaction is shift-click — no previous anchor exists.
    // Should behave like a plain click.
    await user.keyboard("{Shift>}");
    await user.click(findCheckbox("b.mkv"));
    await user.keyboard("{/Shift}");

    expect(checkedKeys()).toEqual([checkKey("/x", "b.mkv")]);
  });

  it("plain click moves the anchor for a subsequent shift-click", async () => {
    const user = userEvent.setup();
    render(<CenterList />);

    // Plain-click b → anchor=b, {b}.
    // Plain-click d → anchor=d, {b, d}.
    // Shift-click a → range a..d, clicked item is `a` (unchecked) so
    // desired = true, the whole range turns on.
    await user.click(findCheckbox("b.mkv"));
    await user.click(findCheckbox("d.mkv"));
    expect(checkedKeys()).toEqual(
      [checkKey("/x", "b.mkv"), checkKey("/x", "d.mkv")].sort(),
    );

    await user.keyboard("{Shift>}");
    await user.click(findCheckbox("a.mkv"));
    await user.keyboard("{/Shift}");

    expect(checkedKeys()).toEqual(
      [
        checkKey("/x", "a.mkv"),
        checkKey("/x", "b.mkv"),
        checkKey("/x", "c.mkv"),
        checkKey("/x", "d.mkv"),
      ].sort(),
    );
  });

  it("the Clear action wipes the selection but the next click still works", async () => {
    const user = userEvent.setup();
    render(<CenterList />);

    await user.click(findCheckbox("a.mkv"));
    await user.click(findCheckbox("b.mkv"));
    expect(checkedKeys()).toHaveLength(2);

    await user.click(screen.getByRole("button", { name: /clear/i }));
    expect(checkedKeys()).toEqual([]);

    await user.click(findCheckbox("c.mkv"));
    expect(checkedKeys()).toEqual([checkKey("/x", "c.mkv")]);
  });
});
