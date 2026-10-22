//! 봉인하고 서명한 그릇.
//!
//! 발행자가 하나이고 수령자가 복수일 때 사용한다. 비밀번호 봉투(`vault`)와
//! 차단하는 대상이 상이하다.
//!
//! ## 겹이 둘이고, 차단 대상이 상호 상이하다
//!
//! | 겹 | 차단 대상 | 열쇠 |
//! |---|---|---|
//! | AES-256-GCM 봉인 | **절취** — 수령한 바이트에서 알맹이를 추출하는 행위 | 대칭키. 앱 내부에 소재 |
//! | Ed25519 서명 | **위조 교체** — 허위 알맹이 | 개인키는 앱 외부 |
//!
//! 하나가 다른 하나를 대체하지 못한다. 서명은 은닉하지 못하고(공개키가 앱에
//! 소재하므로 누구나 검증한다), 봉인은 위조를 차단하지 못한다(대칭키를 취득한
//! 자는 태그까지 정합해 재봉인한다).
//!
//! **열쇠는 여기 부재한다.** 대칭키·공개키·매직을 부르는 곳이 주입한다. 이
//! 크레이트는 공개되어 있어, 여기 기재한 대칭키는 누구나 인지하는 열쇠가 된다.
//! 그러면 봉인 겹이 아무것도 은닉하지 못한다.
//!
//! ## 그릇 형식
//!
//! ```text
//! offset  size  내용
//!      0     8  매직 — 부르는 곳이 결정한다
//!      8     1  container_version — 해제 방법이 변경되면 상승한다
//!      9     6  data_version (yymmdd, ASCII)
//!     15     3  min_app — 이 알맹이를 개방 가능한 최저 앱 판
//!     18    12  AES-GCM 논스 — 그릇마다 신규 생성한다
//!     30     …  AES-256-GCM(알맹이) + 인증 태그 16 B
//!      …    64  Ed25519 서명 — 전단의 **전부**를 포괄한다
//! ```
//!
//! **판과 `min_app` 은 평문이다.** 복수의 그릇 중 어느 것을 개방할지 결정하려면
//! 판을 인지해야 하는데, 그것을 위해 전부 복호화하면 대형 알맹이를 복수 회
//! 해제한다. 서명이 머리를 포괄하므로 평문이어도 위조되지 않는다.
//!
//! `min_app` 은 3바이트 그대로 취급한다. 판을 판독하고 비교하는 방법은 앱의
//! 계약이다.
//!
//! **CRC 를 배치하지 않는다.** `vault` 의 CRC 는 "손상된 파일" 과 "상이한
//! 비밀번호" 를 구분하기 위해 존재한다. 여기는 비밀번호가 부재한다 — 태그가
//! 거절하면 생성 측의 사고이거나 위조이고, 수령자에게 달리 안내할 문구가 없다.
//!
//! ## 봉인 후 서명한다
//!
//! 역순이면 서명을 확인하려고 **선행 복호화해야 한다.** 그것은 미검증 바이트를
//! 복호화기에 투입한다는 의미다. 현 순서면 서명이 불일치하는 그릇은 **해제
//! 이전에** 폐기한다.
//!
//! ## 머리는 AAD 로도 묶는다
//!
//! 서명이 이미 머리를 포괄하는데도 그렇게 한다. 서명과 봉인이 상호 독립이면
//! [`open`] 이 [`parse`] 없이 호출되는 과실을 차단할 수단이 부재하기 때문이다.
//! AAD 로 결속하면 **머리가 상이한 채로 개방되는 경로가 구조적으로 부재한다.**
//! 비용은 0 이다.

use ed25519_dalek::{Signature, Signer, Verifier, VerifyingKey};

/// 서명하는 곳이 보유하는 개인키. **여기서 생성하지 않는다** — 부르는 곳이
/// 주입한다.
///
/// 재노출하는 사유는 판이다. 부르는 곳이 `ed25519-dalek` 을 자체로 수령해
/// 구성하면 판이 두 곳에 기재되고, 상이해지는 날 타입이 불일치하는 것으로
/// 발현된다 — 서명이 불일치하는 것으로 보이는 쪽보다는 양호하나, 기재할 사유가
/// 부재하다.
pub use ed25519_dalek::SigningKey;

use crate::{Error, ErrorCode, Result};

/// **그릇의 판.** 머리 구조나 암호 방식이 변경되면 상승한다. 알맹이의 스키마와
/// 상이한 것이다 — 그쪽은 표가 변경되는 것이고, 이쪽은 해제 방법이 변경되는
/// 것이다.
pub const CONTAINER_VERSION: u8 = 1;

/// 매직의 길이. 머리 배치가 이 값에 결속되어 있다.
pub const MAGIC_LEN: usize = 8;
const DATA_VERSION_LEN: usize = 6;
pub const NONCE_LEN: usize = 12;
const SIGNATURE_LEN: usize = 64;
/// 매직부터 논스 종단까지. AAD 로 전체를 결속하는 범위다.
const HEAD_LEN: usize = MAGIC_LEN + 1 + DATA_VERSION_LEN + 3 + NONCE_LEN;
/// GCM 태그. 알맹이가 공백이어도 이만큼은 존재한다.
const TAG_LEN: usize = 16;

/// 매직만 참조한다. **판정이 아니라 개략 추정이다** — 진위는 [`parse`] 가 판별한다.
pub fn starts_with_magic(bytes: &[u8], magic: &[u8; MAGIC_LEN]) -> bool {
    bytes.starts_with(magic)
}

/// 복호화 없이 판독한 머리.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Head {
    pub container: u8,
    /// 알맹이의 판. `yymmdd` 6자리.
    pub data_version: String,
    /// 이 알맹이를 개방 가능한 최저 앱 판. 판독 방법은 앱이 인지한다.
    pub min_app: [u8; 3],
}

/// **[`parse`] 를 통과한 바이트.** 이 타입이 아니면 [`open`] 을 호출할 수 없다.
///
/// 검사를 생략한 바이트를 복호화기에 투입하는 것을 **구조로** 차단한다. 주석이나
/// 검사로 "선행해 `parse` 를 호출하시오" 라고 기재하는 것과 상이하다 — 그러한
/// 약정은 준수하는 사람이 존재하는 동안만 유효하다.
///
/// 생성 경로가 [`parse`] 하나뿐이라, 이것을 보유했다는 사실이 곧 **매직이
/// 일치하고, 서명이 전달받은 열쇠 중 하나로 검증되었고, 그릇 판을 인지한다**는
/// 의미다.
pub struct Verified<'a> {
    bytes: &'a [u8],
    head: Head,
}

/// **바이트를 출력하지 않는다.** 대형 알맹이가 로그와 패닉 메시지로 유출된다.
/// 머리만 노출되면 어느 그릇인지 식별하기에 충분하다.
impl std::fmt::Debug for Verified<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Verified")
            .field("head", &self.head)
            .field("bytes", &format_args!("{} B", self.bytes.len()))
            .finish()
    }
}

impl<'a> Verified<'a> {
    pub fn head(&self) -> &Head {
        &self.head
    }

    /// 봉인된 바이트 전체. 복수의 그릇 중 채택된 것을 그대로 전달할 때 사용한다.
    pub fn bytes(&self) -> &'a [u8] {
        self.bytes
    }
}

/// **매직 → 서명 → 그릇 판 → 나머지.** 이 순서가 규약이다.
///
/// 매직을 선행 확인하는 것은 "우리 그릇이 아니다" 를 서명 실패와 구분하기
/// 위해서다. 서명을 차순위로 확인하는 것은, 그 이후 판독하는 값이 **전부 서명이
/// 포괄한 것**이어야 하기 때문이다. 그릇 판을 3순위로 확인하면, 추후 판을
/// 상승시켜 머리 구조를 변경해도 구 앱이 **서명을 확인한 뒤에** "미지의 그릇"
/// 으로 안전하게 중단한다.
///
/// **복호화하지 않는다.** 판 비교는 여기까지만 필요하다.
///
/// **공개키를 복수 수령해 하나라도 일치하면 통과다.** 개발 서명본과 릴리스
/// 서명본을 함께 신뢰해야 하는 빌드가 존재한다. 무엇을 전달할지는 앱이 결정한다.
pub fn parse<'a>(
    bytes: &'a [u8],
    magic: &[u8; MAGIC_LEN],
    verify: &[[u8; 32]],
) -> Result<Verified<'a>> {
    if bytes.len() < HEAD_LEN + TAG_LEN + SIGNATURE_LEN {
        return Err(Error::new(ErrorCode::DataSealInvalid)
            .detail(format!("too short: {} bytes", bytes.len())));
    }
    if !bytes.starts_with(magic) {
        return Err(Error::new(ErrorCode::DataSealInvalid).detail("magic does not match"));
    }

    let (signed, signature) = bytes.split_at(bytes.len() - SIGNATURE_LEN);
    let signature = Signature::from_slice(signature).map_err(|e| {
        Error::new(ErrorCode::DataSignatureInvalid).detail(format!("bad signature: {e}"))
    })?;
    // **판독하지 못한 열쇠를 그대로 통과시키지 않는다.** 목록에 수록된 것이
    // 공개키가 아니면 배선이 단절된 것이지 서명이 상이한 것이 아니다. 묵시적으로
    // 생략하면 "어떠한 열쇠도 불일치한다" 로 변질되어, 오타 하나를 위조로 판독한다.
    let mut matched = false;
    for raw in verify {
        let key = VerifyingKey::from_bytes(raw).map_err(|e| {
            Error::new(ErrorCode::DataSignatureInvalid)
                .detail(format!("a configured verifying key is not a valid point: {e}"))
        })?;
        if key.verify(signed, &signature).is_ok() {
            matched = true;
            break;
        }
    }
    if !matched {
        return Err(Error::new(ErrorCode::DataSignatureInvalid)
            .detail(format!("no configured key verifies this signature ({} tried)", verify.len())));
    }

    // 여기서부터 판독하는 값은 전부 서명이 포괄한 것이다.
    let container = bytes[MAGIC_LEN];
    if container != CONTAINER_VERSION {
        return Err(Error::new(ErrorCode::DataContainerUnknown)
            .with("container", container)
            .with("expected", CONTAINER_VERSION));
    }

    let at = MAGIC_LEN + 1;
    let data_version = std::str::from_utf8(&bytes[at..at + DATA_VERSION_LEN])
        .ok()
        .filter(|s| s.bytes().all(|b| b.is_ascii_digit()))
        .ok_or_else(|| {
            Error::new(ErrorCode::DataSealInvalid).detail("data version is not six digits")
        })?
        .to_string();

    let at = at + DATA_VERSION_LEN;
    let min_app = [bytes[at], bytes[at + 1], bytes[at + 2]];

    Ok(Verified { bytes, head: Head { container, data_version, min_app } })
}

/// 봉인을 해제한다. **[`Verified`] 만 수용한다** — 그 타입의 생성 경로가
/// [`parse`] 하나뿐이므로, 검사를 생략하고 여기 도달할 수 없다.
///
/// 머리가 AAD 로도 결속되어 있어, 머리를 1바이트라도 변경한 것은 여기서 재차
/// 거절된다. 중첩 배치하는 사유는 서명과 봉인이 상호 독립이기 때문이다.
pub fn open(verified: &Verified<'_>, key: &[u8; 32]) -> Result<Vec<u8>> {
    let bytes = verified.bytes;
    let head = &bytes[..HEAD_LEN];
    let nonce: [u8; NONCE_LEN] = bytes[HEAD_LEN - NONCE_LEN..HEAD_LEN]
        .try_into()
        .expect("길이는 parse 가 확인했다");
    let body = &bytes[HEAD_LEN..bytes.len() - SIGNATURE_LEN];

    sheetsift_aead::open(key, &nonce, head, body)
        .ok_or_else(|| Error::new(ErrorCode::DataSealInvalid).detail("authentication tag mismatch"))
}

/// 봉인하고 서명한다. 논스는 매회 신규 생성한다.
pub fn seal(
    image: &[u8],
    magic: &[u8; MAGIC_LEN],
    data_version: &str,
    min_app: [u8; 3],
    key: &[u8; 32],
    signing: &SigningKey,
) -> Result<Vec<u8>> {
    let mut nonce = [0u8; NONCE_LEN];
    rand::RngCore::fill_bytes(&mut rand::rng(), &mut nonce);
    seal_with_nonce(image, magic, data_version, min_app, nonce, key, signing)
}

/// 논스를 외부에서 주입한다. **고정 답안 시험만 이 경로로 호출한다** — 논스가
/// 고정되어야 결과가 고정된다. 앱이 자체 열쇠로 형식을 고정하는 시험도 이것을
/// 사용한다.
///
/// 실제 봉인 시에는 [`seal`] 이 매회 신규 논스를 생성한다. 동일한 열쇠에 논스를
/// 재사용하면 GCM 은 **은닉과 위조 차단이 동시에** 붕괴한다.
pub fn seal_with_nonce(
    image: &[u8],
    magic: &[u8; MAGIC_LEN],
    data_version: &str,
    min_app: [u8; 3],
    nonce: [u8; NONCE_LEN],
    key: &[u8; 32],
    signing: &SigningKey,
) -> Result<Vec<u8>> {
    if data_version.len() != DATA_VERSION_LEN
        || !data_version.bytes().all(|b| b.is_ascii_digit())
    {
        return Err(Error::new(ErrorCode::DataSealInvalid)
            .detail(format!("data version must be six digits: '{data_version}'")));
    }

    let mut out = Vec::with_capacity(HEAD_LEN + image.len() + TAG_LEN + SIGNATURE_LEN);
    out.extend_from_slice(magic);
    out.push(CONTAINER_VERSION);
    out.extend_from_slice(data_version.as_bytes());
    out.extend_from_slice(&min_app);
    out.extend_from_slice(&nonce);
    debug_assert_eq!(out.len(), HEAD_LEN);

    let body = sheetsift_aead::seal(key, &nonce, &out[..HEAD_LEN], image)
        .ok_or_else(|| Error::new(ErrorCode::DataSealInvalid).detail("could not seal"))?;
    out.extend_from_slice(&body);

    let signature = signing.sign(&out);
    out.extend_from_slice(&signature.to_bytes());
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 시험 전용 값들. **누구나 인지하는 값이다** — 이 크레이트가 공개되어 있으므로
    /// 실제 열쇠도 여기 기재되는 순간 동일한 처지가 된다.
    const MAGIC: &[u8; MAGIC_LEN] = b"TESTSEAL";
    const KEY: [u8; 32] = [5u8; 32];

    fn test_key() -> SigningKey {
        SigningKey::from_bytes(&[7u8; 32])
    }

    fn test_verify() -> Vec<[u8; 32]> {
        vec![test_key().verifying_key().to_bytes()]
    }

    /// 시험용 알맹이. 실제 SQLite 일 사유가 없다 — 이 층은 바이트만 취급한다.
    const IMAGE: &[u8] = b"SQLite format 3\0-- pretend this is a database --";

    fn sealed() -> Vec<u8> {
        seal_with_nonce(IMAGE, MAGIC, "260801", [0, 5, 3], [3u8; 12], &KEY, &test_key()).unwrap()
    }

    /// **머리 배치·AAD 범위·서명 범위가 단일 값에 결속된다.**
    /// 셋 중 무엇이 변경되어도 이 시험이 중단된다. 앱도 자체 열쇠로 동일한 시험을
    /// 배치한다.
    #[test]
    fn sealing_is_pinned() {
        use blake2::Digest;
        let hex: String = blake2::Blake2b512::digest(sealed())
            .iter()
            .take(16)
            .map(|b| format!("{b:02x}"))
            .collect();
        assert_eq!(
            hex, "2f64d1188a82be7db04cc6b8daa5349b",
            "봉인 방법이 변경되었습니다. 이미 배포된 앱이 신규 그릇을 개방하지 못합니다."
        );
    }

    /// 변경 위치를 제시하는 보조. 상기 시험이 중단되면 여기를 참조한다.
    #[test]
    fn the_header_layout_is_pinned() {
        let bytes = sealed();
        assert_eq!(&bytes[..8], MAGIC, "매직");
        assert_eq!(bytes[8], 1, "그릇 판");
        assert_eq!(&bytes[9..15], b"260801", "알맹이 판");
        assert_eq!(&bytes[15..18], &[0, 5, 3], "min_app");
        assert_eq!(&bytes[18..30], &[3u8; 12], "논스");
        assert_eq!(bytes.len(), 30 + IMAGE.len() + 16 + 64, "전체 길이");
    }

    #[test]
    fn what_was_sealed_comes_back() {
        let bytes = sealed();
        let verified = parse(&bytes, MAGIC, &test_verify()).unwrap();
        assert_eq!(verified.head().data_version, "260801");
        assert_eq!(verified.head().min_app, [0, 5, 3]);
        assert_eq!(open(&verified, &KEY).unwrap(), IMAGE);
    }

    /// **평문이 그릇에 잔존하지 않는다.** 봉인의 목적 그 자체다.
    #[test]
    fn the_plaintext_is_not_in_the_sealed_bytes() {
        let bytes = sealed();
        assert!(
            !bytes.windows(IMAGE.len()).any(|w| w == IMAGE),
            "봉인한 그릇 내부에 평문이 그대로 존재합니다"
        );
        assert!(
            !bytes.windows(15).any(|w| w == b"SQLite format 3"),
            "봉인한 그릇에서 SQLite 매직이 노출됩니다"
        );
    }

    /// 상이한 열쇠로 서명한 것은 신뢰하지 않는다.
    /// **개발 서명본을 릴리스 앱에 주입한 과실이 이 오류로 발현된다.**
    #[test]
    fn a_signature_from_another_key_is_rejected() {
        let other = SigningKey::from_bytes(&[9u8; 32]);
        let bytes =
            seal_with_nonce(IMAGE, MAGIC, "260801", [0, 5, 3], [3u8; 12], &KEY, &other).unwrap();
        assert_eq!(
            parse(&bytes, MAGIC, &test_verify()).unwrap_err().code,
            ErrorCode::DataSignatureInvalid
        );
    }

    /// 대칭키가 상이하면 서명이 일치해도 개방되지 않는다.
    #[test]
    fn another_data_key_does_not_open_it() {
        let bytes = sealed();
        let verified = parse(&bytes, MAGIC, &test_verify()).unwrap();
        assert_eq!(open(&verified, &[6u8; 32]).unwrap_err().code, ErrorCode::DataSealInvalid);
    }

    /// 서명은 **종단 64바이트를 제외한 전부**를 포괄한다. 어느 1바이트를 변경해도 중단한다.
    #[test]
    fn tampering_anywhere_is_detected() {
        let good = sealed();
        for at in [8, 9, 15, 18, 31, good.len() - 65] {
            let mut bad = good.clone();
            bad[at] ^= 1;
            assert_eq!(
                parse(&bad, MAGIC, &test_verify()).unwrap_err().code,
                ErrorCode::DataSignatureInvalid,
                "{at}번째 바이트를 변경했는데 통과했습니다"
            );
        }
    }

    /// 매직이 상이한 것은 **서명 실패가 아니라** 우리 그릇이 아닌 것이다.
    #[test]
    fn something_that_is_not_ours_says_so_before_the_signature() {
        let bytes = vec![0u8; HEAD_LEN + TAG_LEN + SIGNATURE_LEN];
        assert_eq!(
            parse(&bytes, MAGIC, &test_verify()).unwrap_err().code,
            ErrorCode::DataSealInvalid
        );
        // 타 앱의 매직을 부착한 그릇도 동일하다.
        assert_eq!(
            parse(&sealed(), b"OTHERAPP", &test_verify()).unwrap_err().code,
            ErrorCode::DataSealInvalid
        );
    }

    /// 미지의 그릇 판은 **서명을 확인한 뒤에** 중단한다. 그래야 추후 판을
    /// 상승시켜도 구 앱이 안전하게 정지한다.
    #[test]
    fn an_unknown_container_is_rejected_after_the_signature() {
        let mut bytes = sealed();
        bytes[8] = CONTAINER_VERSION + 1;
        // 변경한 상태로 재서명한다 — 실제로 우리 것인데 그릇만 신판인 상황이다.
        let signed_len = bytes.len() - SIGNATURE_LEN;
        let signature = test_key().sign(&bytes[..signed_len]);
        bytes[signed_len..].copy_from_slice(&signature.to_bytes());
        assert_eq!(
            parse(&bytes, MAGIC, &test_verify()).unwrap_err().code,
            ErrorCode::DataContainerUnknown
        );
    }

    /// 머리가 AAD 로 결속되어 있다. 서명을 재정합해 와도 봉인이 거절한다.
    #[test]
    fn a_rewritten_header_does_not_open_even_with_a_fresh_signature() {
        let mut bytes = sealed();
        bytes[9] = b'9'; // 알맹이 판을 260801 → 960801 로
        let signed_len = bytes.len() - SIGNATURE_LEN;
        let signature = test_key().sign(&bytes[..signed_len]);
        bytes[signed_len..].copy_from_slice(&signature.to_bytes());
        let verified =
            parse(&bytes, MAGIC, &test_verify()).expect("서명은 정합했으므로 여기는 통과한다");
        assert_eq!(open(&verified, &KEY).unwrap_err().code, ErrorCode::DataSealInvalid);
    }

    /// 논스가 매회 상이해야 한다. 재사용하면 GCM 이 붕괴한다.
    #[test]
    fn every_sealing_uses_a_fresh_nonce() {
        let a = seal(IMAGE, MAGIC, "260801", [0, 5, 3], &KEY, &test_key()).unwrap();
        let b = seal(IMAGE, MAGIC, "260801", [0, 5, 3], &KEY, &test_key()).unwrap();
        assert_ne!(a[18..30], b[18..30], "논스가 동일합니다");
        assert_ne!(a, b);
    }

    #[test]
    fn a_data_version_that_is_not_six_digits_is_refused() {
        for bad in ["26081", "2608011", "26080a", ""] {
            assert!(
                seal(IMAGE, MAGIC, bad, [0, 5, 3], &KEY, &test_key()).is_err(),
                "'{bad}' 를 수용했습니다"
            );
        }
    }
}
