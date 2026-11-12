import { describe, expect, it } from "vitest";
import {
  POP_W,
  placePop,
  dropColumnState,
  opensRowDetail,
  reorder,
  settleRange,
  canApply,
  changeCommitsAtOnce,
  inRange,
  settleSelection,
  showsRange,
  snapSpan,
} from "./grid";

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
    expect(dropColumnState([], state())).toEqual({ ...state(), filterRanges: {} });
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

describe("settleRange", () => {
  const span = { min: 1.5, max: 9 };

  it("swaps the handles when one crosses the other", () => {
    expect(settleRange(6, 3, span)).toEqual({ min: 3, max: 6 });
    expect(settleRange(3, 6, span)).toEqual({ min: 3, max: 6 });
  });

  it("is no filter when the whole span is kept", () => {
    expect(settleRange(1.5, 9, span)).toBeNull();
    expect(settleRange(0, 100, span)).toBeNull();
    // crossed handles that together still cover everything
    expect(settleRange(9, 1.5, span)).toBeNull();
  });

  it("is a filter as soon as one side is narrowed, however slightly", () => {
    expect(settleRange(1.51, 9, span)).toEqual({ min: 1.51, max: 9 });
    expect(settleRange(1.5, 8.99, span)).toEqual({ min: 1.5, max: 8.99 });
  });

  it("keeps a single value as a range", () => {
    expect(settleRange(4, 4, span)).toEqual({ min: 4, max: 4 });
  });
});

describe("snapSpan", () => {
  it("rounds the span outward onto the step grid", () => {
    expect(snapSpan({ min: 2.415, max: 5.337 })).toEqual({ min: 2.41, max: 5.34 });
  });

  it("leaves a span that is already on the grid alone, without float residue", () => {
    expect(snapSpan({ min: 2.41, max: 5.33 })).toEqual({ min: 2.41, max: 5.33 });
    expect(snapSpan({ min: 1, max: 9 })).toEqual({ min: 1, max: 9 });
  });

  it("rounds a negative span outward too", () => {
    expect(snapSpan({ min: -5.337, max: -2.415 })).toEqual({ min: -5.34, max: -2.41 });
    expect(snapSpan({ min: -3, max: -1.005 })).toEqual({ min: -3, max: -1 });
  });

  it("still covers the span it was made from", () => {
    for (const span of [{ min: 0.1, max: 0.3 }, { min: 2.675, max: 4.005 }, { min: -1.234, max: 7 }]) {
      const snapped = snapSpan(span);
      expect(snapped.min).toBeLessThanOrEqual(span.min);
      expect(snapped.max).toBeGreaterThanOrEqual(span.max);
    }
  });
});

describe("what a column's funnel draws", () => {
  const span = { min: 1.5, max: 9 };
  const flat = { min: 4, max: 4 };

  // The value list is drawn for every column; only the range block is in question.

  it("draws the range block for a number column that has numbers, on top of the value list", () => {
    expect(showsRange("number", span)).toBe(true);
  });

  it("draws the range block even when the span has no width, whether or not a range is set", () => {
    // the shape depends on the column, not on a range that may or may not be set right now
    expect(showsRange("number", flat)).toBe(true);
  });

  it("draws no range block for a number column with no number in it", () => {
    expect(showsRange("number", null)).toBe(false);
  });

  it("draws no range block for a text column, whatever it is handed", () => {
    expect(showsRange("text", null)).toBe(false);
    expect(showsRange("text", span)).toBe(false);
  });
});

describe("inRange", () => {
  const r = { min: 85, max: 100 };

  it("reads the data's text as a number, boundaries included", () => {
    expect(inRange("85", r)).toBe(true);
    expect(inRange("100", r)).toBe(true);
    expect(inRange("84.99", r)).toBe(false);
    expect(inRange("1,200", { min: 1000, max: 2000 })).toBe(true);
  });

  it("puts the blank mark and non-numbers outside every range", () => {
    expect(inRange("￿", r)).toBe(false);
    expect(inRange("", r)).toBe(false);
    expect(inRange("n/a", r)).toBe(false);
  });
});

describe("settleSelection", () => {
  const r = { min: 85, max: 100 };

  it("drops an included value that the range leaves out, without a list to look at", () => {
    // the value 70 is not on any list here: only the number decides
    expect(settleSelection("include", ["70", "90"], r)).toEqual({ mode: "include", values: ["90"] });
  });

  it("returns to all when nothing is left of an inclusion", () => {
    expect(settleSelection("include", ["70"], r)).toEqual({ mode: "all", values: [] });
  });

  it("leaves an inclusion that was empty from the start alone", () => {
    expect(settleSelection("include", [], r)).toEqual({ mode: "include", values: [] });
  });

  it("drops an exclusion outside the range too, so releasing the range does not revive it", () => {
    expect(settleSelection("exclude", ["70", "￿", "90"], r)).toEqual({
      mode: "exclude",
      values: ["90"],
    });
    expect(settleSelection("exclude", ["70"], r)).toEqual({ mode: "all", values: [] });
  });

  it("changes nothing without a range or with no selection", () => {
    expect(settleSelection("include", ["70"], null)).toEqual({ mode: "include", values: ["70"] });
    expect(settleSelection("all", [], r)).toEqual({ mode: "all", values: [] });
  });
});

describe("canApply", () => {
  it("refuses an inclusion with nothing in it, and refuses while loading", () => {
    expect(canApply(false, "include", 0)).toBe(false);
    expect(canApply(true, "all", 0)).toBe(false);
  });

  it("allows everything else", () => {
    expect(canApply(false, "all", 0)).toBe(true);
    expect(canApply(false, "include", 2)).toBe(true);
    expect(canApply(false, "exclude", 0)).toBe(true);
  });
});

describe("changeCommitsAtOnce", () => {
  it("sends a keyboard change on a slider to the delayed commit", () => {
    expect(changeCommitsAtOnce("slider", false)).toBe(false);
  });

  it("commits at once for a released handle and for a typed field", () => {
    expect(changeCommitsAtOnce("slider", true)).toBe(true);
    expect(changeCommitsAtOnce("input", false)).toBe(true);
  });
});

describe("where the funnel popover goes", () => {
  // 가시 영역. 표는 이 안에서 구른다.
  const view = { left: 0, right: 1000, top: 100, bottom: 700 };
  const btn = (left: number, top: number) => ({ left, right: left + 20, top, bottom: top + 24 });

  it("is measured against the visible area, not the scrolling box", () => {
    // 화면 기준이므로 헤더 단추의 자리가 그대로 근거다. 표가 구르면 단추의
    // 자리가 변동하고, 그때 다시 호출하면 팝오버가 헤더을 따라간다.
    const a = placePop(btn(200, 150), view);
    const b = placePop(btn(200, 300), view);
    expect(a.y).toBeLessThan(b.y);
    expect(b.y).toBe(324 + 4);
  });

  it("sticks at the edge when the header scrolls out of the visible area", () => {
    // 상단으로 이탈한 헤더. 폐쇄하지 않는다 — 고르던 선택이 소멸한다.
    const out = placePop(btn(200, -400), view);
    expect(out.y).toBeGreaterThanOrEqual(view.top);
    expect(out.y + out.h).toBeLessThanOrEqual(view.bottom);
    // 하단으로 이탈한 헤더도 동일하다.
    const low = placePop(btn(200, 2000), view);
    expect(low.y + low.h).toBeLessThanOrEqual(view.bottom);
    expect(low.y).toBeGreaterThanOrEqual(view.top);
  });

  it("stays inside the visible area sideways, however far the grid is scrolled", () => {
    expect(placePop(btn(980, 150), view).x).toBe(1000 - POP_W - 12);
    expect(placePop(btn(-300, 150), view).x).toBe(12);
  });

  it("draws above the header when there is more room above", () => {
    const up = placePop(btn(200, 650), view);
    expect(up.y + up.h).toBeLessThanOrEqual(650);
  });

});
