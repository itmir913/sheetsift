// @vitest-environment jsdom
/**
 * **`cell` 슬롯이 응답의 행 객체를 그대로 건네는가.**
 *
 * 받는 앱은 행마다 산출한 것(조건 서식 따위)을 **행 객체를 열쇠로** 조회한다 — 슬롯이 주는 것에
 * 행 번호가 없고, 번호로는 풀리지 않기 때문이다(`docs/shared-ui.md` 의 「The `cell` slot hands over
 * the row object itself」). 그 조회는 복제가 한 번이라도 끼면 **조용히** 실패한다 — 오류도 경고도
 * 없이 서식만 사라지고 표는 멀쩡해 보인다.
 *
 * 그래서 이 시험이 고정하는 것은 꼴이 아니라 **동일성**이다. 자료원이 돌려준 배열의 그 요소가
 * 세 슬롯(본문·행 상세·인쇄)에 그대로 도달하는가를 `toBe` 로 본다. 복제·전개·`map` 이 생기면 깨진다.
 *
 * 환경은 jsdom 이다 — 부품이 `document` 와 `getComputedStyle` 을 참조한다.
 */
import { mount } from "@vue/test-utils";
import { expect, test, vi } from "vitest";
import { defineComponent, h, nextTick } from "vue";
import DataGrid from "./DataGrid.vue";
import type { Row, ScreenDef } from "../lib/grid";
import {
  provideErrors,
  provideGridSource,
  provideMessages,
  provideAppName,
  provideStoragePrefix,
  type GridSource,
} from "../lib/inject";

/* 폭 측정은 jsdom 에서 0 을 돌려준다. 이 시험은 폭이 아니라 동일성을 본다. */
vi.mock("../lib/colWidth", async () => {
  const { ref } = await import("vue");
  return { fontEpoch: ref(0), headerWidthRem: () => 8, autoWidthRem: () => 8 };
});

/* 보관함은 jsdom 이 제공한다 — 접두사만 시험 전용으로 둔다. */
provideStoragePrefix("test");
provideMessages({ t: (k) => k, tn: (k) => k, formatNumber: String, formatDateTime: String });
provideErrors({ recognize: () => undefined, format: String });
provideAppName("Test");

/* jsdom 에는 `<dialog>` 의 `showModal` 이 없다. 행 상세는 그린 뒤에 그것을 부르므로,
   없으면 그리기까지는 되고 그 호출에서 터진다 — 시험의 관심은 그리기라 흉내만 둔다. */
HTMLDialogElement.prototype.showModal = function showModal(this: HTMLDialogElement) {
  this.open = true;
};
HTMLDialogElement.prototype.close = function close(this: HTMLDialogElement) {
  this.open = false;
};

/** 본문 조회가 돌려줄 행과, 인쇄 조회가 돌려줄 행. **다른 객체여야 한다** — 벌을 객체가 가린다. */
const BODY: Row[] = [["a", 1], ["b", 2]];
const PRINT: Row[] = [["a", 1], ["b", 2]];

const SCREEN = {
  id: "s",
  kind: "sheet",
  title: "t",
  description: "d",
  group: "g",
  columns: [
    { key: "name", label: "Name", kind: "text", sortable: true, filterable: true, srcIndex: 0 },
    { key: "n", label: "N", kind: "number", sortable: true, filterable: true, srcIndex: 1 },
  ],
  headerFound: true,
  header: null,
  headerGuessed: false,
  sheetRows: 2,
  tooWide: false,
} as unknown as ScreenDef;

function source(): GridSource {
  return {
    limits: () => ({ pageSize: 100, maxRows: 100, maxCols: 100 }) as never,
    queryRows: async () => ({ rows: BODY, total: BODY.length, elapsedMs: 0 }) as never,
    columnValues: async () => ({ values: [], more: false, elapsedMs: 0 }) as never,
    printRows: async () => ({ rows: PRINT, total: PRINT.length, truncated: false }) as never,
    printEnabled: () => true,
  };
}

/** 슬롯이 받은 행 객체를 순서대로 담는다. */
function host(seen: Row[]) {
  return defineComponent({
    setup() {
      return () =>
        h(DataGrid, { screen: SCREEN }, {
          cell: (p: { row: Row; value: unknown }) => {
            seen.push(p.row);
            return String(p.value ?? "");
          },
        });
    },
  });
}

/** 세 슬롯을 모두 거칠 수 있게 올린다. 반환한 배열에 슬롯이 받은 행 객체가 쌓인다. */
async function mounted() {
  provideGridSource(source());
  const seen: Row[] = [];
  const wrapper = mount(host(seen), { attachTo: document.body });
  // 조회 → 그리기. 한 틱으로는 행이 아직 도달하지 않는다.
  for (let i = 0; i < 4; i += 1) await nextTick();
  return { wrapper, seen };
}

/** 그 행 객체 자체인가. `toEqual` 은 복제를 통과시키므로 쓰지 않는다. */
function isOneOf(got: Row[], want: Row[]): boolean {
  return want.every((w) => got.some((g) => g === w)) && got.every((g) => want.some((w) => w === g));
}

test("the table body slot is handed the very row object the source returned", async () => {
  const { seen } = await mounted();
  expect(seen.length).toBeGreaterThan(0);
  expect(isOneOf(seen, BODY)).toBe(true);
});

test("the row details slot is handed that same row object", async () => {
  const { wrapper, seen } = await mounted();
  seen.length = 0;
  // 행을 누르면 상세가 열린다. 상세는 표와 **같은 슬롯**을 쓴다.
  await wrapper.find(".dg-row").trigger("click");
  for (let i = 0; i < 3; i += 1) await nextTick();
  // 상세가 실제로 그려졌는가 — 그리지 않았다면 아래 확인이 본문만 보고 통과한다.
  expect(wrapper.findAll(".rd-value").length).toBeGreaterThan(0);
  // 누른 것은 첫 행이고, 상세에 도달한 것은 그 **객체**다. 누름이 본문도 다시 그리므로
  // 담긴 것에는 본문의 행도 섞인다 — 그래서 「그 객체가 왔는가」와 「전부 응답의 것인가」를 본다.
  expect(seen).toContain(BODY[0]);
  expect(seen.every((r) => BODY.includes(r))).toBe(true);
});

test("the print slot is handed print's own row objects, not the body's", async () => {
  vi.stubGlobal("print", vi.fn());
  const { wrapper, seen } = await mounted();
  seen.length = 0;
  const button = wrapper.findAll("button").find((b) => b.text().includes("tools.print"));
  expect(button).toBeDefined();
  await button!.trigger("click");
  for (let i = 0; i < 5; i += 1) await nextTick();

  // 지면에 그릴 행이 섰는가 — 서지 않았다면 아래 확인이 본문만 보고 통과한다.
  expect(document.querySelectorAll(".print-root .pv-table td").length).toBeGreaterThan(0);
  // 인쇄 조회가 돌려준 그 **객체**가 전부 슬롯에 도달한다.
  expect(PRINT.every((p) => seen.some((r) => r === p))).toBe(true);
  // **벌이 갈린다.** 같은 값이지만 다른 객체이고, 받는 쪽은 그것만으로 화면과 지면을 가린다.
  expect(PRINT.every((p) => !BODY.includes(p))).toBe(true);
  /* `window.print()` 가 불렸는지는 보지 않는다 — 그쪽은 한 프레임을 기다린 뒤에 불리고, 불린
     직후 `finally` 가 지면의 행을 비운다. 그래서 **그 프레임이 오기 전**이 지면의 행을 볼 수 있는
     유일한 창이다. 흉내는 그 창이 어긋나 실제로 불릴 때 jsdom 이 터지지 않게 두는 것이다. */
  vi.unstubAllGlobals();
});
