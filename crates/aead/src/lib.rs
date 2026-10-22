//! AES-256-GCM 한 겹. **여기 소재하는 사유는 속도뿐이다.**
//!
//! ## 상자를 분리한 사유
//!
//! `[profile.dev.package.aes-gcm] opt-level = 3` 은 **그 상자 내부에서
//! 컴파일되는 코드**만 최적화한다. 그런데 `Aead::encrypt`·`decrypt` 는
//! 제네릭이라 **부르는 곳에서 구체화**되고, 부르는 곳이 우리 상자면 우리
//! 상자의 `opt-level` 로 컴파일된다. AES 블록 함수까지 그 내부로 인라인되므로
//! 자료 전체가 최적화 없이 동작한다. 기동이 현저히 지연된다.
//!
//! `argon2` 에 유효했던 처방이 `aes-gcm` 에 무효한 사유가 그것이다. Argon2·
//! SHA-512 의 중량 부분은 자체 상자 내부에 소재하고, AES 는 트레이트를 경유해
//! 도달한다. 분리 방법과 효과는 `docs/build-speed.md` 에 수록되어 있다.
//!
//! 여기 소재하는 함수 둘은 **제네릭이 아니다.** 그래서 구체화가 이 상자
//! 내부에서 발생하고, 이 상자에만 `opt-level = 3` 을 부여하면 된다. 그 한 행이
//! 워크스페이스 최상위의 `Cargo.toml` 에 소재한다.
//!
//! ## 여기 부재하는 것
//!
//! 그릇의 구조, 머리 배치, 서명, 열쇠 생성은 전부 부르는 곳
//! (`sheetsift-core`)에 소재한다. **속도를 사유로 이전한 것이지 설계를 이전한
//! 것이 아니다.** 여기는 열쇠와 논스와 바이트를 수용해 봉인하고 해제하는
//! 작업만 수행한다.
//!
//! 시험도 배치하지 않는다. 정오는 `sheetsift-core` 의 `seal` 시험이 판정한다 —
//! 고정 입력으로 봉인한 결과의 해시가 거기 고정되어 있고, 이 함수가 변경되면
//! 그것이 중단된다.
//!
//! ## 오류를 생성하지 않는다
//!
//! `Option` 을 반환한다. 여기서 실패하는 경로는 하나뿐이고(태그가 불일치한다),
//! 그것을 무엇으로 명명할지는 부르는 곳이 인지한다. 오류 타입을 여기 배치하면
//! `ErrorCode` 가 수반되어야 하고, 그러면 세 곳 일치 검사까지 이 상자에
//! 결부된다.

use aes_gcm::{
    aead::{Aead, KeyInit, Payload},
    Aes256Gcm, Nonce,
};

/// 봉인한다. 반환하는 것은 암호문 + 인증 태그 16바이트다.
///
/// `aad` 는 **암호문에 수록되지 않으나 태그가 포괄한다.** 부르는 곳은 여기에
/// 머리를 주입해, 머리가 상이한 채로 개방되는 경로를 제거한다.
pub fn seal(key: &[u8; 32], nonce: &[u8; 12], aad: &[u8], plain: &[u8]) -> Option<Vec<u8>> {
    let cipher = Aes256Gcm::new_from_slice(key).ok()?;
    cipher.encrypt(&Nonce::from(*nonce), Payload { msg: plain, aad }).ok()
}

/// 해제한다. 태그가 불일치하면 `None` 이다 — **단 한 바이트만 상이해도** 그렇다.
pub fn open(key: &[u8; 32], nonce: &[u8; 12], aad: &[u8], body: &[u8]) -> Option<Vec<u8>> {
    let cipher = Aes256Gcm::new_from_slice(key).ok()?;
    cipher.decrypt(&Nonce::from(*nonce), Payload { msg: body, aad }).ok()
}
