/* ─────────────────────────────────────────────────────────────
   webview 가 독자 보유한 단축키를 비활성화한다. **기본은 비활성이다.**

   앱으로 인식되지만 내부는 브라우저라, 아무 조치도 하지 않으면 브라우저의
   단축키가 그대로 존속한다. Ctrl+S 는 현재 화면을 .html 로 저장하고, 우측 단추의
   `다른 이름으로 저장` 도 동일한 작용을 한다. 자료를 파일로 반출하는 기능을
   배치하지 않기로 한 제품에서는 배치하지 않은 기능이 webview 에 부속된 것이다.

   Ctrl+P 는 반출과 상이한 사유다 — 앱의 인쇄는 단추가 행을 선행 수령한 이후
   `window.print()` 를 호출한다. 브라우저 인쇄로 직접 진입하면 그 행이 공백이고,
   print.css 가 앱 화면을 은폐한 이후라 **공백 지면이 출력된다.** 경로가 하나여야
   한다.

   개발자 도구(F12, Ctrl+Shift+I)는 여기서 차단하지 못한다. 크로미움이 그 키를
   페이지에 전달하지 않아 preventDefault 가 도달하지 않는다. 윈도에서는
   `sheetsift-webview` 상자가 webview 의 설정으로 차단한다.

   **어느 것을 차단할지는 받는 앱이 결정한다.** sheetsift 의 앱은 호출하지 않는다
   (goe-navi 에서 이식, 기본 비활성). 그 제품의 사정을 공용 기본값으로 삼으면
   타 제품에서 우측 단추의 붙여넣기가 묵시적으로 소멸한다.
   ───────────────────────────────────────────────────────────── */

export interface ShortcutGuard {
  /** Ctrl+S(맥은 Cmd+S) — 화면을 .html 로 저장하는 것. */
  save?: boolean;
  /** Ctrl+P — 앱의 인쇄 단추를 경유하지 않는 인쇄. */
  print?: boolean;
  /** Ctrl+U — 소스 보기. */
  source?: boolean;
  /** 우측 단추 메뉴. `다른 이름으로 저장` 이 그 안에 존재한다. */
  contextMenu?: boolean;
}

/**
 * 차단할 키의 **자판 위치**(`code`).
 *
 * `key` 가 아니라 `code` 로 판정한다. `key` 는 자판이 산출하는 **문자**라
 * 입력기(IME)와 자판 배열의 영향을 수용한다. 한글 입력 상태에서 `key` 가 무엇으로
 * 도달하는지 측정하지 않았다. `code` 는 자판의 위치라 한글이든 영문이든 동일한
 * 값이 도달한다.
 */
function blockedCodes(g: ShortcutGuard): Set<string> {
  const codes = new Set<string>();
  if (g.save) codes.add("KeyS");
  if (g.print) codes.add("KeyP");
  if (g.source) codes.add("KeyU");
  return codes;
}

let installed = false;

/**
 * 받는 앱이 기동 시점에 1회 호출한다. 지정하지 않은 항목은 차단하지 않는다.
 *
 * `capture` 로 수령한다 — 화면 어딘가가 keydown 을 선행 수령해 정지시키면 여기까지
 * 도달하지 않는데, 그러면 차단했다고 인식한 키가 묵시적으로 존속한다.
 */
export function installShortcutGuard(g: ShortcutGuard): void {
  if (installed) return;
  installed = true;

  const codes = blockedCodes(g);
  if (codes.size > 0) {
    window.addEventListener(
      "keydown",
      (e: KeyboardEvent) => {
        if (e.altKey) return;
        // 맥에서 Ctrl+S 는 브라우저 단축키가 아니다. 수식키를 구분해 수령한다.
        const modifier = navigator.platform.startsWith("Mac") ? e.metaKey : e.ctrlKey;
        if (!modifier) return;
        if (!codes.has(e.code)) return;
        e.preventDefault();
      },
      true,
    );
  }

  if (g.contextMenu) {
    // **어디서든 차단한다.** 입력란에서만 존치하면 동일한 위치를 조작해도 항목에
    // 따라 메뉴가 출현했다 소멸했다 한다. 상실하는 것은 우측 단추 붙여넣기뿐이고
    // Ctrl+V 는 그대로 존속한다.
    window.addEventListener("contextmenu", (e: MouseEvent) => e.preventDefault(), true);
  }
}
