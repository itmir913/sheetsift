<script setup lang="ts">
/**
 * 앱의 **유일한** 표.
 *
 * 화면마다 표를 개별 생성하면 일부 화면에는 열 검색이 존재하고 일부 화면에는
 * 부재하는 식으로 분기한다(실제로 그러했다). 그래서 표를 묘사하는 작업은 전부
 * 여기로 집약하고, `dg-*` 클래스는 이 컴포넌트와 `ColumnFilter` 만 사용한다.
 *
 * 이 컴포넌트는 **묘사만** 수행한다. 자료를 어디서 수령하는지, 정렬·필터를
 * 어떻게 적용하는지는 쓰는 곳이 결정한다. 조회 화면은 SQL 로, 성적 검색은
 * 메모리에서 처리한다.
 */
import { computed, nextTick, onBeforeUnmount, ref, watch } from "vue";
import ColumnFilter from "./ColumnFilter.vue";
import { cellText, isBlank, opensRowDetail, own, placePop } from "../lib/grid";
import { t } from "../lib/inject";
import { tableFont } from "../composables/useTableFont";
import { wrap } from "../composables/useWrap";
import type { ColumnValuesResult, NumberRange, Row, Sort, ValuePick } from "../lib/grid";
import type { DetailColumn, LaidOutColumn } from "../composables/useColumnLayout";

const props = withDefaults(
  defineProps<{
    columns: LaidOutColumn[];
    rows: Row[];
    sorts: Sort[];
    /** 열 검색어. 열쇠가 부재하면 공백 문자열로 판정한다. */
    filterText: Record<string, string>;
    /** 엑셀식 값 선택. 열쇠가 부재하거나 null 이면 그 열은 전체 허용. */
    filterValues: Record<string, ValuePick | null>;
    /**
     * 숫자 열의 범위. 열쇠가 부재하거나 null 이면 그 열은 범위를 걸지 않은 것이다.
     * **생략할 수 있다** — 여과하지 않는 표나 범위를 모르는 받는 앱은 그대로 쓴다.
     */
    filterRanges?: Record<string, NumberRange | null>;
    /**
     * **숨긴 열까지 담은 전체 열 목록.** 행 상세 모달이 이것을 묘사한다 —
     * 「열 선택」에서 제외한 열은 화면에서 배제한 것이지 값이 부재하는 것이 아니다.
     *
     * 선택이 아니라 **필수**다. 선택으로 두고 `columns` 로 대체하면, 전달을 망각한
     * 화면에서 숨긴 열이 조용히 누락되고 오류가 발생하지 않는다.
     */
    detailColumns: DetailColumn[];
    /** 좌측에 고정할 열 수. */
    pinned: number;
    /**
     * 값 선택 목록을 수령하는 방법.
     *
     * `needle` 은 후보를 축소하는 문자다. **여과 작업을 바탕까지 전달한다** —
     * 수령한 목록을 화면에서 여과해 표시하면, 값이 다수라 목록이 전단에서
     * 절단된 열에서 탐색하는 값이 그 이후에 소재할 때 "없습니다" 가 표시된다.
     *
     * **세 번째 인자(`range`)를 바탕까지 전달해야 한다.** 숫자 열의 값 목록은 그 범위 안의 값만
     * 담아야 하는데, 무시하면 범위 밖의 값이 목록에 나타나고 그것을 고르는 즉시 범위와 AND 로
     * 겹쳐 조용히 0건이 된다. 바탕이 `range` 를 반환하지 않으면(범위 칸이 나오지 않는다) 안전하나,
     * 반환하기 시작하는 순간 이 함정이 되살아난다. `update:filterRanges` 를 듣지 않는 앱도
     * 같은 사정이다 — 범위를 걸어도 조회에 닿지 않는다.
     */
    loadValues: (
      columnKey: string,
      needle: string,
      range: NumberRange | null,
    ) => Promise<ColumnValuesResult>;
    busy?: boolean;
    emptyText?: string;
    /**
     * 열 검색·값 선택 행을 묘사할지. **비활성화하면 공간도 점유하지 않는다.**
     *
     * 여과하지 않는 표(개별 성적표처럼 행이 수 개뿐인 것)에서는 이 행이 사용
     * 불가한 셀만 배열한다.
     */
    showFilters?: boolean;
  }>(),
  {
    busy: false,
    emptyText: "",
    showFilters: true,
    filterRanges: () => ({}),
  },
);

const emit = defineEmits<{
  /** 헤더를 조작했다(정렬). 쓰는 곳이 cycleSort 로 처리한다. */
  sort: [key: string];
  "update:filterText": [value: Record<string, string>];
  "update:filterValues": [value: Record<string, ValuePick | null>];
  "update:filterRanges": [value: Record<string, NumberRange | null>];
  /** 열을 조작해 이전했다. */
  move: [from: number, to: number];
  /** 열 폭을 조작하는 중. */
  resize: [key: string, width: number];
  /** 폭 조작이 종료되었다(저장 시점). */
  resizeEnd: [];
}>();

/** 클릭과 조작을 구분하는 이동 거리(px). */
const DRAG_THRESHOLD = 5;

const root = ref<HTMLElement | null>(null);
const headEl = ref<HTMLElement | null>(null);
/**
 * 열 폭 한 단위가 화면에서 점유하는 px. 폭은 `em` 이고 `.dg` 의 글자 크기가 곧
 * `tableFont` 이므로 그 값 그대로다. 루트 글자 크기로 나누면 글자를 확대한 표에서
 * 끌어 확장할 때 커서와 열 경계가 불일치한다.
 */
function emPx(): number {
  return tableFont.value;
}

/* ── 열 격자 · 열 고정 ───────────────────────────────────────
   폭의 단위는 `rem` 이 아니라 **`em`** 이다(`...Rem` 이라는 명칭은 유래라 존치한다).
   `rem` 은 루트 글자 크기를 추종하므로 표만 확대해도 열이 고정되어 글자가 말줄임으로
   절단된다. `em` 은 사용하는 요소(`.dg-head`·`.dg-row`)의 글자 크기로 해석되고 그것은
   `.dg` 가 상속한 `--dg-font` 이므로, 열 폭·합계·고정 열 offset 이 일괄 비례한다.
   기본(16px)에서는 `rem` 과 동일한 값이라 저장된 폭이 그대로 유효하다. */

const colTemplate = computed(() =>
  props.columns
    // 배분받는 열은 `width` 가 최소 폭이다. 잔여 공간은 그러한 열 상호간에
    // 균등 배분되므로, 최소 폭이 동일하면 폭도 동일해진다.
    .map((c) => (c.flex ? `minmax(${c.width}em, ${c.flex}fr)` : `${c.width}em`))
    .join(" "),
);

/** 열 폭의 합계. 표의 수평 폭을 브라우저가 추정하지 않도록 직접 제시한다. */
const totalWidth = computed(
  () => `${props.columns.reduce((sum, c) => sum + c.width, 0)}em`,
);

const pinOffsets = computed(() => {
  const out: string[] = [];
  let acc = 0;
  for (let i = 0; i < props.pinned; i += 1) {
    out.push(`${acc}em`);
    acc += props.columns[i]?.width ?? 0;
  }
  return out;
});

function pinIndex(i: number): number | undefined {
  return i < props.pinned ? i : undefined;
}

/** 헤더 셀에 부여할 CSS 변수. 지정하지 않았거나 비어 있으면 `undefined` 라 `style` 이 부착되지 않는다. */
function headerVars(col: LaidOutColumn): Record<string, string> | undefined {
  const v = col.headerVars;
  return v && Object.keys(v).length > 0 ? v : undefined;
}

/** 본문 셀에 부여할 CSS 변수. 지정하지 않았거나 비어 있으면 `undefined` 라 `style` 이 부착되지 않는다. */
function cellVars(col: LaidOutColumn): Record<string, string> | undefined {
  const v = col.cellVars;
  return v && Object.keys(v).length > 0 ? v : undefined;
}

const gridVars = computed(() => {
  const vars: Record<string, string> = {
    "--dg-cols": colTemplate.value,
    "--dg-total": totalWidth.value,
    /* 글자 크기가 변경되면 이 인라인 값만 변경되고, 행 높이(`--dg-row-h`)·열 폭은
       `em` 이므로 자동으로 추종한다. 가상 스크롤(`content-visibility`)의 예약 크기도
       동일한 변수라 불일치하지 않는다. */
    "--dg-font": `${tableFont.value}px`,
  };
  pinOffsets.value.forEach((off, i) => {
    vars[`--dg-pin-${i}`] = off;
  });
  return vars;
});

/* ── 정렬 표식 ─────────────────────────────────────────────── */

function sortOf(key: string): { rank: number; dir: Sort["dir"] } | null {
  const at = props.sorts.findIndex((s) => s.key === key);
  return at === -1 ? null : { rank: at + 1, dir: props.sorts[at].dir };
}

/**
 * 보조기기에 전달하는 정렬 상태.
 *
 * **▲▼ 는 `aria-hidden` 이라 전달되지 않는다.** 그것만 배치하면 화면을 보지
 * 못하는 사람에게는 정렬이 적용되었는지 자체가 부재한다.
 */
function ariaSort(key: string): "ascending" | "descending" | "none" {
  const s = sortOf(key);
  if (!s) return "none";
  return s.dir === "desc" ? "descending" : "ascending";
}

/* ── 열 검색 · 값 선택 ─────────────────────────────────────── */

const openFilterKey = ref<string | null>(null);

/* ── 행 상세 모달 ─────────────────────────────────────────────
   표는 가로로 장대해 한 행을 전부 읽으려면 좌우로 굴려야 하고, 「열 선택」으로 제외한
   열의 값은 아예 보이지 않는다. 행을 누르면 그 행의 **전 열**을 세로로 묘사한다.

   **모달을 여기 배치한 사유.** 조작부와 표를 직접 조립하는 화면(성적 검색·자료 관리)이
   존재해, `DataGrid` 에 배치하면 그 화면들에서는 아무 일도 발생하지 않는다. 공통분모는
   이 컴포넌트뿐이다. */
const detailDialog = ref<HTMLDialogElement | null>(null);
const detailCloseBtn = ref<HTMLButtonElement | null>(null);
/** 묘사 중인 행의 위치. `-1` 이면 폐쇄 상태다. */
const detailAt = ref(-1);
const detailRow = computed(() => (detailAt.value >= 0 ? props.rows[detailAt.value] : undefined));

/** 누름이 시작된 시점에 값 선택 팝오버가 개방되어 있었는가. */
let filterWasOpen = false;
function onRowPointerDown(): void {
  filterWasOpen = openFilterKey.value !== null;
}

async function onRowClick(ev: MouseEvent, i: number): Promise<void> {
  const sel = typeof window === "undefined" ? null : window.getSelection();
  const text = sel && !sel.isCollapsed ? sel.toString() : "";
  if (!opensRowDetail(ev.target as Element | null, text, filterWasOpen)) return;
  detailAt.value = i;
  /* **그린 뒤에 개방한다.** 내용은 `detailRow` 에 달려 있어 이 시점에는 아직 부재하고,
     그 상태로 `showModal()` 하면 초점이 빈 `<dialog>` 에 머물러 하기의 지정이 무위가 된다. */
  await nextTick();
  detailDialog.value?.showModal();
  // 최초 초점을 명시한다. 두지 않으면 목록 내부의 링크 버튼이 초점을 수령한다.
  detailCloseBtn.value?.focus();
}

/** 덮개를 누르면 폐쇄한다. 내용 위의 누름은 `<dialog>` 자신이 대상이 아니라 걸리지 않는다. */
function onDetailClick(ev: MouseEvent): void {
  if (ev.target === detailDialog.value) detailDialog.value?.close();
}

function moveDetail(step: number): void {
  const next = detailAt.value + step;
  if (next < 0 || next >= props.rows.length) return;
  detailAt.value = next;
  // 행을 이전하면 목록은 상단으로 되돌린다. 그러지 않으면 다음 행을 중도부터 읽는다.
  const body = detailDialog.value?.querySelector(".rd-body");
  if (body) body.scrollTop = 0;
}

function onDetailKey(ev: KeyboardEvent): void {
  // 보조 키가 눌린 조합은 그대로 둔다(브라우저·보조기기의 조작이다).
  if (ev.ctrlKey || ev.altKey || ev.metaKey) return;
  if (ev.key === "ArrowDown") {
    ev.preventDefault();
    moveDetail(1);
  } else if (ev.key === "ArrowUp") {
    ev.preventDefault();
    moveDetail(-1);
  }
}

/* 행 목록이 교체되면 **폐쇄한다.** 여과의 지연 조회나 재조회로 교체되므로, 동일한
   위치가 동일한 행이라는 보장이 없다. 상이한 행을 조용히 묘사하는 것보다 낫다.
   얕은 비교다 — 깊은 감시는 매 조회마다 헛닫힘을 유발한다. */
watch(
  () => props.rows,
  () => {
    if (detailAt.value >= 0) detailDialog.value?.close();
  },
);
/* 팝오버의 자리. **화면 기준이다**(`.dg-pop` 은 `position: fixed`).
   표와 함께 구르는 상자 기준으로 두었을 때, 표를 굴리면 팝오버가 헤더을 떠나
   아래로 흘러갔다. 구를 때 폐쇄하는 방법은 기각되었다 — 고르던 선택이 소멸한다.
   그래서 **구를 때·창의 크기가 변동할 때 자리를 재차 산정한다.** */
const popX = ref(0);
const popY = ref(0);
/** 팝오버의 높이(px). 열의 성질과 가시 영역이 정한다(`placePop`). */
const popH = ref(420);

/** 자리를 재차 산정할 근거. 개방한 깔대기의 단추다. */
let popBtn: HTMLElement | null = null;

/** 개방한 깔대기의 자리를 지금의 화면으로 갱신한다. */
function placeFilter() {
  const btn = popBtn;
  const col = openFilterColumn.value;
  if (!btn || !col) return;
  /* 절단되지 말아야 할 기준은 **가시 영역**이다. 표를 담은 상자는 화면보다
     현저히 광범위할 수 있다. */
  const box = root.value?.closest(".app-main")?.getBoundingClientRect();
  const view = box ?? {
    left: 0,
    right: window.innerWidth,
    top: 0,
    bottom: window.innerHeight,
  };
  const at = placePop(btn.getBoundingClientRect(), view, col.kind);
  popX.value = at.x;
  popY.value = at.y;
  popH.value = at.h;
}

/* 표의 수평 스크롤과 쪽의 수직 스크롤 양측을 수령해야 하므로 **포착 단계**로
   청취한다 — 구르는 상자가 창이 아니라 내부의 요소다. */
function watchPlace(on: boolean) {
  const f = on ? window.addEventListener : window.removeEventListener;
  f("scroll", placeFilter, true);
  f("resize", placeFilter);
}

watch(openFilterKey, (key) => {
  watchPlace(key !== null);
  if (key === null) popBtn = null;
});

onBeforeUnmount(() => watchPlace(false));

const openFilterColumn = computed(
  () => props.columns.find((c) => c.key === openFilterKey.value) ?? null,
);

let filterTimer: number | undefined;
/**
 * 발신을 대기 중인 입력들.
 *
 * **타이머 하나를 공유하므로 입력을 누적해야 한다.** 열 하나에 입력한 직후 타
 * 열에 입력하면 선행 입력의 발신이 취소되는데, 발신하는 내용을 입력 시점에
 * 고정해 두면 그 입력이 **문자까지 소멸한다** — 사람이 기재한 조건이 아무
 * 진술 없이 사라진다.
 */
const typed = new Map<string, string>();

function onSearchInput(key: string, value: string) {
  typed.set(key, value);
  window.clearTimeout(filterTimer);
  filterTimer = window.setTimeout(() => {
    const next = { ...props.filterText, ...Object.fromEntries(typed) };
    typed.clear();
    emit("update:filterText", next);
  }, 250);
}

function openFilter(col: LaidOutColumn, ev: MouseEvent) {
  if (!col.filterable) return;
  if (openFilterKey.value === col.key) {
    openFilterKey.value = null;
    return;
  }
  popBtn = ev.currentTarget as HTMLElement | null;
  openFilterKey.value = col.key;
  /* **개방하는 시점에 1회 산정한다.** `openFilterColumn` 을 참조하므로 열쇠를
     먼저 기재한다. */
  placeFilter();
}

function applyFilterValues(pick: ValuePick | null, range: NumberRange | null) {
  const key = openFilterKey.value;
  if (!key) return;
  /* **범위를 먼저 올린다.** 받는 쪽은 값 선택이 올라올 때 조회하므로(`DataGrid.onFilterValues`),
     순서가 반대면 새 범위 없이 한 번 조회하고 다시 조회한다. */
  emit("update:filterRanges", { ...props.filterRanges, [key]: range });
  emit("update:filterValues", { ...props.filterValues, [key]: pick });
  openFilterKey.value = null;
}

/* ── 열 순서 조작 ──────────────────────────────────────────────
   헤더를 누른 채 수평 이동하면 순서 변경, 단순 조작이면 정렬.
   DRAG_THRESHOLD 만큼 이동했는지로 양자를 구분한다. */

const dragFrom = ref<number | null>(null);
const dropAt = ref<number | null>(null);
const dragging = ref(false);
let dragStartX = 0;

function columnIndexAtX(clientX: number): number {
  const cells = headEl.value?.children;
  if (!cells || cells.length === 0) return 0;
  for (let i = 0; i < cells.length; i += 1) {
    const r = cells[i].getBoundingClientRect();
    if (clientX < r.left + r.width / 2) return i;
  }
  return cells.length - 1;
}

function onHeaderPointerDown(index: number, ev: PointerEvent) {
  if (ev.button !== 0) return;
  dragFrom.value = index;
  dragStartX = ev.clientX;
  dragging.value = false;
  dropAt.value = null;
  window.addEventListener("pointermove", onHeaderPointerMove);
  window.addEventListener("pointerup", onHeaderPointerUp);
}

function onHeaderPointerMove(ev: PointerEvent) {
  if (dragFrom.value === null) return;
  if (!dragging.value && Math.abs(ev.clientX - dragStartX) < DRAG_THRESHOLD) return;
  dragging.value = true;
  dropAt.value = columnIndexAtX(ev.clientX);
}

function onHeaderPointerUp() {
  window.removeEventListener("pointermove", onHeaderPointerMove);
  window.removeEventListener("pointerup", onHeaderPointerUp);
  const from = dragFrom.value;
  if (from !== null) {
    if (dragging.value && dropAt.value !== null) {
      emit("move", from, dropAt.value);
    } else {
      // 이동하지 않았으면 클릭으로 판정한다 → 정렬.
      const col = props.columns[from];
      if (col?.sortable) emit("sort", col.key);
    }
  }
  dragFrom.value = null;
  dropAt.value = null;
  dragging.value = false;
}

/* ── 열 폭 조작 ────────────────────────────────────────────── */

let resizeKey: string | null = null;
let resizeStartX = 0;
let resizeStartW = 0;
const resizingKey = ref<string | null>(null);

function onResizeStart(col: LaidOutColumn, ev: PointerEvent) {
  ev.stopPropagation();
  ev.preventDefault();
  resizeKey = col.key;
  resizingKey.value = col.key;
  resizeStartX = ev.clientX;
  resizeStartW = col.width;
  window.addEventListener("pointermove", onResizeMove);
  window.addEventListener("pointerup", onResizeEnd);
}

function onResizeMove(ev: PointerEvent) {
  if (!resizeKey) return;
  emit("resize", resizeKey, resizeStartW + (ev.clientX - resizeStartX) / emPx());
}

function onResizeEnd() {
  window.removeEventListener("pointermove", onResizeMove);
  window.removeEventListener("pointerup", onResizeEnd);
  emit("resizeEnd");
  resizeKey = null;
  resizingKey.value = null;
}

defineExpose({ close: () => (openFilterKey.value = null) });
</script>

<template>
  <div ref="root" class="relative">
    <!-- `data-wrap` 은 **켠 상태에서만 부착한다**(`undefined` 면 속성 자체가
         부착되지 않는다) — 끈 표의 묘사가 종전과 바이트 단위로 동일해진다.
         실제 규칙은 `styles/grid.css` 의 `.dg[data-wrap="true"]` 다. -->
    <div
      class="dg gx-table"
      :data-dragging="dragging"
      :data-wrap="wrap || undefined"
      :style="gridVars"
    >
      <!-- 열 헤더와 열 검색 행. .app-main 기준으로 화면 최상단에 고정된다. -->
      <div class="gx-head">
        <div ref="headEl" class="dg-head">
          <div
            v-for="(col, i) in columns"
            :key="col.key"
            class="dg-hcell"
            :data-kind="col.kind"
            :data-sortable="col.sortable"
            :data-sorted="!!sortOf(col.key)"
            :data-pin="pinIndex(i)"
            :data-pin-last="i === pinned - 1"
            :data-hvars="headerVars(col) ? '' : undefined"
            :style="headerVars(col)"
            :data-drag-src="dragging && dragFrom === i"
            :data-drop="dragging && dropAt === i"
            role="columnheader"
            :tabindex="col.sortable ? 0 : undefined"
            :aria-sort="ariaSort(col.key)"
            :data-tip="col.sortable ? t('grid.head.tip', { label: col.label }) : col.label"
            @keydown.enter.prevent="col.sortable && emit('sort', col.key)"
            @keydown.space.prevent="col.sortable && emit('sort', col.key)"
            @pointerdown="onHeaderPointerDown(i, $event)"
          >
            <span class="dg-hlabel">{{ col.label }}</span>
            <!-- 방향만 노출된다. 몇 번째로 정렬했는지는 상단 정렬 칩이 번호로
                 제시하므로, 여기서 재차 배지를 부착하면 헤더마다 그 폭을 항상
                 확보해 두게 된다(정렬하지 않은 열까지). -->
            <span class="dg-sortmark" :data-visible="!!sortOf(col.key)" aria-hidden="true">
              {{ sortOf(col.key)?.dir === "desc" ? "▼" : "▲" }}
            </span>
            <span
              class="dg-resize"
              :data-active="resizingKey === col.key"
              role="separator"
              :aria-label="t('grid.resize.aria', { label: col.label })"
              @pointerdown="onResizeStart(col, $event)"
            ></span>
          </div>
        </div>

        <!-- 좌측은 열 검색, 우측 수평선 단추는 엑셀식 값 선택 -->
        <div v-if="showFilters" class="dg-filters">
          <div
            v-for="(col, i) in columns"
            :key="col.key"
            class="dg-fcell"
            :data-pin="pinIndex(i)"
            :data-pin-last="i === pinned - 1"
            :data-fvars="headerVars(col) ? '' : undefined"
            :style="headerVars(col)"
          >
            <div class="dg-fwrap">
              <input
                class="dg-finput"
                type="search"
                :value="own(filterText, col.key) ?? ''"
                :disabled="!col.filterable"
                :placeholder="col.filterable ? t('grid.search.ph') : ''"
                :aria-label="t('grid.search.aria', { label: col.label })"
                @input="onSearchInput(col.key, ($event.target as HTMLInputElement).value)"
              />
              <button
                type="button"
                class="dg-fbtn"
                :data-active="own(filterValues, col.key) != null || own(filterRanges, col.key) != null"
                :disabled="!col.filterable"
                :aria-label="t('grid.filter.aria', { label: col.label })"
                :aria-expanded="openFilterKey === col.key"
                :data-tip="t('grid.filter.tip', { label: col.label })"
                @click="openFilter(col, $event)"
              >
                <!-- 필터를 의미하는 수평선 3개. 여기에 ▼ 를 사용하면 상단 헤더의
                     내림차순 표식과 동일한 문자가 되어, 정렬이 적용된 것처럼 인식된다. -->
                <svg class="dg-fbtn-icon" viewBox="0 0 16 16" aria-hidden="true">
                  <path
                    d="M2.4 4h11.2M4.6 8h6.8M6.9 12h2.2"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="1.7"
                    stroke-linecap="round"
                  />
                </svg>
              </button>
            </div>
          </div>
        </div>
      </div>

      <div>
        <div
          v-for="(row, i) in rows"
          :key="i"
          class="dg-row"
          :data-odd="i % 2 === 1"
          @pointerdown="onRowPointerDown"
          @click="onRowClick($event, i)"
        >
          <!-- 가리킬 때의 안내(`data-tip`)는 **줄바꿈을 켠 상태에서 부착하지
               않는다.** 절단이 부재하므로 중복이고, 가리키는 곳마다 전문이
               재출현하면 판독을 방해한다. -->
          <div
            v-for="(col, ci) in columns"
            :key="col.key"
            class="dg-cell"
            :data-kind="col.kind"
            :data-blank="isBlank(row[col.srcIndex])"
            :data-pin="pinIndex(ci)"
            :data-pin-last="ci === pinned - 1"
            :data-cvars="cellVars(col) ? '' : undefined"
            :style="cellVars(col)"
            :data-tip="wrap ? undefined : String(row[col.srcIndex] ?? '')"
          >
            <!-- 셀을 특수하게 묘사해야 하는 화면(가능성 배지 등)만 이 위치를 사용한다. -->
            <slot name="cell" :col="col" :row="row" :value="row[col.srcIndex]">
              {{ cellText(col, row[col.srcIndex]) }}
            </slot>
          </div>
        </div>

        <p v-if="!busy && !rows.length" class="p-6 text-fg-subtle">
          {{ emptyText || t("grid.empty") }}
        </p>
      </div>
    </div>

    <!-- 행 상세. 행을 누르면 그 행의 **전 열**(숨긴 열 포함)을 세로로 묘사한다.
         읽기 전용이다 — 선별·정렬·여과는 표가 담당한다. -->
    <dialog
      ref="detailDialog"
      class="rd-dialog"
      aria-labelledby="rd-title"
      @close="detailAt = -1"
      @click="onDetailClick"
      @keydown="onDetailKey"
    >
      <div v-if="detailRow" class="rd-head">
        <h3 id="rd-title" class="rd-title">{{ t("detail.title") }}</h3>
        <button ref="detailCloseBtn" type="button" class="rd-btn" @click="detailDialog?.close()">
          {{ t("detail.close") }}
        </button>
      </div>

      <!-- **초점을 받지 않는다.** 받으면 Tab 으로 들어왔을 때 본문 전체에 초점 테두리가 그려진다.
           행 이동 키는 `<dialog>` 가 받으므로(`onDetailKey`) 여기가 초점을 보유할 사유가 없다. -->
      <div v-if="detailRow" class="rd-body" role="group" aria-labelledby="rd-title">
        <dl class="rd-list">
          <!-- 열에 준 색은 **줄 전체**에 칠한다. 값 칸만 칠하면 이름과 값이 다른 줄처럼 보인다. -->
          <div
            v-for="col in detailColumns"
            :key="col.key"
            class="rd-pair"
            :data-cvars="cellVars(col) ? '' : undefined"
            :style="cellVars(col)"
          >
            <dt class="rd-name">
              {{ col.label }}
              <span v-if="col.hidden" class="rd-tag">{{ t("detail.hidden") }}</span>
            </dt>
            <dd class="rd-value" :data-kind="col.kind" :data-blank="isBlank(detailRow[col.srcIndex])">
              <!-- 표와 **동일한 슬롯**이다. 그러지 않으면 모달에서만 꼴이 상이해진다. -->
              <slot name="cell" :col="col" :row="detailRow" :value="detailRow[col.srcIndex]">
                {{ cellText(col, detailRow[col.srcIndex]) }}
              </slot>
            </dd>
          </div>
        </dl>
      </div>

      <div v-if="detailRow" class="rd-foot">
        <span class="rd-at" aria-live="polite">
          {{ t("detail.position", { n: detailAt + 1, total: rows.length }) }}
        </span>
        <span class="rd-move">
          <button
            type="button"
            class="rd-btn"
            :aria-disabled="detailAt <= 0"
            @click="moveDetail(-1)"
          >
            {{ t("detail.prev") }}
          </button>
          <button
            type="button"
            class="rd-btn"
            :aria-disabled="detailAt >= rows.length - 1"
            @click="moveDetail(1)"
          >
            {{ t("detail.next") }}
          </button>
        </span>
      </div>
    </dialog>

    <!-- 값 선택 팝오버. 표 상단에 부유하므로 레이아웃에 영향을 미치지 않는다. -->
    <ColumnFilter
      v-if="openFilterColumn"
      :key="openFilterColumn.key"
      :column="openFilterColumn"
      :selected="own(filterValues, openFilterColumn.key) ?? null"
      :range="own(filterRanges, openFilterColumn.key) ?? null"
      :load="(needle: string, range: NumberRange | null) => loadValues(openFilterColumn!.key, needle, range)"
      :x="popX"
      :y="popY"
      :max-height="popH"
      @apply="applyFilterValues"
      @close="openFilterKey = null"
    />
  </div>
</template>
