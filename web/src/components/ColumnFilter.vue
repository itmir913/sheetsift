<script setup lang="ts">
/**
 * 엑셀식 값 선택 팝오버.
 *
 * **후보 축소 작업은 바탕이 수행한다.** goe-navi 에서는 수령한 목록을 화면에서
 * 여과했는데, 그러면 값이 다수라 목록이 전단에서 절단된 열에서 탐색하는 값이 그
 * 이후에 소재할 때 "없습니다" 가 표시된다. 자료는 정상인데 화면만 그렇게
 * 진술한다.
 *
 * 그래서 목록을 **두 벌** 보유한다. 축소 문자로 수령한 것(표시할 것)과, 축소
 * 없이 수령한 것(전체). 둘이어야 하는 사유는 [`leaveAll`] 에 기재했다.
 */
import { computed, onMounted, onBeforeUnmount, ref, watch } from "vue";
import { type ColumnDef, type ColumnValue, type ColumnValuesResult } from "@/lib/grid";
import { AppError, limits } from "sheetsift-book";
import { formatNumber, t, tn } from "@/i18n";

const props = defineProps<{
  column: ColumnDef;
  /** 현 선택 상태. null 이면 전체 선택. */
  selected: string[] | null;
  /** 후보 목록을 수령하는 방법. `needle` 로 축소한 것을 반환한다. */
  load: (needle: string) => Promise<ColumnValuesResult>;
  /** 그리드 기준 위치(px). */
  x: number;
  y: number;
  /** 사용 가능한 최대 높이(px). 화면 외부로 절단되지 않도록 부르는 곳이 측정해 제공한다. */
  maxHeight?: number;
}>();

const emit = defineEmits<{
  apply: [values: string[] | null];
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
 * 축소 없이 수령한 후보 전체. **화면에 묘사하지 않는다** — [`leaveAll`] 이 사용한다.
 */
const everyValue = ref<ColumnValue[]>([]);

/** 그 전체가 상한에 포착되어 절단되었는가. */
const truncated = ref(false);

const loading = ref(true);
const errorMessage = ref<string | null>(null);
const search = ref("");

/**
 * 선택한 값들. **후보를 축소해도 그대로 존치된다** — 축소 문자를 소거했을 때
 * 선행 선택한 것이 소멸해 있으면, 사람은 자신의 선택이 무시되었다고 판독한다.
 */
const checked = ref<Set<string>>(new Set());

/**
 * 여과하지 않는 상태인가.
 *
 * **"전부 선택됨" 과 구분하는 사유가 존재한다.** 후보가 절단되었거나 문자로
 * 축소되어 있으면 노출되는 것이 전체가 아니라, 화면에 선택이 전부 충전되어
 * 있어도 그것은 "전체" 가 아니다. 그 양자를 동일하게 판정하면 선택하지 않은
 * 값들이 묵시적으로 통과한다.
 */
const all = ref(true);

const popVars = computed(() => ({
  "--pop-x": `${props.x}px`,
  "--pop-y": `${props.y}px`,
  "--pop-maxh": `${props.maxHeight ?? 420}px`,
}));

const allShownChecked = computed(
  () => shown.value.length > 0 && shown.value.every((c) => all.value || checked.value.has(c.value)),
);

/** 축소 문자로 재수령한다. 지연 도달한 응답이 신규 응답을 피복하지 않게 번호를 산정한다. */
let token = 0;
async function fetchShown() {
  const mine = ++token;
  loading.value = true;
  errorMessage.value = null;
  try {
    const res = await props.load(search.value.trim());
    if (mine !== token) return;
    shown.value = res.values;
    // 축소 없이 수령한 것이면 그것이 곧 전체다.
    if (!search.value.trim()) {
      everyValue.value = res.values;
      truncated.value = res.truncated;
    }
  } catch (e) {
    if (mine !== token) return;
    errorMessage.value = e instanceof AppError ? e.message : String(e);
  } finally {
    if (mine === token) loading.value = false;
  }
}

/**
 * 전체 상태를 이탈한다. 선택 상태를 **현재 통과하고 있는 값 전체**로 충전한다.
 *
 * **축소되어 노출되는 것으로 충전하면 안 된다.** 「대」로 축소해 대구·대전만
 * 노출된 상태에서 대전 하나를 해제하면, 사람이 의도한 것은 "대전만 제외하라"
 * 인데 조작하지도 않은 광주·부산·서울·인천까지 함께 제외된다. 표는 정상적으로
 * 묘사되고 어디에서도 오류가 발생하지 않는다 — 실제로 그렇게 발생했다.
 *
 * 그래서 축소 없이 수령한 목록에서 충전한다. 그 목록이 상한에 포착되어
 * 절단되었으면 어떠한 목록으로도 "하나만 제외하라" 를 기재할 수 없는데, 그
 * 시점에는 팝오버가 절단되었다고 제시하고 있다.
 */
function leaveAll() {
  if (!all.value) return;
  all.value = false;
  checked.value = new Set(everyValue.value.map((c) => c.value));
}

function toggle(value: string) {
  leaveAll();
  const next = new Set(checked.value);
  if (next.has(value)) next.delete(value);
  else next.add(value);
  checked.value = next;
}

/** 현재 축소되어 노출되는 것만 일괄 설정·해제한다(엑셀과 동일하다). */
function toggleAllShown() {
  const on = allShownChecked.value;
  leaveAll();
  const next = new Set(checked.value);
  for (const c of shown.value) {
    if (on) next.delete(c.value);
    else next.add(c.value);
  }
  checked.value = next;
}

function apply() {
  emit("apply", all.value ? null : [...checked.value]);
}

/**
 * 엔터로 적용한다.
 *
 * 문자를 입력한 상태면 **현재 축소되어 노출되는 값들만 선택한다.** 선택한 것만
 * 적용하면, 문자만 입력하고 엔터를 조작한 사람 앞에서 표가 공백이 된다. 입력해
 * 축소했으면 그것이 선택이라는 의미다 — 상기 [`leaveAll`] 과 달리 여기서는
 * 축소가 사람이 **직접 진술한 의도**라 그대로 수용한다.
 *
 * 부합하는 값이 하나도 부재하면 **아무 작용도 하지 않는다.** 그 시점에 적용하면
 * 아무것도 통과하지 못하는 필터가 적용되는데, 사람이 의도한 것은 그것이 아니다.
 */
function applyTyped() {
  if (!search.value.trim()) {
    apply();
    return;
  }
  if (!shown.value.length) return;
  all.value = false;
  checked.value = new Set(shown.value.map((c) => c.value));
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
  applyTyped();
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
  all.value = props.selected === null;
  checked.value = new Set(props.selected ?? []);
  void fetchShown();
  searchEl.value?.focus();
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
        {{ all ? t("filter.all") : tn("filter.selected", checked.size) }}
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

      <div class="dg-pop-list">
        <label v-for="c in shown" :key="c.value" class="dg-pop-row">
          <input
            type="checkbox"
            :checked="all || checked.has(c.value)"
            @change="toggle(c.value)"
          />
          <span class="dg-pop-val" :data-blank="c.value === blankToken">
            {{ c.value === blankToken ? t("filter.blank") : c.value }}
          </span>
          <span class="dg-pop-count">{{ formatNumber(c.count) }}</span>
        </label>
        <p v-if="!shown.length" class="dg-pop-note">{{ t("filter.none") }}</p>
      </div>

      <!-- 절단되었다는 것을 **제시한다.** 절단 사실을 인지하지 못한 채 선택하면,
           노출되지 않던 값들이 누락된 필터가 적용된다. -->
      <p v-if="truncated" class="dg-pop-note">{{ tn("filter.truncated", maxValues) }}</p>
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
        :disabled="loading"
        @click="apply"
      >
        {{ t("filter.apply") }}
      </button>
    </div>
  </div>
</template>
