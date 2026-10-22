/**
 * 표를 다루는 화면이 쓰는 것 전부. **경계는 여기 하나다.**
 *
 * goe-navi 의 같은 파일에서 옮겨 왔다. 거기서는 `invoke("query_rows", …)` 를
 * 불렀고 여기서는 열어 둔 `Book` 을 부르는데, 요청의 칸 이름이 같은 벌이라
 * **바뀐 것은 그 한 줄뿐**이다.
 */
import { shallowRef } from "vue";
import { Book, limits } from "@/lib/book";

export type ColumnKind = "text" | "number";
export type SortDir = "asc" | "desc";

export interface ColumnDef {
  key: string;
  label: string;
  kind: ColumnKind;
  sortable: boolean;
  filterable: boolean;
  /**
   * **알아보지 못한 열인가.** 바탕이 종류를 가리지 못한 채 짐작한 것.
   * 화면이 그것을 말해 줄 수 있어야 한다 — 짐작해서 숫자로 줄을 세워 놓고
   * 가만히 있으면 틀린 차례를 참으로 본다.
   */
  guessed?: boolean;
}

/**
 * 무엇으로 그리는 화면인가.
 *
 * **여기는 표뿐이다.** goe-navi 는 차트·검색·학생 화면을 정의 파일이 정했지만,
 * 보는 겹에는 그 파일이 없다 — **시트 하나가 조회 화면 하나**다. 그래도 칸을
 * 남겨 둔다. 코드가 자료의 값을 보고 화면을 고르지 않게 하는 자리이고,
 * 정의 파일을 들이는 날 늘어날 곳이 여기 하나다.
 */
export type ScreenKind = "grid";

export interface ScreenDef {
  id: string;
  kind: ScreenKind;
  title: string;
  description: string;
  /** 왼쪽 목록에서 묶이는 자리. 양식 하나가 한 묶음이라 그 파일 이름이다. */
  group: string;
  columns: ColumnDef[];
  /**
   * 머리글을 찾았는가. **못 찾은 것과 줄이 없는 것은 다르다** — 화면이 그
   * 까닭을 말할 수 있어야 한다.
   */
  headerFound: boolean;
}

export interface Sort {
  key: string;
  dir: SortDir;
}

export interface Filter {
  key: string;
  /** 열 검색(부분일치). 빈 글자면 걸지 않는다. */
  contains: string;
  /** 엑셀식 값 선택. null 이면 전체 허용, 빈 배열이면 아무것도 지나지 않는다. */
  values: string[] | null;
}

/** 값 선택 목록의 한 항목. */
export interface ColumnValue {
  value: string;
  count: number;
}

export interface ColumnValuesResult {
  values: ColumnValue[];
  /** 상한을 넘어 잘렸는지. 상한은 바탕이 말한다(`limits().maxValues`). */
  truncated: boolean;
  elapsedMs: number;
}

/** 셀 값. 바탕이 열 차례대로 배열에 담아 보낸다. */
export type Cell = string | number | null;
export type Row = Cell[];

interface QueryResponse {
  total: number;
  offset: number;
  rows: Row[];
  elapsedMs: number;
}

/** 양식을 연 직후 화면이 알아야 하는 것 전부. */
export interface ScreenCatalog {
  groups: string[];
  footGroups: string[];
  /** 값이 없는 칸에 양식이 적어 두는 글자. */
  blank: string;
  /**
   * 숫자 칸의 소수 자릿수. **보는 겹은 모른다**(`null`) — 양식의 값을 고치지
   * 않으므로 정할 자리가 없다. [`cellText`] 가 그때 반올림을 하지 않는다.
   */
  decimals: number | null;
  screens: ScreenDef[];
}

/* ── 열어 둔 양식 ──────────────────────────────────────────────── */

/**
 * 지금 열려 있는 양식.
 *
 * **한 자리에 둔다.** goe-navi 는 자료가 앱 안에 있어 아무 때나 부를 수
 * 있었지만, 여기서는 사람이 파일을 고른 뒤에야 바탕이 선다. 화면마다 책을
 * 들고 다니게 하면 어느 화면은 받고 어느 화면은 못 받는 식으로 갈라진다.
 */
// `shallowRef` 다. `ref` 는 속을 들여다보며 감싸는데, 감싼 것은 더 이상
// `Book` 이 아니다 — 경계를 숨겨 둔 자리가 함께 벗겨진다.
const open = shallowRef<Book | null>(null);

/** 연 양식이 아직 없는데 부른 자리. **조용히 빈 쪽을 돌려주지 않는다.** */
function book(): Book {
  const b = open.value;
  if (!b) throw new Error("no workbook is open");
  return b;
}

/** 양식 바이트를 받아 연다. 앞서 열어 둔 것은 버린다. */
export async function openBook(bytes: Uint8Array, name: string): Promise<ScreenCatalog> {
  const b = await Book.open(bytes, name);
  open.value = b;
  const catalog = await b.listScreens<ScreenCatalog>();
  setBlankMark(catalog.blank);
  setCellDecimals(catalog.decimals);
  return catalog;
}

export function listScreens(): Promise<ScreenCatalog> {
  return book().listScreens<ScreenCatalog>();
}

/* ── 빈칸과 자릿수 ─────────────────────────────────────────────── */

/**
 * 양식이 "없음" 을 적을 때 쓰는 글자. 양식을 열면서 한 번 정해진다.
 *
 * **화면 곳곳에 `"-"` 를 적지 않는다.** 양식이 `–` 나 `N/A` 로 바뀌면 그
 * 값들이 멀쩡한 자료로 보여, 표에는 `-` 가 뜨고 세는 쪽은 그것을 값으로 읽는
 * 식으로 갈라진다.
 */
let blankMark = "-";

export function setBlankMark(v: string): void {
  if (v) blankMark = v;
}

/** 값이 비어 있는가. NULL·빈 글자·양식의 "없음" 표기. */
export function isBlank(v: unknown): boolean {
  return v === null || v === undefined || v === "" || v === blankMark;
}

/**
 * 숫자 칸의 소수 자릿수. **여기에 숫자를 적지 않는다.**
 *
 * 보는 겹에서는 `null` 로 온다 — 양식의 값을 고치지 않으므로 자릿수를 정할
 * 자리가 없다. 그때는 **반올림하지 않고** 양식에 있는 그대로 그린다. 0 으로
 * 메우면 양식의 소수가 조용히 사라지고, 그것은 화면 어디에서도 드러나지 않는다.
 */
let decimals: number | null = null;

export function setCellDecimals(n: number | null): void {
  decimals = n;
}

const cellFormat = new Intl.NumberFormat("ko-KR");

/** 글자를 만드는 데 필요한 만큼의 열 정보. */
export interface CellShape {
  kind: ColumnKind;
}

/**
 * 칸에 실제로 그려지는 글자.
 *
 * **표·인쇄·너비 재는 쪽이 모두 이것을 쓴다.** 사본을 두면 종이와 화면이 다른
 * 글자를 낸다. 너비를 잴 때도 그리는 글자와 달라지면 그만큼 좁게 잡힌다.
 */
export function cellText(col: CellShape, value: unknown): string {
  if (isBlank(value)) return "–";
  if (col.kind === "number") {
    const n = typeof value === "number" ? value : Number(value);
    if (Number.isNaN(n)) return String(value);
    // 자릿수를 아는 앱만 반올림한다. 모르면 양식의 값 그대로다.
    if (decimals === null) return cellFormat.format(n);
    const p = 10 ** decimals;
    return cellFormat.format(Math.round(n * p) / p);
  }
  return String(value);
}

/* ── 조회 ──────────────────────────────────────────────────────── */

/** 한 쪽에 담는 줄 수. 바탕의 상한을 넘지 않는다. */
export function pageSize(): number {
  return limits().maxPage;
}

/**
 * 한 번에 인쇄할 수 있는 최대 줄 수. A4 가로 기준 약 50쪽.
 *
 * **한자리에 둔다.** 화면마다 따로 적으면 같은 인쇄 단추가 화면에 따라 다른
 * 양을 뽑는다. 아무도 모르게.
 */
export const PRINT_LIMIT = 2000;

export interface PageResult {
  rows: Row[];
  /** 필터를 적용한 전체 줄 수. */
  total: number;
  elapsedMs: number;
}

/**
 * 한 쪽을 가져온다.
 *
 * 요청이 **무슨 표를 보는지 들고 간다**(`screenId`). 겹이 기억하게 두면 표를
 * 바꾼 뒤 늦게 온 앞 요청의 답이 새 표의 것처럼 그려진다. 오류는 나지 않는다.
 */
export async function fetchPage(
  screenId: string,
  sorts: Sort[],
  filters: Filter[],
  page: number,
  size: number = pageSize(),
): Promise<PageResult> {
  const limit = Math.min(size, pageSize());
  const res = await book().queryRows<QueryResponse>({
    screenId,
    sorts,
    filters,
    offset: page * limit,
    limit,
  });
  return { rows: res.rows, total: res.total, elapsedMs: res.elapsedMs };
}

export interface FetchAllResult {
  rows: Row[];
  total: number;
  /** 상한에 걸려 일부만 가져왔는지. */
  truncated: boolean;
}

/**
 * 지금 조건의 결과를 `max` 줄까지 한꺼번에 가져온다. 인쇄처럼 화면 밖 자료까지
 * 필요한 경우에만 쓴다 — 12만 줄을 통째로 받으면 안 되므로 부르는 쪽이 반드시
 * 상한을 준다.
 */
export async function fetchRows(
  screenId: string,
  sorts: Sort[],
  filters: Filter[],
  max: number,
): Promise<FetchAllResult> {
  const page = pageSize();
  const rows: Row[] = [];
  let total = 0;
  for (let offset = 0; offset < max; offset += page) {
    const res = await book().queryRows<QueryResponse>({
      screenId,
      sorts,
      filters,
      offset,
      limit: Math.min(page, max - offset),
    });
    total = res.total;
    rows.push(...res.rows);
    if (res.rows.length === 0 || rows.length >= total) break;
  }
  return { rows, total, truncated: total > rows.length };
}

/**
 * 한 열의 고유값 목록. 그 열 자신의 필터는 바탕이 빼므로, 체크를 풀어도 후보가
 * 사라지지 않는다.
 *
 * `needle` 은 후보를 좁히는 글자다. **받은 목록을 화면이 걸러 보여 주는 것으로는
 * 안 된다** — 값이 많으면 바탕이 앞에서 잘라 보내므로, 찾는 값이 그 뒤에 있으면
 * "없습니다" 가 뜬다.
 */
export function columnValues(
  screenId: string,
  columnKey: string,
  filters: Filter[],
  needle: string,
): Promise<ColumnValuesResult> {
  return book().columnValues<ColumnValuesResult>({ screenId, columnKey, filters, needle });
}

/**
 * 머리글을 눌렀을 때 정렬을 한 단계 굴린다.
 *   없음 → 오름차순 → 내림차순 → 없음
 * 새로 들어간 열은 **맨 뒤 우선순위**로 붙는다. 처음 누른 열이 1순위다.
 */
export function cycleSort(sorts: Sort[], key: string): Sort[] {
  const at = sorts.findIndex((s) => s.key === key);
  if (at === -1) return [...sorts, { key, dir: "asc" }];
  if (sorts[at].dir === "asc") {
    const next = [...sorts];
    next[at] = { key, dir: "desc" };
    return next;
  }
  return sorts.filter((s) => s.key !== key);
}
