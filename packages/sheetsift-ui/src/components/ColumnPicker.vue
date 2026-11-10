<script setup lang="ts">
/**
 * 열 선택 모달. 보일 열(숨기기)과 열 순서만 고른다.
 *
 * **적용 전에는 표도 저장소도 건드리지 않는다.** 자기 초안만 고치고, 취소하면
 * 연 시점으로 돌아간다. `useColumnLayout.move()` 를 부르지 않는 사유다 — 그것은
 * 즉시 저장까지 한다. 순서 규칙은 `reorder` 를 공유한다.
 *
 * 칩은 `hidden` 만 바꾸고 순서는 건드리지 않는다. 껐다 다시 켠 열은 끝에 붙지
 * 않고 원래 자리로 돌아온다.
 *
 * **마지막 한 열은 제외할 수 없다** — 여기는 편의 차단이고, 정본은
 * `useColumnLayout` 이다(저장분이 손상되어도 비지 않는다).
 */
import { computed, nextTick, ref } from "vue";
import { reorder } from "../lib/grid";
import { t, tn } from "../lib/inject";
import type { PickableColumn } from "../composables/useColumnLayout";

const props = defineProps<{ columns: PickableColumn[] }>();

const emit = defineEmits<{
  apply: [next: { order: string[]; hidden: string[] }];
}>();

const dlg = ref<HTMLDialogElement | null>(null);
const draft = ref<PickableColumn[]>([]);
const dragFrom = ref<string | null>(null);
const dragOver = ref<string | null>(null);

const shown = computed(() => draft.value.filter((c) => !c.hidden));

/* 오른쪽 칩은 **자료의 열 순서**다. 왼쪽(사람이 정한 순서)을 바꿔도 칩이 따라
   움직이면, 커서 밑에서 조작부가 이동해 엉뚱한 열이 눌린다. */
const chips = computed(() => [...draft.value].sort((a, b) => a.at - b.at));

function open() {
  draft.value = props.columns.map((c) => ({ ...c }));
  dlg.value?.showModal();
}

function close() {
  dlg.value?.close();
}

function apply() {
  emit("apply", {
    order: draft.value.map((c) => c.key),
    hidden: draft.value.filter((c) => c.hidden).map((c) => c.key),
  });
  close();
}

function setHidden(key: string, hidden: boolean) {
  const col = draft.value.find((c) => c.key === key);
  if (!col || col.hidden === hidden) return;
  if (hidden && shown.value.length <= 1) return;
  draft.value = draft.value.map((c) => (c.key === key ? { ...c, hidden } : c));
}

/** 보이는 열 기준으로 고른 두 열을, 초안 전체에서의 위치로 환산해 이전한다. */
function moveShown(fromKey: string, toKey: string) {
  const a = draft.value.findIndex((c) => c.key === fromKey);
  const b = draft.value.findIndex((c) => c.key === toKey);
  if (a < 0 || b < 0) return;
  draft.value = reorder(draft.value, a, b);
}

/** DOM 이 재배열되어도 초점이 남도록 사용한 조작부로 되돌린다. */
async function refocus(key: string, acts: string[]) {
  await nextTick();
  for (const act of acts) {
    const el = dlg.value?.querySelector<HTMLButtonElement>(
      `[data-k="${CSS.escape(key)}"][data-act="${act}"]`,
    );
    if (el && !el.disabled) {
      el.focus();
      return;
    }
  }
}

function step(key: string, d: -1 | 1) {
  const i = shown.value.findIndex((c) => c.key === key);
  const other = shown.value[i + d];
  if (!other) return;
  moveShown(key, other.key);
  void refocus(key, d < 0 ? ["up", "down"] : ["down", "up"]);
}

function remove(key: string) {
  const i = shown.value.findIndex((c) => c.key === key);
  setHidden(key, true);
  /* 눌린 행이 사라지므로 그 자리를 이어받은 행(없으면 직전 행)으로 초점을 옮긴다. */
  const next = shown.value[Math.min(i, shown.value.length - 1)];
  if (next) void refocus(next.key, ["remove", "up", "down"]);
}

function onDragStart(e: DragEvent, key: string) {
  dragFrom.value = key;
  e.dataTransfer?.setData("text/plain", key);
}

function onDragEnd() {
  dragFrom.value = null;
  dragOver.value = null;
}

function onDrop(e: DragEvent, key: string) {
  e.preventDefault();
  const from = dragFrom.value;
  onDragEnd();
  if (from && from !== key) moveShown(from, key);
}

defineExpose({ open });
</script>

<template>
  <dialog ref="dlg" class="cp-dialog" aria-labelledby="cp-title">
    <div class="cp-head">
      <h3 id="cp-title" class="cp-title">{{ t("picker.title") }}</h3>
    </div>
    <div class="cp-body">
      <div class="cp-two">
        <section>
          <h4 class="cp-sub-title">{{ t("picker.visible") }}</h4>
          <p class="cp-sub">{{ t("picker.visible.help") }}</p>
          <div class="cp-list">
            <div v-if="!shown.length" class="cp-empty">{{ t("picker.empty") }}</div>
            <div
              v-for="c in shown"
              :key="c.key"
              class="cp-item"
              draggable="true"
              :data-drag="dragFrom === c.key"
              :data-over="dragOver === c.key && dragFrom !== c.key"
              @dragstart="onDragStart($event, c.key)"
              @dragend="onDragEnd"
              @dragover.prevent="dragOver = c.key"
              @drop="onDrop($event, c.key)"
            >
              <span class="cp-grip" aria-hidden="true">⠿</span>
              <span class="cp-label" :data-tip="c.label">{{ c.label }}</span>
              <button
                type="button"
                class="cp-icon"
                :data-k="c.key"
                data-act="up"
                :disabled="shown[0]?.key === c.key"
                :aria-label="t('picker.up.aria', { label: c.label })"
                @click="step(c.key, -1)"
              >
                ↑
              </button>
              <button
                type="button"
                class="cp-icon"
                :data-k="c.key"
                data-act="down"
                :disabled="shown[shown.length - 1]?.key === c.key"
                :aria-label="t('picker.down.aria', { label: c.label })"
                @click="step(c.key, 1)"
              >
                ↓
              </button>
              <button
                type="button"
                class="cp-icon"
                :data-k="c.key"
                data-act="remove"
                :disabled="shown.length <= 1"
                :aria-label="t('picker.remove.aria', { label: c.label })"
                @click="remove(c.key)"
              >
                ✕
              </button>
            </div>
          </div>
        </section>
        <section>
          <h4 class="cp-sub-title">{{ t("picker.available") }}</h4>
          <p class="cp-sub">{{ t("picker.available.help") }}</p>
          <div class="cp-chips">
            <button
              v-for="c in chips"
              :key="c.key"
              type="button"
              class="cp-chip"
              :aria-pressed="!c.hidden"
              :disabled="!c.hidden && shown.length <= 1"
              @click="setHidden(c.key, !c.hidden)"
            >
              {{ c.label }}
            </button>
          </div>
        </section>
      </div>
    </div>
    <div class="cp-foot">
      <span class="cp-count">
        {{ tn("picker.count", shown.length, { total: draft.length, shown: shown.length }) }}
      </span>
      <span class="cp-actions">
        <button type="button" class="cp-btn" @click="close">{{ t("picker.cancel") }}</button>
        <button type="button" class="cp-btn cp-btn-primary" @click="apply">
          {{ t("picker.apply") }}
        </button>
      </span>
    </div>
  </dialog>
</template>
