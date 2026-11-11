import { ref } from "vue";

import { storageKey } from "../lib/inject";

/**
 * 표의 글자 크기(px). 단계는 이 다섯뿐이다.
 *
 * 연속값이 아니라 단계로 배치한 사유: 열 폭·행 높이가 이 값에 비례하므로 임의의
 * 값을 허용하면 사용자마다 상이한 표를 열람하게 되고, 화면을 설명하는 문장("16px 로
 * 열람하면 …")이 성립하지 않는다. 다섯 단계면 어느 화면을 지칭해도 상대가 동일한
 * 표를 열람한다.
 */
export const TABLE_FONT_STEPS = [14, 16, 18, 20, 22] as const;

export type TableFontPx = (typeof TABLE_FONT_STEPS)[number];

/**
 * 기본 크기. `styles/grid.css` 의 `.dg { --dg-font: 16px }` 와 **일치해야 한다.**
 * 그쪽은 값을 부여받지 않은 `.dg`(폭 측정용 `.dg-probe`)의 크기이고, 이쪽은 화면
 * 표의 크기다. 둘이 분기하면 측정한 폭이 화면의 글자와 불일치하며 오류는 발생하지
 * 않는다. `crates/shell/tests/screen.rs` 가 두 값을 대조한다.
 */
export const DEFAULT_TABLE_FONT: TableFontPx = 16;

/* 저장소 키는 접두사를 주입받은 이후에 구성한다 — 모듈을 반입하는 시점에는
   아직 주입되지 않았을 수 있다. */
const STORAGE_KEY = () => storageKey("fontSize");

function isStep(n: number): n is TableFontPx {
  return (TABLE_FONT_STEPS as readonly number[]).includes(n);
}

function readStored(): TableFontPx {
  try {
    const raw = localStorage.getItem(STORAGE_KEY());
    /* 단계에 부재하는 값(수기로 변경한 값·단계를 변경한 뒤 잔존한 값)은 기본으로
       복귀시킨다. 가장 인접한 단계로 보정하지 않는다 — 어느 쪽이 정확한지 여기서는
       판정할 수 없다. */
    if (raw !== null && /^\d+$/.test(raw) && isStep(Number(raw))) return Number(raw) as TableFontPx;
  } catch {
    /* 저장소 접근이 차단된 환경에서도 동작해야 한다. */
  }
  return DEFAULT_TABLE_FONT;
}

/**
 * 화면 전체가 공유하는 하나의 값이다. 열 배치처럼 화면마다 분리하지 않는다 —
 * 글자 크기는 표가 아니라 열람하는 사람의 시력에 귀속되므로, 조회 화면에서 확대하고
 * 범위 차트로 이동하면 재차 축소되는 것이 오히려 부자연스럽다. 그래서 테마처럼 모듈
 * 하나가 보유하고, 조작부와 표가 직접 판독한다 — 조작부를 사용하는 화면마다 값을
 * 전달하는 배선을 두면, 하나를 누락한 화면에서 `＋` 가 작동하지 않는다.
 */
export const tableFont = ref<TableFontPx>(DEFAULT_TABLE_FONT);

let installed = false;

/**
 * 보관된 선택을 판독한다. **1회만 작동한다.** 받는 앱이
 * [`installSharedUi`](../index.ts) 로 호출하고, 그 시점은 화면을 장착하기 이전이다 —
 * 최초 묘사가 기본 크기로 출력된 뒤 변경되며 점멸하지 않는다.
 */
export function installTableFont(): void {
  if (installed) return;
  installed = true;
  tableFont.value = readStored();
}

/** 한 단계 위(`+1`)·아래(`-1`)로 이동한다. 끝에서는 아무 동작도 하지 않는다. */
export function stepTableFont(by: 1 | -1): void {
  const at = TABLE_FONT_STEPS.indexOf(tableFont.value) + by;
  if (at < 0 || at >= TABLE_FONT_STEPS.length) return;
  tableFont.value = TABLE_FONT_STEPS[at];
  try {
    localStorage.setItem(STORAGE_KEY(), String(tableFont.value));
  } catch {
    /* 저장 실패는 무시한다. 현 세션에서는 변경된 값이 유지된다. */
  }
}
