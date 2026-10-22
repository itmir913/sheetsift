/**
 * 구운 wasm 겹의 모양.
 *
 * **손으로 적는다.** `wasm-bindgen` 은 `--no-typescript` 로 굽기 때문에 `.d.ts`
 * 가 나오지 않는다. 여기 적은 것이 `crates/web/src/bindings.rs` 와 갈리면
 * 타입 검사는 조용히 통과하고 브라우저에서야 드러나므로, 이름이 갈리는 것만은
 * `crates/web/tests/boundary.rs` 가 글자로 막는다.
 *
 * 받는 것도 돌려주는 것도 **JSON 글자 하나**다. 그 약속이 경계의 전부다.
 *
 * 이름으로 들여온다(`sheetsift-wasm`). 구운 파일을 상대 경로로 가리키면,
 * 아직 굽지 않은 컴퓨터에서는 타입 검사가 **파일이 없다고 선다** — wasm 은
 * 여기서 구울 수 없다. 실제 자리는 `vite.config.ts` 의 alias 가 안다.
 */
declare module "sheetsift-wasm" {
  export default function init(): Promise<unknown>;

  /** 화면이 손으로 적으면 안 되는 값들 (JSON). */
  export function limits(): string;

  export class Book {
    constructor(bytes: Uint8Array, name: string);
    listScreens(request: string): string;
    queryRows(request: string): string;
    columnValues(request: string): string;
  }
}
