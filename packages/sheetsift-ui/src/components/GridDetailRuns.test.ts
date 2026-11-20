// @vitest-environment jsdom
/**
 * **행 상세의 묶음 경계가 화면에 부착되는가.**
 *
 * 열에 부여한 색은 왼쪽 띠로 보이고, **색이 이어지는 동안이 한 묶음**이다. 띠는 줄마다
 * 끊지 않고 묶음 전체를 하나로 잇는다 — 스키마에 묶음의 이름이 부재해서 묶음을 보이게
 * 하는 것이 띠의 연속성뿐이다. CSS 는 `data-run` 이 부착된 줄에만 간격을 두어 선을 끊는다.
 *
 * 판정이 `runStarts` 하나에 모여 있고 그쪽은 `lib/grid.test.ts` 가 전수로 본다. 여기서
 * 보는 것은 **그 결과가 실제로 그 속성으로 나오는가**다 — 순함수가 맞아도 화면이 그것을
 * 부착하지 않거나 차례가 어긋나면 띠는 조용히 엉뚱한 자리에서 끊긴다. 색은 받는 앱의
 * CSS 가 입히므로 **이 시험에 색 값은 등장하지 않는다.**
 *
 * 환경은 jsdom 이다 — 부품이 `document` 와 `getComputedStyle` 을 참조한다.
 */
import { mount } from "@vue/test-utils";
import { expect, test, vi } from "vitest";
import DataGrid from "./DataGrid.vue";
import type { ColumnDef, Row, ScreenDef } from "../lib/grid";
import {
  provideAppName,
  provideErrors,
  provideGridSource,
  provideMessages,
  provideStoragePrefix,
  type GridSource,
} from "../lib/inject";

/* 폭 측정은 jsdom 이 0 을 돌려준다. 이 시험은 폭이 아니라 묶음 경계를 본다. */
vi.mock("../lib/colWidth", async () => {
  const { ref } = await import("vue");
  return { fontEpoch: ref(0), headerWidthRem: () => 8, autoWidthRem: () => 8 };
});

provideStoragePrefix("test");
provideMessages({ t: (k) => k, tn: (k) => k, formatNumber: String, formatDateTime: String });
provideErrors({ recognize: () => undefined, format: String });
provideAppName("Test");

/* jsdom 에는 `<dialog>` 의 `showModal` 이 부재한다. 상세는 그린 뒤에 부르므로 흉내만 둔다. */
HTMLDialogElement.prototype.showModal = function showModal(this: HTMLDialogElement) {
  this.open = true;
};
HTMLDialogElement.prototype.close = function close(this: HTMLDialogElement) {
  this.open = false;
};

const ROWS: Row[] = [["a", 1, "b", 2]];

/** 받는 앱이 싣는 변수를 흉내낸 것. **이 패키지는 이름도 값도 해석하지 않는다.** */
const A = { "--hv-light": "#111111" };
const B = { "--hv-light": "#222222" };

/** 열 넷. `cellVars` 는 `detailColumns` 가 열 정의에서 그대로 전개한다. */
function screen(vars: (Record<string, string> | undefined)[]): ScreenDef {
  const columns = vars.map((v, i) => {
    const col: Record<string, unknown> = {
      key: `c${i}`,
      label: `C${i}`,
      kind: "text",
      sortable: true,
      filterable: true,
      srcIndex: i,
    };
    if (v) col.cellVars = v;
    return col as unknown as ColumnDef;
  });
  return {
    id: "s",
    kind: "sheet",
    title: "t",
    description: "d",
    group: "g",
    columns,
    headerFound: true,
    header: null,
    headerGuessed: false,
    sheetRows: ROWS.length,
    tooWide: false,
  } as unknown as ScreenDef;
}

function source(): GridSource {
  return {
    limits: () => ({ pageSize: 100, maxRows: 100, maxCols: 100 }) as never,
    queryRows: async () => ({ rows: ROWS, total: ROWS.length, elapsedMs: 0 }) as never,
    columnValues: async () => ({ values: [], more: false, elapsedMs: 0 }) as never,
    printRows: async () => ({ rows: ROWS, total: ROWS.length, truncated: false }) as never,
    printEnabled: () => true,
  };
}

/**
 * 상세를 개방하고 줄마다 「묶음을 개시하는가」를 돌려준다. 차례는 열의 차례다 —
 * **차례의 정본은 표**이므로 그것이 어긋나면 띠가 엉뚱한 자리에서 끊긴다.
 */
async function openedRuns(vars: (Record<string, string> | undefined)[]): Promise<boolean[]> {
  provideGridSource(source());
  const wrapper = mount(DataGrid, { props: { screen: screen(vars) }, attachTo: document.body });
  for (let i = 0; i < 4; i += 1) await wrapper.vm.$nextTick();
  await wrapper.find(".dg-row").trigger("click");
  for (let i = 0; i < 3; i += 1) await wrapper.vm.$nextTick();

  const pairs = wrapper.findAll(".rd-pair");
  /* 상세가 실제로 그려졌는가 — 그리지 않았다면 아래 확인이 **빈 배열로 통과한다.** */
  expect(pairs.length).toBe(vars.length);
  return pairs.map((p) => p.attributes("data-run") !== undefined);
}

test("a run is one stretch of the same colour, and the same colour coming back apart is another", async () => {
  /* `A A B A` → 묶음 셋이다. 넷째를 첫째와 합치면 **열의 차례를 바꾸는 셈**이다. */
  expect(await openedRuns([A, A, B, A])).toEqual([true, false, true, true]);
});

test("an app that passes no colour gets one colourless run", async () => {
  /* 이 저장소의 앱이 그렇다 — 띠가 투명한 한 묶음이고 이전과 동일한 모양이다. */
  expect(await openedRuns([undefined, undefined, undefined, undefined])).toEqual([
    true,
    false,
    false,
    false,
  ]);
});

test("the colour still rides along on the row, so the app can paint the band", async () => {
  /* 띠의 색은 받는 앱의 CSS 가 `[data-cvars]` 로 입힌다. 그 표식이 사라지면 **색을 입힐
     자리가 소멸**하는데 묶음 경계는 멀쩡해 이 파일의 다른 시험은 통과한다. */
  provideGridSource(source());
  const wrapper = mount(DataGrid, { props: { screen: screen([A, undefined]) }, attachTo: document.body });
  for (let i = 0; i < 4; i += 1) await wrapper.vm.$nextTick();
  await wrapper.find(".dg-row").trigger("click");
  for (let i = 0; i < 3; i += 1) await wrapper.vm.$nextTick();

  const pairs = wrapper.findAll(".rd-pair");
  expect(pairs.length).toBe(2);
  expect(pairs[0].attributes("data-cvars")).toBeDefined();
  expect(pairs[0].attributes("style")).toContain("--hv-light");
  /* 색을 부여하지 않은 열에는 표식도 `style` 도 부착되지 않는다. */
  expect(pairs[1].attributes("data-cvars")).toBeUndefined();
});
