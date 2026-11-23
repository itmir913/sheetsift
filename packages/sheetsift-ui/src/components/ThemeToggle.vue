<script setup lang="ts">
/**
 * 밝기 전환. **도형 하나짜리 단추다.**
 *
 * 종전에는 `밝게 | 어둡게 | 시스템` 의 분절 선택기였다. 유리면 위에서 그것은
 * 머리의 폭을 과점하고, 선택지 셋 중 둘은 사람이 1회 선정한 이후 다시 조작하지
 * 않는다. 그래서 **밝게 ↔ 어둡게를 번갈아 두는 것 하나로 축약한다.**
 *
 * **시스템은 선택지에서 제외하되 소멸시키지 않는다.** 보관된 선택이 부재할 때의
 * 기본값으로 `useTheme` 에 잔존한다 — 최초 1회는 운영체제의 선택을 추종하고,
 * 사람이 이 단추를 조작한 시점부터 그 선택이 보관된다.
 *
 * 도형은 지금의 상태가 아니라 **조작하면 도달하는 쪽**을 묘사한다. 설명 상자의
 * 문구도 동일한 쪽이다 — 양자가 상이한 쪽을 가리키면 어느 것이 현재인지
 * 판별되지 않는다.
 */
import { isDark, setTheme } from "../composables/useTheme";
import { t } from "../lib/inject";

/** 해(밝게)와 달(어둡게). 좌표계와 선 굵기는 `GridIcon` 과 동일하다. */
const SUN = "M12 4V2M12 22v-2M4 12H2M22 12h-2M5.6 5.6L4.2 4.2M19.8 19.8l-1.4-1.4M18.4 5.6l1.4-1.4M4.2 19.8l1.4-1.4";
const MOON = "M20 14.5A8.5 8.5 0 0 1 9.5 4a8.5 8.5 0 1 0 10.5 10.5z";
</script>

<template>
  <button
    type="button"
    class="app-ic"
    :aria-label="t('theme.aria')"
    :data-tip="t(isDark ? 'theme.light' : 'theme.dark')"
    @click="setTheme(isDark ? 'light' : 'dark')"
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
      <template v-if="isDark">
        <circle cx="12" cy="12" r="4" />
        <path :d="SUN" />
      </template>
      <path v-else :d="MOON" />
    </svg>
  </button>
</template>
