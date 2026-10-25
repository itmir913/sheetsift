import { computed, ref } from "vue";
import { formatError } from "@/i18n";
import { AppError } from "sheetsift-book";
import type { AppErrorPayload } from "@/i18n/codes";

/**
 * 포착한 오류를 보유한다. **문구가 아니라 코드와 치환 값을 보유한다.**
 *
 * 문구를 보유하면 언어를 전환해도 **이미 묘사된 오류만 선행 언어로 잔존한다** —
 * 화면의 타 문구는 전부 전환되어 있어, 판독하는 사람에게는 그 한 문장이 번역
 * 누락으로 인식된다. 여기서는 `locale` 을 참조하는 계산 항목이 문구를 산출하므로
 * 전환이 그대로 추종된다.
 *
 * 바탕에서 유래하지 않은 것(겹 자체의 결함)은 코드가 부재하므로 원문을 그대로
 * 보유한다. **"알 수 없는 오류" 로 치환하지 않는다** — 그 원문이 유일한 단서다.
 */
export function useFault() {
  const fault = ref<AppErrorPayload | string | null>(null);

  const message = computed<string | null>(() => {
    const f = fault.value;
    if (f === null) return null;
    return typeof f === "string" ? f : formatError(f);
  });

  function capture(e: unknown) {
    fault.value =
      e instanceof AppError
        ? { code: e.code, params: e.params, detail: e.detail }
        : String(e);
  }

  function clear() {
    fault.value = null;
  }

  return { message, capture, clear };
}
