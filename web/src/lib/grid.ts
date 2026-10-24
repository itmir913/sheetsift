/**
 * 표를 취급하는 화면이 사용하는 것 전부. **경계는 여기 하나다.**
 *
 * goe-navi 의 동일한 파일에서 이전했다. 거기서는 `invoke("query_rows", …)` 를
 * 호출했고 여기서는 개방한 `Book` 을 호출하는데, 요청의 항목 명칭이 동일한
 * 벌이라 **변경된 것은 그 한 행뿐**이다.
 */
import { ref, shallowRef } from "vue";
import { Book, limits } from "sheetsift-book";

export type ColumnKind = "text" | "number";
export type SortDir = "asc" | "desc";

export interface ColumnDef {
  key: string;
  label: string;
  kind: ColumnKind;
  sortable: boolean;
  filterable: boolean;
  /**
   * **판별하지 못한 열인가.** 바탕이 종류를 식별하지 못한 채 추정한 것.
   * 화면이 그것을 제시할 수 있어야 한다 — 추정해 숫자로 정렬해 놓고 침묵하면
   * 상이한 순서를 참으로 판정한다.
   */
  guessed?: boolean;
}

/**
 * 무엇으로 묘사하는 화면인가.
 *
 * **여기는 표뿐이다.** goe-navi 는 차트·검색·학생 화면을 정의 파일이 결정했으나,
 * 열람 겹에는 그 파일이 부재한다 — **시트 하나가 조회 화면 하나**다. 그래도
 * 항목을 존치한다. 코드가 자료의 값을 참조해 화면을 선정하지 않게 하는 위치이고,
 * 정의 파일을 도입하는 시점에 증가할 지점이 여기 하나다.
 */
export type ScreenKind = "grid";

/**
 * 머리글이 어디서 몇 줄인가. **사용자가 조작부에서 지정한다.**
 *
 * 추정으로 전부 적중시킬 수 없다 — 열마다 단수가 상이하거나(`학년` 은 1단,
 * `국어 등급` 은 2단) 상단에 제목 행이 부착된 양식이 실재한다. 바탕은 추정에
 * 실패하면 실패했다고 반환하고, 결정은 사람이 수행한다.
 */
export interface HeaderSpec {
  /** 머리글이 개시되는 행. 0부터 산정한다. */
  at: number;
  /** 머리글 행 수. 1 이상. */
  rows: number;
}

export interface ScreenDef {
  id: string;
  kind: ScreenKind;
  title: string;
  description: string;
  /** 좌측 목록에서 결속되는 위치. 양식 하나가 한 묶음이라 그 파일 명칭이다. */
  group: string;
  columns: ColumnDef[];
  /**
   * 머리글을 탐색했는가. **탐색하지 못한 것과 행이 부재하는 것은 상이하다** —
   * 화면이 그 사유를 제시할 수 있어야 한다.
   */
  headerFound: boolean;
  /** 적용된 머리글 지정. 판별에 실패하면 null 이다. */
  header: HeaderSpec | null;
  /** 그 지정이 **추정**인가. 참이면 사용자가 확인하지 않은 값이다. */
  headerGuessed: boolean;
  /** 시트의 총 행 수. 조작부가 지정 가능한 범위의 상한이다. */
  sheetRows: number;
  /**
   * 이 시트가 양식에서 개시되는 **행**(0부터).
   *
   * 선두의 공백 행은 소거된 상태로 도달한다. 조작부가 머리글 위치를 **엑셀의
   * 행 번호로** 표시하려면 이것을 가산해야 한다.
   */
  rowOrigin: number;
}

export interface Sort {
  key: string;
  dir: SortDir;
}

export interface Filter {
  key: string;
  /** 열 검색(부분일치). 공백 문자면 적용하지 않는다. */
  contains: string;
  /** 엑셀식 값 선택. null 이면 전체 허용, 공백 배열이면 아무것도 통과하지 않는다. */
  values: string[] | null;
}

/** 값 선택 목록의 한 항목. */
export interface ColumnValue {
  value: string;
  count: number;
}

export interface ColumnValuesResult {
  values: ColumnValue[];
  /** 상한을 초과해 절단되었는지. 상한은 바탕이 제시한다(`limits().maxValues`). */
  truncated: boolean;
  elapsedMs: number;
}

/** 셀 값. 바탕이 열 순서대로 배열에 수용해 전송한다. */
export type Cell = string | number | null;
export type Row = Cell[];

interface QueryResponse {
  total: number;
  offset: number;
  rows: Row[];
  elapsedMs: number;
}

/** 양식 개방 직후 화면이 인지해야 하는 것 전부. */
export interface ScreenCatalog {
  groups: string[];
  footGroups: string[];
  /** 값이 부재하는 셀에 양식이 기재하는 문자. */
  blank: string;
  /**
   * 숫자 셀의 소수 자릿수. **열람 겹은 인지하지 못한다**(`null`) — 양식의 값을
   * 변경하지 않으므로 결정할 위치가 부재한다. [`cellText`] 가 그때 반올림을
   * 수행하지 않는다.
   */
  decimals: number | null;
  screens: ScreenDef[];
}

/* ── 개방한 양식 ──────────────────────────────────────────────── */

/**
 * 현재 개방된 양식.
 *
 * **단일 위치에 배치한다.** goe-navi 는 자료가 앱 내부에 존재해 임의 시점에
 * 호출 가능했으나, 여기서는 사람이 파일을 선택한 이후에야 바탕이 구성된다.
 * 화면마다 책을 보유하게 하면 일부 화면은 수령하고 일부 화면은 수령하지 못하는
 * 식으로 분기한다.
 */
// `shallowRef` 다. `ref` 는 내부를 순회하며 포장하는데, 포장한 것은 더 이상
// `Book` 이 아니다 — 경계를 은폐한 위치가 함께 제거된다.
const open = shallowRef<Book | null>(null);

/** 개방한 양식이 아직 부재하는데 호출한 위치. **묵시적으로 공백 페이지를 반환하지 않는다.** */
function book(): Book {
  const b = open.value;
  if (!b) throw new Error("no workbook is open");
  return b;
}

/** 양식 바이트를 수용해 개방한다. 기존에 개방한 것은 폐기한다. */
export async function openBook(bytes: Uint8Array, name: string): Promise<ScreenCatalog> {
  return received(await Book.open(bytes, name));
}

/**
 * **경로로 개방한다.** 선택 대화상자가 반환한 것이 그것이다.
 *
 * 바이트로 개방하는 쪽과 동일한 위치로 귀착시킨다 — 개방 이후의 처리가 두 벌이
 * 되면 일방만 수정된다.
 */
export async function openBookAt(path: string): Promise<ScreenCatalog> {
  return received(await Book.openAt(path));
}

async function received(b: Book): Promise<ScreenCatalog> {
  open.value = b;
  const catalog = await b.listScreens<ScreenCatalog>({});
  setBlankMark(catalog.blank);
  setCellDecimals(catalog.decimals);
  return catalog;
}

/**
 * 화면 목록. 시트별 머리글 지정을 지참한다.
 *
 * 기재하지 않은 시트는 바탕이 추정한 것을 적용한다. 사용자가 한 시트의 지정을
 * 변경하면 그 시트를 기재해 재요청하고, 열 정의 전체를 교체한다.
 */
export function listScreens(
  headers: Record<string, HeaderSpec> = {},
): Promise<ScreenCatalog> {
  return book().listScreens<ScreenCatalog>({ headers });
}

/* ── 공백과 자릿수 ─────────────────────────────────────────────── */

/**
 * 양식이 "없음" 을 기재할 때 사용하는 문자. 양식 개방 시점에 1회 결정된다.
 *
 * **화면 각처에 `"-"` 를 기재하지 않는다.** 양식이 `–` 나 `N/A` 로 변경되면 그
 * 값들이 정상 자료로 인식되어, 표에는 `-` 가 표시되고 산정하는 측은 그것을
 * 값으로 판독하는 식으로 분기한다.
 */
let blankMark = "-";

export function setBlankMark(v: string): void {
  if (v) blankMark = v;
}

/** 값이 공백인가. NULL·공백 문자·양식의 "없음" 표기. */
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

/**
 * 숫자 칸에 천 단위 구분을 넣는가. **사람이 선택한다**(`useGrouping`).
 *
 * 어느 열이 식별자인지 이 앱은 판정하지 않는다 — 열 명칭이나 값의 생김새를
 * 참조해 분기하면 특정 양식 전용 규칙이 코드에 적재된다.
 *
 * **반응형이어야 한다.** 평범한 변수로 보유하면 선택을 변경해도 이미 묘사된
 * 표가 그대로 잔류한다 — 조작했는데 아무 일도 발생하지 않는 것처럼 보이고,
 * 오류는 어디에서도 발생하지 않는다.
 */
const grouping = ref(true);

export function setCellGrouping(on: boolean): void {
  grouping.value = on;
}

/* 구분 기호의 **형태**는 언어가 결정하고(1,000 · 1.000), 넣을지는 사람이
   결정한다. 양자를 혼동하지 않는다.

   자릿수를 절단하지 않는다. 기본값(3)으로 두면 양식의 소수가 묵시적으로
   반올림된다. */
function makeFormat(on: boolean): Intl.NumberFormat {
  return new Intl.NumberFormat(undefined, { useGrouping: on, maximumFractionDigits: 20 });
}

const formats = { on: makeFormat(true), off: makeFormat(false) };

/** 문자를 생성하는 데 필요한 만큼의 열 정보. */
export interface CellShape {
  kind: ColumnKind;
}

/**
 * 셀에 실제로 묘사되는 문자.
 *
 * **표·인쇄·폭 측정 측이 전부 이것을 사용한다.** 사본을 배치하면 지면과 화면이
 * 상이한 문자를 산출한다. 폭을 측정할 때도 묘사하는 문자와 상이하면 그만큼
 * 협소하게 산정된다.
 */
export function cellText(col: CellShape, value: unknown): string {
  if (isBlank(value)) return "–";
  if (col.kind === "number") {
    const n = typeof value === "number" ? value : Number(value);
    if (Number.isNaN(n)) return String(value);
    // 자릿수를 인지하는 앱만 반올림한다. 인지하지 못하면 양식의 값 그대로다.
    const cellFormat = grouping.value ? formats.on : formats.off;
    if (decimals === null) return cellFormat.format(n);
    const p = 10 ** decimals;
    return cellFormat.format(Math.round(n * p) / p);
  }
  return String(value);
}

/* ── 조회 ──────────────────────────────────────────────────────── */

/** 한 페이지에 수용하는 행 수. 바탕의 상한을 초과하지 않는다. */
export function pageSize(): number {
  return limits().maxPage;
}

/**
 * 1회에 인쇄 가능한 최대 행 수. A4 가로 기준 약 50면.
 *
 * **단일 위치에 배치한다.** 화면마다 개별 기재하면 동일한 인쇄 단추가 화면에
 * 따라 상이한 분량을 산출한다. 누구도 인지하지 못한 채.
 */
export const PRINT_LIMIT = 2000;

export interface PageResult {
  rows: Row[];
  /** 필터를 적용한 전체 행 수. */
  total: number;
  elapsedMs: number;
}

/**
 * 한 페이지를 수령한다.
 *
 * 요청이 **어느 표를 열람하는지 지참한다**(`screenId`). 겹이 기억하게 배치하면
 * 표를 변경한 뒤 지연 도착한 선행 요청의 응답이 신규 표의 것처럼 묘사된다.
 * 오류는 발생하지 않는다.
 */
export async function fetchPage(
  screenId: string,
  header: HeaderSpec | null,
  sorts: Sort[],
  filters: Filter[],
  page: number,
  size: number = pageSize(),
): Promise<PageResult> {
  const limit = Math.min(size, pageSize());
  const res = await book().queryRows<QueryResponse>({
    screenId,
    header,
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
  /** 상한에 포착되어 일부만 수령했는지. */
  truncated: boolean;
}

/**
 * 현 조건의 결과를 `max` 행까지 일괄 수령한다. 인쇄처럼 화면 외부 자료까지
 * 필요한 경우에만 사용한다 — 12만 행을 전체 수령하면 안 되므로 부르는 곳이
 * 반드시 상한을 지정한다.
 */
export async function fetchRows(
  screenId: string,
  header: HeaderSpec | null,
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
      header,
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
 * 한 열의 고유값 목록. 그 열 자체의 필터는 바탕이 제외하므로, 선택을 해제해도
 * 후보가 소멸하지 않는다.
 *
 * `needle` 은 후보를 축소하는 문자다. **수령한 목록을 화면이 여과해 표시하는
 * 방식으로는 불가하다** — 값이 다수면 바탕이 전단에서 절단해 전송하므로, 탐색하는
 * 값이 그 이후에 소재하면 "없습니다" 가 표시된다.
 */
export function columnValues(
  screenId: string,
  header: HeaderSpec | null,
  columnKey: string,
  filters: Filter[],
  needle: string,
): Promise<ColumnValuesResult> {
  return book().columnValues<ColumnValuesResult>({
    screenId,
    header,
    columnKey,
    filters,
    needle,
  });
}

/**
 * 머리글을 조작했을 때 정렬을 1단계 순환시킨다.
 *   없음 → 오름차순 → 내림차순 → 없음
 * 신규 진입한 열은 **최후위 우선순위**로 부착된다. 최초 조작한 열이 1순위다.
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
