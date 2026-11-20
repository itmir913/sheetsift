/**
 * **창틀의 경계 — 창 벌.** 창을 조작하는 작업은 여기 단일 위치로만 통과한다.
 *
 * 브라우저에서 구동할 때는 `shell.web.ts` 가 그 위치를 담당한다. 양자는 **동일한 것을
 * 반출해야** 하고(`crates/shell/tests/screen.rs` 가 대조한다), 화면은 `sheetsift-shell`
 * 이라는 명칭으로 반입해 어느 쪽인지 인지하지 않는다.
 *
 * **`@tauri-apps/api` 를 반입하는 위치는 이 파일과 `book.tauri.ts` 둘뿐이다**
 * (`screen.rs` 가 글자로 대조한다). 화면이 직접 반입하면 브라우저 묶음에 적재되어, 그
 * 환경에서 호출하는 시점에 중단된다.
 *
 * **동작은 `capabilities/default.json` 의 권한 셋이 개방한다. 권한이 부재하면 단추가
 * 오류도 없이 아무 작업도 수행하지 않는다** — 그래서 `screen.rs` 가 여기 문마다 대응
 * 권한이 기재되어 있는지 대조한다.
 */
import { getCurrentWindow } from "@tauri-apps/api/window";

/** 데스크톱 창에서 구동 중인가. **창에서는 참이다.** */
export const IS_DESKTOP: boolean = true;

/* 창은 묘사 이전에 존재하므로 적재 시점에 1회 취득한다. */
const win = getCurrentWindow();

/**
 * **운영체제의 제목 표시줄이 존치하는가.** 맥은 신호등 단추를 살리려 `decorations` 를
 * 참으로 두고, 윈도·리눅스는 걷으려 거짓으로 둔다(`tauri.macos.conf.json`).
 *
 * **운영체제의 명칭으로 판정하지 않는다.** 우리가 단추를 묘사하는 사유는 「맥이 아니라서」가
 * 아니라 「운영체제가 묘사하지 않아서」다. 창에 물으면 그 설정 자체가 답이라 **설정과 판정이
 * 갈릴 수 없다** — 명칭으로 판정하면 리눅스에서 신호등을 사용하기로 하는 날 어긋나고, 그것은
 * 그 운영체제에서만 발현된다. 운영체제를 알려면 꾸러미를 더 들여야 하고 그 꾸러미는 러스트의
 * 최소 판을 1.90 으로 끌어올린다(이 상자는 1.82 다).
 */
export function nativeTitleBar(): Promise<boolean> {
  return win.isDecorated();
}

export function minimize(): void {
  void win.minimize();
}

export function toggleMaximize(): void {
  void win.toggleMaximize();
}

export function close(): void {
  void win.close();
}
