//! **데스크톱 창에 부착하는 껍질.**
//!
//! 알맹이는 [`sheetsift_shell`] 에 소재한다. 여기 소재하는 것은 둘뿐이다 —
//! 창이 호출하는 [`명령`](desk) 과, 이 저장소에서 유일하게 **파일을 판독하고
//! 기록하는** [`위치`](file).
//!
//! ## `tauri` 를 여기서 수령하지 않는다
//!
//! 명령이 수행하는 작업을 통상 Rust 로 배치하면 `cargo test --workspace` 가
//! 그것을 전부 판정한다. `#[tauri::command]` 는 이 함수들에 부착하는 한 겹이고
//! `crates/tauri` 에 소재한다 — 거기 논리를 배치하지 않는 한 창이 실재해야
//! 판정 가능한 코드는 증가하지 않는다. 브라우저 껍질을 박막으로 유지하는 것과
//! 동일한 사유다.
//!
//! 그래서 **여기 유입되는 것을 경계한다.** 파일 선택 대화상자, 창의 제목,
//! 메뉴는 그 겹의 것이고, 그것이 호출하는 작업은 여기 소재해야 한다.

pub mod desk;
pub mod error;
pub mod file;

pub use desk::Desk;
/// 조회 결과의 구조. 부착하는 겹이 그것을 반환하므로 여기서 재노출한다 — 바탕을
/// 직접 수령하게 하면 판이 두 곳에 기재된다.
pub use sheetsift_core::grid;
pub use error::{ErrorCode, Fault, Result, ShellError};
