<script setup lang="ts">
/**
 * 엑셀식 값 선택 팝오버.
 *
 * **후보 축소 작업은 바탕이 수행한다.** goe-navi 에서는 수령한 목록을 화면에서
 * 여과했는데, 그러면 값이 다수라 목록이 전단에서 절단된 열에서 탐색하는 값이 그
 * 이후에 소재할 때 "없습니다" 가 표시된다. 자료는 정상인데 화면만 그렇게
 * 진술한다.
 *
 * 선택 상태는 **제외 목록으로도 기재한다.** 전체에서 몇 개를 해제한 것을
 * 그대로 기재하면, 목록이 절단되었거나 축소된 상태에서도 "이것만 제외" 가
 * 정확하다 — 선택한 것을 열거하는 방식은 노출되지 않은 값까지 탈락시킨다.
 */
import { computed, onMounted, onBeforeUnmount, ref, watch } from "vue";
import {
  type ColumnDef,
  type ColumnValue,
  type ColumnValuesResult,
  type ValuePick,
} from "@/lib/grid";
import { AppError, limits } from "sheetsift-book";
import { formatNumber, t, tn } from "@/i18n";
import { fontEpoch } from "@/lib/colWidth";

const props = defineProps<{
  column: ColumnDef;
  /** 현 선택 상태. null 이면 전체 선택. */
  selected: ValuePick | null;
  /** 후보 목록을 수령하는 방법. `needle` 로 축소한 것을 반환한다. */
  load: (needle: string) => Promise<ColumnValuesResult>;
  /** 그리드 기준 위치(px). */
  x: number;
  y: number;
  /** 사용 가능한 최대 높이(px). 화면 외부로 절단되지 않도록 부르는 곳이 측정해 제공한다. */
  maxHeight?: number;
}>();

const emit = defineEmits<{
  apply: [pick: ValuePick | null];
  close: [];
}>();

/* 공백 표식은 바탕이 **값으로** 제공한다. 자료의 값이라 번역 대상이 아니고,
   그 문자를 그대로 묘사하면 영어 화면에 한국어가 출현한다. 그래서 수수하는
   것은 표식 그대로이고 **묘사만** 화면의 문구로 치환한다.

   공백 표식과 목록 상한은 **바탕이 제공한다.** 여기 기재하면 바탕이 그것을
   변경하는 시점에, 공백이 정상 값으로 인식되고 안내문만 구 수치를 제시한다 —
   양자 모두 오류가 발생하지 않는다. */
const { blankToken, maxValues } = limits();

const root = ref<HTMLElement | null>(null);

/**
 * 개방 직후 포커스를 수용하는 입력란.
 *
 * **엑셀은 ▼ 이후 즉시 입력이 가능하다.** 포커스가 단추에 잔류하면 사람이
 * 입력란을 1회 더 조작해야 하고, 키보드만 사용하는 사람은 팝오버 내부로
 * 진입하는 경로 자체가 부재하다.
 */
const searchEl = ref<HTMLInputElement | null>(null);

/** 현재 표시하는 후보. 축소 문자가 존재하면 그것으로 수령한 것이다. */
const shown = ref<ColumnValue[]>([]);

/**
 * 그 후보를 수령할 때 사용한 축소 문자.
 *
 * **입력과 후보가 정합하는지의 유일한 근거다.** 감시자가 대기 중인지로
 * 판정하면 입력 직후의 엔터를 포착하지 못한다 — 감시자는 비동기라 그 시점에
 * 아직 작동하지 않았다.
 */
let shownFor = "";

/** 전체 목록이 상한에 포착되어 절단되었는가. */
const truncated = ref(false);

const loading = ref(true);
const errorMessage = ref<string | null>(null);
const search = ref("");

/**
 * 무엇을 기재해 둔 상태인가.
 *
 * - `all` — 여과하지 않는다.
 * - `include` — 기재한 것만 통과시킨다.
 * - `exclude` — 기재한 것만 탈락시킨다.
 *
 * **전체에서 하나를 해제하면 `exclude` 가 된다.** 선택한 것을 열거하는 방식만
 * 존재하면, 목록이 절단되었거나 문자로 축소된 상태에서 하나를 해제했을 때
 * **노출되지 않은 값들까지 함께 탈락한다** — 실제로 그렇게 발생했다. 제외를
 * 그대로 기재하면 목록의 분량과 무관하게 정확하다.
 */
const mode = ref<"all" | "include" | "exclude">("all");

/** `mode` 가 지시하는 값들. 후보를 축소해도 그대로 존치된다. */
const named = ref<Set<string>>(new Set());

/** 그 값이 통과하는가. 확인란의 상태다. */
function picked(value: string): boolean {
  if (mode.value === "all") return true;
  return mode.value === "include" ? named.value.has(value) : !named.value.has(value);
}

/* ── 묘사하는 범위 ─────────────────────────────────────────────
   목록이 상한까지 도달하면 수천 항목이다. 전부 생성하면 개방·폐쇄에 수 초가
   소요되고 수만에서는 탭이 정지한다(실측). 그래서 보이는 범위와 그 전후만
   생성한다. 행 높이는 양식이 고정한다(`--dg-pop-row-h`). */

const listEl = ref<HTMLElement | null>(null);
const scrollTop = ref(0);
const viewHeight = ref(320);
/** 전후로 추가 생성하는 행 수. 스크롤 도중에 공백이 노출되지 않게 한다. */
const OVERSCAN = 8;

const rowHeight = computed(() => {
  void fontEpoch.value; // 서체가 적재되면 rem 의 실측값이 변동할 수 있다
  const el = listEl.value;
  if (!el) return 30;
  const rem = parseFloat(getComputedStyle(el).getPropertyValue("--dg-pop-row-h"));
  const base = parseFloat(getComputedStyle(document.documentElement).fontSize);
  const px = rem * (Number.isFinite(base) && base > 0 ? base : 16);
  return Number.isFinite(px) && px > 0 ? px : 30;
});

const view = computed(() => {
  const h = rowHeight.value;
  const total = shown.value.length;
  const first = Math.max(0, Math.floor(scrollTop.value / h) - OVERSCAN);
  const count = Math.ceil(viewHeight.value / h) + OVERSCAN * 2;
  const end = Math.min(total, first + count);
  return { start: first, end, rows: shown.value.slice(first, end) };
});

function onScroll() {
  const el = listEl.value;
  if (!el) return;
  scrollTop.value = el.scrollTop;
  viewHeight.value = el.clientHeight;
}

/** 후보가 교체되면 상단부터 묘사한다. 스크롤 위치를 존치하면 공백이 노출된다. */
watch(shown, () => {
  scrollTop.value = 0;
  listEl.value?.scrollTo({ top: 0 });
});

/**
 * 적용하면 **아무 행도 통과하지 못하는 상태**인가.
 *
 * 열거한 것만 통과시키는 상태에서 전부 해제하면 그렇다. 적용을 허용하면 표가
 * 공백이 되는데, 사람이 의도한 것은 그것이 아니다 — 엑셀도 그 상태의 확인을
 * 차단한다.
 */
const nothingPasses = computed(() => mode.value === "include" && named.value.size === 0);

const popVars = computed(() => ({
  "--pop-x": `${props.x}px`,
  "--pop-y": `${props.y}px`,
  "--pop-maxh": `${props.maxHeight ?? 420}px`,
}));

const allShownChecked = computed(
  () => shown.value.length > 0 && shown.value.every((c) => picked(c.value)),
);

/** 축소 문자로 재수령한다. 지연 도달한 응답이 신규 응답을 피복하지 않게 번호를 산정한다. */
let token = 0;
async function fetchShown() {
  const mine = ++token;
  loading.value = true;
  errorMessage.value = null;
  try {
    const needle = search.value.trim();
    const res = await props.load(needle);
    if (mine !== token) return;
    shown.value = res.values;
    shownFor = needle;
    // 축소 없이 수령한 것이면 그것이 곧 전체다.
    if (!search.value.trim()) {
      truncated.value = res.truncated;
    }
  } catch (e) {
    if (mine !== token) return;
    errorMessage.value = e instanceof AppError ? e.message : String(e);
  } finally {
    if (mine === token) {
      loading.value = false;
    }
  }
}

/**
 * 값 하나의 통과 여부를 전환한다.
 *
 * **전체 상태에서 하나를 해제하면 제외 목록이 된다.** 선택한 것을 열거하는
 * 방식으로 전환하면, 목록이 절단되었거나 문자로 축소된 상태에서 노출되지 않은
 * 값들까지 함께 탈락한다 — 「대」로 축소해 대전 하나를 해제했는데 광주·부산·
 * 서울·인천이 함께 탈락하던 것이 그것이다.
 */
function toggle(value: string) {
  const next = new Set(named.value);
  if (mode.value === "all") {
    mode.value = "exclude";
    next.clear();
    next.add(value);
  } else if (mode.value === "exclude") {
    if (next.has(value)) next.delete(value);
    else next.add(value);
    if (next.size === 0) mode.value = "all";
  } else {
    if (next.has(value)) next.delete(value);
    else next.add(value);
  }
  named.value = next;
}

/** 현재 축소되어 노출되는 것만 일괄 설정·해제한다(엑셀과 동일하다). */
function toggleAllShown() {
  const off = allShownChecked.value; // 전부 통과 중이면 해제한다
  const next = new Set(named.value);
  if (mode.value === "all") {
    if (!off) return; // 전체 통과 중인데 설정할 것이 부재하다
    mode.value = "exclude";
    next.clear();
    for (const c of shown.value) next.add(c.value);
  } else if (mode.value === "exclude") {
    for (const c of shown.value) {
      if (off) next.add(c.value);
      else next.delete(c.value);
    }
    if (next.size === 0) mode.value = "all";
  } else {
    for (const c of shown.value) {
      if (off) next.delete(c.value);
      else next.add(c.value);
    }
  }
  named.value = next;
}

function apply() {
  if (mode.value === "all") {
    emit("apply", null);
    return;
  }
  emit("apply", { mode: mode.value, values: [...named.value] });
}

/**
 * 엔터로 적용한다.
 *
 * 문자를 입력한 상태면 **현재 축소되어 노출되는 값들만 선택한다.** 선택한 것만
 * 적용하면, 문자만 입력하고 엔터를 조작한 사람 앞에서 표가 공백이 된다. 입력해
 * 축소했으면 그것이 선택이라는 의미다 — 조작으로 해제한 것과 달리 여기서는
 * 축소가 사람이 **직접 진술한 의도**라 그대로 수용한다.
 *
 * 부합하는 값이 하나도 부재하면 **아무 작용도 하지 않는다.** 그 시점에 적용하면
 * 아무것도 통과하지 못하는 필터가 적용되는데, 사람이 의도한 것은 그것이 아니다.
 */
async function applyTyped() {
  if (!search.value.trim()) {
    apply();
    return;
  }
  /* **아직 갱신되지 않은 후보로 적용하지 않는다.** 입력 직후에 엔터를 조작하면
     축소 요청이 아직 발신되지 않았거나 응답이 도달하지 않은 상태인데, 그
     시점의 후보는 **축소 이전의 전체**다 — 사람이 입력한 것과 무관한 필터가
     묵시적으로 적용된다. 대기 중인 요청을 즉시 수행하고 그 결과로 적용한다. */
  if (shownFor !== search.value.trim() || loading.value) {
    window.clearTimeout(searchTimer);
    await fetchShown();
  }
  if (!shown.value.length) return;
  mode.value = "include";
  named.value = new Set(shown.value.map((c) => c.value));
  apply();
}

/**
 * 팝오버 내부의 입력만 수신한다.
 *
 * `window` 에서 수신하면 표의 **열 검색란에 입력하는 중의 엔터**까지 여기에
 * 도달해, 조작하지 않은 팝오버가 적용된다. 이탈(Esc)은 포커스가 어디에 있든
 * 수용해야 하므로 그것만 외부에서 수신한다.
 */
function onKeydown(e: KeyboardEvent) {
  if (e.key === "Escape") {
    emit("close");
    return;
  }
  if (e.key !== "Enter" || e.isComposing) return;
  if (!root.value?.contains(e.target as Node)) return;
  void applyTyped();
}

function onPointerDown(e: PointerEvent) {
  if (root.value && !root.value.contains(e.target as Node)) emit("close");
}

/** 문자마다 바탕에 질의하지 않는다. 입력 중 요청이 연속 발생한다. */
let searchTimer: number | undefined;
watch(search, () => {
  window.clearTimeout(searchTimer);
  searchTimer = window.setTimeout(() => void fetchShown(), 200);
});

onMounted(() => {
  mode.value = props.selected?.mode ?? "all";
  named.value = new Set(props.selected?.values ?? []);
  void fetchShown();
  searchEl.value?.focus();
  viewHeight.value = listEl.value?.clientHeight ?? viewHeight.value;
  window.addEventListener("keydown", onKeydown);
  // 팝오버 개방 직후의 클릭까지 포착하지 않도록 차기 틱에 등록한다.
  window.setTimeout(() => window.addEventListener("pointerdown", onPointerDown), 0);
});

onBeforeUnmount(() => {
  window.clearTimeout(searchTimer);
  window.removeEventListener("keydown", onKeydown);
  window.removeEventListener("pointerdown", onPointerDown);
});
</script>

<template>
  <div
    ref="root"
    class="dg-pop"
    :style="popVars"
    role="dialog"
    aria-modal="true"
    :aria-label="t('grid.filter.aria', { label: column.label })"
  >
    <div class="dg-pop-head">
      <span class="font-semibold">{{ column.label }}</span>
      <span class="text-fg-subtle">
        <template v-if="mode === 'all'">{{ t("filter.all") }}</template>
        <template v-else-if="mode === 'include'">{{ tn("filter.selected", named.size) }}</template>
        <template v-else>{{ tn("filter.excluded", named.size) }}</template>
      </span>
    </div>

    <input
      v-model="search"
      class="dg-finput"
      type="search"
      ref="searchEl"
      :placeholder="t('filter.search')"
      :aria-label="t('filter.search')"
    />

    <p v-if="loading" class="dg-pop-note">{{ t("filter.loading") }}</p>
    <p v-else-if="errorMessage" class="dg-pop-note text-danger">{{ errorMessage }}</p>

    <template v-else>
      <label class="dg-pop-row dg-pop-all">
        <input type="checkbox" :checked="allShownChecked" @change="toggleAllShown" />
        <span class="font-medium">
          {{ t(search.trim() ? "filter.checkall.shown" : "filter.checkall") }}
        </span>
      </label>

      <!-- **묘사하는 범위만 생성한다.** 목록이 수천 항목이라 전부 생성하면
           개방과 폐쇄에 수 초가 소요되고, 수만에서는 탭이 정지한다. 상하의
           여백이 스크롤 범위를 확보한다. -->
      <div ref="listEl" class="dg-pop-list" @scroll.passive="onScroll">
        <div :style="{ height: `${view.start * rowHeight}px` }"></div>
        <label v-for="c in view.rows" :key="c.value" class="dg-pop-row">
          <input type="checkbox" :checked="picked(c.value)" @change="toggle(c.value)" />
          <span class="dg-pop-val" :data-blank="c.value === blankToken">
            {{ c.value === blankToken ? t("filter.blank") : c.value }}
          </span>
          <span class="dg-pop-count">{{ formatNumber(c.count) }}</span>
        </label>
        <div :style="{ height: `${(shown.length - view.end) * rowHeight}px` }"></div>
        <p v-if="!shown.length" class="dg-pop-note">{{ t("filter.none") }}</p>
      </div>

      <!-- 절단되었다는 것을 **제시한다.** 절단 사실을 인지하지 못한 채 선택하면,
           노출되지 않던 값들이 누락된 필터가 적용된다. -->
      <!-- 절단 안내는 **축소하지 않은 목록**에 관한 것이다. 축소 중에 함께
           노출하면 "일치하는 값이 없습니다" 와 동시에 기재되어 의미가 충돌한다. -->
      <p v-if="truncated && !search.trim()" class="dg-pop-note">
        {{ tn("filter.truncated", maxValues) }}
      </p>
    </template>

    <div class="dg-pop-foot">
      <button
        type="button"
        class="rounded-md border border-line px-3 py-1.5 text-fg-muted hover:bg-surface-hover hover:text-fg"
        @click="emit('close')"
      >
        {{ t("filter.cancel") }}
      </button>
      <button
        type="button"
        class="rounded-md bg-accent px-3 py-1.5 font-medium text-accent-fg hover:bg-accent-hover disabled:opacity-40"
        :disabled="loading || nothingPasses"
        @click="apply"
      >
        {{ t("filter.apply") }}
      </button>
    </div>
  </div>
</template>
