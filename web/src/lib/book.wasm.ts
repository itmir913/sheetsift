/**
 * **경계 — 브라우저 벌.** 화면과 바탕(wasm) 사이는 여기 단일 위치로만 통과한다.
 *
 * 창에서 구동할 때는 `book.tauri.ts` 가 그 위치를 담당한다. 양자는 **동일한
 * 것을 반출해야** 하고(`crates/shell/tests/screen.rs` 가 대조한다), 화면은
 * `sheetsift-book` 이라는 명칭으로 반입해 어느 쪽인지 인지하지 않는다.
 *
 * goe-navi 는 Tauri 명령을 호출하고 이쪽은 wasm 을 호출하지만, 문 명칭도
 * 요청의 항목 명칭도 동일한 벌이라 **변경되는 것은 호출 대상뿐**이다. 화면
 * 코드는 양측에서 동일한 것을 참조한다.
 *
 * **wasm 을 여기서 호출하지 않는다.** 그것은 동기라 호출하면 반환할 때까지 그
 * 스레드를 점유하고, 주 흐름에서 점유하면 수 초간 화면이 묘사도 조작도 수령하지
 * 못한다. 그래서 작업자(`book.worker.ts`)로 이전하고 여기서는 요청과 응답의
 * 문자만 주고받는다 — 창 벌이 명령 겹을 호출하는 것과 동일한 분업이고, 양측이
 * 공히 비동기라 화면이 대기하는 구조도 한 벌이다.
 */
import type { WorkerAsk, WorkerReply } from "./book.worker";
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

/* ── 작업자 ────────────────────────────────────────────────────── */

/**
 * 바탕을 수행하는 작업자. **1회만 구성한다** — 2회 구성하면 두 벌이 생성되고,
 * 그중 하나에 개방한 양식은 타 측에서 노출되지 않는다.
 *
 * `import.meta.url` 기준으로 지시한다. 묶는 쪽이 그 형태를 인지해 별도 산출물로
 * 컴파일한다 — 경로를 글자로 기재하면 컴파일된 위치와 괴리한다.
 */
let worker: Worker | null = null;
function hired(): Worker {
  worker ??= new Worker(new URL("./book.worker.ts", import.meta.url), { type: "module" });
  return worker;
}

/** 회신을 대기하는 요청들. 작업자는 수령한 순서로 회신하지 않을 수 있다. */
let nextId = 0;
const waiting = new Map<number, { resolve: (value: string) => void; reject: (e: unknown) => void }>();

/**
 * 작업자에게 1건을 요청하고 회신을 대기한다.
 *
 * **중단한 것도 회신으로 도달한다.** 작업자의 예외는 주 흐름의 예외가 아니라,
 * 그쪽에서 포착해 `ok: false` 로 운반한 것을 여기서 재차 발생시킨다.
 */
function ask(request: Omit<WorkerAsk, "id">, transfer?: Transferable[]): Promise<string> {
  const hand = hired();
  if (waiting.size === 0) {
    hand.onmessage = (ev: MessageEvent<WorkerReply>) => {
      const pending = waiting.get(ev.data.id);
      if (!pending) return;
      waiting.delete(ev.data.id);
      if (ev.data.ok) pending.resolve(ev.data.value);
      else pending.reject(thrown(ev.data.error));
    };
    /* 작업자 자체가 적재되지 못한 경우다. 그대로 두면 요청한 쪽이 영구히
       대기하므로, 대기 중인 것을 전부 중단시킨다. */
    hand.onerror = (ev) => {
      const failed = new AppError({ code: "UNKNOWN", detail: String(ev.message ?? ev) });
      for (const [id, pending] of waiting) {
        waiting.delete(id);
        pending.reject(failed);
      }
    };
  }
  const id = nextId++;
  return new Promise<string>((resolve, reject) => {
    waiting.set(id, { resolve, reject });
    hand.postMessage({ ...request, id } as WorkerAsk, transfer ?? []);
  });
}

/** 회신한 문자를 해석한다. 해석하지 못한 것은 묵시적으로 통과시키지 않는다. */
function parsed<T>(text: string): T {
  try {
    return JSON.parse(text) as T;
  } catch (e) {
    throw thrown(e);
  }
}

/**
 * 작업자를 기동한다. **1회만 기동한다.** 상한은 그 회신으로 수령한다.
 */
let started: Promise<void> | null = null;
export function start(): Promise<void> {
  started ??= ask({ op: "start" }).then((text) => {
    limitsCache = parsed<Limits>(text);
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
 * **양식은 작업자가 보유한다.** 화면이 보유하는 것은 그것을 지시하는 손잡이
 * 뿐이라 항목이 부재하다 — 창 벌과 동일한 구조이고, 그래서 화면이 양측에서
 * 동일한 코드를 경유한다.
 */
export class Book {
  private constructor() {}

  /**
   * 양식 바이트를 수용한다. 무엇이 수록되었는지는 [`Book.listScreens`] 가 제시한다.
   *
   * **바이트의 소유를 이전한다**(transfer). 복제하면 수십 MB 양식이 1회 더
   * 적재되고, 그 복제본은 주 흐름에 잔류한다. 이전한 이후 투입한 쪽의 바이트는
   * 길이 0 이 되는데, 개방하는 문은 그것을 재참조하지 않는다.
   */
  static async open(bytes: Uint8Array, name: string): Promise<Book> {
    await start();
    await ask({ op: "open", bytes, name }, [bytes.buffer]);
    return new Book();
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

  async listScreens<T>(request: unknown): Promise<T> {
    return parsed<T>(await ask({ op: "listScreens", request: JSON.stringify(request) }));
  }

  async queryRows<T>(request: unknown): Promise<T> {
    return parsed<T>(await ask({ op: "queryRows", request: JSON.stringify(request) }));
  }

  async columnValues<T>(request: unknown): Promise<T> {
    return parsed<T>(await ask({ op: "columnValues", request: JSON.stringify(request) }));
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
