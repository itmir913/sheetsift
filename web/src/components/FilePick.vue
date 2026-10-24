<script setup lang="ts">
/**
 * 양식을 수령하는 위치.
 *
 * **선택도 투입도 동일한 단일 경로를 경유한다.** 두 경로를 개별 배치했더니
 * 투입한 파일만 명칭을 상실해, csv 의 시트 명칭이 매회 상이했다.
 */
import { ref } from "vue";
import { canPickFile, pickFile } from "sheetsift-book";
import { t } from "@/i18n";

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

/**
 * `compact` 는 **개방 이후**의 형태다. 수령하는 경로는 동일하고 묘사만
 * 축소한다 — 경로를 두 벌로 분할하면 일방만 수정된다. 양식을 열람하는 중에
 * 투입 영역이 본문 상단을 점유하면 표가 그만큼 하향 이동한다.
 */
defineProps<{ busy?: boolean; note?: string | null; danger?: boolean; compact?: boolean }>();

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
  <!-- 개방 이후의 축소된 형태. 단추 하나이고, 투입도 동일하게 수령한다. -->
  <component
    v-if="compact"
    :is="native ? 'button' : 'label'"
    :type="native ? 'button' : undefined"
    class="app-head-btn"
    :class="over ? 'app-head-btn-over' : ''"
    :data-tip="t(busy ? 'pick.busy.tip' : 'pick.change.tip')"
    @dragover.prevent="over = true"
    @dragleave="over = false"
    @drop.prevent="onDrop"
    @click="native ? onPickNative() : undefined"
  >
    <input v-if="!native" type="file" class="hidden" accept=".xlsx,.xls,.csv" @change="onInput" />
    {{ t(busy ? "pick.reading" : "pick.change") }}
  </component>

  <div v-else class="flex flex-col gap-3">
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
      <span class="font-medium">{{ t("pick.drop") }}</span>
      <span class="text-fg-subtle">
        {{ busy ? t("pick.reading") : t(native ? "pick.safe.desktop" : "pick.safe.web") }}
      </span>
    </component>

    <p v-if="note" role="status" aria-live="polite" :class="danger ? 'text-danger' : 'text-fg-muted'">
      {{ note }}
    </p>
  </div>
</template>
