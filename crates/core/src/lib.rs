//! 표를 읽고, 봉하고, 조회하는 바탕.
//!
//! ## 여기 없는 것
//!
//! - **열쇠.** 봉인 대칭키·서명 공개키·매직은 부르는 앱이 넣는다. 이 크레이트는
//!   공개 저장소에 있으므로, 여기 적힌 열쇠는 누구나 아는 열쇠다.
//! - **사람이 읽을 문구.** 오류는 코드와 치환 값만 돌려준다([`Error`]). 문구는
//!   앱이 붙인다.
//! - **tauri 와 파일 시스템.** 바이트를 받고 바이트를 돌려준다.

mod error;
pub mod grid;
pub mod seal;
pub mod sheet;
pub mod vault;

/* 아래 둘은 **켜야 들어온다.** 까닭은 상자 파일에 적어 두었다 — 간추리면,
   오류 코드가 한 enum 이라 쓰지 않는 층의 코드까지 앱이 문구를 써야 한다.

   **이 둘을 향한 길은 바깥에서만 들어온다.** 켜지 않은 조합이 컴파일되는지를
   이 저장소의 `cargo test` 가 볼 수 없으므로, 그 대신 안쪽 코드가 이 둘을
   부르지 않는다는 것을 시험이 글자로 본다(`tests/features.rs`). */

#[cfg(feature = "infer")]
pub mod infer;
#[cfg(feature = "sift")]
pub mod sift;

pub use error::{Error, ErrorCode, Result};

/// **이 바탕이 쓰는 SQLite 연결.**
///
/// 공개 API 가 이 타입을 주고받으므로(`grid::run` · `sheet::build` …) 여기서
/// 함께 내보낸다. 쓰는 쪽이 `rusqlite` 를 따로 적으면 판을 맞혀야 하고,
/// 어긋나면 **같은 이름의 다른 타입**이 되어 까닭을 알기 어려운 오류가 난다.
pub use rusqlite::Connection;
