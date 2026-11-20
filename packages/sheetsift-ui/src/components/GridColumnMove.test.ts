// @vitest-environment jsdom
/**
 * **열 제목을 끌어 옮기면 그 열이 옮겨지는가 — 받는 앱이 열을 더 걸러 넘길 때도.**
 *
 * 받는 앱은 `GridTable` 에 `layout.columns` 를 그대로 주지 않고, 묶음 칩으로 감춘 열·조회 전용
 * 열을 **더 뺀 배열**을 준다. 표가 내보내던 번호는 그 배열의 자리이고 `layout.move` 는 정본의
 * 자리로 읽었으므로, 감춘 열이 하나라도 있으면 엉뚱한 열이 옮겨져 저장됐다(조용히).
 *
 * 그래서 이 시험은 **갈라지던 경로** — 정본에는 있고 표가 받은 배열에는 없는 열이 있는 경우 —
 * 를 그대로 사례로 둔다. 감추지 않은 상태만 보면 번호도 열쇠도 같은 답이라 결함이 다시 들어와도
 * 통과한다.
 */
import { mount } from "@vue/test-utils";
import { beforeEach, expect, test, vi } from "vitest";
import { nextTick, ref } from "vue";
import GridTable from "./GridTable.vue";
import { useColumnLayout } from "../composables/useColumnLayout";
import type { ScreenDef } from "../lib/grid";
import {
  provideErrors,
  provideGridSource,
  provideMessages,
  provideStoragePrefix,
  type GridSource,
} from "../lib/inject";

vi.mock("../lib/colWidth", async () => {
  const { ref } = await import("vue");
  return { fontEpoch: ref(0), headerWidthRem: () => 8, autoWidthRem: () => 8 };
});

provideStoragePrefix("test");
provideMessages({ t: (k) => k, tn: (k) => k, formatNumber: String, formatDateTime: String });
provideErrors({ recognize: () => undefined, format: String });
provideGridSource({
  limits: () => ({ blankToken: "(blank)", maxValues: 1000, maxPage: 500 }),
  queryRows: () => Promise.reject(new Error("unused")),
  columnValues: () => Promise.reject(new Error("unused")),
  printRows: () => Promise.reject(new Error("unused")),
  printEnabled: () => false,
} as GridSource);

const KEYS = ["a", "b", "c", "d", "e"];

function layout() {
  const screen = {
    id: "s",
    columns: KEYS.map((key) => ({ key, label: key, kind: "text", sortable: true, filterable: true })),
  } as unknown as ScreenDef;
  return useColumnLayout(ref(screen));
}

beforeEach(() => localStorage.clear());

/** 머리 칸을 `from` 에서 눌러 `to` 칸 위에서 놓는다. jsdom 은 배치를 모르므로 칸마다 100px 로 흉내 낸다. */
function dragHeader(wrapper: { element: Element }, from: number, to: number) {
  const cells = Array.from(wrapper.element.querySelectorAll<HTMLElement>(".dg-head > *"));
  cells.forEach((el, i) => {
    el.getBoundingClientRect = () => ({ left: i * 100, width: 100 }) as DOMRect;
  });
  cells[from].dispatchEvent(
    new MouseEvent("pointerdown", { bubbles: true, button: 0, clientX: from * 100 + 50 }),
  );
  window.dispatchEvent(new MouseEvent("pointermove", { clientX: to * 100 + 10 }));
  window.dispatchEvent(new MouseEvent("pointerup"));
}

/** 받는 앱처럼 표에 주는 배열을 정본보다 더 거른 채 올리고, 헤더 `from` 을 끌어 `to` 에 놓는다. */
async function drag(passed: string[], from: number, to: number) {
  const l = layout();
  const wrapper = mount(GridTable, {
    props: {
      columns: l.columns.value.filter((c) => passed.includes(c.key)),
      rows: [],
      sorts: [],
      filterText: {},
      filterValues: {},
      detailColumns: l.detailColumns.value,
      pinnedKeys: [],
      loadValues: async () => ({ values: [], more: false, elapsedMs: 0 }) as never,
      "onUpdate:filterText": () => {},
      onMove: (a: string, b: string) => l.move(a, b),
    },
    attachTo: document.body,
  });
  // jsdom 은 배치를 모른다. 머리 칸의 가운데를 열마다 100px 간격으로 흉내 낸다.
  dragHeader(wrapper, from, to);
  await nextTick();
  wrapper.unmount();
  return l;
}

test("dragging with no column filtered out moves that column", async () => {
  const l = await drag(KEYS, 0, 2);
  expect(l.order.value).toEqual(["b", "c", "a", "d", "e"]);
});

test("dragging when the table was handed fewer columns still moves that column", async () => {
  // 표가 받은 것: a, c, e (b·d 는 앱이 걸러 냈다). 표의 0 번 a 를 2 번 e 의 자리로.
  // 번호로 정본에서 찾으면 a 가 아니라 **c** 가 e 의 자리가 된다.
  const l = await drag(["a", "c", "e"], 0, 2);
  expect(l.order.value).toEqual(["b", "c", "d", "e", "a"]);
});

test("a column hidden by the layout and one filtered by the app both stay put", async () => {
  const l = layout();
  l.applyColumns({ order: KEYS, hidden: ["b"] });
  const wrapper = mount(GridTable, {
    props: {
      columns: l.columns.value.filter((c) => c.key !== "d"), // a, c, e
      rows: [],
      sorts: [],
      filterText: {},
      filterValues: {},
      detailColumns: l.detailColumns.value,
      pinnedKeys: [],
      loadValues: async () => ({ values: [], more: false, elapsedMs: 0 }) as never,
    },
    attachTo: document.body,
  });
  dragHeader(wrapper, 2, 0);
  await nextTick();
  expect(wrapper.emitted("move")).toEqual([["e", "a"]]);
  l.move("e", "a");
  expect(l.order.value).toEqual(["e", "a", "b", "c", "d"]);
  expect(l.hidden.value).toEqual(["b"]);
  wrapper.unmount();
});

/* ── 고정 열: 「앞에서 몇 개」가 아니라 어느 열인가 ─────────────── */

function pinnedOf(wrapper: { element: Element }): string[] {
  return Array.from(wrapper.element.querySelectorAll(".dg-head > .dg-hcell[data-pin]")).map(
    (el) => el.textContent?.trim() ?? "",
  );
}

function mountPinned(passed: string[], pinnedKeys: string[]) {
  const l = layout();
  return mount(GridTable, {
    props: {
      columns: l.columns.value.filter((c) => passed.includes(c.key)),
      rows: [],
      sorts: [],
      filterText: {},
      filterValues: {},
      detailColumns: l.detailColumns.value,
      pinnedKeys,
      loadValues: async () => ({ values: [], truncated: false, elapsedMs: 0 }),
    },
    attachTo: document.body,
  });
}

test("the pinned columns are the named ones even when the app filtered the array", () => {
  // 정본: a b c d e 의 앞 둘(a, b)이 고정. 앱이 b 를 걸러 a, c, e 를 넘겼다.
  // 개수 2 로 받으면 c 까지 고정되어 **고정하지 않은 열이 고정**된다.
  const w = mountPinned(["a", "c", "e"], ["a", "b"]);
  expect(pinnedOf(w)).toEqual(["a"]);
  w.unmount();
});

test("layout.pinnedKeys names the first pinned visible columns", () => {
  const l = layout();
  l.applyColumns({ order: KEYS, hidden: ["a"] });
  l.setPinned(2);
  expect(l.pinnedKeys.value).toEqual(["b", "c"]);
});

/* ── 범위 칸: 범위를 아는 받는 앱에게만 ────────────────────────── */

async function openNumberFilter(filterRanges?: Record<string, null>) {
  const l = layout();
  const col = { ...l.columns.value[0], kind: "number" as const };
  const w = mount(GridTable, {
    props: {
      columns: [col],
      rows: [],
      sorts: [],
      filterText: {},
      filterValues: {},
      detailColumns: l.detailColumns.value,
      pinnedKeys: [],
      ...(filterRanges ? { filterRanges } : {}),
      loadValues: async () => ({
        values: [],
        truncated: false,
        range: { min: 1, max: 9 },
        elapsedMs: 0,
      }),
    },
    attachTo: document.body,
  });
  await w.find(".dg-fbtn").trigger("click");
  for (let i = 0; i < 5; i += 1) await new Promise((r) => setTimeout(r, 0));
  const n = document.querySelectorAll(".dg-range-num").length;
  w.unmount();
  return n;
}

test("the range inputs are drawn only when the app passes filterRanges", async () => {
  expect(await openNumberFilter({})).toBe(2);
  expect(await openNumberFilter()).toBe(0);
});
