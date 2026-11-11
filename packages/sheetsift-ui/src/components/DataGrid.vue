<script setup lang="ts">
/**
 * 조회 화면. 표와 조작부는 공용 컴포넌트를 사용하고, 여기서는 **자료만**
 * 확보한다. 정렬·필터·페이지는 SQL 로 처리한다(12만 행을 화면에서 취급할 수
 * 없다).
 */
import { computed, onBeforeUnmount, onMounted, ref, toRef, watch } from "vue";
import GridTable from "./GridTable.vue";
import GridToolbar from "./GridToolbar.vue";
import PrintView from "./PrintView.vue";
import {
  own,
  columnValues,
  cycleSort,
  dropColumnState,
  fetchPage,
  pageSize,
  type Filter,
  type HeaderSpec,
  type NumberRange,
  type Row,
  type ScreenDef,
  type Sort,
  type ValuePick,
} from "../lib/grid";
import { useFault } from "../composables/useFault";
import { useColumnLayout } from "../composables/useColumnLayout";
import { formatNumber, source, t, tn } from "../lib/inject";

const props = defineProps<{ screen: ScreenDef }>();

const emit = defineEmits<{ setHeader: [spec: HeaderSpec] }>();


/* ── 상태 ──────────────────────────────────────────────────── */

const rows = ref<Row[]>([]);
const total = ref(0);
const elapsedMs = ref(0);
const page = ref(0);
const sorts = ref<Sort[]>([]);
const filterText = ref<Record<string, string>>({});
const filterValues = ref<Record<string, ValuePick | null>>({});
const filterRanges = ref<Record<string, NumberRange | null>>({});
/* 오류는 코드와 치환 값으로 보유한다. 문구로 보유하면 언어 전환을 추종하지 않는다. */
const { message: errorMessage, capture: captureFault, clear: clearFault } = useFault();
const busy = ref(false);

const layout = useColumnLayout(toRef(props, "screen"));
const columns = layout.columns;

const gridRoot = ref<HTMLElement | null>(null);

/* ── 조회 ──────────────────────────────────────────────────── */

const activeFilters = computed<Filter[]>(() =>
  props.screen.columns
    .map((c) => {
      const pick = own(filterValues.value, c.key) ?? null;
      const range = own(filterRanges.value, c.key) ?? null;
      return {
        key: c.key,
        contains: (own(filterText.value, c.key) ?? "").trim(),
        values: pick?.mode === "include" ? pick.values : null,
        excluded: pick?.mode === "exclude" ? pick.values : null,
        // 숫자 열이 아니면 범위가 올라올 수 없다(그 팝오버에는 범위 칸이 없다).
        min: range?.min ?? null,
        max: range?.max ?? null,
      };
    })
    .filter(
      (f) =>
        f.contains !== "" ||
        f.values !== null ||
        f.excluded !== null ||
        f.min !== null ||
        f.max !== null,
    ),
);

let token = 0;
async function load() {
  const mine = ++token;
  busy.value = true;
  clearFault();
  try {
    const res = await fetchPage(
      props.screen.id,
      props.screen.header,
      sorts.value,
      activeFilters.value,
      page.value,
    );
    if (mine !== token) return;
    rows.value = res.rows;
    total.value = res.total;
    elapsedMs.value = res.elapsedMs;
  } catch (e) {
    if (mine !== token) return;
    captureFault(e);
    rows.value = [];
  } finally {
    if (mine === token) {
      busy.value = false;
      // 최초 조회 결과로 열 폭을 산정한다. 2회차부터는 아무 작용도 하지 않는다.
      // 조회가 실패했어도 호출해야 표가 묘사된다.
      layout.measure(rows.value);
    }
  }
}

/** 정렬·필터가 변경되면 첫 페이지부터 재조회한다. */
function reload() {
  page.value = 0;
  void load();
}

/**
 * 값 선택(▼) 후보. 그 열 자체의 필터는 제외하고 나머지를 반영한다(엑셀과
 * 동일하다). 축소하는 문자까지 바탕에 전달한다 — 절단된 목록을 화면에서
 * 여과하면 안 된다.
 */
function loadValues(columnKey: string, needle: string, range: NumberRange | null) {
  /* **팝오버가 지금 보유한 범위로 목록을 만든다.** 적용하기 전에 손잡이를 움직인 범위가 목록에
     반영되어야 한다 — 적용된 범위(`activeFilters`)는 이전 것이다. 같은 열의 이전 필터는 바탕이
     값 선택은 빼고 범위만 적용하므로, 그 열의 항목은 새 범위로만 갈아 끼운다. */
  const others = activeFilters.value.filter((f) => f.key !== columnKey);
  const ownRange: Filter[] = range
    ? [{ key: columnKey, contains: "", values: null, excluded: null, min: range.min, max: range.max }]
    : [];
  return columnValues(props.screen.id, props.screen.header, columnKey, [...others, ...ownRange], needle);
}

/* ── 페이지 이동 ───────────────────────────────────────────── */

/* 한 페이지에 수용하는 행 수. **바탕이 결정한다** — 여기 기재하면 그보다 큰
   수를 요청하고선, 묵시적으로 적게 수령한 것을 전량 수령한 것으로 판단한다. */
const PAGE = pageSize();

const pageCount = computed(() => Math.max(1, Math.ceil(total.value / PAGE)));
const firstRowNo = computed(() => (total.value === 0 ? 0 : page.value * PAGE + 1));
const lastRowNo = computed(() => Math.min(total.value, (page.value + 1) * PAGE));

function goto(p: number) {
  const next = Math.max(0, Math.min(p, pageCount.value - 1));
  if (next === page.value) return;
  page.value = next;
  void load();
  // 페이지를 이동하면 표 최상단부터 열람하는 것이 자연스럽다.
  gridRoot.value?.closest(".app-main")?.scrollTo({ top: 0 });
}

/** 페이지 번호 입력란. 1부터 산정한다. */


/* ── 조작부에서 전달되는 것들 ───────────────────────────────── */

function onSort(key: string) {
  sorts.value = cycleSort(sorts.value, key);
}

function onFilterText(next: Record<string, string>) {
  filterText.value = next;
  reload();
}

function onFilterValues(next: Record<string, ValuePick | null>) {
  filterValues.value = next;
  reload();
}

function onFilterRanges(next: Record<string, NumberRange | null>) {
  filterRanges.value = next;
  /* 조회하지 않는다. 범위는 값 선택과 **같은 적용에서** 먼저 도달하고, 조회는 뒤따르는 값
     선택이 수행한다(`GridTable.applyFilterValues`). 여기서도 부르면 같은 조회가 두 번 간다. */
}

function clearFilters() {
  filterText.value = {};
  filterValues.value = {};
  filterRanges.value = {};
  reload();
}

/** 열 선택을 적용한다. 숨긴 열에 남은 정렬·여과는 지운다(보이지 않는 규칙이 행을 거른다). */
function applyColumns(next: { order: string[]; hidden: string[]; dropped: string[] }) {
  layout.applyColumns(next);
  if (!next.dropped.length) return;
  const had = activeFilters.value.length;
  const kept = dropColumnState(next.dropped, {
    sorts: sorts.value,
    filterText: filterText.value,
    filterValues: filterValues.value,
    filterRanges: filterRanges.value,
  });
  sorts.value = kept.sorts;
  filterText.value = kept.filterText;
  filterValues.value = kept.filterValues;
  filterRanges.value = kept.filterRanges;
  /* 정렬은 watch 가 다시 읽는다. 여과가 바뀌었을 때만 직접 다시 읽는다. */
  if (activeFilters.value.length !== had) reload();
}

function resetColumns() {
  layout.reset();
}

/* ── 인쇄 ──────────────────────────────────────────────────────
   현 화면의 정렬·필터를 그대로 지면에 출력한다. 노출된 페이지가 아니라
   조건에 부합하는 전체를 출력한다. 몇 행을 출력할지는 받는 앱이 결정하며
   (`GridSource.printRows`), 이 화면은 응답이 준 행을 그대로 출력한다. */

const printRows = ref<Row[]>([]);
const printTotal = ref(0);
const printTruncated = ref(false);
const printing = ref(false);

async function print() {
  if (printing.value) return;
  printing.value = true;
  clearFault();
  try {
    const res = await source().printRows({
      screenId: props.screen.id,
      header: props.screen.header,
      sorts: sorts.value,
      filters: activeFilters.value,
    });
    // 인쇄할 행이 없으면(상한이 0 이거나 부합하는 행이 없다) 조용히 종료한다.
    if (res.rows.length === 0) return;
    printRows.value = res.rows;
    printTotal.value = res.total;
    printTruncated.value = res.total > res.rows.length;
    await new Promise((r) => requestAnimationFrame(() => r(null)));
    window.print();
  } catch (e) {
    captureFault(e);
  } finally {
    printRows.value = [];
    printTotal.value = 0;
    printTruncated.value = false;
    printing.value = false;
  }
}

/* ── 생명주기 ──────────────────────────────────────────────── */

onMounted(load);
onBeforeUnmount(() => {
  token += 1; // 이탈 이후 도달한 응답을 무시한다
});

watch(
  () => props.screen.id,
  () => {
    sorts.value = [];
    filterText.value = {};
    filterValues.value = {};
    filterRanges.value = {};
    reload();
  },
);

watch(sorts, reload, { deep: true });
</script>

<template>
  <div ref="gridRoot" class="app-lane flex flex-col gap-3">
    <GridToolbar
      :sorts="sorts"
      :header="screen.header"
      :row-origin="screen.rowOrigin"
      :sheet-rows="screen.sheetRows"
      :columns="columns"
      :all-columns="layout.allColumns.value"
      :pinned="layout.pinned.value"
      :pin-limit="layout.pinLimit.value"
      :page="page"
      :page-count="pageCount"
      :busy="busy"
      :columns-customized="layout.isCustomized.value"
      :has-filters="activeFilters.length > 0"
      :printing="printing"
      :can-print="source().printEnabled()"
      :print-disabled="total === 0"
      @remove-sort="(k) => (sorts = sorts.filter((s) => s.key !== k))"
      @clear-sorts="sorts = []"
      @clear-filters="clearFilters"
      @reset-columns="resetColumns"
      @apply-columns="applyColumns"
      @set-pinned="layout.setPinned"
      @print="print"
      @goto="goto"
      @set-header="(s: HeaderSpec) => emit('setHeader', s)"
    >
      <template #status>
        <span class="shrink-0 whitespace-nowrap text-right tabular-nums text-fg-muted">
          {{
            t("grid.status", {
              from: formatNumber(firstRowNo),
              to: formatNumber(lastRowNo),
              total: tn("grid.rows", total),
            })
          }}
          <span class="text-fg-subtle">· {{ busy ? "…" : `${elapsedMs.toFixed(1)}ms` }}</span>
        </span>
      </template>
    </GridToolbar>

    <p v-if="errorMessage" class="app-fixed shrink-0 text-danger">{{ errorMessage }}</p>

    <!-- 폭을 전부 측정하기 이전에는 묘사하지 않는다. 헤더만으로 1회 묘사한
         뒤 자료를 참조해 재산정하면, 화면을 개방할 때마다 표가 1회 변동한다. -->
    <GridTable
      v-if="layout.ready.value"
      :columns="columns"
      :detail-columns="layout.detailColumns.value"
      :rows="rows"
      :sorts="sorts"
      :filter-text="filterText"
      :filter-values="filterValues"
      :filter-ranges="filterRanges"
      :pinned="layout.pinned.value"
      :load-values="loadValues"
      :busy="busy"
      @sort="onSort"
      @update:filter-text="onFilterText"
      @update:filter-values="onFilterValues"
      @update:filter-ranges="onFilterRanges"
      @move="layout.move"
      @resize="layout.resize"
      @resize-end="layout.commit"
    >
      <!-- 받는 앱이 셀을 직접 묘사하는 경우만 전달한다. 슬롯이 부재하면 표의
           기본 묘사가 그대로 사용된다. -->
      <template v-if="$slots.cell" #cell="slotProps">
        <slot name="cell" v-bind="slotProps" />
      </template>
    </GridTable>

    <!-- 인쇄 뷰. 화면에서는 은폐되어 있고 @media print 에서만 출현한다. -->
    <Teleport to="body">
      <div v-if="printRows.length" class="print-root">
        <PrintView
          :title="screen.title"
          :description="screen.description"
          :columns="columns"
          :rows="printRows"
          :total="printTotal"
          :truncated="printTruncated"
          :sorts="sorts"
          :filters="activeFilters"
        />
      </div>
    </Teleport>
  </div>
</template>
