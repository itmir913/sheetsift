<script setup lang="ts">
import { computed } from "vue";
import { APP_NAME } from "@/lib/app";
import { cellText } from "@/lib/grid";
import { formatDateTime, formatNumber, t, tn } from "@/i18n";
import { limits } from "sheetsift-book";
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
     * 셀 문자를 생성하는 방법. **화면이 묘사하는 것과 동일한 것을 전달한다.**
     *
     * 여기서 별도 생성하면 지면과 화면이 상이한 문자를 산출한다 — 실제로
     * 인쇄물에만 값이 상이하게 출력된 사례가 존재한다. 지면을 보유한 측이
     * 상이한 값을 판독한다.
     */
    format?: (col: LaidOutColumn, value: Cell) => string;
  }>(),
  { format: (col: LaidOutColumn, value: Cell) => cellText(col, value) },
);

const printedAt = computed(() => formatDateTime(new Date()));

/* 공백 표식은 자료의 값이라 번역 대상이 아니다. 지면에 그대로 출력하면 영어
   화면에서 인쇄한 지면에만 한국어가 출현한다 — 화면과 동일하게 묘사만 치환한다. */
const { blankToken } = limits();

function valueOf(v: string): string {
  return v === blankToken ? t("filter.blank") : v;
}

function labelOf(key: string): string {
  return props.columns.find((c) => c.key === key)?.label ?? key;
}

/** 적용된 정렬을 사람이 판독하는 한 행으로. */
const sortSummary = computed(() =>
  props.sorts
    .map(
      (s, i) =>
        `${i + 1}. ${labelOf(s.key)} ${t(s.dir === "asc" ? "print.asc" : "print.desc")}`,
    )
    .join("  ·  "),
);

/** 적용된 필터를 사람이 판독하는 목록으로. */
const filterSummary = computed(() =>
  props.filters.map((f) => {
    const parts: string[] = [];
    if (f.contains) parts.push(t("print.contains", { text: f.contains }));
    if (f.excluded?.length) {
      parts.push(
        t("print.excluded", {
          list:
            f.excluded.length <= 4
              ? f.excluded.map(valueOf).join(", ")
              : t("print.more", {
                  list: f.excluded.slice(0, 3).map(valueOf).join(", "),
                  n: f.excluded.length - 3,
                }),
        }),
      );
    }
    if (f.values) {
      parts.push(
        f.values.length === 0
          ? t("print.selected.none")
          : f.values.length <= 4
            ? f.values.map(valueOf).join(", ")
            : t("print.more", {
                list: f.values.slice(0, 3).map(valueOf).join(", "),
                n: f.values.length - 3,
              }),
      );
    }
    return `${labelOf(f.key)}: ${parts.join(" / ")}`;
  }),
);

/**
 * 열이 다수면 활자를 자동 축소한다.
 *
 * 기본값은 A4 가로에 열 15~16개를 수용하는 기준인데, 사람이 지참하는 양식은
 * 그 배수인 것도 존재한다. 고정 크기로 설정하면 문자가 1자씩 수직 분할되어
 * 판독할 수 없게 된다. 양식이 무엇이든 조작할 사안이 없도록 여기서 결정한다.
 * 7pt 미만은 지면에서도 판독 불가하므로 그 이하로는 하강하지 않는다.
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

/** `#` 열이 사용하고 잔여한 폭을, 화면에서 사용하는 열 폭 비율대로 배분한다. */
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
          <dt>{{ t("print.sorts") }}</dt>
          <dd>{{ sortSummary }}</dd>
        </div>
        <div v-if="filterSummary.length">
          <dt>{{ t("print.filters") }}</dt>
          <dd>
            <span v-for="(f, i) in filterSummary" :key="i" class="pv-chip">{{ f }}</span>
          </dd>
        </div>
        <div>
          <dt>{{ t("print.count") }}</dt>
          <dd>
            {{ tn("print.rows", rows.length) }}
            <template v-if="truncated">{{ tn("print.oftotal", total) }}</template>
          </dd>
        </div>
        <div>
          <dt>{{ t("print.printed") }}</dt>
          <dd>{{ printedAt }} · {{ APP_NAME }}</dd>
        </div>
      </dl>

      <p v-if="truncated" class="pv-warn">{{ tn("print.truncated", rows.length) }}</p>
    </header>

    <!-- 실제 table 을 사용한다. 브라우저가 페이지마다 thead 를 반복 출력한다. -->
    <table class="pv-table">
      <!-- 열 비율을 화면과 동일하게 부여한다.
           공백으로 두면 브라우저가 문자를 참조해 폭을 배분하는데, 단축 열
           (지역·계열)이 1문자 폭까지 압축되어 "자/연" 처럼 수직 분할된다. -->
      <colgroup>
        <col class="pv-no-col" />
        <col v-for="col in columns" :key="col.key" :style="colVars(col)" />
      </colgroup>
      <thead>
        <tr>
          <th class="pv-no">{{ t("print.no") }}</th>
          <th v-for="col in columns" :key="col.key" :data-kind="col.kind">
            {{ col.label }}
          </th>
        </tr>
      </thead>
      <tbody>
        <tr v-for="(row, i) in rows" :key="i">
          <td class="pv-no">{{ formatNumber(i + 1) }}</td>
          <td v-for="col in columns" :key="col.key" :data-kind="col.kind">
            {{ format(col, row[col.srcIndex]) }}
          </td>
        </tr>
      </tbody>
    </table>
  </div>
</template>
