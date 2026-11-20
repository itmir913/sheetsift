<script setup lang="ts">
/**
 * 창 단추 — 최소화 · 최대화 · 닫기. 운영체제의 제목 표시줄을 걷었으므로
 * (`tauri.conf.json` 의 `decorations: false`) 앱이 직접 묘사한다.
 *
 * **창에서만 묘사한다.** 부르는 곳이 `IS_DESKTOP` 으로 가리고, 조작은
 * `sheetsift-shell` 경계를 경유한다 — 이 파일은 `@tauri-apps/api` 를 반입하지 않는다.
 * 반입하면 브라우저 묶음에 적재되고, 그 환경에서 호출하는 시점에 중단된다.
 *
 * **동작은 `crates/tauri/capabilities/default.json` 의 권한 셋이 개방한다. 권한이
 * 부재하면 단추가 오류도 없이 아무 작업도 수행하지 않는다.**
 *
 * **끌기 영역(`data-tauri-drag-region`)을 부착하지 않는다** — 부착하면 누르는 즉시
 * 창이 끌려 단추가 눌리지 않는다. 머리의 빈 자리와 글자에만 부착한다(`App.vue`).
 */
import { close, minimize, toggleMaximize } from "sheetsift-shell";
import { t } from "@/i18n";
</script>

<template>
  <span class="app-wc">
    <button
      type="button"
      class="app-wc-btn"
      :aria-label="t('win.min')"
      :data-tip="t('win.min')"
      @click="minimize"
    >
      <!-- 도형은 글자가 아니라 선이다. 표의 도형(`ss-icon`)은 공용 꾸러미의 것이고
           창 단추는 이 앱의 것이라, 거기 넣지 않는다 — 받는 앱이 자기 창 단추를 지녔다. -->
      <svg class="app-wc-icon" viewBox="0 0 10 10" aria-hidden="true">
        <path d="M1 5h8" />
      </svg>
    </button>
    <button
      type="button"
      class="app-wc-btn"
      :aria-label="t('win.max')"
      :data-tip="t('win.max')"
      @click="toggleMaximize"
    >
      <svg class="app-wc-icon" viewBox="0 0 10 10" aria-hidden="true">
        <rect x="1.5" y="1.5" width="7" height="7" />
      </svg>
    </button>
    <button
      type="button"
      class="app-wc-btn"
      data-close
      :aria-label="t('win.close')"
      :data-tip="t('win.close')"
      @click="close"
    >
      <svg class="app-wc-icon" viewBox="0 0 10 10" aria-hidden="true">
        <path d="M1.5 1.5l7 7M8.5 1.5l-7 7" />
      </svg>
    </button>
  </span>
</template>
