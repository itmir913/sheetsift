import { describe, expect, it } from "vitest";
import { dropColumnState, opensRowDetail, reorder } from "./grid";

describe("reorder", () => {
  it("moves forward and backward", () => {
    expect(reorder(["a", "b", "c", "d"], 0, 2)).toEqual(["b", "c", "a", "d"]);
    expect(reorder(["a", "b", "c", "d"], 3, 1)).toEqual(["a", "d", "b", "c"]);
  });

  it("returns an equal copy when out of range or in place", () => {
    const src = ["a", "b", "c"];
    for (const [f, t] of [
      [1, 1],
      [-1, 0],
      [0, -1],
      [3, 0],
      [0, 3],
    ] as const) {
      const out = reorder(src, f, t);
      expect(out).toEqual(src);
      expect(out).not.toBe(src);
    }
  });
});

describe("dropColumnState", () => {
  const state = () => ({
    sorts: [
      { key: "a", dir: "asc" as const },
      { key: "b", dir: "desc" as const },
    ],
    filterText: { a: "x", b: "y" },
    filterValues: {
      a: { mode: "include" as const, values: ["1"] },
      b: null,
    },
  });

  it("drops only the sorts and filters of dropped columns", () => {
    const out = dropColumnState(["a"], state());
    expect(out.sorts).toEqual([{ key: "b", dir: "desc" }]);
    expect(out.filterText).toEqual({ b: "y" });
    expect(out.filterValues).toEqual({ b: null });
  });

  it("does not mutate its input", () => {
    const s = state();
    dropColumnState(["a", "b"], s);
    expect(s).toEqual(state());
  });

  it("is the identity when nothing is dropped", () => {
    expect(dropColumnState([], state())).toEqual(state());
  });
});

describe("opensRowDetail", () => {
  /** DOM 없이 시험한다 — 이 함수가 보는 것은 `closest` 의 답뿐이다. */
  const target = (interactive: boolean): Element =>
    ({ closest: () => (interactive ? ({} as Element) : null) }) as unknown as Element;

  it("opens on a plain cell", () => {
    expect(opensRowDetail(target(false), "", false)).toBe(true);
  });

  it("does not open on an interactive element", () => {
    expect(opensRowDetail(target(true), "", false)).toBe(false);
  });

  it("does not open right after selecting text", () => {
    expect(opensRowDetail(target(false), "text", false)).toBe(false);
    expect(opensRowDetail(target(false), "   ", false)).toBe(true);
  });

  it("does not open when the press closed the value popover", () => {
    expect(opensRowDetail(target(false), "", true)).toBe(false);
  });

  it("opens when the press had no element behind it", () => {
    expect(opensRowDetail(null, "", false)).toBe(true);
  });
});
