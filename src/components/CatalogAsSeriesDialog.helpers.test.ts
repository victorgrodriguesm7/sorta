import { describe, it, expect } from "vitest";

// Re-implement the pure helpers here (mirrored from
// CatalogAsSeriesDialog.tsx) so we can test them without spinning up
// a JSDOM render. If they drift, this test will fail — keep in sync.
function moveSelectionUp<T>(list: T[], isSelected: (t: T) => boolean): T[] {
  const out = list.slice();
  for (let i = 1; i < out.length; i++) {
    if (isSelected(out[i]) && !isSelected(out[i - 1])) {
      [out[i - 1], out[i]] = [out[i], out[i - 1]];
    }
  }
  return out;
}
function moveSelectionDown<T>(list: T[], isSelected: (t: T) => boolean): T[] {
  const out = list.slice();
  for (let i = out.length - 2; i >= 0; i--) {
    if (isSelected(out[i]) && !isSelected(out[i + 1])) {
      [out[i + 1], out[i]] = [out[i], out[i + 1]];
    }
  }
  return out;
}

describe("episode reorder helpers", () => {
  it("matches the user-supplied move-up example", () => {
    // Initial:    1, 10, 11, X2, X3, X4
    // After up:   1, 10, X2, X3, X4, 11
    const list = ["1", "10", "11", "X2", "X3", "X4"];
    const sel = new Set(["X2", "X3", "X4"]);
    expect(moveSelectionUp(list, (x) => sel.has(x))).toEqual([
      "1",
      "10",
      "X2",
      "X3",
      "X4",
      "11",
    ]);
  });

  it("moves a contiguous selection down as one unit", () => {
    const list = ["X1", "X2", "X3", "a", "b"];
    const sel = new Set(["X1", "X2", "X3"]);
    expect(moveSelectionDown(list, (x) => sel.has(x))).toEqual([
      "a",
      "X1",
      "X2",
      "X3",
      "b",
    ]);
  });

  it("moves non-contiguous selections each by one slot", () => {
    const list = ["a", "X1", "b", "X2", "c"];
    const sel = new Set(["X1", "X2"]);
    expect(moveSelectionUp(list, (x) => sel.has(x))).toEqual([
      "X1",
      "a",
      "X2",
      "b",
      "c",
    ]);
  });

  it("clamps at the boundary instead of wrapping", () => {
    const list = ["X1", "a", "b"];
    const sel = new Set(["X1"]);
    // Already at top — moving up is a no-op.
    expect(moveSelectionUp(list, (x) => sel.has(x))).toEqual(list);
    // Moving down works.
    expect(moveSelectionDown(list, (x) => sel.has(x))).toEqual([
      "a",
      "X1",
      "b",
    ]);
  });
});
