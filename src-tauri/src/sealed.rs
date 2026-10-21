//! 봉인된 자료(`.bin`).
//!
//! `build_db` 가 만들고 앱이 연다. **같은 코드를 나눠 쓴다** — 두 곳에 베껴
//! 두면 갈라지고, 갈라진 것은 기동할 때야 드러난다.
//!
//! ## 그릇은 `sheetsift-core` 의 것이다
//!
//! 머리 배치·봉하는 차례·서명 범위는 [`sheetsift_core::seal`] 에 있다. 여기서
//! 더하는 것은 **이 앱의 것 셋**이다 — 매직, 대칭키([`keys::data_key`]), 그리고
//! `min_app` 을 앱 판([`Version`])으로 읽는 법. 그릇 코드는 공개 저장소에 있고,
//! 열쇠는 여기 남는다.
//!
//! 형식이 한 비트라도 달라지면 이미 나간 앱이 새 자료를 못 연다. 그것은 아래
//! [`sealing_is_pinned`](tests) 가 **이 앱의 열쇠로** 못 박는다. core 의 시험은
//! 시험용 열쇠로 같은 것을 본다.
//!
//! ## 매직에 `GOENAVI` 를 쓰지 않는다
//!
//! `.navi` 는 사람이 주고받는 파일이라 매직이 이름표 노릇을 하지만, 이쪽은 실행
//! 파일 안에 숨어 있다. 뜻 있는 글자가 보이면 **거기가 자료의 자리라고 알려 주는
//! 꼴**이다.

use ed25519_dalek::SigningKey;
use sheetsift_core::seal;

use crate::{keys, version::Version, AppResult};

/// 뜻 없는 여덟 바이트.
const MAGIC: &[u8; seal::MAGIC_LEN] = &[0x9e, 0x4d, 0x1a, 0xc7, 0x60, 0xb3, 0x2f, 0x08];

pub use seal::CONTAINER_VERSION;

/// **봉인 형식이 처음 들어간 앱 판.** `minApp` 을 이보다 낮게 적을 수 없다 —
/// 그보다 낮은 앱은 이 그릇을 모르므로, 받아도 열지 못한다.
///
/// **값이 굳었다.** `0.6.0` 이 봉인된 자료를 싣고 나가는 첫 판이다. `0.5.3` 까지는
/// 자료가 평문 SQLite 로 실행 파일에 들어 있었고, 갱신이라는 것 자체가 없었다.
///
/// "지금 판" 을 돌려주면 안 된다. 그러면 `0.7.0` 이 나오는 날 바닥이 따라 올라가,
/// **그 아래 앱을 쓰는 선생님이 까닭 없이 잘린다** — 자료 구조가 한 글자도 안
/// 바뀌었는데도 그렇다. 자료만 갈아 끼우는 길이 사실상 죽는다.
///
/// 자료가 실제로 요구하는 판은 `build_db` 의 `MIN_APP_DEFAULT` 가 정하고, 그것도
/// 같은 까닭으로 박제되어 있다. 여기는 그보다 **아래로는 못 내려가는 바닥**이다.
pub fn sealed_since() -> AppResult<Version> {
    Version::parse("0.6.0")
}

/// 매직만 본다. **판정이 아니라 눈짐작이다** — 진짜인지는 [`parse`] 가 본다.
///
/// 시험이 "봉인된 그릇이기는 한가" 를 물을 때 쓴다. 매직은 뜻 없는 바이트라
/// 밖에서 견주려면 여기를 거쳐야 한다.
pub fn starts_with_magic(bytes: &[u8]) -> bool {
    seal::starts_with_magic(bytes, MAGIC)
}

/// 복호화하지 않고 읽어 낸 머리. `min_app` 을 앱 판으로 읽어 둔 것이다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Head {
    pub container: u8,
    /// 자료의 판. `yymmdd` 여섯 자리.
    pub data_version: String,
    /// 이 자료를 열 수 있는 가장 낮은 앱 판.
    pub min_app: Version,
}

/// **[`parse`] 를 지난 바이트.** 이 타입이 아니면 [`open`] 을 부를 수 없다.
/// 만드는 길이 [`parse`] 하나뿐인 것은 core 의 [`seal::Verified`] 와 같다.
#[derive(Debug)]
pub struct Verified<'a> {
    inner: seal::Verified<'a>,
    head: Head,
}

impl Verified<'_> {
    pub fn head(&self) -> &Head {
        &self.head
    }

    /// 봉인된 바이트 전체. 씨앗과 갱신본 중 이긴 쪽을 그대로 넘길 때 쓴다.
    pub fn bytes(&self) -> &[u8] {
        self.inner.bytes()
    }
}

/// **매직 → 서명 → 그릇 판 → 나머지.** 차례는 [`seal::parse`] 가 지킨다.
///
/// 공개키를 **인자로 받는다.** 앱과 `build_db` 는 [`keys::verify_keys`] 를
/// 넘기고, 시험은 제 키를 넘긴다. 상수를 안에서 읽게 하면 시험이 제 키로 만든
/// 자료를 못 보고, 그렇다고 시험만 갈래를 따로 두면 `cargo test --release` 에서
/// 그 시험이 통째로 사라진다.
///
/// **여럿을 받아 하나라도 맞으면 통과다.** 까닭은 [`keys::verify_keys`] 에 있다 —
/// 릴리스 자료를 만드는 검사 잡이 debug 로 돌기 때문이다.
pub fn parse<'a>(bytes: &'a [u8], verify: &[[u8; 32]]) -> AppResult<Verified<'a>> {
    let inner = seal::parse(bytes, MAGIC, verify)?;
    let raw = inner.head();
    let head = Head {
        container: raw.container,
        data_version: raw.data_version.clone(),
        min_app: Version::from_bytes(raw.min_app),
    };
    Ok(Verified { inner, head })
}

/// 봉인을 푼다. **[`Verified`] 만 받는다.**
pub fn open(verified: &Verified<'_>) -> AppResult<Vec<u8>> {
    let key = keys::data_key();
    Ok(seal::open(&verified.inner, &key)?)
}

/// 봉하고 서명한다. **`build_db` 전용.**
pub fn seal(
    image: &[u8],
    data_version: &str,
    min_app: Version,
    signing: &SigningKey,
) -> AppResult<Vec<u8>> {
    let key = keys::data_key();
    Ok(seal::seal(image, MAGIC, data_version, min_app.to_bytes(), &key, signing)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ErrorCode;

    /// 시험 전용 키쌍. **`#[cfg(debug_assertions)]` 로 받지 않는다** —
    /// 그러면 `cargo test --release` 에서 이 시험들이 통째로 사라진다.
    fn test_key() -> SigningKey {
        SigningKey::from_bytes(&[7u8; 32])
    }

    fn test_verify() -> Vec<[u8; 32]> {
        vec![test_key().verifying_key().to_bytes()]
    }

    /// 시험용 자료. 진짜 SQLite 일 까닭이 없다 — 이 층은 바이트만 다룬다.
    const IMAGE: &[u8] = b"SQLite format 3\0-- pretend this is a database --";

    fn sealed() -> Vec<u8> {
        seal::seal_with_nonce(
            IMAGE,
            MAGIC,
            "260801",
            Version::parse("0.5.3").unwrap().to_bytes(),
            [3u8; 12],
            &keys::data_key(),
            &test_key(),
        )
        .unwrap()
    }

    /// **이 앱의 매직과 대칭키로** 봉한 결과를 못 박는다. 그릇 코드가 core 로
    /// 옮겨 가도, 열쇠를 넣는 길이 바뀌어도, 이미 나간 앱이 여는 바이트는
    /// 그대로여야 한다.
    #[test]
    fn sealing_is_pinned() {
        use blake2::Digest;
        let hex: String = blake2::Blake2b512::digest(sealed())
            .iter()
            .take(16)
            .map(|b| format!("{b:02x}"))
            .collect();
        assert_eq!(
            hex, "8896edcd0833e9892179172fb29192f8",
            "봉하는 방법이 바뀌었습니다. 이미 나간 앱이 새 자료를 열지 못합니다."
        );
    }

    #[test]
    fn the_magic_is_ours() {
        let bytes = sealed();
        assert_eq!(&bytes[..8], MAGIC, "매직");
        assert!(starts_with_magic(&bytes));
    }

    /// 앱 판으로 읽는 길까지 이어 본다.
    #[test]
    fn what_was_sealed_comes_back() {
        let bytes = sealed();
        let verified = parse(&bytes, &test_verify()).unwrap();
        assert_eq!(verified.head().data_version, "260801");
        assert_eq!(verified.head().min_app, Version::parse("0.5.3").unwrap());
        assert_eq!(open(&verified).unwrap(), IMAGE);
    }

    /// core 의 오류가 이 앱의 코드로 그대로 건너온다. 화면의 문구 사전이 보는
    /// 이름이 같아야 한다.
    #[test]
    fn a_signature_from_another_key_is_rejected() {
        let other = SigningKey::from_bytes(&[9u8; 32]);
        let bytes = seal(IMAGE, "260801", Version::parse("0.5.3").unwrap(), &other).unwrap();
        let e = parse(&bytes, &test_verify()).unwrap_err();
        assert_eq!(e.code, ErrorCode::Core(sheetsift_core::ErrorCode::DataSignatureInvalid));
        assert_eq!(
            serde_json::to_value(e.code).unwrap(),
            serde_json::json!("DATA_SIGNATURE_INVALID")
        );
    }

    /// 바닥이 지금 판보다 높으면 **이 앱조차 제가 만든 자료를 못 여는** 상태가 된다.
    ///
    /// 판을 내리는 일은 없겠지만, 바닥을 손으로 올려 적는 실수는 있을 수 있다.
    #[test]
    fn the_floor_is_not_above_the_current_version() {
        assert!(sealed_since().unwrap() <= Version::from_config().unwrap());
    }

    /// **바닥은 굳은 값이다.** "지금 판" 을 돌려주면 판을 올릴 때마다 따라 올라가,
    /// 자료만 갈아 끼우는 길이 사실상 죽는다.
    #[test]
    fn the_floor_does_not_follow_the_app_version() {
        assert_eq!(sealed_since().unwrap(), Version::parse("0.6.0").unwrap());
    }
}
