<script setup lang="ts">
/**
 * 엑셀식 값 선택 팝오버.
 *
 * **후보를 좁히는 일은 바탕이 한다.** goe-navi 에서는 받아 둔 목록을 화면에서
 * 걸렀는데, 그러면 값이 많아 목록이 앞에서 잘린 열에서 찾는 값이 그 뒤에
 * 있을 때 "없습니다" 가 뜬다. 자료는 멀쩡한데 화면만 그렇게 말한다.
 *
 * 그래서 목록을 **두 벌** 든다. 좁힌 글자로 받은 것(보여 줄 것)과, 좁히지 않고
 * 받은 것(전부). 왜 둘이어야 하는지는 [`leaveAll`] 에 적었다.
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

/** 지금 보여 주는 후보. 좁힌 글자가 있으면 그것으로 받은 것이다. */
const shown = ref<ColumnValue[]>([]);

/**
 * 좁히지 않고 받은 후보 전부. **화면에 그리지 않는다** — [`leaveAll`] 이 쓴다.
 */
const everyValue = ref<ColumnValue[]>([]);

/** 그 전부가 상한에 걸려 잘렸는가. */
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
  () => shown.value.length > 0 && shown.value.every((c) => all.value || checked.value.has(c.value)),
);

/** 좁힌 글자로 다시 받는다. 늦게 온 답이 새 답을 덮지 않게 번호를 센다. */
let token = 0;
async function fetchShown() {
  const mine = ++token;
  loading.value = true;
  errorMessage.value = null;
  try {
    const res = await props.load(search.value.trim());
    if (mine !== token) return;
    shown.value = res.values;
    // 좁히지 않고 받은 것이면 그것이 곧 전부다.
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
 * 전체 상태를 벗어난다. 체크 상태를 **지금 통과하고 있는 값 전부**로 채운다.
 *
 * **좁혀 보이는 것으로 채우면 안 된다.** 「대」로 좁혀 대구·대전만 보이는 채로
 * 대전 하나를 끄면, 사람이 뜻한 것은 "대전만 빼라" 인데 손대지도 않은 광주·
 * 부산·서울·인천까지 함께 빠진다. 표는 멀쩡히 그려지고 어디에서도 오류가 나지
 * 않는다 — 실제로 그렇게 났다.
 *
 * 그래서 좁히지 않고 받아 둔 목록에서 채운다. 그 목록이 상한에 걸려 잘렸으면
 * 어떤 목록으로도 "하나만 빼라" 를 적을 수 없는데, 그때는 팝오버가 잘렸다고
 * 말하고 있다.
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

/** 지금 좁혀져 보이는 것만 한꺼번에 켜고 끈다(엑셀과 같다). */
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
 * 글자를 쳐 둔 상태면 **지금 좁혀져 보이는 값들만 고른다.** 체크한 것만
 * 적용하면, 글자만 치고 엔터를 누른 사람 앞에서 표가 텅 빈다. 쳐서 좁혔으면
 * 그게 고른 것이라는 뜻이다 — 위의 [`leaveAll`] 과 달리 여기서는 좁힌 것이
 * 사람이 **직접 말한 뜻**이라 그대로 받는다.
 *
 * 맞는 값이 하나도 없으면 **아무 일도 하지 않는다.** 그때 적용하면 아무것도
 * 지나지 못하는 필터가 걸리는데, 사람이 원한 것은 그게 아니다.
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
  searchTimer = window.setTimeout(() => void fetchShown(), 200);
});

onMounted(() => {
  all.value = props.selected === null;
  checked.value = new Set(props.selected ?? []);
  void fetchShown();
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
        <label v-for="c in shown" :key="c.value" class="dg-pop-row">
          <input
            type="checkbox"
            :checked="all || checked.has(c.value)"
            @change="toggle(c.value)"
          />
          <span class="dg-pop-val" :data-blank="c.value === blankToken">{{ c.value }}</span>
          <span class="dg-pop-count">{{ numberFormat.format(c.count) }}</span>
        </label>
        <p v-if="!shown.length" class="dg-pop-note">일치하는 값이 없습니다</p>
      </div>

      <!-- 잘렸다는 것을 **말한다.** 잘린 줄 모르고 고르면, 보이지 않던 값들이
           빠진 필터가 걸린다. -->
      <p v-if="truncated" class="dg-pop-note">
        고유값이 많아 {{ numberFormat.format(maxValues) }}개까지만 봅니다.
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
