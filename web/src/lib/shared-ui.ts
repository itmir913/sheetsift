/**
 * 공용 화면(`sheetsift-ui`)에 이 앱이 주입하는 것.
 *
 * 문구는 이 앱의 사전(`i18n`)이, 자료는 개방한 양식(`workbook.ts`)이, 오류의
 * 판독은 경계(`sheetsift-book`)가 제공한다. 저장소 접두사는 이 앱의 것을 그대로
 * 사용한다 — 패키지 분리 이전에 보관된 테마·열 배치가 그대로 판독된다.
 *
 * **webview 단축키 차단은 지정하지 않는다**(공용 기본값인 비활성). 이 앱은
 * 그것을 요구하는 결정을 수행한 바가 없다.
 */
import type { SharedUiOptions } from "sheetsift-ui";
import { AppError } from "sheetsift-book";
import { APP_NAME, STORAGE_PREFIX } from "@/lib/app";
import { bookSource } from "@/lib/workbook";
import { formatDateTime, formatError, formatNumber, t, tn } from "@/i18n";
import type { AppErrorPayload } from "@/i18n/codes";

export const sharedUi: SharedUiOptions = {
  storagePrefix: STORAGE_PREFIX,
  appName: APP_NAME,
  messages: { t, tn, formatNumber, formatDateTime },
  source: bookSource,
  errors: {
    recognize: (e) =>
      e instanceof AppError
        ? ({ code: e.code, params: e.params, detail: e.detail } satisfies AppErrorPayload)
        : undefined,
    format: (p) => formatError(p as AppErrorPayload),
  },
};
