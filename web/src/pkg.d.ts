/**
 * 구운 wasm 겹의 구조.
 *
 * **수동 기재한다.** `wasm-bindgen` 은 `--no-typescript` 로 굽기 때문에 `.d.ts`
 * 가 산출되지 않는다. 여기 기재한 것이 `crates/web/src/bindings.rs` 와 상이하면
 * 타입 검사는 묵시적으로 통과하고 브라우저에서야 발현되므로, 명칭이 상이해지는
 * 것만은 `crates/web/tests/boundary.rs` 가 글자로 차단한다.
 *
 * 수용하는 것도 반환하는 것도 **JSON 문자 하나**다. 그 약정이 경계의 전부다.
 *
 * 명칭으로 반입한다(`sheetsift-wasm`). 구운 파일을 상대 경로로 지시하면, 아직
 * 굽지 않은 컴퓨터에서는 타입 검사가 **파일 부재로 중단된다** — wasm 은 여기서
 * 구울 수 없다. 실제 위치는 `vite.config.ts` 의 alias 가 인지한다.
 */
declare module "sheetsift-wasm" {
  export default function init(): Promise<unknown>;

  /** 화면이 임의로 기재하면 안 되는 값들 (JSON). */
  export function limits(): string;

  export class Book {
    constructor(bytes: Uint8Array, name: string);
    listScreens(request: string): string;
    queryRows(request: string): string;
    columnValues(request: string): string;
  }
}
