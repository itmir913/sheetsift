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
   * 본문 폭을 측정할 문자. **셀을 슬롯으로 직접 묘사하는 열**이 지정한다.
   *
   * 지정하면 열 폭을 값이 아니라 이 문자로 산정한다. 값이 긴 주소인데 묘사가
   * 단추 하나라면, 값을 측정해 폭을 부여하면 열이 불필요하게 광폭이 된다.
   * 지정하지 않으면 종래대로 값을 측정한다.
   */
  measure?: string;
  /**
   * 양식이 명칭을 기재하지 않은 열의 **엑셀 열 기호**(`D`).
   *
   * 그 경우 `label` 도 이 기호이나, 화면은 자체 문구로 묘사한다(`D열` ·
   * `Column D`) — 바탕이 문구를 합성하면 영어 화면에도 한국어가 출현한다.
   */
  letter?: string;
  /**
   * 헤더 셀에 부여할 CSS 변수(`--이름` → 값). **받는 앱이 헤더를 칠할 때** 지정한다.
   *
   * 값은 이 패키지가 해석하지 않고 헤더 셀의 `style` 로 전달할 뿐이다. 색의 혼합
   * 규칙과 변수의 이름은 받는 앱의 CSS 가 결정한다 — 이 패키지는 색 값을 기재하지
   * 않는다. 지정하지 않거나 비어 있으면 헤더 셀(과 열 검색 칸)에 `style` 도 `data-hvars`(검색 칸은 `data-fvars`)도 없다.
   *
   * 열 검색·깔대기 칸(`.dg-fcell`)에도 같은 변수를 `data-fvars` 와 함께 싣는다. 색을 부여한 열이
   * 머리에서 끊겼다가 본문에서 다시 이어지는 것처럼 보이지 않게 하려는 것이다.
   */
  headerVars?: Record<string, string>;
  /**
   * 본문 셀에 부여할 CSS 변수. `headerVars` 와 같이 전달만 하며 해석은 받는 앱의 CSS 가 한다.
   * 지정하지 않거나 비어 있으면 본문 셀에 `style` 도 `data-cvars` 도 없다.
   */
  cellVars?: Record<string, string>;
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
  /**
   * **숫자 열의 하한·상한.** 경계를 포함하고, 숫자로 판독되지 않는 값(공백·문자)은
   * 하나라도 기재되면 탈락한다. 둘 다 생략하면 적용하지 않는다. 숫자 열이 아닌 열에
   * 기재하거나 하한이 상한을 초과하면 바탕이 중단한다.
   */
  min?: number | null;
  max?: number | null;
}

/** 숫자 열에 걸어 둔 범위. 하한이 상한을 넘지 않는다(`settleRange` 가 뒤집어 받는다). */
export interface NumberRange {
  min: number;
  max: number;
}

/**
 * 범위 입력의 눈금. **정수로 올리지 않는다** — 내신처럼 소수 둘째 자리까지 오는
 * 자료에서 정수로 반올림하면 1.95 와 2.04 가 한 칸이 된다.
 */
export const RANGE_STEP = 0.01;

/** 눈금의 소수 자릿수. `RANGE_STEP` 에서 끌어낸다 — 눈금을 따로 적지 않는다. */
const RANGE_DIGITS = Math.round(-Math.log10(RANGE_STEP));

/** 눈금 위의 수로 맞춘다. 부동소수의 잔여(`2.4099999`)를 남기지 않는다. */
function onGrid(n: number): number {
  return Number(n.toFixed(RANGE_DIGITS));
}

/**
 * 슬라이더가 칸으로 삼는 범위. **자료의 범위를 눈금 바깥쪽으로 맞춘다.**
 *
 * 자료의 끝(`5.337`)이 눈금 위에 있지 않으면 브라우저가 슬라이더의 값을 눈금 위의
 * 가장 가까운 안쪽(`5.33`)으로 끌어당기는데, 그러면 손잡이를 끝까지 밀어도 가장 큰
 * 값의 행이 탈락한다. 바깥쪽으로 맞추면 끝까지 민 손잡이가 항상 자료 전체를 덮는다.
 */
export function snapSpan(span: NumberRange): NumberRange {
  return {
    min: onGrid(Math.floor(span.min / RANGE_STEP + 1e-9) * RANGE_STEP),
    max: onGrid(Math.ceil(span.max / RANGE_STEP - 1e-9) * RANGE_STEP),
  };
}

/* ── 값 선택 팝오버의 자리와 높이 ──────────────────────────────── */

/** 팝오버의 폭(px). 양식(`.dg-pop` 의 `width`)과 동일한 수다. */
export const POP_W = 320;

/** 가시 영역의 끝에서 띄우는 여백(px). */
const POP_MARGIN = 12;

/** 깔대기 단추와 팝오버 사이의 간격(px). */
const POP_GAP = 4;

/**
 * 팝오버의 최대 높이(px). **문자 열과 숫자 열을 하나로 두지 않는다.**
 *
 * 숫자 열은 범위 칸(슬라이더와 입력칸 둘)이 상단을 점유하므로, 동일한 높이를
 * 부여하면 값 목록에 두 행만 남는다 — 실측으로 `57.5` 와 `59` 두 행이었다.
 * 목록에서 고르는 작업이 그 높이로는 성립하지 않는다.
 *
 * **두 수를 여기 함께 둔다.** 사용하는 곳마다 기재하면 한쪽만 수정된다.
 */
export const POP_MAX_H: Record<ColumnKind, number> = { text: 420, number: 560 };

/**
 * 공간이 협소해도 이 높이까지는 요구한다(px). 값 목록이 `POP_MIN_ROWS` 행을
 * 보유하는 높이다 — 그보다 축소되면 목록이 아니라 틈이 된다. 가시 영역을
 * 초과하는 분량은 팝오버 자신이 구른다(`.dg-pop` 의 `overflow-y`).
 */
export const POP_MIN_H = 300;

/** 값 목록이 최소한 보유하는 행 수. 양식의 `min-height` 와 동일한 근거다. */
export const POP_MIN_ROWS = 6;

/** 가시 영역·단추의 자리. `DOMRect` 중 배치에 사용하는 부분만이다. */
export interface Box {
  left: number;
  right: number;
  top: number;
  bottom: number;
}

/** 팝오버를 그리는 자리. **화면 기준**이다(`position: fixed`). */
export interface PopPlace {
  x: number;
  y: number;
  h: number;
}

/**
 * 깔대기 팝오버의 자리와 높이. **화면 기준으로 산출한다.**
 *
 * 표와 함께 구르는 상자 기준으로 배치하면 **표를 굴릴 때 팝오버가 헤더을
 * 떠나 아래로 흘러간다.** 구를 때 폐쇄하는 방법은 기각되었다 — 고르던 선택이
 * 소멸한다. 그래서 화면 기준으로 배치하고, 구를 때·창의 크기가 변동할 때·표가
 * 수평으로 구를 때 이 함수를 재차 호출해 자리를 갱신한다.
 *
 * **헤더이 가시 영역을 이탈하면 끝에 정착한다**(폐쇄하지 않는다). 사유는
 * 구를 때 폐쇄하지 않는 것과 동일하다 — 고르던 선택을 소멸시키지 않는다.
 * 그래서 산출한 자리를 가시 영역 내부로 고정한다(`clamp`). 수평도 동일하다.
 */
export function placePop(btn: Box, view: Box, kind: ColumnKind): PopPlace {
  const cap = POP_MAX_H[kind];
  const below = view.bottom - btn.bottom - POP_MARGIN - POP_GAP;
  const above = btn.top - view.top - POP_MARGIN - POP_GAP;
  const room = Math.max(below, above);
  // 공간이 협소해도 최소 높이는 요구한다. 초과하는 분량은 팝오버가 구른다.
  const h = Math.max(POP_MIN_H, Math.min(cap, room));
  const y = below >= above ? btn.bottom + POP_GAP : btn.top - POP_GAP - h;
  return {
    x: clamp(btn.left, view.left + POP_MARGIN, view.right - POP_W - POP_MARGIN),
    y: clamp(y, view.top + POP_MARGIN, view.bottom - POP_MARGIN - h),
    h,
  };
}

/** 하한이 상한을 초과하는 경우(가시 영역이 협소하다)에는 하한을 취한다. */
function clamp(v: number, lo: number, hi: number): number {
  return Math.min(Math.max(v, lo), Math.max(lo, hi));
}

/**
 * 범위 칸을 그리는가. **열의 성질만으로 정한다** — 숫자 열이고 수치가 하나라도 있으면(`span`)
 * 언제나 그린다. 값 목록은 모든 열이 그린다(숫자 열은 범위 칸 아래에 둔다).
 *
 * 걸린 범위가 있는지, 자료의 폭이 있는지(최소 < 최대)로 정하지 않는다. 그것들은 지나가는
 * 상태다. 그것으로 꼴을 정하면 같은 열의 깔대기가 조작에 따라 두 모양을 오간다. 폭이 0 이면
 * 범위 칸은 그대로 두고 조작만 못 하게 한다.
 *
 * 수치가 하나도 없는 열(`span` 이 `null`)에 범위 칸이 없는 것은 **자료가 정하는 것이지
 * 조작이 정하는 것이 아니다.** 같은 자료에서는 꼴이 오가지 않는다.
 */
export function showsRange(kind: ColumnKind, span: NumberRange | null): boolean {
  return kind === "number" && span !== null;
}

/**
 * 범위의 숫자를 **셀과 같은 표기**로. 「천 단위 쉼표」를 끈 사람에게 표는 `44850` 인데 안내만
 * `44,850` 이면 같은 수가 둘로 보인다. `formatNumber` 는 화면이 산출한 개수용이라 자료의
 * 값에 쓰지 않는다(그 함수의 주석).
 */
function rangeNumber(n: number): string {
  return cellText({ kind: "number" }, n);
}

/**
 * 범위를 한 글자열로. **구분 기호는 사전의 한 열쇠(`filter.range.sep`)에서만 온다** — 안내·
 * 요약·인쇄가 각자 기재하면 한쪽만 바뀐다. 한쪽만 걸린 범위(바탕은 허용한다)는 없는 쪽을
 * 비워 둔다.
 */
export function rangeText(min: number | null | undefined, max: number | null | undefined): string {
  const at = (n: number | null | undefined) => (n == null ? "" : rangeNumber(n));
  return `${at(min)} ${t("filter.range.sep")} ${at(max)}`.trim();
}

/**
 * 그 값이 범위 안에 있는가. **자료의 값(글자)을 수치로 읽어** 판정한다 — 바탕이 범위를 거를 때와
 * 같은 규칙이다(쉼표는 천 단위 구분으로 보고 지운다). 공백 표식과 수치가 아닌 글자는 어떤
 * 범위에도 들지 않는다.
 */
export function inRange(value: string, range: NumberRange): boolean {
  const text = value.replace(/,/g, "").trim();
  if (text === "") return false;
  const n = Number(text);
  return Number.isFinite(n) && n >= range.min && n <= range.max;
}

export type PickMode = "all" | "include" | "exclude";

/**
 * 범위를 걸었을 때 **값 선택에서 범위 밖이 된 것을 거둔다.**
 *
 * 남겨 두면 보이지 않는 선택이 범위와 AND 로 겹쳐 조용히 0건이 된다(`include`). `exclude` 도
 * 같다 — 범위 밖의 제외는 지금은 결과에 영향이 없지만, 범위를 풀면 보지 못한 제외가 되살아나
 * 값들이 조용히 빠진다.
 *
 * **목록에 기대지 않고 수치로 판정한다.** 목록은 검색으로 줄거나 상한에서 절단될 수 있어, 목록에
 * 없다는 것이 범위 밖이라는 뜻이 아니다.
 *
 * 거둔 뒤 선택이 비면 `all` 로 되돌린다 — 그때 사람의 뜻은 「범위 하나만」이다. 처음부터 비어
 * 있던 `include`(전부 해제)는 건드리지 않는다.
 */
export function settleSelection(
  mode: PickMode,
  values: readonly string[],
  range: NumberRange | null,
): { mode: PickMode; values: string[] } {
  if (!range || mode === "all" || values.length === 0) return { mode, values: [...values] };
  const kept = values.filter((v) => inRange(v, range));
  return kept.length === 0 ? { mode: "all", values: [] } : { mode, values: kept };
}

/**
 * 적용할 수 있는가. **적용 단추와 엔터가 이 한 판정을 쓴다.** 단추만 막으면 엔터로 표를 통째로
 * 비울 수 있다(열거한 것만 통과시키는데 하나도 열거하지 않은 상태). 불러오는 중에도 적용하지
 * 않는다 — 화면에 보이는 목록이 아직 최신이 아니다.
 */
export function canApply(loading: boolean, mode: PickMode, picked: number): boolean {
  return !loading && !(mode === "include" && picked === 0);
}

/**
 * 범위 입력의 `change` 가 **즉시 확정하는가.** 입력칸의 `change` 는 벗어남·엔터뿐이라 즉시
 * 확정해도 된다. 슬라이더는 방향키로도 `change` 를 매번 발신하므로, 손잡이를 잡았다 놓은
 * 경우(`held`)만 즉시 확정하고 나머지는 지연 확정으로 보낸다 — 키를 누르고 있으면 초당
 * 수십 번 확정되고 확정마다 목록 질의가 나간다.
 */
export function changeCommitsAtOnce(source: "slider" | "input", held: boolean): boolean {
  return source === "input" || held;
}

/**
 * 손잡이 둘이 가리키는 값을 하나의 범위로 확정한다.
 *
 * - **서로를 넘으면 뒤집어 받는다.** 하한을 상한 너머로 끌었을 때 막지 않고 두 값을
 *   바꾸어 받으면, 사람이 의도한 구간이 그대로 선택된다.
 * - **전체를 그대로 둔 것은 필터가 아니다.** `null` 을 반환한다 — 필터가 걸린 것으로
 *   표시되면 아무것도 거르지 않는데 걸려 있다고 진술하게 된다.
 */
export function settleRange(a: number, b: number, span: NumberRange): NumberRange | null {
  const min = Math.min(a, b);
  const max = Math.max(a, b);
  return min <= span.min && max >= span.max ? null : { min, max };
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
  /**
   * 숫자 열의 **자료가 존재하는 범위.** 숫자 열이 아니거나 숫자 값이 부재하거나 축소 문자가 있는
   * 요청이면 `null`
   * 이거나 생략된다. 생략하는 바탕(범위를 산출하지 않는 받는 앱)에서는 범위 칸이
   * 나타나지 않고 값 목록만 남는다.
   */
  range?: NumberRange | null;
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

/**
 * `keys` 에서 `from` 위치의 열쇠를 `to` 위치로 이전한 새 배열.
 *
 * **열 순서 규칙의 정본이다.** 표의 끌어 옮기기(`useColumnLayout.move`)와 열 선택
 * 모달의 초안이 이 함수를 공유한다 — 공유하는 것은 함수가 아니라 규칙이다.
 * 범위를 이탈하면 원본과 동일한 내용을 반환한다.
 */
export function reorder<T>(keys: readonly T[], from: number, to: number): T[] {
  const next = [...keys];
  if (from === to || from < 0 || to < 0 || from >= next.length || to >= next.length) {
    return next;
  }
  const [k] = next.splice(from, 1);
  next.splice(to, 0, k as T);
  return next;
}

/**
 * 숨긴 열에 남은 정렬·여과를 제거한다. **보이지 않는 규칙이 행을 거르면 건수가
 * 맞지 않는 이유를 화면이 진술하지 못하고, 정렬이 남으면 정렬 칩에 열쇠 원문이
 * 출현한다.** 소비자가 상태를 소유하므로 새 값을 반환하고 입력은 변경하지 않는다.
 */
export function dropColumnState(
  dropped: readonly string[],
  state: {
    sorts: Sort[];
    filterText: Record<string, string>;
    filterValues: Record<string, ValuePick | null>;
    /** 생략하면 범위가 없는 것이다. */
    filterRanges?: Record<string, NumberRange | null>;
  },
): {
  sorts: Sort[];
  filterText: Record<string, string>;
  filterValues: Record<string, ValuePick | null>;
  filterRanges: Record<string, NumberRange | null>;
} {
  const gone = new Set(dropped);
  const omit = <V>(map: Record<string, V>): Record<string, V> =>
    Object.fromEntries(Object.entries(map).filter(([k]) => !gone.has(k)));
  return {
    sorts: state.sorts.filter((s) => !gone.has(s.key)),
    filterText: omit(state.filterText),
    filterValues: omit(state.filterValues),
    filterRanges: omit(state.filterRanges ?? {}),
  };
}

/**
 * 행을 누른 것이 **행 상세 모달을 여는 누름인가.**
 *
 * 순수 함수로 분리한 사유는 시험 가능성이다 — 이 판정이 틀리면 글자를 복사하려던
 * 조작이나 바로가기 버튼이 모달을 열어 버린다.
 *
 * 열지 않는 경우는 셋이다.
 * - 조작 가능한 요소(버튼·링크·입력) 위의 누름. 그 요소가 자기 일을 한다.
 * - 문자를 끌어 선택한 직후. 선택이 존재하면 복사하려는 조작이다.
 * - 값 선택 팝오버가 개방된 상태로 시작한 누름. 그것은 팝오버를 폐쇄하는 누름이다.
 */
export function opensRowDetail(
  target: Element | null,
  selectedText: string,
  filterWasOpen: boolean,
): boolean {
  if (filterWasOpen) return false;
  if (selectedText.trim() !== "") return false;
  return !target?.closest('button, a, input, select, textarea, label, [role="button"]');
}
