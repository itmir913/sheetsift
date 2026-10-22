import { ref, watchEffect } from "vue";

import { storageKey } from "@/lib/app";

export type ThemePreference = "light" | "dark" | "system";

const STORAGE_KEY = storageKey("theme");

function readStored(): ThemePreference {
  try {
    const v = localStorage.getItem(STORAGE_KEY);
    if (v === "light" || v === "dark" || v === "system") return v;
  } catch {
    /* 저장소 접근이 차단된 환경에서도 동작해야 한다. */
  }
  return "system";
}

export const themePreference = ref<ThemePreference>(readStored());

const media = window.matchMedia("(prefers-color-scheme: dark)");
const systemPrefersDark = ref(media.matches);
media.addEventListener("change", (e) => (systemPrefersDark.value = e.matches));

export const isDark = ref(false);

watchEffect(() => {
  const pref = themePreference.value;
  isDark.value = pref === "system" ? systemPrefersDark.value : pref === "dark";
  document.documentElement.classList.toggle("dark", isDark.value);
  try {
    localStorage.setItem(STORAGE_KEY, pref);
  } catch {
    /* 저장 실패는 무시한다. */
  }
});

export function setTheme(pref: ThemePreference) {
  themePreference.value = pref;
}
