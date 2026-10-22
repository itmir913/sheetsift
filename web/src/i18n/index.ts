import { computed, ref } from "vue";
import type { AppErrorPayload, ErrorCode } from "./codes";
import { errors as koErrors, ui as koUi } from "./messages.ko";

export type Locale = "ko";

const bundles = {
  ko: { errors: koErrors, ui: koUi },
} satisfies Record<Locale, { errors: Record<ErrorCode, string>; ui: Record<string, string> }>;

/** 현재는 한국어 하나뿐이나, 언어를 추가할 여지를 사전에 확보해 둔다. */
export const locale = ref<Locale>("ko");

export const ui = computed(() => bundles[locale.value].ui);

function interpolate(template: string, params: Record<string, string>): string {
  return template.replace(/\{(\w+)\}/g, (whole, key: string) =>
    key in params ? params[key] : whole,
  );
}

/**
 * 바탕의 오류를 사람에게 노출할 문구로.
 *
 * **미지의 코드를 은폐하지 않는다** — 코드를 그대로 부기해, 사전에서 누락된 것이
 * 그 위치에서 발현되게 한다. 묵시적으로 "미상의 오류" 로만 기재하면 무엇이
 * 누락되었는지 누구도 인지하지 못한다.
 */
export function formatError(err: AppErrorPayload): string {
  const table = bundles[locale.value].errors;
  const template = table[err.code];
  if (!template) return `${table.UNKNOWN} (${err.code})`;
  return interpolate(template, err.params ?? {});
}
