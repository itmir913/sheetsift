import { ref, watchEffect } from "vue";

import { storageKey } from "../lib/inject";

export type ThemePreference = "light" | "dark" | "system";

/* 저장소 키는 접두사를 주입받은 이후에 구성한다 — 모듈을 반입하는 시점에는
   아직 주입되지 않았을 수 있다. */
const STORAGE_KEY = () => storageKey("theme");

function readStored(): ThemePreference {
  try {
    const v = localStorage.getItem(STORAGE_KEY());
    if (v === "light" || v === "dark" || v === "system") return v;
  } catch {
    /* 저장소 접근이 차단된 환경에서도 동작해야 한다. */
  }
  return "system";
}

export const themePreference = ref<ThemePreference>("system");

export const isDark = ref(false);

let installed = false;

/**
 * 보관된 선택을 판독해 `<html>` 에 반영하고, 이후의 변경을 추종한다.
 *
 * **1회만 작동한다.** 받는 앱이 [`installSharedUi`](../index.ts) 로 호출하고,
 * 그 시점은 화면을 장착하기 이전이다 — 최초 묘사가 그릇된 테마로 점멸하지 않는다.
 */
export function installTheme(): void {
  if (installed) return;
  installed = true;
  themePreference.value = readStored();

  const media = window.matchMedia("(prefers-color-scheme: dark)");
  const systemPrefersDark = ref(media.matches);
  media.addEventListener("change", (e) => (systemPrefersDark.value = e.matches));

  watchEffect(() => {
    const pref = themePreference.value;
    isDark.value = pref === "system" ? systemPrefersDark.value : pref === "dark";
    document.documentElement.classList.toggle("dark", isDark.value);
    try {
      localStorage.setItem(STORAGE_KEY(), pref);
    } catch {
      /* 저장 실패는 무시한다. */
    }
  });
}

export function setTheme(pref: ThemePreference) {
  themePreference.value = pref;
}
