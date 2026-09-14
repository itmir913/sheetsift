//! 봉인된 자료(`.bin`).
//!
//! `build_db` 가 만들고 앱이 연다. **같은 코드를 나눠 쓴다** — 두 곳에 베껴
//! 두면 갈라지고, 갈라진 것은 기동할 때야 드러난다.
//!
//! ## 겹이 둘이고, 막는 것이 서로 다르다
//!
//! | 겹 | 막는 것 | 열쇠 |
//! |---|---|---|
//! | AES-256-GCM 봉인 | **훔쳐보기** — 실행 파일에서 SQLite 를 잘라내는 레시피 | 대칭키. 앱 안에 있다 |
//! | Ed25519 서명 | **바꿔치기** — 가짜 입시 자료 | 개인키는 앱 밖 |
//!
//! 하나가 다른 하나를 갈음하지 못한다. 서명은 숨기지 못하고(공개키가 앱에 있으니
//! 누구나 검증한다), 봉인은 위조를 막지 못한다(대칭키를 캐낸 사람은 태그까지
//! 맞춰 다시 봉한다).
//!
//! ## 파일 형식
//!
//! ```text
//! offset  size  내용
//!      0     8  매직
//!      8     1  container_version — 푸는 방법이 바뀌면 오른다
//!      9     6  data_version (yymmdd, ASCII)
//!     15     3  min_app — 이 자료를 열 수 있는 가장 낮은 앱 판
//!     18    12  AES-GCM 논스 — 파일마다 새로 뽑는다
//!     30     …  AES-256-GCM(직렬화된 SQLite) + 인증 태그 16 B
//!      …    64  Ed25519 서명 — 앞의 **전부**를 덮는다
//! ```
//!
//! **매직에 `GOENAVI` 를 쓰지 않는다.** `.navi` 는 사람이 주고받는 파일이라
//! 매직이 이름표 노릇을 하지만, 이쪽은 실행 파일 안에 숨어 있다. 뜻 있는 글자가
//! 보이면 **거기가 자료의 자리라고 알려 주는 꼴**이다.
//!
//! **판과 `min_app` 은 평문이다.** 씨앗과 갱신본 중 어느 것을 열지 정하려면 판을
//! 알아야 하는데, 그러자고 둘 다 복호화하면 31MB 를 두 번 푼다. 서명이 머리를
//! 덮으므로 평문이어도 위조되지 않는다.
//!
//! **CRC 를 두지 않는다.** `vault.rs` 의 CRC 는 "깨진 파일" 과 "틀린 비밀번호" 를
//! 가르려고 있다. 여기는 비밀번호가 없다 — 태그가 거절하면 빌드 사고이거나
//! 위조이고, 담임에게 달리 안내할 말이 없다.
//!
//! ## 봉하고 나서 서명한다
//!
//! 반대로 하면 서명을 보려고 **먼저 복호화해야 한다.** 그것은 검증 안 된 바이트를
//! 복호화기에 밀어 넣는다는 뜻이다. 지금 차례면 서명이 안 맞는 파일은 **풀어
//! 보기도 전에** 버린다.
//!
//! ## 머리는 AAD 로도 묶는다
//!
//! 서명이 이미 머리를 덮는데도 그렇게 한다. 서명과 봉인이 서로 독립이면
//! [`open`] 이 [`parse`] 없이 불리는 실수를 막을 것이 없기 때문이다. AAD 로
//! 묶으면 **머리가 틀린 채 열리는 길이 구조적으로 없다.** 비용은 0 이다.
//! `vault.rs` 도 같은 모양이다.

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};

use crate::{keys, version::Version, AppError, AppResult, ErrorCode};

/// 뜻 없는 여덟 바이트.
const MAGIC: &[u8; 8] = &[0x9e, 0x4d, 0x1a, 0xc7, 0x60, 0xb3, 0x2f, 0x08];

/// **그릇의 판.** 머리 모양이나 암호 방식이 바뀌면 오른다. 안의 스키마와
/// 다른 것이다 — 그쪽은 표가 바뀌는 것이고, 이쪽은 푸는 방법이 바뀌는 것이다.
pub const CONTAINER_VERSION: u8 = 1;

/// **봉인 형식이 처음 들어간 앱 판.** `minApp` 을 이보다 낮게 적을 수 없다 —
/// 그보다 낮은 앱은 이 그릇을 모르므로, 받아도 열지 못한다.
///
/// 아직 값이 굳지 않았다. 첫 봉인 판이 몇 번으로 나갈지는 릴리스 키를 만드는
/// 날 정해지므로, 그때까지는 **지금 판**과 같게 둔다. 그러면
/// [`tests::the_floor_is_not_above_the_current_version`] 이 참이고, 낮추는
/// 길도 당장은 열리지 않는다.
pub fn sealed_since() -> AppResult<Version> {
    Version::from_config()
}

const MAGIC_LEN: usize = 8;
const DATA_VERSION_LEN: usize = 6;
const NONCE_LEN: usize = 12;
const SIGNATURE_LEN: usize = 64;
/// 매직부터 논스 끝까지. AAD 로 통째로 묶는 범위다.
const HEAD_LEN: usize = MAGIC_LEN + 1 + DATA_VERSION_LEN + 3 + NONCE_LEN;
/// GCM 태그. 알맹이가 비어 있어도 이만큼은 있다.
const TAG_LEN: usize = 16;

/// 매직만 본다. **판정이 아니라 눈짐작이다** — 진짜인지는 [`parse`] 가 본다.
///
/// 시험이 "봉인된 그릇이기는 한가" 를 물을 때 쓴다. 매직은 뜻 없는 바이트라
/// 밖에서 견주려면 여기를 거쳐야 한다.
pub fn starts_with_magic(bytes: &[u8]) -> bool {
    bytes.starts_with(MAGIC)
}

/// 복호화하지 않고 읽어 낸 머리.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Head {
    pub container: u8,
    /// 자료의 판. `yymmdd` 여섯 자리.
    pub data_version: String,
    /// 이 자료를 열 수 있는 가장 낮은 앱 판.
    pub min_app: Version,
}

/// **[`parse`] 를 지난 바이트.** 이 타입이 아니면 [`open`] 을 부를 수 없다.
///
/// 검사를 건너뛴 바이트를 복호화기에 밀어 넣는 것을 **구조로** 막는다. 주석이나
/// 검사로 "먼저 `parse` 를 부르시오" 라고 적어 두는 것과 다르다 — 그런 약속은
/// 지키는 사람이 있는 동안만 지켜진다.
///
/// 만드는 길이 [`parse`] 하나뿐이라, 이것을 손에 쥐었다는 사실이 곧 **매직이
/// 맞고, 서명이 우리 열쇠 가운데 하나로 검증되었고, 그릇 판을 안다**는 뜻이다.
pub struct Verified<'a> {
    bytes: &'a [u8],
    head: Head,
}

/// **바이트를 찍지 않는다.** 31MB 가 로그와 패닉 메시지로 쏟아진다.
/// 머리만 보이면 어느 자료인지 가리기에 넉넉하다.
impl std::fmt::Debug for Verified<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Verified")
            .field("head", &self.head)
            .field("bytes", &format_args!("{} B", self.bytes.len()))
            .finish()
    }
}

impl Verified<'_> {
    pub fn head(&self) -> &Head {
        &self.head
    }

    /// 봉인된 바이트 전체. 씨앗과 갱신본 중 이긴 쪽을 그대로 넘길 때 쓴다.
    pub fn bytes(&self) -> &[u8] {
        self.bytes
    }
}

/// **매직 → 서명 → 그릇 판 → 나머지.** 이 차례가 규약이다.
///
/// 매직을 먼저 보는 것은 "우리 파일이 아니다" 를 서명 실패와 가르기 위해서다.
/// 서명을 그다음에 보는 것은, 그 뒤로 읽는 값이 **전부 서명이 덮은 것**이어야
/// 하기 때문이다. 그릇 판을 셋째로 보면, 나중에 판을 올려 머리 모양을 바꿔도
/// 옛 앱이 **서명을 확인한 뒤에** "모르는 그릇" 으로 안전하게 선다.
///
/// **복호화하지 않는다.** 판 견주기는 여기까지만 필요하다.
///
/// 공개키를 **인자로 받는다.** 앱과 `build_db` 는 [`keys::verify_keys`] 를
/// 넘기고, 시험은 제 키를 넘긴다. 상수를 안에서 읽게 하면 시험이 제 키로 만든
/// 자료를 못 보고, 그렇다고 시험만 갈래를 따로 두면 `cargo test --release` 에서
/// 그 시험이 통째로 사라진다.
///
/// **여럿을 받아 하나라도 맞으면 통과다.** 까닭은 [`keys::verify_keys`] 에 있다 —
/// 릴리스 자료를 만드는 검사 잡이 debug 로 돌기 때문이다.
pub fn parse<'a>(bytes: &'a [u8], verify: &[[u8; 32]]) -> AppResult<Verified<'a>> {
    if bytes.len() < HEAD_LEN + TAG_LEN + SIGNATURE_LEN {
        return Err(AppError::new(ErrorCode::DataSealInvalid)
            .detail(format!("too short: {} bytes", bytes.len())));
    }
    if !bytes.starts_with(MAGIC) {
        return Err(AppError::new(ErrorCode::DataSealInvalid).detail("magic does not match"));
    }

    let (signed, signature) = bytes.split_at(bytes.len() - SIGNATURE_LEN);
    let signature = Signature::from_slice(signature).map_err(|e| {
        AppError::new(ErrorCode::DataSignatureInvalid).detail(format!("bad signature: {e}"))
    })?;
    // **읽지 못한 열쇠를 그냥 넘어가지 않는다.** 목록에 든 것이 공개키가 아니면
    // 배선이 끊긴 것이지 서명이 틀린 것이 아니다. 조용히 건너뛰면 "아무 열쇠도
    // 안 맞는다" 로 둔갑해, 오타 하나를 위조로 읽게 된다.
    let mut matched = false;
    for raw in verify {
        let key = VerifyingKey::from_bytes(raw).map_err(|e| {
            AppError::new(ErrorCode::DataSignatureInvalid)
                .detail(format!("a configured verifying key is not a valid point: {e}"))
        })?;
        if key.verify(signed, &signature).is_ok() {
            matched = true;
            break;
        }
    }
    if !matched {
        return Err(AppError::new(ErrorCode::DataSignatureInvalid)
            .detail(format!("no configured key verifies this signature ({} tried)", verify.len())));
    }

    // 여기서부터 읽는 값은 전부 서명이 덮은 것이다.
    let container = bytes[MAGIC_LEN];
    if container != CONTAINER_VERSION {
        return Err(AppError::new(ErrorCode::DataContainerUnknown)
            .with("container", container)
            .with("expected", CONTAINER_VERSION));
    }

    let at = MAGIC_LEN + 1;
    let data_version = std::str::from_utf8(&bytes[at..at + DATA_VERSION_LEN])
        .ok()
        .filter(|s| s.bytes().all(|b| b.is_ascii_digit()))
        .ok_or_else(|| {
            AppError::new(ErrorCode::DataSealInvalid).detail("data version is not six digits")
        })?
        .to_string();

    let at = at + DATA_VERSION_LEN;
    let min_app = Version::from_bytes([bytes[at], bytes[at + 1], bytes[at + 2]]);

    Ok(Verified { bytes, head: Head { container, data_version, min_app } })
}

/// 봉인을 푼다. **[`Verified`] 만 받는다** — 그 타입을 만드는 길이 [`parse`]
/// 하나뿐이므로, 검사를 건너뛰고 여기 올 수 없다.
///
/// 머리가 AAD 로도 묶여 있어, 머리를 한 바이트라도 고친 것은 여기서 또 거절된다.
/// 겹쳐 두는 까닭은 서명과 봉인이 서로 독립이기 때문이다.
pub fn open(verified: &Verified<'_>) -> AppResult<Vec<u8>> {
    let bytes = verified.bytes;
    let head = &bytes[..HEAD_LEN];
    let nonce: [u8; NONCE_LEN] = bytes[HEAD_LEN - NONCE_LEN..HEAD_LEN]
        .try_into()
        .expect("길이는 위에서 확인했다");
    let body = &bytes[HEAD_LEN..bytes.len() - SIGNATURE_LEN];

    let key = keys::data_key();
    navi_aead::open(&key, &nonce, head, body)
        .ok_or_else(|| AppError::new(ErrorCode::DataSealInvalid).detail("authentication tag mismatch"))
}

/// 봉하고 서명한다. **`build_db` 전용.**
pub fn seal(
    image: &[u8],
    data_version: &str,
    min_app: Version,
    signing: &SigningKey,
) -> AppResult<Vec<u8>> {
    let mut nonce = [0u8; NONCE_LEN];
    rand::RngCore::fill_bytes(&mut rand::rng(), &mut nonce);
    seal_with(image, data_version, min_app, nonce, signing)
}

/// 논스를 밖에서 넣는다. **알려진 답 테스트만 이 길로 부른다** — 논스가
/// 고정되어야 결과가 고정된다.
///
/// 실제로 봉할 때는 [`seal`] 이 매번 새 논스를 뽑는다. 같은 열쇠에 논스를
/// 되쓰면 GCM 은 **숨기기와 위조 막기가 동시에** 무너진다.
fn seal_with(
    image: &[u8],
    data_version: &str,
    min_app: Version,
    nonce: [u8; NONCE_LEN],
    signing: &SigningKey,
) -> AppResult<Vec<u8>> {
    if data_version.len() != DATA_VERSION_LEN
        || !data_version.bytes().all(|b| b.is_ascii_digit())
    {
        return Err(AppError::new(ErrorCode::DataSealInvalid)
            .detail(format!("data version must be six digits: '{data_version}'")));
    }

    let mut out = Vec::with_capacity(HEAD_LEN + image.len() + TAG_LEN + SIGNATURE_LEN);
    out.extend_from_slice(MAGIC);
    out.push(CONTAINER_VERSION);
    out.extend_from_slice(data_version.as_bytes());
    out.extend_from_slice(&min_app.to_bytes());
    out.extend_from_slice(&nonce);
    debug_assert_eq!(out.len(), HEAD_LEN);

    let key = keys::data_key();
    let body = navi_aead::seal(&key, &nonce, &out[..HEAD_LEN], image)
        .ok_or_else(|| AppError::new(ErrorCode::DataSealInvalid).detail("could not seal"))?;
    out.extend_from_slice(&body);

    let signature = signing.sign(&out);
    out.extend_from_slice(&signature.to_bytes());
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

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
        seal_with(IMAGE, "260801", Version::parse("0.5.3").unwrap(), [3u8; 12], &test_key())
            .unwrap()
    }

    /// **머리 배치·AAD 범위·서명 범위가 한 값에 묶인다.**
    /// 셋 중 무엇이 바뀌어도 이 시험이 선다.
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

    /// 어디가 바뀌었는지 말해 주는 보조. 위 시험이 서면 여기를 본다.
    #[test]
    fn the_header_layout_is_pinned() {
        let bytes = sealed();
        assert_eq!(&bytes[..8], MAGIC, "매직");
        assert_eq!(bytes[8], 1, "그릇 판");
        assert_eq!(&bytes[9..15], b"260801", "자료 판");
        assert_eq!(&bytes[15..18], &[0, 5, 3], "min_app");
        assert_eq!(&bytes[18..30], &[3u8; 12], "논스");
        assert_eq!(bytes.len(), 30 + IMAGE.len() + 16 + 64, "전체 길이");
    }

    #[test]
    fn what_was_sealed_comes_back() {
        let bytes = sealed();
        let verified = parse(&bytes, &test_verify()).unwrap();
        assert_eq!(verified.head().data_version, "260801");
        assert_eq!(verified.head().min_app, Version::parse("0.5.3").unwrap());
        assert_eq!(open(&verified).unwrap(), IMAGE);
    }

    /// **평문이 파일에 남지 않는다.** 이 작업의 목적 그 자체다.
    #[test]
    fn the_plaintext_is_not_in_the_sealed_bytes() {
        let bytes = sealed();
        assert!(
            !bytes.windows(IMAGE.len()).any(|w| w == IMAGE),
            "봉한 파일 안에 평문이 그대로 있습니다"
        );
        assert!(
            !bytes.windows(15).any(|w| w == b"SQLite format 3"),
            "봉한 파일에서 SQLite 매직이 보입니다"
        );
    }

    /// 다른 열쇠로 서명한 것은 우리 앱이 안 믿는다.
    /// **개발 서명본을 릴리스 앱에 넣은 실수가 이 오류로 드러난다.**
    #[test]
    fn a_signature_from_another_key_is_rejected() {
        let other = SigningKey::from_bytes(&[9u8; 32]);
        let bytes =
            seal_with(IMAGE, "260801", Version::parse("0.5.3").unwrap(), [3u8; 12], &other)
                .unwrap();
        assert_eq!(parse(&bytes, &test_verify()).unwrap_err().code, ErrorCode::DataSignatureInvalid);
    }

    /// 서명은 **끝 64바이트를 뺀 전부**를 덮는다. 어느 한 바이트를 고쳐도 선다.
    #[test]
    fn tampering_anywhere_is_detected() {
        let good = sealed();
        for at in [8, 9, 15, 18, 31, good.len() - 65] {
            let mut bad = good.clone();
            bad[at] ^= 1;
            assert_eq!(
                parse(&bad, &test_verify()).unwrap_err().code,
                ErrorCode::DataSignatureInvalid,
                "{at}번째 바이트를 고쳤는데 지나갔습니다"
            );
        }
    }

    /// 매직이 다른 것은 **서명 실패가 아니라** 우리 파일이 아닌 것이다.
    #[test]
    fn something_that_is_not_ours_says_so_before_the_signature() {
        let bytes = vec![0u8; HEAD_LEN + TAG_LEN + SIGNATURE_LEN];
        assert_eq!(parse(&bytes, &test_verify()).unwrap_err().code, ErrorCode::DataSealInvalid);
    }

    /// 모르는 그릇 판은 **서명을 확인한 뒤에** 선다. 그래야 나중에 판을 올려도
    /// 옛 앱이 안전하게 멈춘다.
    #[test]
    fn an_unknown_container_is_rejected_after_the_signature() {
        let mut bytes = sealed();
        bytes[8] = CONTAINER_VERSION + 1;
        // 고친 채로 다시 서명한다 — 진짜 우리 것인데 그릇만 새 판인 상황이다.
        let signed_len = bytes.len() - SIGNATURE_LEN;
        let signature = test_key().sign(&bytes[..signed_len]);
        bytes[signed_len..].copy_from_slice(&signature.to_bytes());
        assert_eq!(parse(&bytes, &test_verify()).unwrap_err().code, ErrorCode::DataContainerUnknown);
    }

    /// 머리가 AAD 로 묶여 있다. 서명을 다시 맞춰 와도 봉인이 거절한다.
    #[test]
    fn a_rewritten_header_does_not_open_even_with_a_fresh_signature() {
        let mut bytes = sealed();
        bytes[9] = b'9'; // 자료 판을 260801 → 960801 로
        let signed_len = bytes.len() - SIGNATURE_LEN;
        let signature = test_key().sign(&bytes[..signed_len]);
        bytes[signed_len..].copy_from_slice(&signature.to_bytes());
        let verified = parse(&bytes, &test_verify()).expect("서명은 맞춰 왔으므로 여기는 지나간다");
        assert_eq!(open(&verified).unwrap_err().code, ErrorCode::DataSealInvalid);
    }

    /// 논스가 매번 달라야 한다. 되쓰면 GCM 이 무너진다.
    #[test]
    fn every_sealing_uses_a_fresh_nonce() {
        let a = seal(IMAGE, "260801", Version::parse("0.5.3").unwrap(), &test_key()).unwrap();
        let b = seal(IMAGE, "260801", Version::parse("0.5.3").unwrap(), &test_key()).unwrap();
        assert_ne!(a[18..30], b[18..30], "논스가 같습니다");
        assert_ne!(a, b);
    }

    /// 바닥이 지금 판보다 높으면 **낮출 수 없는데 낮추라고 서는** 상태가 된다.
    #[test]
    fn the_floor_is_not_above_the_current_version() {
        assert!(sealed_since().unwrap() <= Version::from_config().unwrap());
    }

    #[test]
    fn a_data_version_that_is_not_six_digits_is_refused() {
        let v = Version::parse("0.5.3").unwrap();
        for bad in ["26081", "2608011", "26080a", ""] {
            assert!(seal(IMAGE, bad, v, &test_key()).is_err(), "'{bad}' 를 받아들였습니다");
        }
    }
}
