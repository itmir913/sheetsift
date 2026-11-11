/**
 * **부품을 올려 눌러 보는 시험을 적재하는 설정.**
 *
 * `.vue` 를 컴파일할 것(`@vitejs/plugin-vue`)이 필요하다. 환경(jsdom)은 설정이
 * 아니라 **각 시험 파일의 최상단**이 지시한다 — 순수 함수를 판정하는 시험들은
 * DOM 을 요구하지 않고, 전부에 부여하면 그 시험들이 사유 없이 지연된다.
 *
 * 이 파일은 받는 앱으로 반출되지 않는다(`package.json` 의 `files` 는 `src` 와
 * 선언 파일뿐이다). 받는 앱은 자체 설정으로 이 부품들을 컴파일한다.
 */
import vue from "@vitejs/plugin-vue";
import { defineConfig } from "vitest/config";

export default defineConfig({
  plugins: [vue()],
});
