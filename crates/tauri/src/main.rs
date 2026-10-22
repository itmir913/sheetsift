// 창을 기동한다. 수행하는 작업은 라이브러리에 소재한다.
//
// `windows_subsystem` 은 **배포본에서만** 부착한다. 개발 중에는 콘솔이 존재해야
// 패닉 원문이 노출된다 — 부재하면 창이 묵시적으로 폐쇄되는 것으로 보인다.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    sheetsift_tauri_lib::run()
}
