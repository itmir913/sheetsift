//! 자료를 개방하는 열쇠의 **그릇.** 재료는 앱이 주입한다.
//!
//! 봉인키 재료와 신뢰하는 공개키는 **앱마다 상이하다.** 두 앱이 동일한 코드를
//! 사용하더라도 일방의 자료가 타방에서 개방되어서는 안 되고, 일방의 개인키가
//! 유출되어도 타방까지 위조되어서는 안 된다. 그래서 이 파일은 값을 하나도 보유하지
//! 않고, 앱이 기동할 때 [`Keys::new`] 로 구성해 전달한다.
//!
//! **개발 열쇠를 선별하는 분기도 여기 부재한다.** `cfg(debug_assertions)` 는 그것을
//! 기재한 상자의 빌드 설정을 추종하므로, 그 분기는 열쇠를 보유한 앱에 배치한다.
//!
//! 도출 방식(재료를 Blake2b-512 로 해시해 선두 32바이트)은 **변경하지 않는다.** 이미
//! 배포된 앱이 그 방식으로 도출한 열쇠로 자료를 개방한다. 고정값 시험이 수호한다.

use blake2::{Blake2b512, Digest};
use zeroize::Zeroizing;

/// 한 앱의 열쇠. 봉인키 재료 하나와 신뢰하는 공개키 목록.
#[derive(Clone)]
pub struct Keys {
    salt: &'static [u8],
    verify: Vec<[u8; 32]>,
}

impl Keys {
    /// **신뢰하는 공개키가 하나도 부재하면 정지한다.** 그대로 두면 모든 자료가 서명
    /// 오류로 거절되는데, 그것은 자료 측의 사고처럼 보인다. 배선이 단절된 것은 여기서
    /// 진술한다.
    pub fn new(salt: &'static [u8], verify: Vec<[u8; 32]>) -> Self {
        assert!(!salt.is_empty(), "the sealing key material is empty");
        assert!(!verify.is_empty(), "no trusted public key was given");
        Self { salt, verify }
    }

    /// 자료를 해제하는 대칭키.
    ///
    /// 완성된 값은 어디에도 보관하지 않는다. 호출할 때마다 도출하고, 사용한 뒤에는
    /// [`Zeroizing`] 이 소거한다.
    pub fn data_key(&self) -> Zeroizing<[u8; 32]> {
        let mut h = Blake2b512::new();
        h.update(self.salt);
        let out = h.finalize();
        let mut key = Zeroizing::new([0u8; 32]);
        key.copy_from_slice(&out[..32]);
        key
    }

    /// 자료와 갱신 색인의 서명을 판정하는 공개키 **목록.** 하나라도 일치하면 우리 것이다.
    pub fn verify(&self) -> &[[u8; 32]] {
        &self.verify
    }
}

/// 재료와 공개키가 로그에 출력되지 않게 한다.
impl std::fmt::Debug for Keys {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Keys").field("verify", &self.verify.len()).finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **누구나 인지하는 시험용 재료다.** 실제 앱의 재료가 아니다.
    const PUBLIC_TEST_SALT: &[u8] = b"public test salt - everyone knows this";

    /// 시험용 공개키 자리. 값에 의미가 없다.
    const PUBLIC_TEST_VERIFY: [u8; 32] = [7u8; 32];

    /// **도출 방식이 변경되지 않았는가.** 변경되면 이미 배포된 자료가 개방되지 않는다.
    /// 기대값은 이 코드가 아니라 타 구현(Python `hashlib.blake2b`)으로 산출했다.
    #[test]
    fn the_key_recipe_is_pinned() {
        let keys = Keys::new(PUBLIC_TEST_SALT, vec![PUBLIC_TEST_VERIFY]);
        let hex: String = keys.data_key().iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(hex, "61b1ecbd90c34f1978668b172ad74d0354e2cda8f8d8cd1eec76649d5d403c59");
    }

    #[test]
    fn debug_shows_neither_the_material_nor_the_keys() {
        let keys = Keys::new(PUBLIC_TEST_SALT, vec![PUBLIC_TEST_VERIFY]);
        let shown = format!("{keys:?}");
        assert!(!shown.contains("public test salt"), "{shown}");
        assert!(!shown.contains('7'), "공개키의 바이트가 출력되었다: {shown}");
        assert_eq!(keys.verify(), &[PUBLIC_TEST_VERIFY]);
    }

    #[test]
    #[should_panic(expected = "no trusted public key")]
    fn no_trusted_key_stops_at_once() {
        Keys::new(PUBLIC_TEST_SALT, Vec::new());
    }

    #[test]
    #[should_panic(expected = "key material is empty")]
    fn empty_material_stops_at_once() {
        Keys::new(b"", vec![PUBLIC_TEST_VERIFY]);
    }
}
