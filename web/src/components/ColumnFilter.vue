<script setup lang="ts">
/**
 * 엑셀식 값 선택 팝오버.
 *
 * **후보를 좁히는 일은 바탕이 한다.** goe-navi 에서는 받아 둔 목록을 화면에서
 * 걸렀는데, 그러면 값이 많아 목록이 앞에서 잘린 열에서 찾는 값이 그 뒤에
 * 있을 때 "없습니다" 가 뜬다. 자료는 멀쩡한데 화면만 그렇게 말한다.
 */
import { computed, onMounted, onBeforeUnmount, ref, watch } from "vue";
import { type ColumnDef, type ColumnValue, type ColumnValuesResult } from "@/lib/grid";
import { AppError, limits } from "@/lib/book";

const props = defineProps<{
  column: ColumnDef;
  /** 지금 선택 상태. null 이면 전체 선택. */
  selected: string[] | null;
  /** 후보 목록을 가져오는 방법. `needle` 로 좁힌 것을 돌려준다. */
  load: (needle: string) => Promise<ColumnValuesResult>;
  /** 그리드 기준 자리(px). */
  x: number;
  y: number;
  /** 쓸 수 있는 최대 높이(px). 화면 밖으로 잘리지 않도록 부르는 쪽이 재 준다. */
  maxHeight?: number;
}>();

const emit = defineEmits<{
  apply: [values: string[] | null];
  close: [];
}>();

/* 빈칸 표식과 목록 상한은 **바탕이 내준다.** 여기 적어 두면 바탕이 그것을
   바꾸는 날, 빈칸이 멀쩡한 값으로 보이고 안내문만 옛 숫자를 말한다 — 둘 다
   오류가 나지 않는다. */
const { blankToken, maxValues } = limits();

const root = ref<HTMLElement | null>(null);
const candidates = ref<ColumnValue[]>([]);
const truncated = ref(false);
const loading = ref(true);
const errorMessage = ref<string | null>(null);
const search = ref("");

/**
 * 체크한 값들. **후보를 좁혀도 그대로 남는다** — 좁힌 글자를 지웠을 때 앞서
 * 고른 것이 사라져 있으면, 사람은 자기가 고른 것이 무시당했다고 읽는다.
 */
const checked = ref<Set<string>>(new Set());

/**
 * 거르지 않는 상태인가.
 *
 * **"전부 체크됨" 과 가르는 까닭이 있다.** 후보가 잘렸거나 글자로 좁혀져
 * 있으면 보이는 것이 전부가 아니라, 화면에 체크가 다 차 있어도 그것은 "전체"
 * 가 아니다. 그 둘을 같게 보면 안 고른 값들이 조용히 통과한다.
 */
const all = ref(true);

const numberFormat = new Intl.NumberFormat("ko-KR");

const popVars = computed(() => ({
  "--pop-x": `${props.x}px`,
  "--pop-y": `${props.y}px`,
  "--pop-maxh": `${props.maxHeight ?? 420}px`,
}));

const allShownChecked = computed(
  () =>
    candidates.value.length > 0 &&
    candidates.value.every((c) => all.value || checked.value.has(c.value)),
);

let token = 0;
async function fetchCandidates() {
  const mine = ++token;
  loading.value = true;
  errorMessage.value = null;
  try {
    const res = await props.load(search.value.trim());
    if (mine !== token) return;
    candidates.value = res.values;
    truncated.value = res.truncated;
  } catch (e) {
    if (mine !== token) return;
    errorMessage.value = e instanceof AppError ? e.message : String(e);
  } finally {
    if (mine === token) loading.value = false;
  }
}

/** 전체 상태에서 하나를 건드리면, 지금 보이는 후보 전부를 고른 것에서 시작한다. */
function leaveAll() {
  if (!all.value) return;
  all.value = false;
  checked.value = new Set(candidates.value.map((c) => c.value));
}

function toggle(value: string) {
  leaveAll();
  const next = new Set(checked.value);
  if (next.has(value)) next.delete(value);
  else next.add(value);
  checked.value = next;
}

/** 지금 좁혀져 보이는 것만 한꺼번에 켜고 끈다(엑셀과 같다). */
function toggleAllShown() {
  const on = allShownChecked.value;
  leaveAll();
  const next = new Set(checked.value);
  for (const c of candidates.value) {
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
 * 글자를 쳐 둔 상태면 **지금 좁혀져 보이는 값들을 그대로 고른다.** 체크한
 * 것만 적용하면, 글자만 치고 엔터를 누른 사람 앞에서 표가 텅 빈다. 쳐서
 * 좁혔으면 그게 고른 것이라는 뜻이다.
 *
 * 맞는 값이 하나도 없으면 **아무 일도 하지 않는다.** 그때 적용하면 아무것도
 * 지나지 못하는 필터가 걸리는데, 사람이 원한 것은 그게 아니다.
 */
function applyTyped() {
  if (!search.value.trim()) {
    apply();
    return;
  }
  if (!candidates.value.length) return;
  all.value = false;
  checked.value = new Set(candidates.value.map((c) => c.value));
  apply();
}

function onKeydown(e: KeyboardEvent) {
  if (e.key === "Escape") emit("close");
  if (e.key === "Enter" && !e.isComposing) applyTyped();
}

function onPointerDown(e: PointerEvent) {
  if (root.value && !root.value.contains(e.target as Node)) emit("close");
}

/** 글자마다 바탕에 묻지 않는다. 치는 동안 요청이 줄줄이 나간다. */
let searchTimer: number | undefined;
watch(search, () => {
  window.clearTimeout(searchTimer);
  searchTimer = window.setTimeout(() => void fetchCandidates(), 200);
});

onMounted(() => {
  all.value = props.selected === null;
  checked.value = new Set(props.selected ?? []);
  void fetchCandidates();
  window.addEventListener("keydown", onKeydown);
  // 팝오버가 열린 직후의 클릭까지 잡지 않도록 다음 틱에 등록한다.
  window.setTimeout(() => window.addEventListener("pointerdown", onPointerDown), 0);
});

onBeforeUnmount(() => {
  window.clearTimeout(searchTimer);
  window.removeEventListener("keydown", onKeydown);
  window.removeEventListener("pointerdown", onPointerDown);
});
</script>

<template>
  <div ref="root" class="dg-pop" :style="popVars" role="dialog" :aria-label="`${column.label} 필터`">
    <div class="dg-pop-head">
      <span class="font-semibold">{{ column.label }}</span>
      <span class="text-fg-subtle">
        {{ all ? "전체" : `${numberFormat.format(checked.size)}개 선택` }}
      </span>
    </div>

    <input
      v-model="search"
      class="dg-finput"
      type="search"
      placeholder="값 검색"
      aria-label="값 검색"
    />

    <p v-if="loading" class="dg-pop-note">불러오는 중…</p>
    <p v-else-if="errorMessage" class="dg-pop-note text-danger">{{ errorMessage }}</p>

    <template v-else>
      <label class="dg-pop-row dg-pop-all">
        <input type="checkbox" :checked="allShownChecked" @change="toggleAllShown" />
        <span class="font-medium">
          {{ search.trim() ? "검색 결과 모두 선택" : "모두 선택" }}
        </span>
      </label>

      <div class="dg-pop-list">
        <label v-for="c in candidates" :key="c.value" class="dg-pop-row">
          <input
            type="checkbox"
            :checked="all || checked.has(c.value)"
            @change="toggle(c.value)"
          />
          <span class="dg-pop-val" :data-blank="c.value === blankToken">{{ c.value }}</span>
          <span class="dg-pop-count">{{ numberFormat.format(c.count) }}</span>
        </label>
        <p v-if="!candidates.length" class="dg-pop-note">일치하는 값이 없습니다</p>
      </div>

      <!-- 잘렸다는 것을 **말한다.** 잘린 줄 모르고 '모두 선택' 을 누르면
           보이지 않던 값들이 빠진 필터가 걸린다. -->
      <p v-if="truncated" class="dg-pop-note">
        고유값이 많아 {{ numberFormat.format(maxValues) }}개까지만 보여 줍니다.
        검색으로 좁혀 주세요.
      </p>
    </template>

    <div class="dg-pop-foot">
      <button
        type="button"
        class="rounded-md border border-line px-3 py-1.5 text-fg-muted hover:bg-surface-hover hover:text-fg"
        @click="emit('close')"
      >
        취소
      </button>
      <button
        type="button"
        class="rounded-md bg-accent px-3 py-1.5 font-medium text-accent-fg hover:bg-accent-hover disabled:opacity-40"
        :disabled="loading"
        @click="apply"
      >
        적용
      </button>
    </div>
  </div>
</template>
