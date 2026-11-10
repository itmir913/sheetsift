import { beforeEach, describe, expect, it, vi } from "vitest";
import { ref } from "vue";
import { provideMessages, provideStoragePrefix } from "../lib/inject";
import { normalizeLayout, useColumnLayout } from "./useColumnLayout";
import type { ScreenDef } from "../lib/grid";

/* 폭 측정은 DOM 을 요구한다. 이 시험은 폭이 아니라 열의 구성을 대상으로 한다. */
vi.mock("../lib/colWidth", async () => {
  const { ref } = await import("vue");
  return { fontEpoch: ref(0), headerWidthRem: () => 8, autoWidthRem: () => 8 };
});

/* 시험에 쓰는 값이다. 저장소는 메모리 흉내이고 접두사·문구는 시험 전용이다. */
const store = new Map<string, string>();
Object.assign(globalThis, {
  localStorage: {
    getItem: (k: string) => store.get(k) ?? null,
    setItem: (k: string, v: string) => void store.set(k, v),
    removeItem: (k: string) => void store.delete(k),
  },
});
provideStoragePrefix("test");
provideMessages({
  t: (k) => k,
  tn: (k) => k,
  formatNumber: String,
  formatDateTime: String,
});

const KEYS = ["a", "b", "c", "d"];

function screen(): ScreenDef {
  return {
    id: "s",
    columns: KEYS.map((key) => ({
      key,
      label: key.toUpperCase(),
      kind: "text" as const,
      sortable: true,
      filterable: true,
    })),
  } as unknown as ScreenDef;
}

function layout() {
  return useColumnLayout(ref(screen()));
}

const shown = (l: ReturnType<typeof layout>) => l.columns.value.map((c) => c.key);

beforeEach(() => store.clear());

describe("normalizeLayout", () => {
  it("clears hidden when it would cover everything", () => {
    expect(normalizeLayout(KEYS, KEYS, KEYS).hidden).toEqual([]);
  });

  it("drops unknown keys and appends new columns", () => {
    const n = normalizeLayout(KEYS, ["c", "zz", "a"], ["zz", "b"]);
    expect(n.order).toEqual(["c", "a", "b", "d"]);
    expect(n.hidden).toEqual(["b"]);
  });

  it("falls back to defaults on damaged input", () => {
    expect(normalizeLayout(KEYS, "x", 3)).toEqual({ order: KEYS, hidden: [] });
  });
});

describe("useColumnLayout", () => {
  it("columns excludes hidden while order keeps all", () => {
    const l = layout();
    l.applyColumns({ order: ["d", "c", "b", "a"], hidden: ["c"] });
    expect(l.order.value).toEqual(["d", "c", "b", "a"]);
    expect(shown(l)).toEqual(["d", "b", "a"]);
    expect(l.allColumns.value.map((c) => [c.key, c.hidden])).toEqual([
      ["d", false],
      ["c", true],
      ["b", false],
      ["a", false],
    ]);
  });

  it("detailColumns keeps hidden ones in order and columns derives from it", () => {
    const l = layout();
    l.applyColumns({ order: ["d", "c", "b", "a"], hidden: ["c"] });
    // 전체 목록은 숨긴 열까지 order 순으로 담는다.
    expect(l.detailColumns.value.map((c) => [c.key, c.hidden])).toEqual([
      ["d", false],
      ["c", true],
      ["b", false],
      ["a", false],
    ]);
    // 보이는 열은 그것에서 숨김을 거른 것이다 — 매핑이 한 곳이어야 분기하지 않는다.
    expect(shown(l)).toEqual(l.detailColumns.value.filter((c) => !c.hidden).map((c) => c.key));
    /* 숨긴 열도 **자료의 원위치**를 그대로 보유한다. 이것이 틀리면 모달이 상이한 열의
       값을 묘사한다 — 순서를 역전한 이 사례에서 값으로 단언한다. */
    expect(l.detailColumns.value.map((c) => [c.key, c.srcIndex])).toEqual([
      ["d", 3],
      ["c", 2],
      ["b", 1],
      ["a", 0],
    ]);
    expect(l.allColumns.value.map((c) => [c.key, c.at])).toEqual([
      ["d", 3],
      ["c", 2],
      ["b", 1],
      ["a", 0],
    ]);
    for (const c of l.detailColumns.value) expect(c.width).toBeGreaterThan(0);
  });

  it("move converts visible indexes into order indexes", () => {
    const l = layout();
    l.applyColumns({ order: KEYS, hidden: ["b"] });
    expect(shown(l)).toEqual(["a", "c", "d"]);
    l.move(0, 1); // 보이는 열 기준: a 를 c 자리로
    expect(shown(l)).toEqual(["c", "a", "d"]);
    expect(l.order.value).toEqual(["b", "c", "a", "d"]);
    expect(l.hidden.value).toEqual(["b"]);
    l.move(2, 0); // d 를 맨 앞으로
    expect(shown(l)).toEqual(["d", "c", "a"]);
  });

  it("applyColumns cannot hide everything", () => {
    const l = layout();
    l.applyColumns({ order: KEYS, hidden: KEYS });
    expect(shown(l)).toEqual(KEYS);
    expect(l.hidden.value).toEqual([]);
  });

  it("load clears a stored hidden that covers everything", () => {
    store.set("test.cols.s.none", JSON.stringify({ order: KEYS, hidden: KEYS }));
    expect(shown(layout())).toEqual(KEYS);
  });

  it("hidden is persisted and reset clears it", () => {
    const l = layout();
    l.applyColumns({ order: KEYS, hidden: ["b"] });
    expect(l.isCustomized.value).toBe(true);
    expect(shown(layout())).toEqual(["a", "c", "d"]);
    l.reset();
    expect(shown(l)).toEqual(KEYS);
    expect(l.isCustomized.value).toBe(false);
  });

  it("pinned is clamped to the visible columns", () => {
    const l = layout();
    l.setPinned(3);
    l.applyColumns({ order: KEYS, hidden: ["b", "c"] });
    expect(l.columns.value.length).toBe(2);
    expect(l.pinLimit.value).toBe(1);
    expect(l.pinned.value).toBe(1);
  });

  it("move works when the hidden column is last", () => {
    const l = layout();
    l.applyColumns({ order: KEYS, hidden: ["d"] });
    l.move(0, 2); // a goes to c's place; d stays last in order
    expect(shown(l)).toEqual(["b", "c", "a"]);
    expect(l.order.value).toEqual(["b", "c", "a", "d"]);
  });

  it("srcIndex keeps the original position of every column", () => {
    const l = layout();
    l.applyColumns({ order: ["d", "c", "b", "a"], hidden: ["c"] });
    expect(l.columns.value.map((c) => [c.key, c.srcIndex])).toEqual([
      ["d", 3],
      ["b", 1],
      ["a", 0],
    ]);
    expect(l.allColumns.value.map((c) => [c.key, c.at])).toEqual([
      ["d", 3],
      ["c", 2],
      ["b", 1],
      ["a", 0],
    ]);
  });
});
