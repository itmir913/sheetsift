<script setup lang="ts">
/**
 * 앱의 **유일한** 표.
 *
 * 화면마다 표를 따로 만들면 어느 화면에는 열 검색이 있고 어느 화면에는 없는
 * 식으로 갈라진다(실제로 그랬다). 그래서 표를 그리는 일은 전부 여기로 모으고,
 * 다른 컴포넌트는 `dg-*` 클래스를 직접 쓰지 못하게 막아 두었다
 * (`scripts/check-grid.mjs`).
 *
 * 이 컴포넌트는 **그리기만** 한다. 데이터를 어디서 가져오는지, 정렬·필터를
 * 어떻게 적용하는지는 쓰는 쪽이 정한다. 조회 화면은 SQL 로, 성적 검색은
 * 메모리에서 처리한다.
 */
import { computed, ref } from "vue";
import ColumnFilter from "@/components/ColumnFilter.vue";
import { cellText, isBlank } from "@/lib/grid";
import type { ColumnValuesResult, Row, Sort } from "@/lib/grid";
import type { LaidOutColumn } from "@/composables/useColumnLayout";

const props = withDefaults(
  defineProps<{
    columns: LaidOutColumn[];
    rows: Row[];
    sorts: Sort[];
    /** 열 검색어. 키가 없으면 빈 문자열로 본다. */
    filterText: Record<string, string>;
    /** 엑셀식 값 선택. 키가 없거나 null 이면 그 열은 전체 허용. */
    filterValues: Record<string, string[] | null>;
    /** 왼쪽에 붙잡아 둘 열 수. */
    pinned: number;
    /**
     * 값 선택 목록을 가져오는 방법.
     *
     * `needle` 은 후보를 좁히는 글자다. **거르는 일을 바탕까지 넘긴다** —
     * 받아 둔 목록을 화면에서 걸러 보여 주면, 값이 많아 목록이 앞에서 잘린
     * 열에서 찾는 값이 그 뒤에 있을 때 "없습니다" 가 뜬다.
     */
    loadValues: (columnKey: string, needle: string) => Promise<ColumnValuesResult>;
    busy?: boolean;
    emptyText?: string;
    /**
     * 열 검색·값 선택 줄을 그릴지. **끄면 자리도 차지하지 않는다.**
     *
     * 거르지 않는 표(한 학생의 성적표처럼 줄이 몇 개뿐인 것)에서는 이 줄이
     * 못 쓰는 칸만 늘어놓는다.
     */
    showFilters?: boolean;
  }>(),
  {
    busy: false,
    emptyText: "조건에 맞는 행이 없습니다. 필터를 줄여 보세요.",
    showFilters: true,
  },
);

const emit = defineEmits<{
  /** 헤더를 눌렀다(정렬). 쓰는 쪽이 cycleSort 로 처리한다. */
  sort: [key: string];
  "update:filterText": [value: Record<string, string>];
  "update:filterValues": [value: Record<string, string[] | null>];
  /** 열을 끌어 옮겼다. */
  move: [from: number, to: number];
  /** 열 너비를 끄는 중. */
  resize: [key: string, width: number];
  /** 너비 끌기가 끝났다(저장할 시점). */
  resizeEnd: [];
}>();

/** 클릭과 드래그를 가르는 이동 거리(px). */
const DRAG_THRESHOLD = 5;

const root = ref<HTMLElement | null>(null);
const headEl = ref<HTMLElement | null>(null);
function remPx(): number {
  const v = parseFloat(getComputedStyle(document.documentElement).fontSize);
  return Number.isFinite(v) && v > 0 ? v : 16;
}

/* ── 열 격자 · 열 고정 ─────────────────────────────────────── */

const colTemplate = computed(() =>
  props.columns
    // 나눠 갖는 열은 `width` 가 최소 너비다. 남는 자리는 그런 열끼리 똑같이
    // 갈라 가지므로, 최소 너비가 같으면 폭도 같아진다.
    .map((c) => (c.flex ? `minmax(${c.width}rem, ${c.flex}fr)` : `${c.width}rem`))
    .join(" "),
);

/** 열 너비의 합. 표의 가로 폭을 브라우저가 추정하지 않도록 직접 알려 준다. */
const totalWidth = computed(
  () => `${props.columns.reduce((sum, c) => sum + c.width, 0)}rem`,
);

const pinOffsets = computed(() => {
  const out: string[] = [];
  let acc = 0;
  for (let i = 0; i < props.pinned; i += 1) {
    out.push(`${acc}rem`);
    acc += props.columns[i]?.width ?? 0;
  }
  return out;
});

function pinIndex(i: number): number | undefined {
  return i < props.pinned ? i : undefined;
}

const gridVars = computed(() => {
  const vars: Record<string, string> = {
    "--dg-cols": colTemplate.value,
    "--dg-total": totalWidth.value,
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

/* ── 열 검색 · 값 선택 ─────────────────────────────────────── */

const openFilterKey = ref<string | null>(null);
const popX = ref(0);
const popY = ref(0);
/** 팝오버가 쓸 수 있는 최대 높이(px). 화면 밖으로 잘리지 않게 한다. */
const popH = ref(420);

const openFilterColumn = computed(
  () => props.columns.find((c) => c.key === openFilterKey.value) ?? null,
);

let filterTimer: number | undefined;
function onSearchInput(key: string, value: string) {
  const next = { ...props.filterText, [key]: value };
  window.clearTimeout(filterTimer);
  filterTimer = window.setTimeout(() => emit("update:filterText", next), 250);
}

function openFilter(col: LaidOutColumn, ev: MouseEvent) {
  if (!col.filterable) return;
  if (openFilterKey.value === col.key) {
    openFilterKey.value = null;
    return;
  }
  const box = root.value;
  const btn = ev.currentTarget as HTMLElement | null;
  if (box && btn) {
    const r = box.getBoundingClientRect();
    const b = btn.getBoundingClientRect();
    const POP_W = 320;
    const MARGIN = 12;
    // 팝오버는 표와 함께 가로로 스크롤되는 상자 안에 놓이지만, 잘리지 말아야 할
    // 기준은 **보이는 영역**이다. 상자는 화면보다 훨씬 넓을 수 있다.
    const view = box.closest(".app-main")?.getBoundingClientRect();
    const left = view?.left ?? 0;
    const right = view?.right ?? window.innerWidth;
    const top = view?.top ?? 0;
    const bottom = view?.bottom ?? window.innerHeight;

    popX.value =
      Math.max(left + MARGIN, Math.min(b.left, right - POP_W - MARGIN)) - r.left;

    // 위아래 중 넓은 쪽에 놓고, 그 공간에 맞춰 높이를 줄인다.
    const below = bottom - b.bottom - MARGIN;
    const above = b.top - top - MARGIN;
    if (below >= above) {
      popH.value = Math.min(420, below);
      popY.value = b.bottom - r.top + 4;
    } else {
      popH.value = Math.min(420, above);
      popY.value = b.top - r.top - popH.value - 4;
    }
  }
  openFilterKey.value = col.key;
}

function applyFilterValues(values: string[] | null) {
  const key = openFilterKey.value;
  if (!key) return;
  emit("update:filterValues", { ...props.filterValues, [key]: values });
  openFilterKey.value = null;
}

/* ── 열 순서 드래그 ────────────────────────────────────────────
   헤더를 누른 채 옆으로 끌면 순서 바꾸기, 그냥 누르면 정렬.
   DRAG_THRESHOLD 만큼 움직였는지로 둘을 가른다. */

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
      // 움직이지 않았으면 클릭으로 본다 → 정렬.
      const col = props.columns[from];
      if (col?.sortable) emit("sort", col.key);
    }
  }
  dragFrom.value = null;
  dropAt.value = null;
  dragging.value = false;
}

/* ── 열 너비 드래그 ────────────────────────────────────────── */

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
  emit("resize", resizeKey, resizeStartW + (ev.clientX - resizeStartX) / remPx());
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
    <div class="dg gx-table" :data-dragging="dragging" :style="gridVars">
      <!-- 열 헤더와 열 검색 줄. .app-main 기준으로 화면 맨 위에 붙는다. -->
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
            :data-drag-src="dragging && dragFrom === i"
            :data-drop="dragging && dropAt === i"
            :data-tip="
              col.sortable
                ? `${col.label}\n누르면 오름차순 → 내림차순 → 해제 · 옆으로 끌면 순서 바꾸기`
                : col.label
            "
            @pointerdown="onHeaderPointerDown(i, $event)"
          >
            <span class="dg-hlabel">{{ col.label }}</span>
            <!-- 방향만 보인다. 몇 번째로 정렬했는지는 바로 위 정렬 칩이 번호로
                 말해 주므로, 여기서 또 배지를 달면 머리글마다 그 폭을 늘 비워
                 두게 된다(정렬하지 않은 열까지). -->
            <span class="dg-sortmark" :data-visible="!!sortOf(col.key)" aria-hidden="true">
              {{ sortOf(col.key)?.dir === "desc" ? "▼" : "▲" }}
            </span>
            <span
              class="dg-resize"
              :data-active="resizingKey === col.key"
              role="separator"
              :aria-label="`${col.label} 너비 조절`"
              @pointerdown="onResizeStart(col, $event)"
            ></span>
          </div>
        </div>

        <!-- 왼쪽은 열 검색, 오른쪽 가로줄 단추는 엑셀식 값 선택 -->
        <div v-if="showFilters" class="dg-filters">
          <div
            v-for="(col, i) in columns"
            :key="col.key"
            class="dg-fcell"
            :data-pin="pinIndex(i)"
            :data-pin-last="i === pinned - 1"
          >
            <div class="dg-fwrap">
              <input
                class="dg-finput"
                type="search"
                :value="filterText[col.key] ?? ''"
                :disabled="!col.filterable"
                :placeholder="col.filterable ? '검색' : ''"
                :aria-label="`${col.label} 검색`"
                @input="onSearchInput(col.key, ($event.target as HTMLInputElement).value)"
              />
              <button
                type="button"
                class="dg-fbtn"
                :data-active="filterValues[col.key] != null"
                :disabled="!col.filterable"
                :aria-label="`${col.label} 값 선택`"
                :aria-expanded="openFilterKey === col.key"
                :data-tip="`${col.label} — 값 선택`"
                @click="openFilter(col, $event)"
              >
                <!-- 필터를 뜻하는 가로줄 셋. 여기에 ▼ 를 쓰면 바로 위 머리글의
                     내림차순 표식과 같은 글자가 되어, 정렬이 걸린 것처럼 보인다. -->
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
        <div v-for="(row, i) in rows" :key="i" class="dg-row" :data-odd="i % 2 === 1">
          <div
            v-for="(col, ci) in columns"
            :key="col.key"
            class="dg-cell"
            :data-kind="col.kind"
            :data-blank="isBlank(row[col.srcIndex])"
            :data-pin="pinIndex(ci)"
            :data-pin-last="ci === pinned - 1"
            :data-tip="String(row[col.srcIndex] ?? '')"
          >
            <!-- 칸을 특별하게 그려야 하는 화면(가능성 배지 등)만 이 자리를 쓴다. -->
            <slot name="cell" :col="col" :row="row" :value="row[col.srcIndex]">
              {{ cellText(col, row[col.srcIndex]) }}
            </slot>
          </div>
        </div>

        <p v-if="!busy && !rows.length" class="p-6 text-fg-subtle">{{ emptyText }}</p>
      </div>
    </div>

    <!-- 값 선택 팝오버. 표 위에 떠 있으므로 레이아웃에 영향을 주지 않는다. -->
    <ColumnFilter
      v-if="openFilterColumn"
      :key="openFilterColumn.key"
      :column="openFilterColumn"
      :selected="filterValues[openFilterColumn.key] ?? null"
      :load="(needle: string) => loadValues(openFilterColumn!.key, needle)"
      :x="popX"
      :y="popY"
      :max-height="popH"
      @apply="applyFilterValues"
      @close="openFilterKey = null"
    />
  </div>
</template>
