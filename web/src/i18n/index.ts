import { computed, ref } from "vue";
import type { AppErrorPayload, ErrorCode } from "./codes";
import { errors as koErrors, ui as koUi } from "./messages.ko";

export type Locale = "ko";

const bundles = {
  ko: { errors: koErrors, ui: koUi },
} satisfies Record<Locale, { errors: Record<ErrorCode, string>; ui: Record<string, string> }>;

/** 지금은 한국어 하나뿐이지만, 말을 늘릴 자리를 미리 열어 둔다. */
export const locale = ref<Locale>("ko");

export const ui = computed(() => bundles[locale.value].ui);

function interpolate(template: string, params: Record<string, string>): string {
  return template.replace(/\{(\w+)\}/g, (whole, key: string) =>
    key in params ? params[key] : whole,
  );
}

/**
 * 바탕의 오류를 사람에게 보일 문구로.
 *
 * **모르는 코드를 삼키지 않는다** — 코드를 그대로 덧붙여, 사전에서 빠진 것이
 * 그 자리에서 드러나게 한다. 조용히 "알 수 없는 오류" 로만 적으면 무엇이
 * 빠졌는지 아무도 모른다.
 */
export function formatError(err: AppErrorPayload): string {
  const table = bundles[locale.value].errors;
  const template = table[err.code];
  if (!template) return `${table.UNKNOWN} (${err.code})`;
  return interpolate(template, err.params ?? {});
}
