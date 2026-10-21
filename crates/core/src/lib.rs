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
pub mod sift;
pub mod vault;

pub use error::{Error, ErrorCode, Result};
