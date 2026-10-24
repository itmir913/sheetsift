<script setup lang="ts">
/**
 * 화면의 언어.
 *
 * **테마 토글과 동일한 형태다.** 양자 모두 "이 화면을 어떻게 묘사할지" 이고,
 * 일방만 상이한 형태로 배치하면 동급의 조작이 아닌 것처럼 인식된다.
 *
 * 각 언어의 명칭은 **그 언어로** 기재한다(`한국어`·`English`). 현재 언어로
 * 번역해 기재하면, 판독하지 못하는 언어가 선택된 사람이 복구할 단추를 탐색할
 * 수 없다.
 */
import { locale, setLocale, t, type Locale, type UiKey } from "@/i18n";

const options: { value: Locale; key: UiKey }[] = [
  { value: "ko", key: "locale.ko" },
  { value: "en", key: "locale.en" },
];
</script>

<template>
  <div class="app-seg" role="radiogroup" :aria-label="t('locale.aria')">
    <button
      v-for="opt in options"
      :key="opt.value"
      type="button"
      role="radio"
      :aria-checked="locale === opt.value"
      :lang="opt.value"
      class="app-seg-btn"
      :data-on="locale === opt.value"
      @click="setLocale(opt.value)"
    >
      {{ t(opt.key) }}
    </button>
  </div>
</template>
