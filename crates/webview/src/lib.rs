//! 창의 webview 가 독자 보유한 단축키를 비활성화한다 — 윈도 측.
//!
//! ## 화면 측만으로 부족한 사유
//!
//! 화면에서 `preventDefault` 로 차단하는 것은 **크로미움이 그 키를 페이지에
//! 전달할 때만** 성립한다. `Ctrl+S` 는 전달되나 `F12` 와 `Ctrl+Shift+I` 는 전달되지
//! 않는다. 화면 측 코드는 "성립할 수도 있는" 위치이고, 성립하지 않는 것을 진술하지도
//! 않는다. 여기서 비활성화하는 것은 webview 자체의 설정이라 **전달 여부와 무관하다.**
//!
//! 양측을 전부 배치한다. 이쪽은 윈도에서만 작동하기 때문이다 — 맥의 WKWebView 에는
//! 이러한 설정이 부재해 화면 측이 유일한 수단이다.
//!
//! ## 무엇이 비활성화되는가
//!
//! `AreBrowserAcceleratorKeysEnabled` 는 **브라우저의 것만** 비활성화한다. 저장·
//! 인쇄·개발자 도구·소스 보기가 비활성화되고, `Ctrl+C`·`Ctrl+V`·`Ctrl+A` 와 방향키는
//! 그대로 존속한다.
//!
//! 앱의 인쇄는 단추가 `window.print()` 를 호출하는 것이라 그대로 작동한다.
//! 비활성화되는 것은 `Ctrl+P` 로 직접 진입하는 경로뿐이다.
//!
//! ## 적용하지 못하면 정지한다
//!
//! 적용하지 못했을 때 기록만 남기고 진행하지 않는다. 그러면 **차단되었다고 인식하고
//! 반출한 실행 파일이 차단되지 않은 채로 작동한다.** 이 설정은 webview 런타임이
//! 노후하면 부재할 수 있으나(`ICoreWebView2Settings3`), 현재 tauri 가 요구하는
//! 런타임에는 반드시 존재한다. 부재하다면 그것은 미지의 상황이므로 그 시점에 정지한다.

/// 창이 생성된 이후 1회 호출한다. 윈도가 아니면 수행하는 작업이 없다.
#[cfg(not(windows))]
pub fn harden(_window: &tauri::WebviewWindow) -> tauri::Result<()> {
    Ok(())
}

/// 창이 생성된 이후 1회 호출한다. 윈도가 아니면 수행하는 작업이 없다.
#[cfg(windows)]
pub fn harden(window: &tauri::WebviewWindow) -> tauri::Result<()> {
    use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2Settings3;
    use windows::core::Interface;

    window.with_webview(|webview| {
        // COM 인터페이스라 호출하는 위치가 전부 unsafe 다. 대상은 tauri 가 보유한
        // 생존 중인 컨트롤러이고, 이 클로저는 창이 생성된 이후 주 스레드에서만
        // 작동한다.
        //
        // 여기서 실패하는 것은 런타임이 기지의 것과 상이하다는 의미다. 묵시적으로
        // 경과시키면 **차단되지 않은 실행 파일이 반출된다** — 그 시점에 정지한다.
        unsafe {
            let core = webview.controller().CoreWebView2().expect("CoreWebView2 unavailable");
            let settings = core.Settings().expect("CoreWebView2Settings unavailable");
            let settings: ICoreWebView2Settings3 = settings
                .cast()
                .expect("ICoreWebView2Settings3 unavailable; WebView2 runtime too old");
            settings
                .SetAreBrowserAcceleratorKeysEnabled(false)
                .expect("failed to disable browser accelerator keys");
        }
    })
}
