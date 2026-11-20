/**
 * **창틀의 경계 — 브라우저 벌.** 창을 조작하는 작업은 여기 단일 위치로만 통과한다.
 *
 * 창에서 구동할 때는 `shell.tauri.ts` 가 그 위치를 담당한다. 양자는 **동일한 것을
 * 반출해야** 하고(`crates/shell/tests/screen.rs` 가 대조한다), 화면은 `sheetsift-shell`
 * 이라는 명칭으로 반입해 어느 쪽인지 인지하지 않는다 — `book.wasm.ts`·`book.tauri.ts` 와
 * 동일한 분업이다.
 *
 * **`window.__TAURI__` 를 판정하지 않는다.** 그 검사는 런타임이라 `@tauri-apps/api` 가
 * 브라우저 묶음에 그대로 적재되고, 검사하는 위치가 화면마다 증가하면 사본이 된다. 별칭이
 * 파일 자체를 교체하므로 **판정은 빌드 시점 1회**이고, 이 벌의 묶음에는 그 꾸러미가
 * 들어갈 경로가 부재한다.
 */

/** 데스크톱 창에서 구동 중인가. **브라우저에서는 거짓이다.** */
export const IS_DESKTOP: boolean = false;

/*
 * 창을 조작하는 문 셋. 브라우저에는 조작할 창이 부재하므로 아무 작업도 수행하지 않는다.
 *
 * **호출되지 않는다** — 창 단추는 `IS_DESKTOP` 이 참인 경우에만 묘사된다. 그래도
 * 반출한다. 반출이 양측에서 동일해야 화면이 어느 벌인지 인지하지 않을 수 있고, 일방에만
 * 존재하는 문은 그 벌에서만 화면이 중단되는데 타입 검사는 기준으로 삼은 일방만 참조해
 * 통과한다.
 */

/**
 * 브라우저에는 제목 표시줄이 부재한다. **호출되지 않는다** — `isDesktop` 이 거짓이면
 * 공용 화면이 묻지 않는다. 그래도 반출해야 두 벌의 반출이 동일하다.
 */
export function nativeTitleBar(): Promise<boolean> {
  return Promise.resolve(false);
}

export function minimize(): void {}

export function toggleMaximize(): void {}

export function close(): void {}
