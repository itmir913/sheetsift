import { ref, watch } from "vue";

import { storageKey } from "../lib/inject";

/**
 * 셀의 내용을 열 폭에서 줄바꿈할지.
 *
 * **사람이 선택한다.** 어느 열이 장문인지 이 앱은 판정하지 않는다 — 열 명칭이나
 * 값의 길이로 판정하면 특정 양식에만 적중하는 규칙이 코드에 적재된다.
 *
 * 기본은 끈 것이다. 행 높이가 일정한 쪽이 훑어보기에 양호하고, 켠 상태는 1회
 * 누르면 보관된다.
 *
 * ## 줄 수에 상한을 두지 않는 사유
 *
 * 상한의 공학적 효용은 예약 크기를 상한으로 고정해 **측정을 회피**하는 것인데,
 * 이 표에는 측정 경로가 부재하다 — 행을 창으로 절단하는 가상화기가 없고
 * (`GridTable` 이 그 페이지의 행 전부를 묘사한다), 성능을 산출하는 것은
 * `content-visibility: auto` 하나다. 그래서 상한을 두어도 회피할 측정이 없고,
 * 남는 것은 자료의 절단뿐이다 — 줄바꿈을 켜는 사유가 그 칸을 끝까지 판독하는
 * 것이므로 상한은 기능의 목적을 자체 부정한다.
 *
 * 미묘사 행의 예약 크기는 `grid.css` 가 `contain-intrinsic-size` 의 `auto`
 * 열쇠말로 브라우저에 위임한다 — 1회 묘사된 행의 실측 높이를 기억한다.
 */
const STORAGE_KEY = () => storageKey("wrap");

function stored(): boolean {
  try {
    return localStorage.getItem(STORAGE_KEY()) === "1";
  } catch {
    /* 저장소 접근이 차단된 환경에서도 동작해야 한다. */
    return false;
  }
}

export const wrap = ref(false);

let installed = false;

/**
 * 보관된 선택을 판독하고, 이후의 변경을 보관한다.
 *
 * **1회만 작동한다.** 저장소 접두사가 주입된 이후여야 하므로 모듈을 반입하는
 * 시점이 아니라 [`installSharedUi`](../index.ts) 가 호출한다.
 *
 * 보관의 범위는 표마다가 아니라 **앱 전체**다 — 글자 크기(`useTableFont`)와
 * 동일하다. 표마다 분리하면 조회 화면에서 켜고 타 화면으로 이동하면 해제되어
 * 오히려 부자연스럽다.
 */
export function installWrap(): void {
  if (installed) return;
  installed = true;
  wrap.value = stored();

  watch(wrap, (on) => {
    try {
      localStorage.setItem(STORAGE_KEY(), on ? "1" : "0");
    } catch {
      /* 저장 실패는 무시한다. 현 세션에서는 변경된 값이 유지된다. */
    }
  });
}

export function setWrap(on: boolean): void {
  wrap.value = on;
}
