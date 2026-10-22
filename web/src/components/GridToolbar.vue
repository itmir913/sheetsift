<script setup lang="ts">
/**
 * 표 위의 조작부. 표를 쓰는 화면은 전부 이것만 쓴다.
 *
 * **두 줄로 못 박아 둔다.** 위는 설정·페이지, 아래는 정렬 칩이다.
 * 한 줄에 몰아 넣었더니 정렬이 늘어날수록 오른쪽 설정 단추를 밀어내 잘렸다.
 * 정렬이 없을 때도 아래 줄 높이를 남겨 두므로, 정렬을 걸고 풀어도 표가
 * 위아래로 움직이지 않는다.
 */
import { ref, watch } from "vue";
import type { Sort } from "@/lib/grid";
import type { LaidOutColumn } from "@/composables/useColumnLayout";

const props = withDefaults(
  defineProps<{
    /**
     * 열을 다루는 칸(열 고정·초기화·정렬 칩)을 접는다.
     *
     * 표가 아니라 그림을 그리는 화면(범위 차트)이 쓴다. 거기엔 고정할 열도
     * 정렬 칩도 없는데, 쪽 넘기기는 표와 **같은 것**을 써야 갈라지지 않는다.
     */
    compact?: boolean;
    sorts?: Sort[];
    columns?: LaidOutColumn[];
    pinned?: number;
    pinLimit?: number;
    /** 열 순서·너비를 손댄 적이 있어 '열 초기화'가 뜻이 있는가. */
    columnsCustomized?: boolean;
    hasFilters?: boolean;
    /** 인쇄를 쓰는 화면인가. 인쇄할 것이 없으면 숨긴다. */
    canPrint?: boolean;
    printing?: boolean;
    printDisabled?: boolean;
    /** 지금 쪽(0부터). 쪽 넘김이 없는 화면은 넘기지 않는다. */
    page?: number;
    /** 전체 쪽 수. 넘기지 않으면 쪽 넘김을 그리지 않는다. */
    pageCount?: number;
    busy?: boolean;
  }>(),
  {
    compact: false,
    sorts: () => [],
    columns: () => [],
    pinned: 0,
    pinLimit: 0,
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
}>();

/* ── 쪽 넘김 ───────────────────────────────────────────────────
   **여기 있어야 한다.** 화면마다 따로 그리면 조회 화면에는 있고 성적
   검색에는 없는 식으로 갈라진다. 실제로 그렇게 갈라져 있었다.      */

const numberFormat = new Intl.NumberFormat("ko-KR");
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
    <!-- 위 줄: 페이지 이동(왼쪽) · 설정(오른쪽).
         폭이 모자라면 설정 단추가 다음 줄로 넘어간다. -->
    <div class="gx-tools-row">
      <span v-if="pageCount !== undefined" class="gx-pager">
        <button
          type="button"
          class="gx-pager-btn"
          :disabled="(page ?? 0) === 0 || busy"
          aria-label="첫 페이지"
          data-tip="첫 페이지"
          @click="goto(0)"
        >
          ⏮
        </button>
        <button
          type="button"
          class="gx-pager-btn"
          :disabled="(page ?? 0) === 0 || busy"
          aria-label="이전 페이지"
          data-tip="이전 페이지"
          @click="goto((page ?? 0) - 1)"
        >
          ◀
        </button>
        <input
          v-model="pageInput"
          class="gx-pager-input"
          inputmode="numeric"
          aria-label="페이지 번호"
          @keydown.enter="applyPageInput"
          @blur="applyPageInput"
        />
        <span class="text-fg-muted">/ {{ numberFormat.format(pageCount) }}쪽</span>
        <button
          type="button"
          class="gx-pager-btn"
          :disabled="(page ?? 0) >= pageCount - 1 || busy"
          aria-label="다음 페이지"
          data-tip="다음 페이지"
          @click="goto((page ?? 0) + 1)"
        >
          ▶
        </button>
        <button
          type="button"
          class="gx-pager-btn"
          :disabled="(page ?? 0) >= pageCount - 1 || busy"
          aria-label="마지막 페이지"
          data-tip="마지막 페이지"
          @click="goto(pageCount - 1)"
        >
          ⏭
        </button>
      </span>

      <span v-if="!compact" class="gx-tools-set">
        <span class="gx-pin">
          <span class="whitespace-nowrap text-fg-muted">열 고정</span>
          <button
            type="button"
            class="gx-pin-btn"
            :disabled="pinned <= 0"
            aria-label="고정 열 줄이기"
            @click="emit('setPinned', pinned - 1)"
          >
            −
          </button>
          <span class="w-4 text-center tabular-nums text-fg">{{ pinned }}</span>
          <button
            type="button"
            class="gx-pin-btn"
            :disabled="pinned >= pinLimit"
            aria-label="고정 열 늘리기"
            @click="emit('setPinned', pinned + 1)"
          >
            ＋
          </button>
        </span>

        <button
          v-if="canPrint"
          type="button"
          class="gx-tool-btn"
          :disabled="printing || printDisabled"
          data-tip="지금 화면의 정렬·필터를 그대로 인쇄합니다"
          @click="emit('print')"
        >
          {{ printing ? "준비 중…" : "인쇄" }}
        </button>
        <button
          type="button"
          class="gx-tool-btn"
          :disabled="!columnsCustomized"
          data-tip="열 순서와 너비를 기본값으로"
          @click="emit('resetColumns')"
        >
          열 초기화
        </button>
        <button
          type="button"
          class="gx-tool-btn"
          :disabled="!sorts.length"
          @click="emit('clearSorts')"
        >
          정렬 초기화
        </button>
        <button
          type="button"
          class="gx-tool-btn"
          :disabled="!hasFilters"
          @click="emit('clearFilters')"
        >
          필터 초기화
        </button>
      </span>
    </div>

    <!-- 아래 줄: 정렬 칩. 많아지면 이 줄만 가로로 밀린다. -->
    <div v-if="!compact" class="gx-tools-row gx-sorts">
      <span class="shrink-0 font-medium text-fg-muted">
        {{ sorts.length ? "정렬" : "설명" }}
      </span>
      <div class="gx-sort-list">
        <button
          v-for="(s, i) in sorts"
          :key="s.key"
          type="button"
          class="gx-sort-chip"
          :data-tip="`${labelOf(s.key)} 정렬 해제`"
          @click="emit('removeSort', s.key)"
        >
          <span class="dg-rank">{{ i + 1 }}</span>
          <span class="font-medium">{{ labelOf(s.key) }}</span>
          <span>{{ s.dir === "asc" ? "▲" : "▼" }}</span>
          <span class="text-fg-subtle">✕</span>
        </button>

        <!-- 정렬이 하나도 없을 때만 뜬다. 정렬만이 아니라 표를 다루는 법 전체를
             알려 주는 자리라, 왼쪽 딱지도 '설명'으로 바뀐다. 두 글자로 길이가
             같아 딱지가 바뀌어도 문구가 좌우로 움직이지 않는다. -->
        <span v-if="!sorts.length" class="truncate text-fg-subtle">
          열 제목을 눌러 정렬 / 열을 드래그해 순서 변경 /
          열 사이 선을 드래그해 너비 조절
        </span>
      </div>
      <slot name="status" />
    </div>
  </div>
</template>
