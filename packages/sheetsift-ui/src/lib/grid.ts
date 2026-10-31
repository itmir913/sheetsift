/**
 * 표를 취급하는 화면이 사용하는 것 전부. **경계는 여기 하나다.**
 *
 * 자료를 수령하는 문은 이 패키지가 보유하지 않는다 — 받는 앱이 주입한
 * [`GridSource`] 를 호출한다(`inject.ts`). sheetsift 는 개방한 `Book` 을,
 * goe-navi 는 tauri 명령을 경유하는데, 요청의 항목 명칭이 동일한 벌이라
 * 화면은 어느 쪽인지 인지하지 않는다.
 */
import { ref } from "vue";
import { source, t } from "./inject";

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
  /**
   * 양식이 명칭을 기재하지 않은 열의 **엑셀 열 기호**(`D`).
   *
   * 그 경우 `label` 도 이 기호이나, 화면은 자체 문구로 묘사한다(`D열` ·
   * `Column D`) — 바탕이 문구를 합성하면 영어 화면에도 한국어가 출현한다.
   */
  letter?: string;
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
 * 헤더가 어디서 몇 줄인가. **사용자가 조작부에서 지정한다.**
 *
 * 추정으로 전부 적중시킬 수 없다 — 열마다 단수가 상이하거나(`학년` 은 1단,
 * `국어 등급` 은 2단) 상단에 제목 행이 부착된 양식이 실재한다. 바탕은 추정에
 * 실패하면 실패했다고 반환하고, 결정은 사람이 수행한다.
 */
export interface HeaderSpec {
  /** 헤더가 개시되는 행. 0부터 산정한다. */
  at: number;
  /** 헤더 행 수. 1 이상. */
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
   * 헤더를 탐색했는가. **탐색하지 못한 것과 행이 부재하는 것은 상이하다** —
   * 화면이 그 사유를 제시할 수 있어야 한다.
   */
  headerFound: boolean;
  /** 적용된 헤더 지정. 판별에 실패하면 null 이다. */
  header: HeaderSpec | null;
  /** 그 지정이 **추정**인가. 참이면 사용자가 확인하지 않은 값이다. */
  headerGuessed: boolean;
  /** 시트의 총 행 수. 조작부가 지정 가능한 범위의 상한이다. */
  sheetRows: number;
  /**
   * 열이 조회 엔진의 상한을 초과하는가.
   *
   * **표를 묘사하지 않는다.** 열은 전부 도달해 묘사할 수 있으나 자료는 도달할
   * 수 없어, 묘사하면 자료가 부재하는 표가 완결된 것처럼 출현한다.
   */
  tooWide: boolean;
  /**
   * 이 시트가 양식에서 개시되는 **행**(0부터).
   *
   * 선두의 공백 행은 소거된 상태로 도달한다. 조작부가 헤더 위치를 **엑셀의
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
  /**
   * **제외할 값들.** 전체에서 몇 개를 해제한 상태를 그대로 기재한다.
   *
   * 목록이 상한에 걸려 절단된 열에서도 "하나만 제외" 가 정확해진다 — 선택한
   * 것을 열거하는 방식으로는 노출되지 않은 값까지 함께 제외된다. 양자를 동시에
   * 기재하면 바탕이 중단한다.
   */
  excluded?: string[] | null;
}

/**
 * 한 열의 값 선택 상태.
 *
 * `include` 는 열거한 것만 통과시키고, `exclude` 는 열거한 것만 탈락시킨다.
 * null 은 전체 허용이다.
 */
export type ValuePick =
  | { mode: "include"; values: string[] }
  | { mode: "exclude"; values: string[] };

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

export interface QueryResponse {
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

/**
 * 자료에서 유래한 열쇠로 조회한다.
 *
 * **원형(prototype)의 항목을 수령하지 않는다.** 열 명칭과 시트 명칭은 양식이
 * 결정하므로 `toString`·`constructor` 같은 명칭이 실재할 수 있고, 평범한
 * 조회(`map[key]`)는 그때 원형의 함수를 반환한다 — 화면이 그것을 문자로
 * 취급하는 순간 전체가 중단되고 **백지가 묘사된다.** 실제로 그렇게 발생했다.
 */
export function own<T>(map: Record<string, T>, key: string): T | undefined {
  return Object.hasOwn(map, key) ? map[key] : undefined;
}

/** 값이 공백인가. NULL·공백 문자·양식의 "없음" 표기. */
export function isBlank(v: unknown): boolean {
  return v === null || v === undefined || v === "" || v === blankMark;
}

/**
 * 값 선택 목록의 한 항목을 사람이 판독할 문자로.
 *
 * 공백 표식(`limits().blankToken`)은 **수수하는 값이지 묘사하는 문구가 아니다.**
 * 자료에서 유래할 수 없는 문자라, 그대로 묘사하면 판독할 수 없는 문자가 출현한다.
 * 묘사만 받는 앱의 문구(`filter.blank`)로 치환한다.
 *
 * **화면 각처에서 표식을 대조하지 않는다.** 값 목록과 인쇄가 동일한 문자를
 * 제시해야 하는데, 두 곳에 개별 기재하면 한 곳만 수정된다(goe-navi `65f4137`).
 */
export function valueText(v: string): string {
  return v === source().limits().blankToken ? t("filter.blank") : v;
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

/**
 * 언어가 사용하는 천 단위 구분 기호.
 *
 * 수를 **해석하지 않고** 구분을 삽입하려면 기호만 필요하다. 언어마다 상이하다
 * (`1,000` · `1.000` · `1 000`).
 */
function groupMark(): string {
  const parts = formats.on.formatToParts(11111);
  return parts.find((p) => p.type === "group")?.value ?? ",";
}

/** 부호·정수부·소수부로만 구성된 수인가. 지수 표기는 해당하지 않는다. */
const PLAIN_NUMBER = /^([+-]?)(\d+)(\.\d+)?$/;

/**
 * 수를 **해석하지 않고** 천 단위 구분만 삽입한다.
 *
 * `Number()` 를 경유하면 양식의 값이 훼손된다 — `007` 이 `7` 이 되고,
 * `9007199254740993` 이 `...992` 가 되고, `1e308` 이 309 자리로 전개된다.
 * 셀의 문자는 바탕이 양식에서 판독한 그대로이고, **열람기는 그것을 변경할
 * 권한이 부재하다.** 해석하지 못하는 형태는 그대로 묘사한다.
 */
function grouped(raw: string): string {
  const m = PLAIN_NUMBER.exec(raw);
  if (!m) return raw;
  const [, sign, int, frac] = m;
  return sign + int.replace(/\B(?=(\d{3})+(?!\d))/g, groupMark()) + (frac ?? "");
}

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
    const raw = String(value).trim();
    /* 자릿수를 인지하는 앱만 반올림한다 — 그 앱은 양식의 값을 변경할 권한을
       보유한다고 진술한 것이다. 인지하지 못하면 **해석하지 않는다.** */
    if (decimals !== null) {
      const n = Number(raw);
      if (Number.isNaN(n)) return raw;
      const p = 10 ** decimals;
      const cellFormat = grouping.value ? formats.on : formats.off;
      return cellFormat.format(Math.round(n * p) / p);
    }
    return grouping.value ? grouped(raw) : raw;
  }
  return String(value);
}

/* ── 조회 ──────────────────────────────────────────────────────── */

/** 한 페이지에 수용하는 행 수. 바탕의 상한을 초과하지 않는다. */
export function pageSize(): number {
  return source().limits().maxPage;
}

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
  const res = await source().queryRows({
    screenId,
    header,
    sorts,
    filters,
    offset: page * limit,
    limit,
  });
  return { rows: res.rows, total: res.total, elapsedMs: res.elapsedMs };
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
  return source().columnValues({
    screenId,
    header,
    columnKey,
    filters,
    needle,
  });
}

/**
 * 헤더를 조작했을 때 정렬을 1단계 순환시킨다.
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
