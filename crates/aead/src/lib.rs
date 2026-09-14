//! AES-256-GCM 한 겹. **여기 있는 까닭은 속도뿐이다.**
//!
//! ## 왜 상자를 따로 뺐나
//!
//! `[profile.dev.package.aes-gcm] opt-level = 3` 은 **그 상자 안에서 컴파일되는
//! 코드**만 최적화한다. 그런데 `Aead::encrypt`·`decrypt` 는 제네릭이라 **부르는
//! 쪽에서 구체화**되고, 부르는 쪽이 우리 상자면 우리 상자의 `opt-level = 0` 으로
//! 컴파일된다. AES 블록 함수까지 그 안으로 인라인되므로 31MB 전체가 최적화 없이
//! 돈다.
//!
//! 실측으로 **복호화 6,054ms 대 46ms**, 131배였다. 같은 자리에서 서명 확인은
//! 93ms 대 85ms 로 거의 같았는데, Ed25519·SHA-512 의 무거운 부분은 제네릭이
//! 아니라 그 상자 안에 있기 때문이다. `argon2` 에 통했던 처방이 `aes-gcm` 에
//! 안 통한 이유가 그것이다.
//!
//! 여기 있는 함수 둘은 **제네릭이 아니다.** 그래서 구체화가 이 상자 안에서
//! 일어나고, 이 상자에만 `opt-level = 3` 을 주면 된다. 그 한 줄이
//! `src-tauri/Cargo.toml` 에 있다.
//!
//! ## 여기 없는 것
//!
//! 그릇의 모양, 머리 배치, 서명, 열쇠 만들기는 전부 `app_lib` 에 있다.
//! **속도 때문에 옮긴 것이지 설계를 옮긴 것이 아니다.** 여기는 열쇠와 논스와
//! 바이트를 받아 봉하고 푸는 일만 한다.
//!
//! 시험도 두지 않는다. 옳고 그름은 `sealed.rs` 의 알려진 답 시험이 본다 —
//! 고정 입력으로 봉한 결과의 해시가 거기 박혀 있고, 이 함수가 달라지면 그것이
//! 선다.
//!
//! ## 오류를 만들지 않는다
//!
//! `Option` 을 돌려준다. 여기서 실패하는 길은 하나뿐이고(태그가 안 맞는다),
//! 그것을 무엇이라 부를지는 부르는 쪽이 안다. 오류 타입을 여기 두면
//! `ErrorCode` 가 따라와야 하고, 그러면 세 곳 일치 검사까지 이 상자에 얽힌다.

use aes_gcm::{
    aead::{Aead, KeyInit, Payload},
    Aes256Gcm, Nonce,
};

/// 봉한다. 돌려주는 것은 암호문 + 인증 태그 16바이트다.
///
/// `aad` 는 **암호문에 담기지 않지만 태그가 덮는다.** 부르는 쪽은 여기에 머리를
/// 넣어, 머리가 틀린 채 열리는 길을 없앤다.
pub fn seal(key: &[u8; 32], nonce: &[u8; 12], aad: &[u8], plain: &[u8]) -> Option<Vec<u8>> {
    let cipher = Aes256Gcm::new_from_slice(key).ok()?;
    cipher.encrypt(&Nonce::from(*nonce), Payload { msg: plain, aad }).ok()
}

/// 푼다. 태그가 안 맞으면 `None` 이다 — **한 바이트만 달라도** 그렇다.
pub fn open(key: &[u8; 32], nonce: &[u8; 12], aad: &[u8], body: &[u8]) -> Option<Vec<u8>> {
    let cipher = Aes256Gcm::new_from_slice(key).ok()?;
    cipher.decrypt(&Nonce::from(*nonce), Payload { msg: body, aad }).ok()
}
