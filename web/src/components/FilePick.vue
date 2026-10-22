<script setup lang="ts">
/**
 * 양식을 수령하는 위치.
 *
 * **선택도 투입도 동일한 단일 경로를 경유한다.** 두 경로를 개별 배치했더니
 * 투입한 파일만 명칭을 상실해, csv 의 시트 명칭이 매회 상이했다.
 */
import { ref } from "vue";
import { canPickFile, pickFile } from "sheetsift-book";

const emit = defineEmits<{ pick: [file: File]; pickAt: [path: string, name: string] }>();

/**
 * **창에서는 운영체제의 대화상자를 경유한다.** 바이트가 아니라 경로가 도달해,
 * 파일 전체를 화면이 1회 적재하지 않는다 — 데스크톱을 도입한 사유가 그것이다.
 *
 * 투입(drag-drop)은 양측 모두 바이트다. 그쪽까지 경로로 전환하려면 창의
 * 사건을 수신해야 하고, 그것은 화면이 어느 쪽에서 구동 중인지 인지하는 코드를
 * 증가시킨다.
 */
const native = canPickFile();

async function onPickNative() {
  const chosen = await pickFile();
  if (chosen) emit("pickAt", chosen.path, chosen.name);
}

defineProps<{ busy?: boolean; note?: string | null; danger?: boolean }>();

const over = ref(false);

function onInput(ev: Event) {
  const file = (ev.target as HTMLInputElement).files?.[0];
  if (file) emit("pick", file);
  // 동일한 파일을 재선택할 수 있어야 한다. 초기화하지 않으면 change 가 발생하지 않는다.
  (ev.target as HTMLInputElement).value = "";
}

function onDrop(ev: DragEvent) {
  over.value = false;
  const file = ev.dataTransfer?.files?.[0];
  if (file) emit("pick", file);
}
</script>

<template>
  <div class="flex flex-col gap-3">
    <component
      :is="native ? 'div' : 'label'"
      class="flex cursor-pointer flex-col items-center justify-center gap-2 rounded-xl border-2 border-dashed border-line bg-surface px-6 py-10 text-center text-fg-muted transition-colors hover:border-accent hover:text-fg"
      :class="over ? 'border-accent bg-accent-surface text-fg' : ''"
      @dragover.prevent="over = true"
      @dragleave="over = false"
      @drop.prevent="onDrop"
      @click="native ? onPickNative() : undefined"
    >
      <input v-if="!native" type="file" class="hidden" accept=".xlsx,.xls,.csv" @change="onInput" />
      <span class="font-medium">xlsx 또는 csv 파일을 선택하거나 이 영역에 투입하십시오</span>
      <span class="text-fg-subtle">{{
        busy ? "판독 중…" : native ? "파일은 이 컴퓨터 내부에서만 개방됩니다" : "파일은 이 브라우저 내부에서만 개방됩니다"
      }}</span>
    </component>

    <p v-if="note" role="status" aria-live="polite" :class="danger ? 'text-danger' : 'text-fg-muted'">
      {{ note }}
    </p>
  </div>
</template>
