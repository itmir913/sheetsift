//! **명령을 창에 부착하는 겹.** 여기 논리를 배치하지 않는다.
//!
//! 수행하는 작업은 전부 [`sheetsift_desk`] 에 소재하고, 그것은 통상 Rust 라
//! `cargo test --workspace` 가 판정한다. 여기 소재하는 것은 `#[tauri::command]`
//! 가 포착할 수 있는 형태로 변환하는 박막 껍질뿐이다 — 브라우저 껍질
//! (`sheetsift-wasm`)과 동일한 구조다.
//!
//! **여기 유입된 만큼이 관문 외부로 이탈한다.** 창이 실재해야 판정 가능한
//! 코드이기 때문이다.

/// 부착하는 문들. **시험이 동일한 목록을 부착해 호출하므로** 공개한다.
pub mod commands;

use std::sync::Mutex;

use sheetsift_desk::Desk;

/// 창을 구성하고 명령을 부착한다.
///
/// 상태는 [`Desk`] 하나다. 명령이 복수의 스레드에서 도달하므로 `Mutex` 로
/// 보호한다 — 조회가 진행 중일 때 신규 양식이 개방되면 어느 표를 조회 중인지가
/// 분기한다.
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(Mutex::new(Desk::new()))
        .invoke_handler(tauri::generate_handler![
            commands::open_workbook,
            commands::open_bytes,
            commands::list_screens,
            commands::query_rows,
            commands::column_values,
            commands::limits,
        ])
        .run(tauri::generate_context!())
        .expect("창을 구성하지 못했습니다");
}
