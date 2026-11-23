<script setup lang="ts">
/**
 * 화면의 언어.
 *
 * **밝기 전환과 동일한 형태의 도형 단추다.** 양자 모두 "이 화면을 어떻게
 * 묘사할지" 이고, 일방만 상이한 형태로 배치하면 동급의 조작이 아닌 것처럼
 * 인식된다.
 *
 * 밝기와 상이한 점은 **선택지가 둘로 한정되지 않는다**는 것이다. 번갈아 두면
 * 언어가 증가하는 즉시 조작이 성립하지 않으므로, 조작하면 목록이 개방된다.
 *
 * 각 언어의 명칭은 **그 언어로** 기재한다(`한국어`·`English`). 현재 언어로
 * 번역해 기재하면, 판독하지 못하는 언어가 선택된 사람이 복구할 단추를 탐색할
 * 수 없다.
 */
import { onBeforeUnmount, onMounted, ref } from "vue";
import { locale, setLocale, t, type Locale, type UiKey } from "@/i18n";

const options: { value: Locale; key: UiKey }[] = [
  { value: "ko", key: "locale.ko" },
  { value: "en", key: "locale.en" },
];

const open = ref(false);
const root = ref<HTMLElement | null>(null);

function choose(value: Locale) {
  setLocale(value);
  open.value = false;
}

/**
 * 바깥을 조작하면 폐쇄한다. **목록을 상주시키지 않는다** — 개방한 채로 타
 * 조작을 수행하면 그 목록이 머리의 하부를 계속 차폐한다.
 */
function onPointerDown(ev: PointerEvent) {
  if (!open.value) return;
  const el = ev.target as Node | null;
  if (el && root.value?.contains(el)) return;
  open.value = false;
}

function onKeydown(ev: KeyboardEvent) {
  if (ev.key === "Escape" && open.value) {
    open.value = false;
    ev.stopPropagation();
  }
}

onMounted(() => {
  window.addEventListener("pointerdown", onPointerDown, true);
  window.addEventListener("keydown", onKeydown);
});
onBeforeUnmount(() => {
  window.removeEventListener("pointerdown", onPointerDown, true);
  window.removeEventListener("keydown", onKeydown);
});

/** 지구. 좌표계와 선 굵기는 표의 도형과 동일하다. */
const GLOBE = "M3 12h18M12 3a15 15 0 0 1 0 18a15 15 0 0 1 0-18";
</script>

<template>
  <div ref="root" class="app-pop-at">
    <button
      type="button"
      class="app-ic"
      :aria-expanded="open"
      aria-haspopup="menu"
      :aria-label="t('locale.aria')"
      :data-tip="t('locale.aria')"
      @click="open = !open"
    >
      <svg
        class="ss-icon"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="1.8"
        stroke-linecap="round"
        stroke-linejoin="round"
        aria-hidden="true"
      >
        <circle cx="12" cy="12" r="9" />
        <path :d="GLOBE" />
      </svg>
    </button>

    <div v-if="open" class="app-pop" role="menu" :aria-label="t('locale.aria')">
      <button
        v-for="opt in options"
        :key="opt.value"
        type="button"
        role="menuitemradio"
        :aria-checked="locale === opt.value"
        :lang="opt.value"
        class="app-pop-item"
        :data-on="locale === opt.value"
        @click="choose(opt.value)"
      >
        {{ t(opt.key) }}
      </button>
    </div>
  </div>
</template>
