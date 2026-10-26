<script setup lang="ts">
/**
 * 표 상단의 조작부. 표를 사용하는 화면은 전부 이것만 사용한다.
 *
 * **2행으로 고정한다.** 상단은 설정·페이지, 하단은 정렬 칩이다. 1행에 집약했더니
 * 정렬이 증가할수록 우측 설정 단추를 압출해 절단되었다. 정렬이 부재할 때도
 * 하단 행 높이를 존치하므로, 정렬을 적용하고 해제해도 표가 수직 이동하지
 * 않는다.
 */
import { ref, watch } from "vue";
import { t, tn } from "@/i18n";
import { grouping, setGrouping } from "@/composables/useGrouping";
import type { HeaderSpec, Sort } from "@/lib/grid";
import type { LaidOutColumn } from "@/composables/useColumnLayout";

const props = withDefaults(
  defineProps<{
    /**
     * 열을 취급하는 영역(열 고정·초기화·정렬 칩)을 축소한다.
     *
     * 표가 아니라 도형을 묘사하는 화면(범위 차트)이 사용한다. 거기에는 고정할
     * 열도 정렬 칩도 부재하는데, 페이지 이동은 표와 **동일한 것**을 사용해야
     * 분기하지 않는다.
     */
    compact?: boolean;
    sorts?: Sort[];
    columns?: LaidOutColumn[];
    pinned?: number;
    pinLimit?: number;
    /** 열 순서·폭을 조작한 이력이 존재해 '열 초기화' 가 의미를 보유하는가. */
    columnsCustomized?: boolean;
    hasFilters?: boolean;
    /** 인쇄를 사용하는 화면인가. 인쇄 대상이 부재하면 은폐한다. */
    canPrint?: boolean;
    printing?: boolean;
    printDisabled?: boolean;
    /**
     * 헤더 지정. 부재하면 조작부에 그 영역을 묘사하지 않는다.
     *
     * 양식마다 헤더가 1행이기도 2행이기도 하고, 상단에 제목 행이 부착되기도
     * 한다. **추정으로 전부 적중시킬 수 없으므로** 조작부를 상주시킨다.
     *
     * 그 지정이 추정인지는 **표시하지 않는다.** 적중 여부는 묘사된 표가
     * 진술하고, 상이하면 사람이 여기서 이동시킨다 — 매 시트에 부착되는 경고는
     * 그 조작을 안내하지 못한 채 잡음만 증가시킨다.
     */
    header?: HeaderSpec | null;
    /** 시트의 총 행 수. 지정 가능한 범위의 상한이다. */
    sheetRows?: number;
    /**
     * 시트가 양식에서 개시되는 행(0부터). **표시에만 가산한다** — 바탕과
     * 수수하는 지정은 소거된 범위를 기준으로 산정한 값 그대로다.
     */
    rowOrigin?: number;
    /** 현 페이지(0부터). 페이지 이동이 부재하는 화면은 전달하지 않는다. */
    page?: number;
    /** 전체 페이지 수. 전달하지 않으면 페이지 이동을 묘사하지 않는다. */
    pageCount?: number;
    busy?: boolean;
  }>(),
  {
    compact: false,
    sorts: () => [],
    columns: () => [],
    pinned: 0,
    pinLimit: 0,
    header: null,
    sheetRows: 0,
    rowOrigin: 0,
    columnsCustomized: false,
    hasFilters: false,
    canPrint: true,
    printing: false,
    printDisabled: false,
    busy: false,
  },
);

const emit = defineEmits<{
  removeSort: [key: string];
  clearSorts: [];
  clearFilters: [];
  resetColumns: [];
  setPinned: [n: number];
  print: [];
  goto: [page: number];
  /** 헤더 범위를 변경했다. */
  setHeader: [spec: HeaderSpec];
}>();

/* ── 헤더 범위 ───────────────────────────────────────────────
   사람이 참조하는 수치는 **1부터**다. 엑셀의 행 번호와 일치해야 원본을 참조하며
   지정할 수 있다. 바탕은 0부터 산정하므로 수수 시 환산하고, 선두의 공백 행이
   소거된 양식은 그 소거량(`rowOrigin`)까지 가산해야 엑셀과 일치한다. */

function moveStart(by: number) {
  const h = props.header;
  if (!h) return;
  const at = Math.max(0, Math.min(h.at + by, Math.max(0, props.sheetRows - h.rows)));
  if (at !== h.at) emit("setHeader", { at, rows: h.rows });
}

function moveRows(by: number) {
  const h = props.header;
  if (!h) return;
  const rows = Math.max(1, Math.min(h.rows + by, Math.max(1, props.sheetRows - h.at)));
  if (rows !== h.rows) emit("setHeader", { at: h.at, rows });
}

/* ── 페이지 이동 ───────────────────────────────────────────────
   **여기 존재해야 한다.** 화면마다 개별 묘사하면 조회 화면에는 존재하고 성적
   검색에는 부재하는 식으로 분기한다. 실제로 그렇게 분기해 있었다.  */

const pageInput = ref("1");

watch(
  () => props.page,
  (p) => (pageInput.value = String((p ?? 0) + 1)),
  { immediate: true },
);

function goto(p: number) {
  const count = props.pageCount ?? 1;
  emit("goto", Math.max(0, Math.min(p, count - 1)));
}

function applyPageInput() {
  const n = Number(pageInput.value);
  if (Number.isFinite(n) && n >= 1) goto(Math.floor(n) - 1);
  else pageInput.value = String((props.page ?? 0) + 1);
}

function labelOf(key: string): string {
  return props.columns.find((c) => c.key === key)?.label ?? key;
}
</script>

<template>
  <div class="app-fixed gx-tools">
    <!-- 상단 행: 페이지 이동(좌측) · 설정(우측).
         폭이 부족하면 설정 단추가 차기 행으로 이월된다. -->
    <div class="gx-tools-row">
      <span v-if="pageCount !== undefined" class="gx-pager">
        <button
          type="button"
          class="gx-pager-btn"
          :disabled="(page ?? 0) === 0 || busy"
          :aria-label="t('pager.first')"
          :data-tip="t('pager.first')"
          @click="goto(0)"
        >
          ⏮
        </button>
        <button
          type="button"
          class="gx-pager-btn"
          :disabled="(page ?? 0) === 0 || busy"
          :aria-label="t('pager.prev')"
          :data-tip="t('pager.prev')"
          @click="goto((page ?? 0) - 1)"
        >
          ◀
        </button>
        <input
          v-model="pageInput"
          class="gx-pager-input"
          inputmode="numeric"
          :aria-label="t('pager.page.aria')"
          @keydown.enter="applyPageInput"
          @blur="applyPageInput"
        />
        <span class="text-fg-muted">{{ tn("pager.of", pageCount) }}</span>
        <button
          type="button"
          class="gx-pager-btn"
          :disabled="(page ?? 0) >= pageCount - 1 || busy"
          :aria-label="t('pager.next')"
          :data-tip="t('pager.next')"
          @click="goto((page ?? 0) + 1)"
        >
          ▶
        </button>
        <button
          type="button"
          class="gx-pager-btn"
          :disabled="(page ?? 0) >= pageCount - 1 || busy"
          :aria-label="t('pager.last')"
          :data-tip="t('pager.last')"
          @click="goto(pageCount - 1)"
        >
          ⏭
        </button>
      </span>

      <span v-if="!compact" class="gx-tools-set">
        <span class="gx-pin">
          <span class="whitespace-nowrap text-fg-muted">{{ t("tools.pin") }}</span>
          <button
            type="button"
            class="gx-pin-btn"
            :disabled="pinned <= 0"
            :aria-label="t('tools.pin.less.aria')"
            @click="emit('setPinned', pinned - 1)"
          >
            −
          </button>
          <span
            class="min-w-4 shrink-0 whitespace-nowrap text-center tabular-nums text-fg"
          >
            {{ pinned }}
          </span>
          <button
            type="button"
            class="gx-pin-btn"
            :disabled="pinned >= pinLimit"
            :aria-label="t('tools.pin.more.aria')"
            @click="emit('setPinned', pinned + 1)"
          >
            ＋
          </button>
        </span>

        <span v-if="header" class="gx-pin">
          <span class="whitespace-nowrap text-fg-muted">{{ t("tools.header") }}</span>
          <button
            type="button"
            class="gx-pin-btn"
            :disabled="header.at <= 0"
            :aria-label="t('tools.header.up.aria')"
            :data-tip="t('tools.header.up.tip')"
            @click="moveStart(-1)"
          >
            ↑
          </button>
          <span class="whitespace-nowrap tabular-nums text-fg">
            <template v-if="header.rows === 1">
              {{ t("tools.header.single", { a: rowOrigin + header.at + 1 }) }}
            </template>
            <template v-else>
              {{
                t("tools.header.range", {
                  a: rowOrigin + header.at + 1,
                  b: rowOrigin + header.at + header.rows,
                })
              }}
            </template>
          </span>
          <button
            type="button"
            class="gx-pin-btn"
            :disabled="header.at + header.rows >= sheetRows"
            :aria-label="t('tools.header.down.aria')"
            :data-tip="t('tools.header.down.tip')"
            @click="moveStart(1)"
          >
            ↓
          </button>
          <button
            type="button"
            class="gx-pin-btn"
            :disabled="header.rows <= 1"
            :aria-label="t('tools.header.less.aria')"
            :data-tip="t('tools.header.less.tip')"
            @click="moveRows(-1)"
          >
            −
          </button>
          <span
            class="min-w-4 shrink-0 whitespace-nowrap text-center tabular-nums text-fg"
          >
            {{ header.rows }}
          </span>
          <button
            type="button"
            class="gx-pin-btn"
            :disabled="header.at + header.rows >= sheetRows"
            :aria-label="t('tools.header.more.aria')"
            :data-tip="t('tools.header.more.tip')"
            @click="moveRows(1)"
          >
            ＋
          </button>
        </span>

        <button
          v-if="canPrint"
          type="button"
          class="gx-tool-btn"
          :disabled="printing || printDisabled"
          :data-tip="t('tools.print.tip')"
          @click="emit('print')"
        >
          {{ t(printing ? "tools.print.busy" : "tools.print") }}
        </button>
        <button
          type="button"
          class="gx-tool-btn"
          :disabled="!columnsCustomized"
          :data-tip="t('tools.columns.reset.tip')"
          @click="emit('resetColumns')"
        >
          {{ t("tools.columns.reset") }}
        </button>
        <button
          type="button"
          class="gx-tool-btn"
          :disabled="!sorts.length"
          @click="emit('clearSorts')"
        >
          {{ t("tools.sorts.clear") }}
        </button>
        <button
          type="button"
          class="gx-tool-btn"
          :disabled="!hasFilters"
          @click="emit('clearFilters')"
        >
          {{ t("tools.filters.clear") }}
        </button>

        <!-- 천 단위 구분. **사람이 선택한다** — 어느 열이 식별자인지 이 앱은
             판정하지 않는다. -->
        <label class="gx-tool-btn gx-tool-check" :data-tip="t('tools.grouping.tip')">
          <input type="checkbox" :checked="grouping" @change="setGrouping(!grouping)" />
          {{ t("tools.grouping") }}
        </label>
      </span>
    </div>

    <!-- 하단 행: 정렬 칩. 증가하면 이 행만 수평 이동한다. -->
    <div v-if="!compact" class="gx-tools-row gx-sorts">
      <span class="shrink-0 font-medium text-fg-muted">
        {{ t(sorts.length ? "tools.sorts" : "tools.help") }}
      </span>
      <div class="gx-sort-list">
        <button
          v-for="(s, i) in sorts"
          :key="s.key"
          type="button"
          class="gx-sort-chip"
          :data-tip="t('tools.sort.remove.tip', { label: labelOf(s.key) })"
          @click="emit('removeSort', s.key)"
        >
          <span class="dg-rank">{{ i + 1 }}</span>
          <span class="font-medium">{{ labelOf(s.key) }}</span>
          <span>{{ s.dir === "asc" ? "▲" : "▼" }}</span>
          <span class="text-fg-subtle">✕</span>
        </button>

        <!-- 정렬이 전무할 때만 표시된다. 정렬뿐 아니라 표를 취급하는 방법
             전체를 제시하는 위치라, 좌측 표지도 '설명' 으로 전환된다. 2문자로
             길이가 동일해 표지가 전환되어도 문구가 수평 이동하지 않는다. -->
        <span v-if="!sorts.length" class="truncate text-fg-subtle">
          {{ t("tools.help.text") }}
        </span>
      </div>
      <slot name="status" />
    </div>
  </div>
</template>
