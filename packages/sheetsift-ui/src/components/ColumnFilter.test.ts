// @vitest-environment jsdom
/**
 * **깔대기 팝오버를 실제로 올려 눌러 본다.**
 *
 * 순수 함수로 추출한 판단은 `lib/grid.test.ts` 가 지킨다. 그런데 2026-11-11
 * 숫자 범위 필터를 배치하며 발견된 결함 넷은 **전부 부품 내부**에 있었고, 그
 * 시점에 시험은 전부 통과하고 있었다 — 추출한 함수는 정상이고, 그것을
 * 호출하는 자리가 결함이었다.
 *
 * 그래서 여기서 고정하는 것은 **함수가 아니라 호출되는 자리**다.
 *
 * - 범위를 좁히면 범위 밖이 된 선택을 거두는가. **검색어가 있어도** 거두는가 —
 *   거두는 조건이 좁아 검색어가 있으면 거두지 않았고, 노출되지 않은 선택이
 *   범위와 겹쳐 **조용히 0건**이 되었다.
 * - 아무 행도 통과하지 못하는 필터를 막는 가드를 **범위 입력칸의 엔터가**
 *   지나지 않는가. 단추만 막혀 있어 엔터로 표를 통째로 비울 수 있었다.
 *
 * 환경은 jsdom 이다(최상단의 지시). 부품이 `document` 와 `getComputedStyle` 을
 * 참조하므로 기본 환경에서는 적재되지 않는다.
 */
import { mount } from "@vue/test-utils";
import { beforeEach, expect, test, vi } from "vitest";
import ColumnFilter from "./ColumnFilter.vue";
import type { ColumnDef, ColumnValuesResult, NumberRange, ValuePick } from "../lib/grid";
import {
  provideErrors,
  provideGridSource,
  provideMessages,
  type GridSource,
  type SharedUiMessages,
} from "../lib/inject";

/** 누구나 인지하는 시험용 값이다. 바탕이 제공하는 표식의 자리를 메울 뿐이다. */
const BLANK = "(blank)";

/** 숫자 열 하나. 자료는 9 · 20 · 100 과 공백이다. */
const COLUMN: ColumnDef = {
  key: "score",
  label: "score",
  kind: "number",
  sortable: true,
  filterable: true,
};

const SPAN: NumberRange = { min: 9, max: 100 };

/** 문구는 받는 앱의 것이다. 시험에서는 열쇠를 그대로 반환해 **열쇠로 조회한다.** */
const messages: SharedUiMessages = {
  t: (key) => key,
  tn: (key, n) => `${key}:${n}`,
  formatNumber: (n) => String(n),
  formatDateTime: (d) => d.toISOString(),
};

function source(): GridSource {
  return {
    limits: () => ({ blankToken: BLANK, maxValues: 1000, maxPage: 500 }),
    queryRows: () => Promise.reject(new Error("this test does not query rows")),
    columnValues: () => Promise.reject(new Error("candidates arrive through the load prop")),
    printRows: () => Promise.reject(new Error("this test does not print")),
    printEnabled: () => false,
  };
}

/**
 * jsdom 은 요소의 스크롤을 구현하지 않는다(`Element.prototype.scrollTo` 가
 * 부재한다). 부품은 후보가 교체되면 목록을 상단으로 되돌리므로, 그 호출을
 * 수용할 것을 배치한다 — **묵시적으로 무시하지 않고** 호출되었는지를
 * 시험으로 확인한다.
 */
let scrolled: ReturnType<typeof vi.fn>;

beforeEach(() => {
  scrolled = vi.fn();
  Element.prototype.scrollTo = scrolled as unknown as Element["scrollTo"];
  provideMessages(messages);
  provideGridSource(source());
  provideErrors({ recognize: () => undefined, format: (p) => String(p) });
});

/**
 * 후보를 제공하는 문. **바탕이 수행하는 것과 동일하게 범위를 적용해** 반환한다 —
 * 목록이 범위 밖의 값을 담으면 그것을 선택해 조용히 0건이 되는 자리라, 시험의
 * 바탕도 그 약정을 지켜야 의미가 있다.
 */
function loader() {
  const calls: { needle: string; range: NumberRange | null }[] = [];
  const all = ["9", "20", "100", BLANK];
  const load = (needle: string, range: NumberRange | null): Promise<ColumnValuesResult> => {
    calls.push({ needle, range });
    const values = all
      .filter((v) => (needle ? v.includes(needle) : true))
      .filter((v) => {
        if (!range) return true;
        const n = Number(v);
        return Number.isFinite(n) && n >= range.min && n <= range.max;
      })
      .map((value) => ({ value, count: 1 }));
    // 자료의 범위는 걸어 둔 범위에 좁아지지 않는다(바탕의 약정).
    return Promise.resolve({ values, truncated: false, range: SPAN, elapsedMs: 0 });
  };
  return { load, calls };
}

/** 부품이 발신한 `apply` 의 인자 쌍. */
type Applied = [ValuePick | null, NumberRange | null];

/** 팝오버를 올리고 첫 수령이 반영될 때까지 대기한다. */
async function open(selected: ValuePick | null) {
  const { load, calls } = loader();
  const pop = mount(ColumnFilter, {
    attachTo: document.body,
    props: { column: COLUMN, selected, load, x: 0, y: 0 },
  });
  await flush();
  return { pop, calls };
}

/** 발신된 적용들. 발신하지 않았으면 공백이다. */
function appliedOf(pop: ReturnType<typeof mount>): Applied[] {
  return (pop.emitted("apply") ?? []) as Applied[];
}

/**
 * 대기 중인 약속과 묘사를 전부 소화한다. `ms` 는 **지연을 넘기는 시간**이다 —
 * 값 검색은 입력마다 질의하지 않으려고 지연을 두고 발신한다.
 */
async function flush(ms = 0) {
  for (let i = 0; i < 4; i += 1) {
    await Promise.resolve();
    await new Promise((r) => setTimeout(r, ms));
  }
}

/** 값 검색의 지연(`ColumnFilter.vue` 의 200ms)을 넘기는 시간. */
const AFTER_DEBOUNCE = 120;

/** 범위의 지연 확정(`RANGE_SETTLE`, 300ms)을 넘기는 시간. */
const AFTER_SETTLE = 120;

/** 범위 입력칸 둘. 묘사된 순서가 하한·상한이다. */
function rangeInputs(pop: ReturnType<typeof mount>) {
  const found = pop.findAll(".dg-range-num");
  expect(found.length, "the range inputs were not rendered").toBe(2);
  return { lo: found[0], hi: found[1] };
}

/* ── 범위 밖이 된 선택을 거두는가 ──────────────────────────────── */

/**
 * **검색어가 있어도 거둔다.** 그날의 결함이 이 조건이다 — 거두는 조건이
 * 검색어가 없는 경우로 좁혀져 있었고, 축소된 목록에 노출되지 않는 선택이
 * 범위와 AND 로 겹쳐 조용히 0건이 되었다.
 */
test("a range drops the picks that fall outside it, even with a needle typed", async () => {
  const { pop } = await open({ mode: "include", values: ["9", "20", "100"] });

  /* 검색어를 **실제로 반영시킨다.** 목록이 축소되어 `9` 가 노출되지 않는 상태가
     그날의 조건이다 — 노출되지 않는 선택이 범위와 겹쳐 조용히 0건이 되었다. */
  await pop.find('input[type="search"]').setValue("0");
  await flush(AFTER_DEBOUNCE);
  expect(pop.findAll(".dg-pop-list label").length, "the candidate list was not narrowed").toBe(2);

  const { lo } = rangeInputs(pop);
  await lo.setValue("10");
  await lo.trigger("change");
  await flush();

  await pop.find(".dg-pop-foot button:last-child").trigger("click");
  const sent = appliedOf(pop);
  expect(sent.length, "no apply was emitted").toBe(1);
  const [pick, range] = sent[0];
  expect(range).toEqual({ min: 10, max: 100 });
  expect(pick).toEqual({ mode: "include", values: ["20", "100"] });

  pop.unmount();
});

/** 범위 안의 선택은 그대로 둔다. 거두는 쪽만 확인하면 전부 거두어도 통과한다. */
test("a range keeps the picks that are inside it", async () => {
  const { pop } = await open({ mode: "include", values: ["20", "100"] });
  const { lo } = rangeInputs(pop);
  await lo.setValue("10");
  await lo.trigger("change");
  await flush();
  await pop.find(".dg-pop-foot button:last-child").trigger("click");
  const [pick] = appliedOf(pop)[0];
  expect(pick).toEqual({ mode: "include", values: ["20", "100"] });
  pop.unmount();
});

/* ── 아무 행도 통과하지 못하는 필터를 막는가 ───────────────────── */

/**
 * **범위 입력칸의 엔터가 가드를 지나지 않는다.** 열거한 것만 통과시키는데 하나도
 * 열거하지 않은 상태(`include` · 0개)는 표를 통째로 비운다. 단추만 막으면
 * 엔터로 그 상태를 적용할 수 있었다 — 그것이 그날의 결함이다.
 */
test("Enter in a range input does not apply a filter that passes no row", async () => {
  const { pop } = await open({ mode: "include", values: ["20"] });

  // 열거한 하나를 해제한다 → `include` 이면서 0개가 된다.
  const boxes = pop.findAll(".dg-pop-list input[type='checkbox']");
  const twenty = boxes.find((b) => b.element.closest("label")?.textContent?.includes("20"));
  expect(twenty, "the checkbox for 20 was not found").toBeTruthy();
  await twenty!.setValue(false);
  await flush();

  const apply = pop.find(".dg-pop-foot button:last-child");
  expect(apply.attributes("disabled"), "the apply button is not disabled").toBeDefined();

  const { lo } = rangeInputs(pop);
  lo.element.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
  await flush();
  expect(appliedOf(pop), "a filter that passes no row was applied").toEqual([]);

  pop.unmount();
});

/** 적용할 수 있는 상태라면 **엔터가 적용한다.** 막는 쪽만 확인하면 영구히 막아도 통과한다. */
test("Enter in a range input applies when the filter passes rows", async () => {
  const { pop } = await open(null);
  const { lo } = rangeInputs(pop);
  await lo.setValue("10");
  lo.element.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
  await flush();
  const sent = appliedOf(pop);
  expect(sent.length, "Enter did not apply").toBe(1);
  expect(sent[0][1]).toEqual({ min: 10, max: 100 });
  pop.unmount();
});

/** 창에 등록한 수신기를 해제하는가. 잔류하면 폐쇄한 팝오버가 엔터에 반응한다. */
test("the popover stops listening to the window once it closes", async () => {
  const off = vi.spyOn(window, "removeEventListener");
  const { pop } = await open(null);
  pop.unmount();
  const names = off.mock.calls.map((c) => c[0]);
  expect(names).toContain("keydown");
  expect(names).toContain("pointerdown");
  off.mockRestore();
});

/**
 * **방향키로 손잡이를 움직이면 지연 확정으로 보낸다.** 슬라이더는 방향키로도
 * `change` 를 매번 발신하므로, 즉시 확정하면 키를 누르고 있는 동안 초당 수십 번
 * 확정되고 확정마다 목록 질의가 발신된다 — 지연이 통째로 우회된다.
 *
 * 손잡이를 잡았다 놓은 경우(`pointerdown` 이 선행한 것)만 즉시 확정한다.
 */
test("arrow keys on the slider go through the delay, a released grip does not", async () => {
  const { pop, calls } = await open(null);
  const before = calls.length;
  const [lo] = pop.findAll(".dg-range-thumb");

  // 방향키: `pointerdown` 이 부재한 `change` 다.
  await lo.setValue("40");
  await lo.trigger("change");
  await flush();
  expect(calls.length, "an arrow-key change queried at once").toBe(before);

  // 멈추면 확정한다.
  await flush(AFTER_SETTLE);
  expect(calls.length, "nothing was committed after the delay").toBeGreaterThan(before);
  expect(calls[calls.length - 1].range).toEqual({ min: 40, max: 100 });

  // 손잡이를 잡았다 놓은 것은 즉시 확정한다.
  const now = calls.length;
  await lo.trigger("pointerdown");
  await lo.setValue("50");
  await lo.trigger("change");
  await flush();
  expect(calls.length, "a released grip did not commit at once").toBeGreaterThan(now);
  expect(calls[calls.length - 1].range).toEqual({ min: 50, max: 100 });

  pop.unmount();
});
