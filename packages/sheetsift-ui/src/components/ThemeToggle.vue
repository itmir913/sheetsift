<script setup lang="ts">
import { themePreference, setTheme, type ThemePreference } from "../composables/useTheme";
import { t, type SharedUiKey } from "../lib/inject";

const options: { value: ThemePreference; key: SharedUiKey }[] = [
  { value: "light", key: "theme.light" },
  { value: "dark", key: "theme.dark" },
  { value: "system", key: "theme.system" },
];
</script>

<template>
  <div class="app-seg" role="radiogroup" :aria-label="t('theme.aria')">
    <button
      v-for="opt in options"
      :key="opt.value"
      type="button"
      role="radio"
      :aria-checked="themePreference === opt.value"
      class="app-seg-btn"
      :data-on="themePreference === opt.value"
      @click="setTheme(opt.value)"
    >
      {{ t(opt.key) }}
    </button>
  </div>
</template>
