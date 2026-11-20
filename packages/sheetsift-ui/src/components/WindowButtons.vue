<script setup lang="ts">
/**
 * 창 단추 — 최소화 · 최대화 · 닫기. 운영체제의 제목 표시줄을 걷은 창이 그것을 대신한다.
 *
 * **묘사할지는 이 부품이 판정하지 않는다** — `useWindowFrame` 의 `showsWindowButtons` 가
 * 한 곳에서 판정하고, 부르는 곳이 그것으로 가린다. 여기에 또 판정을 두면 사본이 된다.
 *
 * **도형은 받는 앱이 슬롯으로 전달한다.** 공용이 도형을 보유하면 받는 앱이 영영 사용하지
 * 않을 수도 있는 것이 증가하고, 받는 앱은 이미 자기 도형을 한 곳에 집약해 두었다.
 *
 * **끌기 영역(`data-tauri-drag-region`)을 부착하지 않는다** — 부착하면 누르는 즉시 창이
 * 끌려 단추가 눌리지 않는다. 그 속성은 머리의 공백과 글자에만 부착한다.
 */
import { closeWindow, minimizeWindow, toggleMaximizeWindow } from "../composables/useWindowFrame";
import { t } from "../lib/inject";
</script>

<template>
  <span class="wc">
    <button type="button" class="wc-btn" :aria-label="t('win.min')" :data-tip="t('win.min')" @click="minimizeWindow">
      <slot name="min" />
    </button>
    <button type="button" class="wc-btn" :aria-label="t('win.max')" :data-tip="t('win.max')" @click="toggleMaximizeWindow">
      <slot name="max" />
    </button>
    <!-- 되돌릴 수 없는 조작이 셋 중 이것뿐이라 색으로 가른다(`window.css`). -->
    <button type="button" class="wc-btn" data-close :aria-label="t('win.close')" :data-tip="t('win.close')" @click="closeWindow">
      <slot name="close" />
    </button>
  </span>
</template>
