<script setup lang="ts">
/**
 * 조작부의 도형.
 *
 * **문자를 사용하지 않는 사유.** 종전에는 `⏮ ◀ ▶ ⏭ − ＋ ↑ ↓ ✕ ▲ ▼` 를 그대로
 * 기재했다. 그것들은 서체의 문자라 서체·플랫폼마다 자형과 굵기와 폭이 변동하고,
 * 전각(`＋`)과 반각(`−`)이 한 쌍으로 배치되면 두 단추의 중심이 어긋난다. 선의
 * 굵기가 제각각이라 조작부가 거칠게 판독되는 주된 사유였다.
 *
 * 전부 동일한 좌표계(24)와 동일한 선 굵기로 작도한다. 크기는 `1em` 이라 표의
 * 글자 크기를 따라간다 — 사용자가 글자를 축소하면 도형도 함께 축소한다.
 *
 * 색은 `currentColor` 다. 이 파일은 색 값을 기재하지 않는다.
 */
const PATHS = {
  /** 처음 쪽. */
  first: "M18 6l-6 6 6 6M7 6v12",
  /** 이전 쪽. */
  prev: "M14.5 6l-6 6 6 6",
  /** 다음 쪽. */
  next: "M9.5 6l6 6-6 6",
  /** 마지막 쪽. */
  last: "M6 6l6 6-6 6M17 6v12",
  /** 감소. */
  minus: "M5 12h14",
  /** 증가. */
  plus: "M12 5v14M5 12h14",
  /** 위로 이동. */
  up: "M12 19V5M6 11l6-6 6 6",
  /** 아래로 이동. */
  down: "M12 5v14M6 13l6 6 6-6",
  /** 해제·제거. */
  close: "M6 6l12 12M18 6L6 18",
  /** 오름차순 정렬. */
  asc: "M6 14l6-6 6 6",
  /** 내림차순 정렬. */
  desc: "M6 10l6 6 6-6",
} as const;

export type GridIconName = keyof typeof PATHS;

defineProps<{ name: GridIconName }>();
</script>

<template>
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
    <path :d="PATHS[name]" />
  </svg>
</template>
