<script setup lang="ts">
/**
 * 양식을 받는 자리.
 *
 * **고르는 것도 끌어다 놓는 것도 같은 한 길로 간다.** 두 길을 따로 두었더니
 * 끌어다 놓은 파일만 이름을 잃어, csv 의 시트 이름이 매번 달라졌다.
 */
import { ref } from "vue";

const emit = defineEmits<{ pick: [file: File] }>();

defineProps<{ busy?: boolean; note?: string | null; danger?: boolean }>();

const over = ref(false);

function onInput(ev: Event) {
  const file = (ev.target as HTMLInputElement).files?.[0];
  if (file) emit("pick", file);
  // 같은 파일을 다시 고를 수 있어야 한다. 비우지 않으면 change 가 오지 않는다.
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
    <label
      class="flex cursor-pointer flex-col items-center justify-center gap-2 rounded-xl border-2 border-dashed border-line bg-surface px-6 py-10 text-center text-fg-muted transition-colors hover:border-accent hover:text-fg"
      :class="over ? 'border-accent bg-accent-surface text-fg' : ''"
      @dragover.prevent="over = true"
      @dragleave="over = false"
      @drop.prevent="onDrop"
    >
      <input type="file" class="hidden" accept=".xlsx,.xls,.csv" @change="onInput" />
      <span class="font-medium">xlsx 또는 csv 파일을 고르거나 여기에 끌어다 놓습니다</span>
      <span class="text-fg-subtle">{{ busy ? "읽는 중…" : "파일은 이 브라우저 안에서만 열립니다" }}</span>
    </label>

    <p v-if="note" role="status" aria-live="polite" :class="danger ? 'text-danger' : 'text-fg-muted'">
      {{ note }}
    </p>
  </div>
</template>
