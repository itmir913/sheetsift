<script setup lang="ts">
/**
 * 조회 화면. 표와 조작부는 공용 컴포넌트를 쓰고, 여기서는 **데이터만** 챙긴다.
 * 정렬·필터·페이지는 SQL 로 처리한다(12만 행을 화면에서 다룰 수 없다).
 */
import { computed, onBeforeUnmount, onMounted, ref, toRef, watch } from "vue";
import GridTable from "@/components/GridTable.vue";
import GridToolbar from "@/components/GridToolbar.vue";
import PrintView from "@/components/PrintView.vue";
import {
  columnValues,
  cycleSort,
  fetchPage,
  fetchRows,
  pageSize,
  PRINT_LIMIT,
  type Filter,
  type HeaderSpec,
  type Row,
  type ScreenDef,
  type Sort,
} from "@/lib/grid";
import { useColumnLayout } from "@/composables/useColumnLayout";
import { AppError } from "@/lib/book";

const props = defineProps<{ screen: ScreenDef }>();

const emit = defineEmits<{ setHeader: [spec: HeaderSpec] }>();


/* ── 상태 ──────────────────────────────────────────────────── */

const rows = ref<Row[]>([]);
const total = ref(0);
const elapsedMs = ref(0);
const page = ref(0);
const sorts = ref<Sort[]>([]);
const filterText = ref<Record<string, string>>({});
const filterValues = ref<Record<string, string[] | null>>({});
const errorMessage = ref<string | null>(null);
const busy = ref(false);

const layout = useColumnLayout(toRef(props, "screen"));
const columns = layout.columns;

const gridRoot = ref<HTMLElement | null>(null);
const numberFormat = new Intl.NumberFormat("ko-KR");

/* ── 조회 ──────────────────────────────────────────────────── */

const activeFilters = computed<Filter[]>(() =>
  props.screen.columns
    .map((c) => ({
      key: c.key,
      contains: (filterText.value[c.key] ?? "").trim(),
      values: filterValues.value[c.key] ?? null,
    }))
    .filter((f) => f.contains !== "" || f.values !== null),
);

let token = 0;
async function load() {
  const mine = ++token;
  busy.value = true;
  errorMessage.value = null;
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
    errorMessage.value = e instanceof AppError ? e.message : String(e);
    rows.value = [];
  } finally {
    if (mine === token) {
      busy.value = false;
      // 첫 조회 결과로 열 너비를 잡는다. 두 번째부터는 아무 일도 하지 않는다.
      // 조회가 실패했어도 불러야 표가 그려진다.
      layout.measure(rows.value);
    }
  }
}

/** 정렬·필터가 바뀌면 첫 페이지부터 다시 본다. */
function reload() {
  page.value = 0;
  void load();
}

/**
 * 값 선택(▼) 후보. 그 열 자신의 필터는 빼고 나머지를 반영한다(엑셀과 같다).
 * 좁히는 글자까지 바탕에 넘긴다 — 잘린 목록을 화면에서 거르면 안 된다.
 */
function loadValues(columnKey: string, needle: string) {
  return columnValues(
    props.screen.id,
    props.screen.header,
    columnKey,
    activeFilters.value,
    needle,
  );
}

/* ── 페이지 이동 ───────────────────────────────────────────── */

/* 한 쪽에 담는 줄 수. **바탕이 정한다** — 여기 적어 두면 그보다 큰 수를
   달라고 하고선, 말없이 적게 받은 것을 다 받은 줄 안다. */
const PAGE = pageSize();

const pageCount = computed(() => Math.max(1, Math.ceil(total.value / PAGE)));
const firstRowNo = computed(() => (total.value === 0 ? 0 : page.value * PAGE + 1));
const lastRowNo = computed(() => Math.min(total.value, (page.value + 1) * PAGE));

function goto(p: number) {
  const next = Math.max(0, Math.min(p, pageCount.value - 1));
  if (next === page.value) return;
  page.value = next;
  void load();
  // 페이지를 넘기면 표 맨 위부터 보는 게 자연스럽다.
  gridRoot.value?.closest(".app-main")?.scrollTo({ top: 0 });
}

/** 페이지 번호 입력칸. 1부터 센다. */


/* ── 조작부에서 오는 것들 ───────────────────────────────────── */

function onSort(key: string) {
  sorts.value = cycleSort(sorts.value, key);
}

function onFilterText(next: Record<string, string>) {
  filterText.value = next;
  reload();
}

function onFilterValues(next: Record<string, string[] | null>) {
  filterValues.value = next;
  reload();
}

function clearFilters() {
  filterText.value = {};
  filterValues.value = {};
  reload();
}

function resetColumns() {
  layout.reset();
}

/* ── 인쇄 ──────────────────────────────────────────────────────
   지금 화면의 정렬·필터를 그대로 종이에 낸다. 보이는 페이지가 아니라
   조건에 맞는 전체(상한까지)를 낸다. */

const printRows = ref<Row[]>([]);
const printTruncated = ref(false);
const printing = ref(false);

async function print() {
  if (printing.value) return;
  printing.value = true;
  errorMessage.value = null;
  try {
    const res = await fetchRows(
      props.screen.id,
      props.screen.header,
      sorts.value,
      activeFilters.value,
      PRINT_LIMIT,
    );
    printRows.value = res.rows;
    printTruncated.value = res.truncated;
    await new Promise((r) => requestAnimationFrame(() => r(null)));
    window.print();
  } catch (e) {
    errorMessage.value = e instanceof AppError ? e.message : String(e);
  } finally {
    printRows.value = [];
    printTruncated.value = false;
    printing.value = false;
  }
}

/* ── 생명주기 ──────────────────────────────────────────────── */

onMounted(load);
onBeforeUnmount(() => {
  token += 1; // 떠난 뒤 도착한 응답을 무시한다
});

watch(
  () => props.screen.id,
  () => {
    sorts.value = [];
    filterText.value = {};
    filterValues.value = {};
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
      :header-guessed="screen.headerGuessed"
      :sheet-rows="screen.sheetRows"
      :columns="columns"
      :pinned="layout.pinned.value"
      :pin-limit="layout.pinLimit.value"
      :page="page"
      :page-count="pageCount"
      :busy="busy"
      :columns-customized="layout.isCustomized.value"
      :has-filters="activeFilters.length > 0"
      :printing="printing"
      :print-disabled="total === 0"
      @remove-sort="(k) => (sorts = sorts.filter((s) => s.key !== k))"
      @clear-sorts="sorts = []"
      @clear-filters="clearFilters"
      @reset-columns="resetColumns"
      @set-pinned="layout.setPinned"
      @print="print"
      @goto="goto"
      @set-header="(s: HeaderSpec) => emit('setHeader', s)"
    >
      <template #status>
        <span class="shrink-0 whitespace-nowrap text-right tabular-nums text-fg-muted">
          {{ numberFormat.format(firstRowNo) }}–{{ numberFormat.format(lastRowNo) }} /
          {{ numberFormat.format(total) }}행
          <span class="text-fg-subtle">· {{ busy ? "…" : `${elapsedMs.toFixed(1)}ms` }}</span>
        </span>
      </template>
    </GridToolbar>

    <p v-if="errorMessage" class="app-fixed shrink-0 text-danger">{{ errorMessage }}</p>

    <!-- 너비를 다 재기 전에는 그리지 않는다. 머리글만으로 한 번 그린 뒤
         자료를 보고 다시 잡으면, 화면을 열 때마다 표가 한 번 튄다. -->
    <GridTable
      v-if="layout.ready.value"
      :columns="columns"
      :rows="rows"
      :sorts="sorts"
      :filter-text="filterText"
      :filter-values="filterValues"
      :pinned="layout.pinned.value"
      :load-values="loadValues"
      :busy="busy"
      @sort="onSort"
      @update:filter-text="onFilterText"
      @update:filter-values="onFilterValues"
      @move="layout.move"
      @resize="layout.resize"
      @resize-end="layout.commit"
    />

    <!-- 인쇄 뷰. 화면에서는 숨겨져 있고 @media print 에서만 나타난다. -->
    <Teleport to="body">
      <div v-if="printRows.length" class="print-root">
        <PrintView
          :title="screen.title"
          :description="screen.description"
          :columns="columns"
          :rows="printRows"
          :total="total"
          :truncated="printTruncated"
          :sorts="sorts"
          :filters="activeFilters"
        />
      </div>
    </Teleport>
  </div>
</template>
