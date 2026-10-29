/**
 * **받는 앱이 주입하는 것 전부.** 이 패키지는 문구·저장소 명칭·자료의 문·오류의
 * 문구를 보유하지 않는다.
 *
 * 공용 화면은 두 제품(sheetsift · goe-navi)이 동일하게 사용한다. 제품마다 상이한
 * 것을 여기 기재하면 한쪽 제품의 사정이 타 제품의 화면에 출현한다. 그래서
 * 그것들은 받는 앱이 [`installSharedUi`](../index.ts) 로 1회 주입하고, 화면은
 * 여기 문을 경유해서만 참조한다.
 *
 * **주입하지 않은 채로 호출하면 정지한다.** 기본값을 배치하면 주입을 누락한
 * 앱이 타 제품의 저장소 명칭이나 공백 문구로 묵시적으로 작동하고, 그것은
 * 어디에서도 오류로 발현되지 않는다.
 */
import type { ColumnValuesResult, Filter, HeaderSpec, QueryResponse, Sort } from "./grid";

/* ── 문구 ──────────────────────────────────────────────────────── */

/**
 * 공용 화면이 수에 따라 형태가 변하는 문구로 사용하는 열쇠.
 *
 * 사전에는 `<열쇠>_one`·`<열쇠>_other` 쌍으로 기재된다([`SharedUiKey`]).
 */
export type SharedPluralKey =
  | "filter.excluded"
  | "filter.selected"
  | "filter.truncated"
  | "grid.rows"
  | "pager.of"
  | "print.oftotal"
  | "print.rows"
  | "print.truncated";

/**
 * **공용 화면이 사용하는 문구의 열쇠 전부.** 받는 앱의 사전은 이것을 전부 충족해야
 * 한다 — `satisfies Record<SharedUiKey, string>` 로 컴파일 시점에 확인한다.
 *
 * 열쇠만 정의하고 문구는 정의하지 않는다. 표기 규칙이 제품마다 상이하고, 그것은
 * 각자의 사전이 결정한다.
 */
export type SharedUiKey =
  | `${SharedPluralKey}_one`
  | `${SharedPluralKey}_other`
  | "column.letter"
  | "filter.all"
  | "filter.apply"
  | "filter.blank"
  | "filter.cancel"
  | "filter.checkall"
  | "filter.checkall.shown"
  | "filter.loading"
  | "filter.none"
  | "filter.search"
  | "grid.empty"
  | "grid.filter.aria"
  | "grid.filter.tip"
  | "grid.head.tip"
  | "grid.resize.aria"
  | "grid.search.aria"
  | "grid.search.ph"
  | "grid.status"
  | "pager.first"
  | "pager.last"
  | "pager.next"
  | "pager.page.aria"
  | "pager.prev"
  | "print.asc"
  | "print.contains"
  | "print.desc"
  | "print.count"
  | "print.excluded"
  | "print.filters"
  | "print.more"
  | "print.no"
  | "print.printed"
  | "print.selected.none"
  | "print.sorts"
  | "theme.aria"
  | "theme.dark"
  | "theme.light"
  | "theme.system"
  | "tools.columns.reset"
  | "tools.columns.reset.tip"
  | "tools.filters.clear"
  | "tools.grouping"
  | "tools.grouping.tip"
  | "tools.header"
  | "tools.header.down.aria"
  | "tools.header.down.tip"
  | "tools.header.less.aria"
  | "tools.header.less.tip"
  | "tools.header.more.aria"
  | "tools.header.more.tip"
  | "tools.header.range"
  | "tools.header.single"
  | "tools.header.up.aria"
  | "tools.header.up.tip"
  | "tools.help"
  | "tools.help.text"
  | "tools.pin"
  | "tools.pin.less.aria"
  | "tools.pin.more.aria"
  | "tools.print"
  | "tools.print.busy"
  | "tools.print.tip"
  | "tools.sort.remove.tip"
  | "tools.sorts"
  | "tools.sorts.clear";

export type MessageParams = Record<string, string | number>;

/**
 * 받는 앱의 문구 함수들.
 *
 * **함수 항목으로 기재한다**(메서드 표기가 아니다). 메서드 표기는 인자의 타입을
 * 양방향으로 수용해, 공용 열쇠를 인지하지 못하는 `t` 를 넘겨도 컴파일이 통과한다.
 *
 * 반응형으로 작동해야 한다 — 언어를 전환하면 화면이 재묘사되도록, 받는 앱의
 * 함수가 자체 언어 상태를 참조하는 그대로 넘긴다.
 */
export interface SharedUiMessages {
  t: (key: SharedUiKey, params?: MessageParams) => string;
  tn: (key: SharedPluralKey, n: number, params?: MessageParams) => string;
  /** 화면이 산출한 개수. 셀 값은 이것을 사용하지 않는다(`cellText`). */
  formatNumber: (n: number) => string;
  /** 인쇄 시각처럼 화면이 산출하는 일시. */
  formatDateTime: (d: Date) => string;
}

/* ── 자료의 문 ─────────────────────────────────────────────────── */

/** 바탕이 제공하는, 화면이 임의로 기재하면 안 되는 값들. */
export interface GridLimits {
  /** 값이 부재하는 셀을 값 선택 목록에 기재하는 문자. 여과할 때도 이 문자로 반환한다. */
  blankToken: string;
  /** 값 선택 목록이 1회에 제공하는 최대 종류 수. */
  maxValues: number;
  /** 한 페이지에 수령 가능한 최대 행 수. */
  maxPage: number;
}

/** 행을 요청하는 항목. 바탕의 요청과 동일한 벌이다. */
export interface QueryRequest {
  screenId: string;
  header: HeaderSpec | null;
  sorts: Sort[];
  filters: Filter[];
  offset: number;
  limit: number;
}

/** 값 선택 목록을 요청하는 항목. */
export interface ColumnValuesRequest {
  screenId: string;
  header: HeaderSpec | null;
  columnKey: string;
  filters: Filter[];
  needle: string;
}

/**
 * 표가 자료를 수령하는 문. **어디서 수령하는지 화면은 인지하지 않는다.**
 *
 * 발생시키는 오류는 [`SharedUiErrors`] 가 판독할 수 있는 것이어야 한다.
 */
export interface GridSource {
  limits: () => GridLimits;
  queryRows: (req: QueryRequest) => Promise<QueryResponse>;
  columnValues: (req: ColumnValuesRequest) => Promise<ColumnValuesResult>;
}

/* ── 오류 ──────────────────────────────────────────────────────── */

/**
 * 포착한 오류를 문구로 전환하는 방법.
 *
 * **문구가 아니라 코드를 보유하게 분리한다**(`useFault`). `recognize` 가 바탕의
 * 오류에서 코드와 치환 값을 추출하고, `format` 이 묘사하는 시점에 문구를
 * 산출한다 — 언어를 전환하면 이미 표시된 오류도 추종한다.
 *
 * 바탕에서 유래하지 않은 것은 `recognize` 가 `undefined` 를 반환하고, 화면은
 * 원문을 그대로 표시한다.
 */
export interface SharedUiErrors {
  recognize: (e: unknown) => unknown;
  format: (payload: unknown) => string;
}

/* ── 보관 ──────────────────────────────────────────────────────── */

let messages: SharedUiMessages | null = null;
let gridSource: GridSource | null = null;
let errorCodec: SharedUiErrors | null = null;
let prefix: string | null = null;
let name: string | null = null;

function missing(what: string): never {
  throw new Error(`sheetsift-ui: ${what} was not provided — call installSharedUi first`);
}

export function provideMessages(m: SharedUiMessages): void {
  messages = m;
}

export function provideGridSource(s: GridSource): void {
  gridSource = s;
}

export function provideErrors(e: SharedUiErrors): void {
  errorCodec = e;
}

/**
 * 브라우저 저장소 키의 접두사. **제품마다 상이해야 한다** — 동일한 위치에 두
 * 제품이 배포되어도 테마·열 폭 기억이 혼합되지 않게 한다.
 */
export function provideStoragePrefix(p: string): void {
  if (!p) throw new Error("sheetsift-ui: the storage prefix must not be empty");
  prefix = p;
}

/** 인쇄 머리에 기재하는 제품의 명칭. */
export function provideAppName(n: string): void {
  name = n;
}

export function t(key: SharedUiKey, params?: MessageParams): string {
  return (messages ?? missing("messages")).t(key, params);
}

export function tn(key: SharedPluralKey, n: number, params?: MessageParams): string {
  return (messages ?? missing("messages")).tn(key, n, params);
}

export function formatNumber(n: number): string {
  return (messages ?? missing("messages")).formatNumber(n);
}

export function formatDateTime(d: Date): string {
  return (messages ?? missing("messages")).formatDateTime(d);
}

export function source(): GridSource {
  return gridSource ?? missing("the grid source");
}

export function errors(): SharedUiErrors {
  return errorCodec ?? missing("the error codec");
}

/** 저장소 키를 구성한다. 형태는 `<접두사>.<명칭>` 이다. */
export function storageKey(key: string): string {
  return `${prefix ?? missing("the storage prefix")}.${key}`;
}

export function appName(): string {
  return name ?? missing("the app name");
}
