import { ref, watch } from "vue";

import { storageKey } from "../lib/inject";
import { setCellGrouping } from "../lib/grid";

/**
 * 숫자 칸에 천 단위 쉼표를 넣을지.
 *
 * **사람이 선택한다.** 어느 열이 금액이고 어느 열이 학번인지 이 앱은 판정하지
 * 않는다 — 열 명칭이나 값의 생김새로 판정하면 특정 양식에만 적중하는 규칙이
 * 코드에 적재되고, 그 규칙이 빗나간 양식에서는 식별자가 `20,260,001` 로
 * 묘사된다. 무엇이 식별자인지는 그 자료를 보유한 사람이 인지한다.
 *
 * 기본은 넣는 것이다. 금액·수량이 더 흔하고, 아닌 경우 1회 해제하면 보관된다.
 */
const STORAGE_KEY = () => storageKey("grouping");

function stored(): boolean {
  try {
    return localStorage.getItem(STORAGE_KEY()) !== "0";
  } catch {
    /* 저장소 접근이 차단된 환경에서도 동작해야 한다. */
    return true;
  }
}

export const grouping = ref(true);

let installed = false;

/**
 * 보관된 선택을 판독해 셀 묘사에 반영하고, 이후의 변경을 보관한다.
 *
 * **1회만 작동한다.** 저장소 접두사가 주입된 이후여야 하므로 모듈을 반입하는
 * 시점이 아니라 [`installSharedUi`](../index.ts) 가 호출한다.
 */
export function installGrouping(): void {
  if (installed) return;
  installed = true;
  grouping.value = stored();
  setCellGrouping(grouping.value);

  watch(grouping, (on) => {
    setCellGrouping(on);
    try {
      localStorage.setItem(STORAGE_KEY(), on ? "1" : "0");
    } catch {
      /* 저장 실패는 무시한다. */
    }
  });
}

export function setGrouping(on: boolean): void {
  grouping.value = on;
}
