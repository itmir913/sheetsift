/**
 * **sheetsift-ui — 표를 열람하는 공용 화면.** sheetsift 의 앱과 goe-navi 가 동일한
 * 원본을 사용한다.
 *
 * 받는 앱은 화면을 장착하기 이전에 [`installSharedUi`] 를 1회 호출한다. 이
 * 패키지는 문구·색 값·저장소 명칭·자료의 문을 보유하지 않고, 전부 여기서
 * 수령한다(`docs/shared-ui.md`).
 *
 * 부품은 원본(`.vue`·`.ts`)으로 반출한다. 받는 쪽의 vite 가 컴파일한다.
 */
import { installGrouping } from "./composables/useGrouping";
import { installTheme } from "./composables/useTheme";
import {
  provideAppName,
  provideErrors,
  provideGridSource,
  provideMessages,
  provideStoragePrefix,
  type GridSource,
  type SharedUiErrors,
  type SharedUiMessages,
} from "./lib/inject";
import { installShortcutGuard, type ShortcutGuard } from "./lib/shortcuts";

export type {
  ColumnValuesRequest,
  GridLimits,
  GridSource,
  MessageParams,
  PrintRequest,
  PrintResponse,
  QueryRequest,
  SharedPluralKey,
  SharedUiErrors,
  SharedUiKey,
  SharedUiMessages,
} from "./lib/inject";
export type { ShortcutGuard } from "./lib/shortcuts";

export interface SharedUiOptions {
  /** 브라우저 저장소 키의 접두사. 제품마다 상이해야 한다. */
  storagePrefix: string;
  /** 인쇄 머리에 기재하는 제품의 명칭. */
  appName: string;
  messages: SharedUiMessages;
  source: GridSource;
  errors: SharedUiErrors;
  /**
   * webview 의 단축키 차단. **생략하면 아무것도 차단하지 않는다.**
   * 창에 탑재하는 제품만 지정한다.
   */
  shortcuts?: ShortcutGuard;
}

/**
 * 주입하고, 저장소에 보관된 선택(테마·천 단위 구분)을 반영한다.
 *
 * **화면을 장착하기 이전에 호출한다.** 장착 이후에 호출하면 최초 묘사가 기본
 * 테마로 점멸하고, 주입 이전에 묘사된 부품은 정지한다.
 */
export function installSharedUi(o: SharedUiOptions): void {
  provideStoragePrefix(o.storagePrefix);
  provideAppName(o.appName);
  provideMessages(o.messages);
  provideGridSource(o.source);
  provideErrors(o.errors);
  installTheme();
  installGrouping();
  if (o.shortcuts) installShortcutGuard(o.shortcuts);
}
