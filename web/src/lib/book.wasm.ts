/**
 * **경계 — 브라우저 벌.** 화면과 바탕(wasm) 사이는 여기 단일 위치로만 통과한다.
 *
 * 창에서 구동할 때는 `book.tauri.ts` 가 그 위치를 담당한다. 양자는 **동일한
 * 것을 반출해야** 하고(`crates/shell/tests/screen.rs` 가 대조한다), 화면은
 * `sheetsift-book` 이라는 명칭으로 반입해 어느 쪽인지 인지하지 않는다.
 *
 * goe-navi 는 Tauri 명령을 호출하고 이쪽은 wasm 메서드를 호출하지만, 문 명칭도
 * 요청의 항목 명칭도 동일한 벌이라 **변경되는 것은 호출 대상뿐**이다. 화면
 * 코드는 양측에서 동일한 것을 참조한다.
 *
 * 상이점 하나: Tauri 는 비동기고 wasm 은 즉시 응답한다. 그래도 `Promise` 로
 * 정합해 둔다 — 화면이 대기하는 구조까지 두 벌이 되면, 이전할 때마다 `await`
 * 를 제거하고 재부착하게 된다.
 */
import init, { Book as WasmBook, limits as wasmLimits } from "sheetsift-wasm";
import type { AppErrorPayload, ErrorCode } from "@/i18n/codes";
import { formatError } from "@/i18n";

/** 바탕의 오류를 포장한 예외. 화면은 `message` 를 그대로 표시하면 된다. */
export class AppError extends Error {
  readonly code: ErrorCode;
  readonly params: Record<string, string>;
  readonly detail?: string;

  constructor(payload: AppErrorPayload) {
    super(formatError(payload));
    this.name = "AppError";
    this.code = payload.code;
    this.params = payload.params ?? {};
    this.detail = payload.detail;
  }
}

/**
 * wasm 이 발생시킨 것을 `AppError` 로.
 *
 * 겹은 오류를 **JSON 문자**로 발생시킨다. 문자로 변환하지 못하는 지점까지
 * 도달하면 `UNKNOWN` 이 반환된다. 해석하지 못한 것은 은폐하지 않고 원문을
 * `detail` 에 수용한다 — 개발자만 참조하는 위치이고, 여기서 상실하면 재탐색할
 * 경로가 부재한다.
 */
function thrown(raw: unknown): AppError {
  const text = typeof raw === "string" ? raw : String((raw as Error)?.message ?? raw);
  try {
    const payload = JSON.parse(text) as AppErrorPayload;
    if (payload && typeof payload.code === "string") return new AppError(payload);
  } catch {
    /* JSON 이 아니면 하단으로 진행한다. */
  }
  return new AppError({ code: "UNKNOWN", detail: text });
}

/** 바탕이 제공하는, 화면이 임의로 기재하면 안 되는 값들. */
export interface Limits {
  /** 값이 부재하는 셀을 값 선택 목록에 기재하는 문자. **여과할 때도 이 문자로 반환한다.** */
  blankToken: string;
  /** 값 선택 목록이 1회에 제공하는 최대 종류 수. 초과하면 절단되어 도달한다. */
  maxValues: number;
  /** 한 페이지에 수령 가능한 최대 행 수. */
  maxPage: number;
}

let limitsCache: Limits | null = null;

/**
 * wasm 을 기동한다. **1회만 기동한다** — 2회 호출하면 두 벌이 구성되고, 그중
 * 하나에 개방한 양식은 타 측에서 노출되지 않는다.
 */
let started: Promise<void> | null = null;
export function start(): Promise<void> {
  started ??= init().then(() => {
    limitsCache = JSON.parse(wasmLimits()) as Limits;
  });
  return started;
}

/**
 * 바탕이 제공한 값들. [`start`] 이후에만 호출한다.
 *
 * **복제 기재하지 않는다.** 공백 표식을 화면에 기재하면 바탕이 그것을 변경하는
 * 시점에 값 선택이 묵시적으로 0건이 된다.
 */
export function limits(): Limits {
  if (!limitsCache) throw new Error("limits read before start()");
  return limitsCache;
}

/**
 * 개방한 양식 하나.
 *
 * 반환하는 문자를 해석하는 작업까지 여기서 수행한다. 화면이 `JSON.parse` 를
 * 보유하면 경계가 위치마다 분산된다.
 */
export class Book {
  readonly #inner: WasmBook;

  private constructor(inner: WasmBook) {
    this.#inner = inner;
  }

  /** 양식 바이트를 수용한다. 무엇이 수록되었는지는 [`Book.listScreens`] 가 제시한다. */
  static async open(bytes: Uint8Array, name: string): Promise<Book> {
    await start();
    try {
      return new Book(new WasmBook(bytes, name));
    } catch (e) {
      throw thrown(e);
    }
  }

  #call<T>(what: () => string): Promise<T> {
    try {
      return Promise.resolve(JSON.parse(what()) as T);
    } catch (e) {
      // `JSON.parse` 의 중단과 바탕이 발생시킨 것을 구분하지 않는다. 어느 쪽이든
      // 수령한 문자를 화면이 판독할 수 없다는 의미이고, 양자 모두 묵시적으로
      // 통과시키지 않는다.
      return Promise.reject(thrown(e));
    }
  }

  /**
   * 경로로 개방한다. **브라우저에서는 중단한다** — 경로라는 개념이 부재하다.
   *
   * [`canPickFile`] 이 거짓이라 화면이 여기 도달하지 않는다. 그래도 반출하는
   * 사유는 양측의 반출 목록이 동일해야 하기 때문이다 — 상이하면 화면이 어느
   * 쪽에서 구동 중인지 분기해 판독하게 된다.
   */
  static openAt(path: string): Promise<Book> {
    return Promise.reject(new Error(`no paths in a browser: ${path}`));
  }

  listScreens<T>(request: unknown): Promise<T> {
    return this.#call<T>(() => this.#inner.listScreens(JSON.stringify(request)));
  }

  queryRows<T>(request: unknown): Promise<T> {
    return this.#call<T>(() => this.#inner.queryRows(JSON.stringify(request)));
  }

  columnValues<T>(request: unknown): Promise<T> {
    return this.#call<T>(() => this.#inner.columnValues(JSON.stringify(request)));
  }
}

/* ── 파일을 선택하는 방법 ───────────────────────────────────────── */

/**
 * **브라우저는 선택 대화상자를 보유하지 않는다.** 파일을 수령하는 경로는
 * `<input type=file>` 과 투입 둘뿐이고, 양자 모두 바이트로 도달한다.
 */
export function canPickFile(): boolean {
  return false;
}

/** 선택 대화상자. 브라우저에는 부재하므로 아무것도 반환하지 않는다. */
export function pickFile(): Promise<{ path: string; name: string } | null> {
  return Promise.resolve(null);
}
