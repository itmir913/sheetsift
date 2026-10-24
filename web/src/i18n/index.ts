import { computed, ref } from "vue";
import type { AppErrorPayload, ErrorCode } from "./codes";
import { errors as koErrors, ui as koUi, type UiKey } from "./messages.ko";
import { errors as enErrors, ui as enUi } from "./messages.en";
import { storageKey } from "@/lib/app";

export type Locale = "ko" | "en";

/** 화면 문구의 열쇠. **한국어 벌이 정본이다** — 영어 벌은 이 집합을 충족해야 한다. */
export type { UiKey };

/**
 * 수에 따라 형태가 변하는 문구의 열쇠.
 *
 * `x_one`·`x_other` 쌍에서 `x` 를 추출한다. 한국어에는 변화가 부재하나 영어에는
 * 존재하므로, 쌍으로 기재하고 [`tn`] 이 선택한다.
 */
type StripOther<K> = K extends `${infer B}_other` ? B : never;
export type PluralKey = StripOther<UiKey>;

const numberFormat = computed(() => new Intl.NumberFormat(locale.value));

const bundles = {
  ko: { errors: koErrors, ui: koUi as Record<UiKey, string> },
  en: { errors: enErrors, ui: enUi },
} satisfies Record<Locale, { errors: Record<ErrorCode, string>; ui: Record<UiKey, string> }>;

const STORAGE_KEY = storageKey("locale");

/**
 * 최초의 언어. 보관된 선택이 존재하면 그것이고, 아니면 브라우저의 언어다.
 *
 * **한국어가 아니면 영어다.** 지원하지 않는 언어를 한국어로 묘사하면 판독하지
 * 못하는 문자가 표시되고, 영어는 적어도 자모를 판독할 수 있다.
 */
function first(): Locale {
  try {
    const v = localStorage.getItem(STORAGE_KEY);
    if (v === "ko" || v === "en") return v;
  } catch {
    /* 저장소 접근이 차단된 환경에서도 동작해야 한다. */
  }
  const langs = typeof navigator === "undefined" ? [] : (navigator.languages ?? [navigator.language]);
  return langs.some((l) => l?.toLowerCase().startsWith("ko")) ? "ko" : "en";
}

export const locale = ref<Locale>(first());

/**
 * 문서의 언어를 화면의 언어와 일치시킨다.
 *
 * **`<html lang>` 은 장식이 아니다.** 보조기기의 발음, 줄바꿈 규칙, 서체의
 * 한글 조판 기능(`ss06`)이 전부 이 값을 참조한다. 갱신하지 않으면 영어 화면이
 * 한국어 조판 규칙으로 묘사된다.
 */
function syncDocument() {
  if (typeof document !== "undefined") document.documentElement.lang = locale.value;
}
syncDocument();

export function setLocale(next: Locale): void {
  locale.value = next;
  syncDocument();
  try {
    localStorage.setItem(STORAGE_KEY, next);
  } catch {
    /* 저장 실패는 무시한다. */
  }
}

function interpolate(template: string, params: Record<string, string | number>): string {
  return template.replace(/\{(\w+)\}/g, (whole, key: string) =>
    key in params ? String(params[key]) : whole,
  );
}

/**
 * 화면 문구 하나.
 *
 * **누락된 열쇠를 은폐하지 않는다** — 열쇠를 그대로 반환해 그 위치에서
 * 발현되게 한다. 공백으로 묘사하면 무엇이 누락되었는지 누구도 인지하지 못한다.
 * 타입이 이미 차단하므로 여기까지 도달하는 것은 사전이 파손된 경우다.
 */
export function t(key: UiKey, params?: Record<string, string | number>): string {
  const table = bundles[locale.value].ui;
  const template = table[key];
  if (template === undefined) return key;
  return params ? interpolate(template, params) : template;
}

/** 수에 따라 형태가 변하는 문구. 영어의 단수·복수를 `Intl.PluralRules` 가 판정한다. */
export function tn(
  key: PluralKey,
  n: number,
  params?: Record<string, string | number>,
): string {
  const rule = new Intl.PluralRules(locale.value).select(n);
  const exact = `${key}_${rule}` as UiKey;
  const table = bundles[locale.value].ui;
  const picked = exact in table ? exact : (`${key}_other` as UiKey);
  return t(picked, { n: formatNumber(n), ...params });
}

/**
 * 화면에 묘사하는 수.
 *
 * **자료의 셀은 이것을 사용하지 않는다**(`lib/grid.ts` 의 `cellText`). 셀은
 * 양식에 기재된 값이라 사람이 구분 기호를 선택할 수 있어야 하고, 여기 수는
 * 화면이 산출한 개수라 언제나 읽기 좋은 형태로 묘사한다.
 */
export function formatNumber(n: number): string {
  return numberFormat.value.format(n);
}

/** 인쇄 시각처럼 화면이 산출하는 일시. */
export function formatDateTime(d: Date): string {
  return new Intl.DateTimeFormat(locale.value, {
    dateStyle: "long",
    timeStyle: "short",
  }).format(d);
}

/**
 * 바탕의 오류를 사람에게 노출할 문구로.
 *
 * **미지의 코드를 은폐하지 않는다** — 코드를 그대로 부기해, 사전에서 누락된 것이
 * 그 위치에서 발현되게 한다. 묵시적으로 "알 수 없는 오류" 로만 기재하면 무엇이
 * 누락되었는지 누구도 인지하지 못한다.
 */
export function formatError(err: AppErrorPayload): string {
  const table = bundles[locale.value].errors;
  const template = table[err.code];
  if (!template) return `${table.UNKNOWN} (${err.code})`;
  return interpolate(template, err.params ?? {});
}
