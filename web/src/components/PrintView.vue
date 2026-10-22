<script setup lang="ts">
import { computed } from "vue";
import { APP_NAME } from "@/lib/app";
import { cellText } from "@/lib/grid";
import type { Cell, Filter, Row, Sort } from "@/lib/grid";
import type { LaidOutColumn } from "@/composables/useColumnLayout";

const props = withDefaults(
  defineProps<{
    title: string;
    description: string;
    columns: LaidOutColumn[];
    rows: Row[];
    total: number;
    truncated: boolean;
    sorts: Sort[];
    filters: Filter[];
    /**
     * 칸 글자를 만드는 방법. **화면이 그리는 것과 같은 것을 넘긴다.**
     *
     * 여기서 따로 만들면 종이와 화면이 다른 글자를 낸다 — 실제로 인쇄물에만
     * 값이 다르게 찍힌 적이 있다. 종이를 든 쪽이 틀린 값을 읽는다.
     */
    format?: (col: LaidOutColumn, value: Cell) => string;
  }>(),
  { format: (col: LaidOutColumn, value: Cell) => cellText(col, value) },
);

const numberFormat = new Intl.NumberFormat("ko-KR");

const printedAt = computed(() =>
  new Intl.DateTimeFormat("ko-KR", { dateStyle: "long", timeStyle: "short" }).format(
    new Date(),
  ),
);

function labelOf(key: string): string {
  return props.columns.find((c) => c.key === key)?.label ?? key;
}

/** 적용된 정렬을 사람이 읽는 한 줄로. */
const sortSummary = computed(() =>
  props.sorts
    .map((s, i) => `${i + 1}. ${labelOf(s.key)} ${s.dir === "asc" ? "오름차순" : "내림차순"}`)
    .join("  ·  "),
);

/** 적용된 필터를 사람이 읽는 목록으로. */
const filterSummary = computed(() =>
  props.filters.map((f) => {
    const parts: string[] = [];
    if (f.contains) parts.push(`"${f.contains}" 포함`);
    if (f.values) {
      parts.push(
        f.values.length === 0
          ? "선택 없음"
          : f.values.length <= 4
            ? f.values.join(", ")
            : `${f.values.slice(0, 3).join(", ")} 외 ${f.values.length - 3}개`,
      );
    }
    return `${labelOf(f.key)}: ${parts.join(" / ")}`;
  }),
);

/**
 * 열이 많으면 활자를 스스로 줄인다.
 *
 * 기본값은 A4 가로에 열 열대여섯 개를 넣는 기준인데, 사람이 들고 오는 양식은
 * 그 곱절인 것도 있다. 고정 크기로 두면 글자가 한 자씩 세로로 쪼개져 읽을 수
 * 없게 된다. 양식이 무엇이든 손댈 일이 없도록 여기서 정한다. 7pt 아래로는
 * 종이에서도 못 읽으므로 그 아래로는 내려가지 않는다.
 */
const BASE_COLUMNS = 16;
const BASE_PT = 9;
const MIN_PT = 7;

const fontVars = computed(() => {
  const n = props.columns.length;
  const pt = n <= BASE_COLUMNS ? BASE_PT : Math.max(MIN_PT, (BASE_PT * BASE_COLUMNS) / n);
  return {
    "--pv-font": `${pt.toFixed(2)}pt`,
    "--pv-head-font": `${pt.toFixed(2)}pt`,
  };
});

/** `#` 열이 쓰고 남은 폭을, 화면에서 쓰는 열 너비 비율대로 나눈다. */
const widthSum = computed(() =>
  props.columns.reduce((sum, c) => sum + c.width, 0) || 1,
);

function colVars(col: LaidOutColumn): Record<string, string> {
  return { "--pv-col-w": `${((col.width / widthSum.value) * 100).toFixed(3)}%` };
}

</script>

<template>
  <div class="pv" :style="fontVars">
    <header class="pv-head">
      <h1 class="pv-title">{{ title }}</h1>
      <p class="pv-desc">{{ description }}</p>

      <dl class="pv-meta">
        <div v-if="sortSummary">
          <dt>정렬</dt>
          <dd>{{ sortSummary }}</dd>
        </div>
        <div v-if="filterSummary.length">
          <dt>필터</dt>
          <dd>
            <span v-for="(f, i) in filterSummary" :key="i" class="pv-chip">{{ f }}</span>
          </dd>
        </div>
        <div>
          <dt>건수</dt>
          <dd>
            {{ numberFormat.format(rows.length) }}건 인쇄
            <template v-if="truncated">
              (조건에 맞는 전체 {{ numberFormat.format(total) }}건 중 상위 일부)
            </template>
          </dd>
        </div>
        <div>
          <dt>출력</dt>
          <dd>{{ printedAt }} · {{ APP_NAME }}</dd>
        </div>
      </dl>

      <p v-if="truncated" class="pv-warn">
        조건에 맞는 행이 많아 상위 {{ numberFormat.format(rows.length) }}건만 인쇄합니다.
        필터를 더 좁혀 주세요.
      </p>
    </header>

    <!-- 실제 table 을 쓴다. 브라우저가 페이지마다 thead 를 반복해 준다. -->
    <table class="pv-table">
      <!-- 열 비율을 화면과 똑같이 준다.
           비워 두면 브라우저가 글자를 보고 폭을 나누는데, 짧은 열(지역·계열)이
           한 글자 폭까지 눌려 "자/연" 처럼 세로로 쪼개진다. -->
      <colgroup>
        <col class="pv-no-col" />
        <col v-for="col in columns" :key="col.key" :style="colVars(col)" />
      </colgroup>
      <thead>
        <tr>
          <th class="pv-no">#</th>
          <th v-for="col in columns" :key="col.key" :data-kind="col.kind">
            {{ col.label }}
          </th>
        </tr>
      </thead>
      <tbody>
        <tr v-for="(row, i) in rows" :key="i">
          <td class="pv-no">{{ numberFormat.format(i + 1) }}</td>
          <td v-for="col in columns" :key="col.key" :data-kind="col.kind">
            {{ format(col, row[col.srcIndex]) }}
          </td>
        </tr>
      </tbody>
    </table>
  </div>
</template>
