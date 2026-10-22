/**
 * **경계.** 화면과 바탕(wasm) 사이는 여기 한 자리로만 넘는다.
 *
 * goe-navi 는 Tauri 명령을 부르고 이쪽은 wasm 메서드를 부르지만, 문 이름도
 * 요청의 칸 이름도 같은 벌이라 **바뀌는 것은 부르는 대상뿐**이다. 화면 코드는
 * 두 곳에서 같은 것을 본다.
 *
 * 다른 점 하나: Tauri 는 비동기고 wasm 은 그 자리에서 답한다. 그래도 `Promise`
 * 로 맞춰 둔다 — 화면이 기다리는 모양까지 두 벌이 되면, 옮겨 올 때마다
 * `await` 를 지우고 다시 붙이게 된다.
 */
import init, { Book as WasmBook, limits as wasmLimits } from "sheetsift-wasm";
import type { AppErrorPayload, ErrorCode } from "@/i18n/codes";
import { formatError } from "@/i18n";

/** 바탕의 오류를 감싼 예외. 화면은 `message` 를 그대로 보여 주면 된다. */
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
 * wasm 이 던진 것을 `AppError` 로.
 *
 * 겹은 오류를 **JSON 글자**로 던진다. 글자로 바꾸지 못하는 자리까지 가면
 * `UNKNOWN` 이 온다. 풀지 못한 것은 삼키지 않고 원문을 `detail` 에 담는다 —
 * 개발자만 보는 자리이고, 여기서 잃으면 다시 찾을 길이 없다.
 */
function thrown(raw: unknown): AppError {
  const text = typeof raw === "string" ? raw : String((raw as Error)?.message ?? raw);
  try {
    const payload = JSON.parse(text) as AppErrorPayload;
    if (payload && typeof payload.code === "string") return new AppError(payload);
  } catch {
    /* JSON 이 아니면 아래로 떨어진다. */
  }
  return new AppError({ code: "UNKNOWN", detail: text });
}

/** 바탕이 내주는, 화면이 손으로 적으면 안 되는 값들. */
export interface Limits {
  /** 값이 없는 칸을 값 선택 목록에 적는 글자. **거를 때도 이 글자로 돌려준다.** */
  blankToken: string;
  /** 값 선택 목록이 한 번에 주는 최대 가짓수. 넘으면 잘려 온다. */
  maxValues: number;
  /** 한 쪽에 받을 수 있는 최대 줄 수. */
  maxPage: number;
}

let limitsCache: Limits | null = null;

/**
 * wasm 을 켠다. **한 번만 켠다** — 두 번 부르면 두 벌이 서고, 그중 하나에
 * 열어 둔 양식은 다른 쪽에서 보이지 않는다.
 */
let started: Promise<void> | null = null;
export function start(): Promise<void> {
  started ??= init().then(() => {
    limitsCache = JSON.parse(wasmLimits()) as Limits;
  });
  return started;
}

/**
 * 바탕이 내준 값들. [`start`] 뒤에만 부른다.
 *
 * **베껴 적지 않는다.** 빈칸 표식을 화면에 적어 두면 바탕이 그것을 바꾸는 날
 * 값 선택이 조용히 0건이 된다.
 */
export function limits(): Limits {
  if (!limitsCache) throw new Error("limits read before start()");
  return limitsCache;
}

/**
 * 열어 둔 양식 하나.
 *
 * 돌려주는 글자를 푸는 일까지 여기서 한다. 화면이 `JSON.parse` 를 들고 있으면
 * 경계가 자리마다 흩어진다.
 */
export class Book {
  readonly #inner: WasmBook;

  private constructor(inner: WasmBook) {
    this.#inner = inner;
  }

  /** 양식 바이트를 받는다. 무엇이 들었는지는 [`Book.listScreens`] 가 말한다. */
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
      // `JSON.parse` 가 선 것과 바탕이 던진 것을 가르지 않는다. 어느 쪽이든
      // 받은 글자를 화면이 읽을 수 없다는 뜻이고, 둘 다 조용히 넘기지 않는다.
      return Promise.reject(thrown(e));
    }
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
