<script setup lang="ts">
import { themePreference, setTheme, type ThemePreference } from "@/composables/useTheme";
import { ui } from "@/i18n";

const options: { value: ThemePreference; labelKey: "themeLight" | "themeDark" | "themeSystem" }[] = [
  { value: "light", labelKey: "themeLight" },
  { value: "dark", labelKey: "themeDark" },
  { value: "system", labelKey: "themeSystem" },
];
</script>

<template>
  <div
    class="inline-flex items-center gap-1 rounded-lg border border-line bg-surface-sunken p-1"
    role="radiogroup"
    aria-label="화면 모드"
  >
    <button
      v-for="opt in options"
      :key="opt.value"
      type="button"
      role="radio"
      :aria-checked="themePreference === opt.value"
      class="rounded-md px-3 py-1.5 text-base font-medium transition-colors"
      :class="
        themePreference === opt.value
          ? 'bg-accent text-accent-fg'
          : 'text-fg-muted hover:bg-surface-hover hover:text-fg'
      "
      @click="setTheme(opt.value)"
    >
      {{ ui[opt.labelKey] }}
    </button>
  </div>
</template>
