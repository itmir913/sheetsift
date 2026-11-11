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
import { computed, nextTick, onMounted, onBeforeUnmount, ref, watch } from "vue";
import {
  type ColumnDef,
  type ColumnValue,
  type ColumnValuesResult,
  type NumberRange,
  type ValuePick,
  RANGE_STEP,
  canApply,
  changeCommitsAtOnce,
  rangeText,
  settleRange,
  settleSelection,
  showsRange,
  snapSpan,
  valueText,
} from "../lib/grid";
import { useFault } from "../composables/useFault";
import { formatNumber, source, t, tn } from "../lib/inject";
import { fontEpoch } from "../lib/colWidth";

const props = defineProps<{
  column: ColumnDef;
  /** 현 선택 상태. null 이면 전체 선택. */
  selected: ValuePick | null;
  /** 현 범위(숫자 열). null 이면 범위를 걸지 않은 것이다. */
  range?: NumberRange | null;
  /**
   * 후보 목록을 수령하는 방법. `needle` 로 축소한 것을 반환한다. `range` 는 **이 팝오버가 지금
   * 보유한 범위**다(확정했으나 아직 적용하지 않은 것) — 목록이 그 범위 안의 값만 담아야 한다.
   */
  load: (needle: string, range: NumberRange | null) => Promise<ColumnValuesResult>;
  /** 그리드 기준 위치(px). */
  x: number;
  y: number;
  /** 사용 가능한 최대 높이(px). 화면 외부로 절단되지 않도록 부르는 곳이 측정해 제공한다. */
  maxHeight?: number;
}>();

const emit = defineEmits<{
  apply: [pick: ValuePick | null, range: NumberRange | null];
  close: [];
}>();

/* 공백 표식은 바탕이 **값으로** 제공한다. 자료의 값이라 번역 대상이 아니고,
   그 문자를 그대로 묘사하면 영어 화면에 한국어가 출현한다. 그래서 수수하는
   것은 표식 그대로이고 **묘사만** 화면의 문구로 치환한다.

   공백 표식과 목록 상한은 **바탕이 제공한다.** 여기 기재하면 바탕이 그것을
   변경하는 시점에, 공백이 정상 값으로 인식되고 안내문만 구 수치를 제시한다 —
   양자 모두 오류가 발생하지 않는다. */
const { blankToken, maxValues } = source().limits();

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
/* 오류는 코드와 치환 값으로 보유한다. 문구로 보유하면 언어 전환을 추종하지 않는다. */
const { message: errorMessage, capture: captureFault, clear: clearFault } = useFault();
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
/**
 * `quiet` 는 범위가 바뀌어 다시 수령할 때다 — **직전 목록을 둔 채 교체한다.** 불러오는 중
 * 안내로 갈아 끼우면 목록과 안내가 통째로 접혀 적용 단추가 위로 올라붙는데, 마우스를 내리는
 * 동안 단추가 움직이고 그 순간 눌리지도 않는다.
 */
async function fetchShown(quiet = false) {
  const mine = ++token;
  if (!quiet) loading.value = true;
  clearFault();
  try {
    const needle = search.value.trim();
    const res = await props.load(needle, range.value);
    if (mine !== token) return;
    shown.value = res.values;
    shownFor = needle;
    /* 자료의 범위는 목록이 자기 범위를 적용해도 변하지 않는다(바탕이 자기 필터를 통째로 빼고
       산출한다). 축소한 목록에서도 같은 값이 오지만 한 곳에서만 받는다. */
    if (!needle) takeRangeFacts(res);
    // 축소 없이 수령한 것이면 그것이 곧 전체다.
    if (!search.value.trim()) {
      truncated.value = res.truncated;
    }
  } catch (e) {
    if (mine !== token) return;
    captureFault(e);
  } finally {
    if (mine === token) {
      loading.value = false;
      loadedOnce.value = true;
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

/* ── 숫자 범위 ─────────────────────────────────────────────────
   숫자 열은 고유값이 수백 개라 목록에서 고르는 것이 쓸모가 적다(내신 `2.41`). 범위로 거른다.
   값 목록과 **함께** 걸 수 있다 — 둘은 AND 로 겹친다. */

/** 멈춘 뒤 확정하기까지의 지연(ms). */
const RANGE_SETTLE = 300;

/** 자료가 존재하는 범위. 축소하지 않은 목록을 수령할 때 갱신한다. */
const span = ref<NumberRange | null>(null);


/** **확정한** 범위. 적용할 때 이것이 올라간다. 전체를 그대로 둔 것은 null 이다. */
const range = ref<NumberRange | null>(props.range ?? null);

/**
 * 화면에 떠 있는 두 손잡이의 값. 확정 전에도 여기까지는 즉시 따라온다.
 * **뒤집지 않은 그대로** 보유한다 — 하한 손잡이가 상한 너머로 가도 각 손잡이는 자기가
 * 잡힌 자리에 있어야 하고, 뒤집는 것은 확정할 때다(`settleRange`).
 */
const live = ref<NumberRange>({ min: 0, max: 0 });

const minEl = ref<HTMLInputElement | null>(null);
const maxEl = ref<HTMLInputElement | null>(null);

/** 손잡이를 잡고 있는가. 잡고 있는 동안에는 지연 확정이 확정하지 않는다. */
let gripped = false;
let settleTimer: number | undefined;

/** 슬라이더와 입력칸이 칸으로 삼는 범위. 자료의 끝이 눈금 위에 있지 않아도 덮는다. */
const grid = computed(() => (span.value ? snapSpan(span.value) : null));

/** 범위 칸을 그리는가(판정은 `showsRange`). */
const showRange = computed(() => showsRange(props.column.kind, span.value));

/** 자료의 폭이 0 인가(값이 한 가지뿐). 손잡이를 움직일 데가 없어 조작만 막는다. */
const flat = computed(() => !!span.value && span.value.min >= span.value.max);

/**
 * 값 검색칸을 그리는가. 값 목록은 모든 열이 그리고, 숫자 열은 그 위에 범위 칸이 더해진다.
 *
 * **숫자 열은 첫 수령이 끝나기 전에는 검색칸을 그리지 않는다.** 범위 칸이 그 응답과 함께
 * 나타나 검색칸을 아래로 밀어내므로, 먼저 그리면 열 때 팝오버가 튄다.
 *
 * 값 목록은 **지금 범위 안의 값만** 담는다(바탕이 자기 범위를 적용해 만든다). 범위 밖의 값이
 * 목록에 있으면 그것을 선택하는 순간 범위와 AND 로 겹쳐 조용히 0건이 된다.
 */
const loadedOnce = ref(false);
const showSearch = computed(() => props.column.kind !== "number" || loadedOnce.value);


/**
 * 범위가 걸려 있거나 걸리려는 중인가. 「범위 해제」의 활성 여부가 이것을 따른다.
 * 확정한 범위(`range`)도 본다 — 자료의 폭이 0 이면 손잡이가 전체를 덮어 보여도 걸린 것이다.
 */
const narrowed = computed(
  () =>
    range.value !== null ||
    (!!span.value && settleRange(live.value.min, live.value.max, span.value) !== null),
);

/**
 * 채워진 구간의 위치(%). 손잡이가 서로를 넘어도 칠하는 구간은 앞이 작다.
 * **값이 바뀌어도 크기가 변하지 않는 배경 변수**일 뿐이라 배치가 이동하지 않는다.
 */
const fillVars = computed(() => {
  const g = grid.value;
  if (!g || g.max <= g.min) return {};
  const at = (v: number) => Math.min(100, Math.max(0, ((v - g.min) / (g.max - g.min)) * 100));
  return {
    "--lo": `${at(Math.min(live.value.min, live.value.max))}%`,
    "--hi": `${at(Math.max(live.value.min, live.value.max))}%`,
  };
});

/** 헤더에 적는 요약. 값 선택과 범위를 한 줄로 잇는다. */
const summary = computed(() => {
  const parts: string[] = [];
  if (mode.value === "include") parts.push(tn("filter.selected", named.value.size));
  else if (mode.value === "exclude") parts.push(tn("filter.excluded", named.value.size));
  if (range.value) {
    parts.push(
      t("filter.range.active", { span: rangeText(range.value.min, range.value.max) }),
    );
  }
  return parts.length ? parts.join(" · ") : t("filter.all");
});

function takeRangeFacts(res: ColumnValuesResult) {
  const r = res.range;
  const next = r && Number.isFinite(r.min) && Number.isFinite(r.max) ? r : null;
  if (next?.min !== span.value?.min || next?.max !== span.value?.max) span.value = next;
}

/* 자료의 범위가 처음 도달하면 손잡이를 놓는다. 입력칸은 DOM 이 생긴 뒤에 쓴다. */
watch(span, (s) => {
  if (!s) return;
  /* **눈금에 맞춘 범위로 놓는다.** 원본(`2.415`)을 놓으면 입력칸이 `step` 에 어긋나 무효가
     되고, 슬라이더는 눈금으로 끌려가 손잡이와 `live` 가 갈린다. 전체로 보이는 값이 필터가
     되지는 않는다 — `settleRange` 는 원본과 대조하고 눈금 범위는 원본을 덮는다. */
  live.value = range.value ?? { ...snapSpan(s) };
  void nextTick(() => syncInputs(null));
});

/**
 * 입력칸 둘을 지금 값에 맞춘다. **다시 그리지 않고 제자리에서 고쳐 쓴다.**
 *
 * 입력칸을 `:value` 로 묶으면 사람이 `2.` 까지 친 순간 되쓰이며 입력이 끊긴다. 그래서
 * 입력칸은 손으로 쓰고, `skip` 은 지금 치고 있는 칸이다. 입력칸에는 **뒤집은 값**을 적는다
 * (앞이 작다).
 */
function syncInputs(skip: EventTarget | null) {
  const lo = Math.min(live.value.min, live.value.max);
  const hi = Math.max(live.value.min, live.value.max);
  if (minEl.value && minEl.value !== skip) minEl.value.value = String(lo);
  if (maxEl.value && maxEl.value !== skip) maxEl.value.value = String(hi);
}

/**
 * 범위가 바뀌면 값 목록을 다시 수령한다. 목록은 지금 범위 안의 값만 담아야 하므로, 범위를
 * 좁히면 목록이 따라 줄고 「범위 해제」로 풀면 전부로 돌아온다. 값이 같으면 다시 받지 않는다.
 */
watch(
  () => (range.value ? `${range.value.min}~${range.value.max}` : ""),
  () => {
    window.clearTimeout(searchTimer);
    void fetchShown(true);
  },
);

/**
 * 범위를 바꾼다. **값 선택에서 범위 밖이 된 것을 같은 자리에서 거둔다**(`settleSelection`) —
 * 적용이 곧바로 뒤따를 수 있으므로 비동기로 거두면 늦는다.
 */
function setRange(next: NumberRange | null) {
  range.value = next;
  const s = settleSelection(mode.value, [...named.value], next);
  mode.value = s.mode;
  named.value = new Set(s.values);
}

/** 확정한다. 전체를 그대로 둔 것은 필터가 아니라 null 이다. */
function commitRange() {
  if (!span.value) return;
  setRange(settleRange(live.value.min, live.value.max, span.value));
}

/**
 * 멈춘 뒤에만 확정한다. **끄는 중에는 확정하지 않는다.**
 *
 * 확정하면 헤더의 요약과 해제 단추가 바뀌고 팝오버가 다시 그려지는데, 그 순간 잡고 있던
 * 손잡이를 놓칠 수 있다. 아직 잡고 있으면 손을 뗄 때 `release`·`change` 가 확정한다.
 */
function settle() {
  window.clearTimeout(settleTimer);
  settleTimer = window.setTimeout(() => {
    if (!gripped) commitRange();
  }, RANGE_SETTLE);
}

/** 손잡이를 잡는다. 놓는 것은 창 전체에서 듣는다 — 끌다가 팝오버 바깥에서 놓을 수 있다. */
function grip() {
  gripped = true;
  window.addEventListener("pointerup", release);
  window.addEventListener("pointercancel", release);
}

/**
 * 손잡이를 놓는다. 값이 바뀌지 않은 채 놓으면 `change` 가 오지 않아 잡은 상태가
 * 남는데, 그러면 이후의 지연 확정이 영영 확정하지 않는다.
 */
function release() {
  window.removeEventListener("pointerup", release);
  window.removeEventListener("pointercancel", release);
  if (!gripped) return;
  gripped = false;
  commitRange();
}

/**
 * 한쪽 값을 읽는다. 비었거나 숫자가 아닌 것(치던 도중의 `-`)은 받지 않는다 — `Number("")`
 * 이 0 이라, 지우는 순간 하한이 0 으로 확정되는 것을 막는다.
 */
function takeSide(e: Event, side: "min" | "max"): boolean {
  const raw = (e.target as HTMLInputElement).value;
  const n = Number(raw);
  if (raw.trim() === "" || !Number.isFinite(n)) return false;
  live.value = { ...live.value, [side]: n };
  return true;
}

function onRangeInput(e: Event, side: "min" | "max") {
  if (!takeSide(e, side)) return;
  syncInputs(e.target);
  settle();
}

/**
 * 손을 떼거나 입력칸을 벗어났다. 여기서 확정하고, 뒤집힌 값을 입력칸에 반영한다.
 * 슬라이더를 **방향키로** 움직인 `change` 는 즉시 확정하지 않고 지연 확정으로 보낸다
 * (`changeCommitsAtOnce`).
 */
function onRangeChange(e: Event, side: "min" | "max", source: "slider" | "input") {
  takeSide(e, side);
  const held = gripped;
  gripped = false;
  if (!changeCommitsAtOnce(source, held)) {
    settle();
    return;
  }
  window.clearTimeout(settleTimer);
  /* 뒤집고, 자료의 범위 밖으로 친 값(0)은 눈금 범위로 집는다. 그대로 두면 「데이터 범위
     1.5 ~ 9」 옆 입력칸에 `0 ~ 9` 가 남는다. */
  const g = grid.value;
  const clamp = (n: number) => (g ? Math.min(g.max, Math.max(g.min, n)) : n);
  live.value = {
    min: clamp(Math.min(live.value.min, live.value.max)),
    max: clamp(Math.max(live.value.min, live.value.max)),
  };
  syncInputs(null);
  commitRange();
}

function clearRange() {
  window.clearTimeout(settleTimer);
  gripped = false;
  setRange(null);
  if (grid.value) live.value = { ...grid.value };
  syncInputs(null);
}

function apply() {
  /* **적용할 수 없는 상태면 어느 길로 와도 적용하지 않는다.** 단추만 막으면 엔터로 표를 통째로
     비울 수 있다(`canApply`). */
  if (!canApply(loading.value, mode.value, named.value.size)) return;
  /* **끌고 있던 값이나 입력 중인 값까지 담아 적용한다.** 확정은 멈춘 뒤에야 따라오는데,
     그 사이에 적용을 누르면 사람이 보고 있던 값과 다른 범위가 적용된다. */
  window.clearTimeout(settleTimer);
  commitRange();
  const pick: ValuePick | null =
    mode.value === "all" ? null : { mode: mode.value, values: [...named.value] };
  emit("apply", pick, range.value);
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
  /* 범위 입력칸의 엔터는 **그 값을 적용하라는 뜻**이다. 값 검색칸의 엔터(`applyTyped`)는
     검색 결과를 선택으로 삼는데, 범위를 고치던 사람에게 그것은 의도가 아니다. */
  if ((e.target as Element).closest?.(".dg-range")) {
    apply();
    return;
  }
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
  window.clearTimeout(settleTimer);
  window.removeEventListener("pointerup", release);
  window.removeEventListener("pointercancel", release);
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
      <!-- 한 줄에 고정한다. 범위를 확정하면 요약이 길어지는데, 줄이 바뀌면 팝오버 높이가
           출렁인다. 넘치면 말줄임이다. -->
      <span class="dg-pop-sum text-fg-subtle" :title="summary">{{ summary }}</span>
    </div>

    <!-- 숫자 범위. 이 블록은 **로딩 표시와 무관하게** 둔다 — 값 검색으로 목록이 다시
         수령될 때마다 사라졌다 나타나면 끌던 손잡이와 입력 중인 값이 소멸한다. -->
    <div v-if="showRange && span && grid" class="dg-range" role="group">
      <!-- 값 목록의 「모두 선택」 줄과 같은 위치(맨 위)에 둔다. -->
      <div class="dg-range-top">
        <button
          type="button"
          class="ml-auto rounded-md border border-line px-2 py-0.5 text-fg-muted hover:bg-surface-hover hover:text-fg disabled:opacity-40"
          :disabled="!narrowed"
          @click="clearRange"
        >
          {{ t("filter.range.clear") }}
        </button>
      </div>
      <div class="dg-range-inputs">
        <input
          ref="minEl"
          class="dg-finput dg-range-num"
          type="number"
          :step="RANGE_STEP"
          :min="grid.min"
          :max="grid.max"
          :disabled="flat"
          :aria-label="t('filter.range.min', { label: column.label })"
          @input="onRangeInput($event, 'min')"
          @change="onRangeChange($event, 'min', 'input')"
        />
        <span class="text-fg-subtle" aria-hidden="true">{{ t("filter.range.sep") }}</span>
        <input
          ref="maxEl"
          class="dg-finput dg-range-num"
          type="number"
          :step="RANGE_STEP"
          :min="grid.min"
          :max="grid.max"
          :disabled="flat"
          :aria-label="t('filter.range.max', { label: column.label })"
          @input="onRangeInput($event, 'max')"
          @change="onRangeChange($event, 'max', 'input')"
        />
      </div>
      <div class="dg-range-slider" :style="fillVars">
        <input
          class="dg-range-thumb"
          type="range"
          :step="RANGE_STEP"
          :min="grid.min"
          :max="grid.max"
          :disabled="flat"
          :value="live.min"
          :aria-label="t('filter.range.min.slider', { label: column.label })"
          @pointerdown="grip"
          @input="onRangeInput($event, 'min')"
          @change="onRangeChange($event, 'min', 'slider')"
        />
        <input
          class="dg-range-thumb"
          type="range"
          :step="RANGE_STEP"
          :min="grid.min"
          :max="grid.max"
          :disabled="flat"
          :value="live.max"
          :aria-label="t('filter.range.max.slider', { label: column.label })"
          @pointerdown="grip"
          @input="onRangeInput($event, 'max')"
          @change="onRangeChange($event, 'max', 'slider')"
        />
      </div>
      <!-- **자료의 범위만 적는다. 지금 고른 값은 적지 않는다.** 둘을 한 줄에 적으면 끄는
           동안 앞의 숫자가 바뀌며 줄 길이가 변하고, 줄바꿈이 오가며 높이가 출렁여 끌고 있는
           손잡이가 움직인다. 고른 값은 바로 위 입력칸에 이미 있다. 이 글은 자료가 바뀌기
           전에는 변하지 않고, 한 줄에 고정한다. -->
      <p class="dg-range-data">
        {{ t("filter.range.data", { span: rangeText(span.min, span.max) }) }}
      </p>
    </div>

    <input
      v-if="showSearch"
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
            {{ valueText(c.value) }}
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
        :disabled="!canApply(loading, mode, named.size)"
        @click="apply"
      >
        {{ t("filter.apply") }}
      </button>
    </div>
  </div>
</template>
