/**
 * **경계 — 창 벌.** 화면과 바탕(tauri 명령) 사이는 여기 단일 위치로만 통과한다.
 *
 * 브라우저에서 구동할 때는 `book.wasm.ts` 가 그 위치를 담당한다. 양자는
 * **동일한 것을 반출해야** 하고(`crates/shell/tests/screen.rs` 가 대조한다),
 * 화면은 `sheetsift-book` 이라는 명칭으로 반입해 어느 쪽인지 인지하지 않는다.
 *
 * 문 명칭과 요청의 항목 명칭은 양측이 동일한 벌이다. 상이한 것은 **개방하는
 * 위치 하나** — 창은 경로를 수령해 직접 판독한다. 바이트를 화면으로 경유시키면
 * 파일 전체가 1회 적재되어, 데스크톱이 해소하려는 제약이 그대로 잔존한다.
 */
import { invoke } from "@tauri-apps/api/core";
import { open as pick } from "@tauri-apps/plugin-dialog";
import type { AppErrorPayload, ErrorCode } from "@/i18n/codes";
import { formatError, t } from "@/i18n";

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
 * 명령이 중단시킨 것을 `AppError` 로.
 *
 * tauri 는 **구조 그대로** 전달한다 — 브라우저 껍질처럼 JSON 문자로 포장하지
 * 않는다. 해석하지 못한 것은 은폐하지 않고 원문을 `detail` 에 수용한다.
 */
function thrown(raw: unknown): AppError {
  if (raw && typeof raw === "object" && typeof (raw as AppErrorPayload).code === "string") {
    return new AppError(raw as AppErrorPayload);
  }
  return new AppError({ code: "UNKNOWN", detail: String((raw as Error)?.message ?? raw) });
}

function call<T>(door: string, body: Record<string, unknown> = {}): Promise<T> {
  return invoke<T>(door, body).catch((e) => {
    throw thrown(e);
  });
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
 * 상한을 1회 수령한다. 창은 기동할 대상이 부재하나(명령은 이미 실재한다),
 * 화면이 양측에서 동일한 순서로 진행해야 하므로 동일한 문을 배치한다.
 */
let started: Promise<void> | null = null;
export function start(): Promise<void> {
  started ??= call<Limits>("limits").then((l) => {
    limitsCache = l;
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
 * 창은 양식을 **명령 겹이 보유한다.** 화면이 보유하는 것은 그것을 지시하는
 * 손잡이뿐이라 항목이 부재하다 — 그래도 동일한 구조로 배치하는 사유는, 화면이
 * 양측에서 동일한 코드를 경유해야 하기 때문이다.
 */
export class Book {
  private constructor() {}

  /** 양식 바이트를 수용한다. 창에 투입한 파일이 그 경로다. */
  static async open(bytes: Uint8Array, name: string): Promise<Book> {
    await start();
    await call<void>("open_bytes", { bytes: Array.from(bytes), name });
    return new Book();
  }

  /** **경로로 개방한다.** 선택 대화상자가 반환한 것이 그것이다. */
  static async openAt(path: string): Promise<Book> {
    await start();
    await call<void>("open_workbook", { path });
    return new Book();
  }

  listScreens<T>(request: unknown): Promise<T> {
    return call<T>("list_screens", { request });
  }

  queryRows<T>(request: unknown): Promise<T> {
    return call<T>("query_rows", { request });
  }

  columnValues<T>(request: unknown): Promise<T> {
    return call<T>("column_values", { request });
  }
}

/* ── 파일을 선택하는 방법 ───────────────────────────────────────── */

/** 창은 운영체제의 선택 대화상자를 보유한다. */
export function canPickFile(): boolean {
  return true;
}

/**
 * 선택 대화상자. **경로만 반환한다.**
 *
 * 폐쇄하면 `null` 이다 — 그것을 오류로 전환하지 않는다. 사람이 선택하지 않은
 * 것은 실패가 아니다.
 */
export async function pickFile(): Promise<{ path: string; name: string } | null> {
  const chosen = await pick({
    multiple: false,
    directory: false,
    filters: [{ name: t("pick.filter"), extensions: ["xlsx", "xls", "csv"] }],
  });
  if (typeof chosen !== "string") return null;
  /* 경로의 최종 조각이 파일 명칭이다. **구분자 양자를 전부 분리한다** —
     윈도의 대화상자는 `C:\…\a.xlsx` 를 반환하므로 슬래시만 분리하면 명칭이
     전체 경로가 되고, 그것이 머리글에 묘사된다. 거기에는 사용자 명칭이
     포함되고, 그 화면은 갈무리되어 외부로 반출된다. */
  const name = chosen.split(/[\\/]/).pop() || chosen;
  return { path: chosen, name };
}
