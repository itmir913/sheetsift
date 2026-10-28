//! 표를 판독하고, 봉인하고, 조회하는 바탕.
//!
//! ## 여기 부재하는 것
//!
//! - **열쇠.** 봉인 대칭키·서명 공개키·매직은 부르는 앱이 주입한다. 이
//!   크레이트는 공개 저장소에 소재하므로, 여기 기재된 열쇠는 누구나 인지하는
//!   열쇠다.
//! - **사람이 읽을 문구.** 오류는 코드와 치환 값만 반환한다([`Error`]). 문구는
//!   앱이 부여한다.
//! - **tauri 와 파일 시스템.** 바이트를 수용하고 바이트를 반환한다.

mod error;
pub mod grid;
pub mod seal;
pub mod sheet;
pub mod vault;

/* 아래 층들은 **활성화해야 포함된다.** 사유는 상자 파일에 기재했다 — 요약하면,
   오류 코드가 단일 enum 이라 사용하지 않는 층의 코드까지 앱이 문구를 작성해야
   한다.

   **이 층들로 향하는 경로는 외부에서만 진입한다.** 활성화하지 않은 조합이
   컴파일되는지를 이 저장소의 `cargo test` 가 확인할 수 없으므로, 대신 내부
   코드가 이 층들을 호출하지 않는다는 것을 시험이 글자로 대조한다
   (`tests/features.rs`). */

#[cfg(feature = "infer")]
pub mod infer;
#[cfg(feature = "keyring")]
pub mod keyring;
#[cfg(feature = "migrate")]
pub mod migrate;
#[cfg(feature = "sift")]
pub mod sift;

pub use error::{Error, ErrorCode, Result};

/// **이 바탕이 사용하는 SQLite 연결.**
///
/// 공개 API 가 이 타입을 수수하므로(`grid::run` · `sheet::build` …) 여기서 함께
/// 노출한다. 쓰는 곳이 `rusqlite` 를 별도로 기재하면 판을 일치시켜야 하고,
/// 상이하면 **동일 명칭의 상이한 타입**이 되어 원인 규명이 곤란한 오류가
/// 발생한다.
pub use rusqlite::Connection;
