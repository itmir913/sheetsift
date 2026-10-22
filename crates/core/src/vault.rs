//! 비밀번호로 봉인한 SQLite.
//!
//! 자신의 파일을 자신이 봉인해 자신이(또는 비밀번호를 전달받은 자가) 재개방한다.
//! 발행자가 부재하므로 서명이 없다 — 그쪽은 [`seal`](crate::seal) 이다.
//!
//! ## SQLCipher 를 채택하지 않은 사유
//! SQLCipher 는 Windows 에서 OpenSSL 빌드가 결부되어 CI 구성이 난해하다.
//! **직렬화된 SQLite 바이트 전체를 AES-256-GCM 으로 포장하면** 충분하다.
//! 순수 Rust 로 완결되고, GCM 이라 위·변조 탐지도 수반된다.
//! 평문 SQLite 가 디스크에 잔존하는 시점이 부재한다.
//!
//! ## 파일 형식
//! ```text
//! offset  size  내용
//!      0     7  매직 — 앱이 결정한다([`Format::MAGIC`])
//!      7     1  container_version  그릇의 판 — 해제 방법이 변경되면 상승한다
//!      8     4  schema_version (little endian)  내부의 표가 변경되면 상승한다
//!     12    16  Argon2id 솔트
//!     28    12  AES-GCM 논스
//!     40     4  머리·알맹이의 CRC-32 — **손상과 비밀번호를 구분하는 데만 사용한다**
//!     44     …  AES-256-GCM(직렬화된 SQLite) + 인증 태그 16 B
//! ```
//!
//! **전단 40바이트(검산 직전까지)가 AAD 로 함께 인증된다.** 1바이트만 변경해도
//! 복호화가 실패한다. 검산이 수록된 4바이트는 인증 범위 외부다 — 사유는 봉인하는
//! 위치에 기재한다.
//!
//! ## 손상된 파일과 상이한 비밀번호를 구분한다
//!
//! 인증 태그는 **1바이트만 상이해도** 복호화를 거절한다. 그것이 암호의 역할이라
//! 어떠한 설계로도 손상된 파일을 개방할 수는 없다. 그런데 그 시점에 "비밀번호가
//! 일치하지 않거나 파일이 손상되었습니다" 라고만 안내하면, **정확한 비밀번호를
//! 종일 재입력하게 된다.**
//!
//! 그래서 CRC 를 머리에 기재한다. 개방 시 선행 대조해 상이하면 **전송 중에
//! 손상되었다**고 안내한다 — 파일을 재수신하라는 안내가 비밀번호를 의심하라는
//! 안내보다 훨씬 유용하다. CRC 는 암호문에서 산출한 값이라 비밀번호에 관해 아무것도
//! 제시하지 않는다(누구나 재산출할 수 있다). 변조하는 측이 CRC 까지 정합해 오면
//! 그때는 태그가 차단하고, 그것은 이미 "손상된 파일" 이 아니다.
//!
//! ## 표의 구조는 앱이 인지한다
//!
//! 그릇은 SQLite 가 무엇을 수용하는지 인지하지 않는다. 표를 생성하고 상승시키는
//! 방법(마이그레이션)과 해제한 알맹이가 **우리가 생성한 구조인지** 대조하는
//! 방법은 [`Format`] 으로 앱이 주입한다. 그릇이 직접 판정하는 것은 장부의 정합
//! 여부(`integrity_check`)뿐이다.
//!
//! ## 개방 상태에서는 잠금하지 않는다 — 결정 사항이다
//!
//! 일정 시간 경과 후 자동 잠금하는 경로를 배치하지 않는다. 그 사이에 비밀번호를
//! 재질의하는 프로그램은 사용자가 창을 폐쇄해 두거나 더 단순한 비밀번호를 선택하게
//! 유도한다. 개방 상태의 보안은 화면 잠금이 담당한다.

use std::marker::PhantomData;

use argon2::Argon2;
use rand::RngCore;
use rusqlite::Connection;
use zeroize::Zeroize;

use crate::{Error, ErrorCode, Result};

/// 매직의 길이. 머리 배치가 이 값에 결속되어 있다.
pub const MAGIC_LEN: usize = 7;

/// **그릇의 판.** 머리 구조나 열쇠 도출 방법이 변경되면 상승한다.
pub const CONTAINER_VERSION: u8 = 1;

const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 12;
const CRC_LEN: usize = 4;
const HEADER_LEN: usize = MAGIC_LEN + 1 + 4 + SALT_LEN + NONCE_LEN + CRC_LEN;
/// GCM 태그. 알맹이가 공백이어도 이만큼은 존재한다.
const TAG_LEN: usize = 16;

/// 비밀번호의 최소 길이(표기를 정규화한 뒤의 문자 수).
pub const MIN_PASSWORD_LEN: usize = 8;

/// **이 그릇에 무엇이 수용되는가.** 앱이 결정한다.
///
/// 매직과 스키마 판, 그리고 표를 취급하는 두 경로다. 그릇의 코드는 단일하고,
/// 수용되는 것만 앱마다 상이하다.
pub trait Format {
    /// 앱의 오류. 그릇이 발생시키는 [`Error`] 를 수용할 수 있어야 한다.
    type Error: From<Error>;

    /// 파일 최선두의 7바이트. 사람이 수수하는 파일이라 표식 역할을 수행한다.
    const MAGIC: [u8; MAGIC_LEN];

    /// 이 앱이 인지하는 최고 스키마 판. 머리에 기재된 판이 이보다 상위면 **해제
    /// 이전에** 거절한다.
    const SCHEMA_VERSION: u32;

    /// 표를 생성하거나 구 판에서 상승시킨다. 공백 DB 에도, 해제한 구 파일에도
    /// 호출된다. 종료 시 `PRAGMA user_version` 이 [`Self::SCHEMA_VERSION`] 이어야
    /// 한다.
    fn migrate(conn: &Connection) -> std::result::Result<(), Self::Error>;

    /// 해제한 알맹이가 **우리가 생성한 구조인지** 판정한다. 표를 뷰로 교체하거나
    /// 트리거를 삽입한 것은 `integrity_check` 가 포착하지 못한다.
    fn check(conn: &Connection) -> std::result::Result<(), Self::Error>;
}

/* 열쇠 도출 비용. **크레이트 기본값에 의존하지 않는다.**

   `Argon2::default()` 는 시점별 기본값을 사용한다. argon2 는 실제로 1회 기본을
   변경했고(m=4096·t=3 → m=19456·t=2), 재차 변경되면 **동일한 비밀번호에서 상이한
   열쇠가 산출된다.** 3월에 생성한 파일을 11월에 개방하면 `비밀번호가 맞지 않거나
   파일이 손상되었습니다` 가 표시되는데 양자 모두 아니다. 누구도 원인을 규명하지
   못한다.

   64 MiB · 3회는 RFC 9106 이 차순위로 권고하는 값이고, OWASP 최소치(12 MiB·3회)의
   5배를 초과한다. 병렬도를 1로 설정하는 사유는 우리가 단일 경로로 연산하기
   때문이다 — 병렬도를 증가시키면 우리는 그만큼 가속되지 않는데 다수의 코어를
   보유한 공격자만 이득을 취한다.

   **유출된 파일에 대한 현실적 공격은 오프라인 추측 하나뿐이다.** 개방 소요 시간은
   파일 개방 시 1회이고, 추측하는 측은 그 시간을 수억 회 지불해야 한다. 그 교환은
   항상 우리 측이 유리하다.

   상승시키려면 그릇의 판을 함께 상승시켜야 한다 — 구 파일은 구 비용으로 도출해야
   개방된다. */
const KDF_MEMORY_KIB: u32 = 65_536;
const KDF_PASSES: u32 = 3;
const KDF_LANES: u32 = 1;

/// 그릇 판별 열쇠 도출 방법. **`CONTAINERS[v-1]` 이 판 v 의 것이다.**
///
/// 비용을 고정하는 것만으로는 불충분하다. 추후 비용을 상승시키려면 판을 상승시켜야
/// 하는데, **개방하는 측이 판을 참조해 분기하지 않으면** 이미 배포된 파일이 신규
/// 비용으로 해제되어 전부 개방되지 않는다. 그 시점에 화면에 표시되는 문구는
/// "비밀번호가 맞지 않거나…" 다.
///
/// 그래서 판 상승 경로를 현 시점에 개설한다. 상승 시 수행하는 작업은 이 배열에 한
/// 행을 추가하고 `CONTAINER_VERSION` 을 상승시키는 것뿐이고, **구 파일은 구 행으로
/// 개방된다.**
#[cfg(not(test))]
const CONTAINERS: &[Kdf] = &[Kdf { memory_kib: KDF_MEMORY_KIB, passes: KDF_PASSES, lanes: KDF_LANES }];

/// 시험에서는 판이 하나 더 존재한다. **판 상승 시점을 사전에 실연하려는 것**이다.
///
/// 2번째 행의 값은 1번째와 상이해야 의미가 존재한다(열쇠가 실제로 상이해져야 한다).
/// 비용은 의도적으로 낮게 설정한다 — 시험이 대기할 사유가 없다.
#[cfg(test)]
const CONTAINERS: &[Kdf] = &[
    Kdf { memory_kib: KDF_MEMORY_KIB, passes: KDF_PASSES, lanes: KDF_LANES },
    Kdf { memory_kib: 8, passes: 1, lanes: 1 },
];

#[derive(Clone, Copy)]
struct Kdf {
    memory_kib: u32,
    passes: u32,
    lanes: u32,
}

/// 판 수와 비용 행 수가 불일치하면 **컴파일이 중단된다.** (시험에서는 한 행이 더 존재한다.)
#[cfg(not(test))]
const _: () = assert!(
    CONTAINER_VERSION as usize == CONTAINERS.len(),
    "그릇 판마다 열쇠 도출 방법이 한 행씩 존재해야 한다"
);

fn kdf_for(container: u8) -> Result<Argon2<'static>> {
    let Some(cost) = container.checked_sub(1).and_then(|i| CONTAINERS.get(i as usize)) else {
        return Err(Error::new(ErrorCode::VaultContainerUnknown)
            .with("fileVersion", container)
            .with("appVersion", CONTAINER_VERSION));
    };
    let params = argon2::Params::new(cost.memory_kib, cost.passes, cost.lanes, None)
        .map_err(|e| Error::new(ErrorCode::VaultKeyDerivationFailed).detail(e))?;
    Ok(Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params))
}

/// 비밀번호의 유니코드 표기를 단일 형태로 정규화한다.
///
/// **동일한 문자가 두 바이트열로 기재된다.** `한` 은 윈도우 자판에서 완성형 1자
/// (NFC, 3바이트)로, 맥 자판·맥에서 복제한 문장에서는 자모 3개(NFD, 9바이트)로
/// 도달한다. 육안으로는 동일한데 열쇠가 상이하다. 맥에서 생성한 파일을 윈도우에서
/// 개방하지 못하고, 그 시점에 표시되는 문구는 `비밀번호가 맞지 않거나 파일이
/// 손상되었습니다` 다.
///
/// 비밀번호는 통상 **파일과 상이한 경로로**(문자·메신저) 전달된다. 그 문장이 어느
/// 표기로 도달할지 우리가 결정할 수 없다. 그래서 수신 측에서 정규화한다.
fn normalize(password: &str) -> String {
    use unicode_normalization::UnicodeNormalization;
    // **용량을 1회에 확보한다.** `collect()` 에 위임하면 8→16→32→… 로 재배치되며,
    // 재배치 이전 버퍼에 비밀번호 전단부가 **소거되지 않은 채** 해제된다.
    //
    // 3배로 확보하는 사유: NFC 는 대개 축소하지만 확대되는 문자도 존재한다
    // (`U+0344` 는 2배가 된다). 부족하면 재배치가 발생해 차단하려던 사태가 그대로
    // 발생한다.
    let mut out = String::with_capacity(password.len() * 3);
    out.extend(password.nfc());
    out
}

fn derive_key(password: &str, salt: &[u8]) -> Result<zeroize::Zeroizing<[u8; 32]>> {
    derive_key_for(CONTAINER_VERSION, password, salt)
}

/// 판 `container` 의 방법으로 열쇠를 도출한다. 구 파일은 구 방법으로 해제한다.
///
/// **포장해 반환한다.** `[u8; 32]` 는 Copy 라 그대로 반환하면 이 함수의 위치에
/// 사본이 잔존한다 — 잠금 이후에도 스택에 열쇠가 존재하는 셈이다. `Zeroizing` 은
/// 소멸 시점에 소거한다.
fn derive_key_for(
    container: u8,
    password: &str,
    salt: &[u8],
) -> Result<zeroize::Zeroizing<[u8; 32]>> {
    let mut key = zeroize::Zeroizing::new([0u8; 32]);
    let mut normalized = normalize(password);
    let hashed = kdf_for(container)?.hash_password_into(normalized.as_bytes(), salt, key.as_mut());
    normalized.zeroize();
    hashed.map_err(|e| Error::new(ErrorCode::VaultKeyDerivationFailed).detail(e))?;
    Ok(key)
}

/// `PRAGMA user_version`. 머리에 기재하는 스키마 판의 근거다.
pub fn user_version(conn: &Connection) -> Result<u32> {
    let v: i64 = conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .map_err(|e| Error::new(ErrorCode::VaultOpenFailed).detail(e))?;
    Ok(v.max(0) as u32)
}

/// 해제한 SQLite 알맹이가 **온전한지** 판정한다.
///
/// 태그가 일치했다는 것은 "이 파일을 생성한 자가 비밀번호를 인지했다" 까지만
/// 제시한다. 그런데 **파일과 비밀번호를 함께 전달하는 것이 정상 흐름**이라, 수령한
/// 자가 변조한 파일을 반송하는 경로가 개방되어 있다. SQLite 는 조작된 b-tree
/// 이미지에 대해 자체적으로 안전을 보장하지 않는다고 문서에 명시했다.
///
/// `DEFENSIVE` 는 SQL 로 스키마를 직접 변경하는 경로를 차단하고, `integrity_check`
/// 는 장부의 정합 여부를 점검한다. **수용된 것이 우리 것인지**는 그 이후
/// [`Format::check`] 가 판정한다 — 장부가 온전한 것과 구조가 우리 것인 것은 상이한
/// 진술이다.
fn check_image<F: Format>(conn: &Connection) -> std::result::Result<(), F::Error> {
    let verdict: String = conn
        .query_row("PRAGMA integrity_check", [], |r| r.get(0))
        .map_err(|e| Error::new(ErrorCode::VaultFormatInvalid).detail(e))?;
    if verdict != "ok" {
        return Err(Error::new(ErrorCode::VaultFormatInvalid).detail(verdict).into());
    }
    F::check(conn)
}

/// 알맹이를 수용하는 메모리 DB. **임시 파일을 디스크에 생성하지 않는다.**
///
/// SQLite 는 정렬이나 임시 인덱스가 캐시를 초과하면 `%TEMP%` 에 임시 파일을
/// 생성한다. 그 내부에는 수용된 것이 평문으로 기록된다. 이 파일이 전체 암호화된다는
/// 진술은 이 한 행이 존재해야 참이 된다.
fn memory_db() -> Result<Connection> {
    let conn = Connection::open_in_memory()
        .map_err(|e| Error::new(ErrorCode::VaultOpenFailed).detail(e))?;
    /* `foreign_keys` 는 SQLite 에서 **기본이 비활성**이다. 스키마에 `ON DELETE
       CASCADE` 를 기재해도 활성화하지 않으면 그 선언이 아무 작용도 하지 않는다.
       삭제된 행에 종속된 행이 파일 내부에 고아로 잔존한다 — 개인정보 파일에서
       삭제가 삭제가 아니게 된다.

       기존 수록 자료에는 영향이 없다(이 설정은 향후 기록에만 적용된다). 그래서
       보유 중인 파일이 이것을 사유로 개방되지 않는 사태는 발생하지 않는다. */
    conn.execute_batch("PRAGMA temp_store = MEMORY; PRAGMA foreign_keys = ON;")
        .map_err(|e| Error::new(ErrorCode::VaultOpenFailed).detail(e))?;

    // **생성 경로에도 적용한다.** 개방 경로에만 적용하면, 동일한 자료를 수용하는
    // 동일한 연결인데 어느 경로로 도달했는지에 따라 설정이 상이해진다. 활성화
    // 여부도 확인한다 — `set_db_config` 는 **적용된 값**을 반환한다.
    let on = conn
        .set_db_config(rusqlite::config::DbConfig::SQLITE_DBCONFIG_DEFENSIVE, true)
        .map_err(|e| Error::new(ErrorCode::VaultOpenFailed).detail(e))?;
    if !on {
        return Err(Error::new(ErrorCode::VaultOpenFailed).detail("defensive mode refused"));
    }
    Ok(conn)
}

/// 파일이 전송 경로에서 손상되었는지 판정하는 검산. **암호가 아니라 진단이다.**
///
/// CRC-32(IEEE). 바이트가 변경되거나 절단되면 사실상 항상 포착된다. 의도적으로
/// 정합해 오는 것을 차단하는 장치가 아니다 — 그것은 인증 태그가 담당한다.
///
/// **머리와 알맹이를 함께 산정한다.** 알맹이만 산정하면 솔트나 논스가 손상된 파일이
/// "비밀번호가 맞지 않습니다" 로, 판 항목이 손상된 파일이 "최신 프로그램을
/// 요청하세요" 로 노출된다 — 이 검산을 도입한 사유가 바로 그 두 문구를 제거하자는
/// 것이다. 자체 검산이 수록된 4바이트만 제외한다.
fn crc32(parts: &[&[u8]]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for part in parts {
        for b in *part {
            crc ^= u32::from(*b);
            for _ in 0..8 {
                let mask = (crc & 1).wrapping_neg();
                crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
            }
        }
    }
    !crc
}

fn check_password(password: &str) -> Result<()> {
    // 길이도 **정규화한 뒤에** 산정한다. NFD 로 도달한 `한글비밀번호` 는 자모
    // 각각이 문자로 산정되어, 동일한 비밀번호가 표기에 따라 통과하기도 하고
    // 통과하지 않기도 한다.
    let mut normalized = normalize(password);
    let count = normalized.chars().count();
    normalized.zeroize();
    if count < MIN_PASSWORD_LEN {
        return Err(Error::new(ErrorCode::VaultPasswordTooShort).with("min", MIN_PASSWORD_LEN));
    }
    Ok(())
}

/// 개방된 봉투. 메모리에만 존재한다.
pub struct Vault<F: Format> {
    pub conn: Connection,
    /// 재봉인 시 사용하는 키. Drop 에서 소거한다.
    key: [u8; 32],
    /// 키를 유도한 솔트. 동일한 키를 계속 사용하려면 함께 보관해야 한다.
    salt: [u8; SALT_LEN],
    /// **이 열쇠를 도출한 그릇의 판.** 봉인 시 머리에 기재하는 값이다.
    ///
    /// 상수를 기재하면 안 된다. 구 판 파일을 신규 앱에서 개방하면 열쇠는 구 비용으로
    /// 도출되어 있는데 머리에는 신판이 기재되고, 차회 개방 시 신규 비용으로 도출해
    /// 열쇠가 상이해진다 — 1회 저장하는 것만으로 파일이 영구히 개방되지 않는다.
    container: u8,
    pub schema_version: u32,
    format: PhantomData<F>,
}

impl<F: Format> Drop for Vault<F> {
    fn drop(&mut self) {
        self.key.zeroize();
    }
}

/// 키·솔트가 로그나 패닉 메시지로 유출되지 않도록 직접 구현한다.
impl<F: Format> std::fmt::Debug for Vault<F> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Vault")
            .field("schema_version", &self.schema_version)
            .field("key", &"<redacted>")
            .finish()
    }
}

impl<F: Format> Vault<F> {
    /// 공백 봉투를 신규 생성한다.
    pub fn create(password: &str) -> std::result::Result<Self, F::Error> {
        check_password(password)?;
        let mut salt = [0u8; SALT_LEN];
        rand::rng().fill_bytes(&mut salt);
        let key = derive_key(password, &salt)?;

        let conn = memory_db()?;
        F::migrate(&conn)?;

        Ok(Self {
            conn,
            key: *key,
            salt,
            container: CONTAINER_VERSION,
            schema_version: F::SCHEMA_VERSION,
            format: PhantomData,
        })
    }

    /// 파일 내용을 복호화해 메모리에서 개방한다.
    pub fn open(bytes: &[u8], password: &str) -> std::result::Result<Self, F::Error> {
        Self::open_as(bytes, password, CONTAINER_VERSION)
    }

    /// 개방 경로. **이 앱이 몇 판인지를 수용한다.**
    ///
    /// 항상 `CONTAINER_VERSION` 이다. 판 상승 경로(구 그릇을 개방해 신규 그릇으로
    /// 상승시키는 것)는 판이 둘이 되기 전에는 도달할 수 없어, 그 경로만 시험이
    /// 통과할 수 있도록 여지를 개방해 둔다. **2회나 이 위치에서 오류를 범했다** —
    /// 1회는 개방하는 측이 판을 참조하지 않았고, 1회는 봉인하는 측이 상수를
    /// 기재했다. 차후 판을 상승시키는 자가 수동으로 확인하기를 기대하지 않는다.
    fn open_as(
        bytes: &[u8],
        password: &str,
        app_container: u8,
    ) -> std::result::Result<Self, F::Error> {
        if bytes.len() < HEADER_LEN + TAG_LEN || bytes[..MAGIC_LEN] != F::MAGIC {
            return Err(Error::new(ErrorCode::VaultFormatInvalid)
                .detail("magic mismatch or file too short")
                .into());
        }
        /* 그릇의 판. **인지하는 판이 아니면 해제 방법을 인지하지 못하는 파일이다.**

           판 0 인 그릇은 존재한 적이 없으므로, 0 을 기재한 파일은 신규도 구판도
           아니고 단지 우리 것이 아니다. 1바이트가 손상된 파일도 여기서 포착된다 —
           그 시점에 "더 신규 프로그램을 확보하세요" 라고 안내하면 존재하지도 않는
           것을 탐색하러 간다. */
        let container = bytes[MAGIC_LEN];
        if container == 0 || container > app_container {
            return Err(Error::new(ErrorCode::VaultContainerUnknown)
                .with("fileVersion", container)
                .with("appVersion", app_container)
                .into());
        }
        // 인증하는 머리는 검산 직전까지다(봉인하는 측과 동일해야 한다).
        let header = &bytes[..HEADER_LEN - CRC_LEN];
        let ciphertext = &bytes[HEADER_LEN..];

        /* **전송 경로에서 손상되었는지 선행 판정한다.** 태그가 차단하는 것과
           동일한 결과지만 안내가 상이하다 — 여기서 포착되면 "파일을 재수신하세요"
           이고, 태그에서 포착되면 "비밀번호를 확인하세요" 다.

           **그릇 판 검사 이후에 배치한다.** 판이 상승하면 머리 구조가 변경될 수
           있는데, 그 시점에 구 앱이 신규 파일을 "손상" 이라 지칭하면 안 된다. 그것은
           미지의 파일이지 손상된 파일이 아니다. 반대로 **스키마 판 검사보다는
           선행한다** — 그 항목이 손상된 것을 "더 신규 프로그램을 확보하세요" 라고
           안내하면 존재하지 않는 것을 탐색하러 간다. */
        let recorded = u32::from_le_bytes(
            bytes[HEADER_LEN - CRC_LEN..HEADER_LEN].try_into().expect("length checked above"),
        );
        if crc32(&[header, ciphertext]) != recorded {
            return Err(Error::new(ErrorCode::VaultFileDamaged).into());
        }

        // 머리에 기재된 스키마 판. **알맹이 해제 이전에** 상위 판을 여과한다 —
        // 미지의 구조를 굳이 해제해 놓고 그제야 차단할 사유가 없다.
        //
        // 함께 인증되므로 **위조는 불가하다.** 근거는 어디까지나 복호화 이후 판독하는
        // DB 내부의 `user_version` 이고, 이것은 사전 여과용으로 배치하는 사본이다.
        let at = MAGIC_LEN + 1;
        let header_version =
            u32::from_le_bytes(bytes[at..at + 4].try_into().expect("length checked above"));
        if header_version > F::SCHEMA_VERSION {
            return Err(Error::new(ErrorCode::VaultVersionTooNew)
                .with("fileVersion", header_version)
                .with("appVersion", F::SCHEMA_VERSION)
                .into());
        }

        let at = at + 4;
        let mut salt = [0u8; SALT_LEN];
        salt.copy_from_slice(&bytes[at..at + SALT_LEN]);
        let nonce: [u8; NONCE_LEN] = bytes[at + SALT_LEN..at + SALT_LEN + NONCE_LEN]
            .try_into()
            .expect("length checked above");
        // 수용할 공백 그릇을 선행 개방한다.
        let mut conn = memory_db()?;

        // **해당 파일의 판으로 도출한다.** 비용이 판마다 상이할 수 있다.
        let key = derive_key_for(container, password, &salt)?;
        /* **자동 소거되는 그릇에 수용한다.** 여기 수록된 것은 평문 DB 다. 수동
           소거 행을 배치했을 때 2회 오류를 범했다 — 1회는 `?` 가 그 행 이전에
           복귀해서, 1회는 그릇을 개방하는 `?` 가 그 행 이전에 복귀해서. 양자 모두
           시험이 포착하지 못한다. 소거 위치를 정확히 선정하는 대신 **선정할 사안을
           제거한다**: `Zeroizing` 은 어떤 경로로 함수를 이탈하든, 되감기는 패닉에서도
           소거한다. */
        let plain = zeroize::Zeroizing::new(
            sheetsift_aead::open(&key, &nonce, header, ciphertext)
                // 비밀번호 오류와 파일 손상은 구분할 수 없다. 구분해서도 안 된다.
                .ok_or_else(|| Error::new(ErrorCode::VaultPasswordWrong))?,
        );

        let len = plain.len();
        conn.deserialize_read_exact("main", std::io::Cursor::new(&*plain), len, false)
            .map_err(|e| Error::new(ErrorCode::VaultOpenFailed).detail(e))?;

        check_image::<F>(&conn)?;

        // 여기서부터 근거는 DB 내부의 판이다. 구 파일은 차단하지 않고 상승시킨다 —
        // 장기간 이전에 생성한 파일을 개방하는 것이 통상이다.
        F::migrate(&conn)?;
        let schema_version = user_version(&conn)?;

        /* **구 그릇은 개방하면서 신규 그릇으로 상승시킨다.** 비밀번호가 여기 존재할
           때만 수행 가능한 작업이다 — 저장 시점에는 이미 부재한다. 상승시킨 열쇠는
           차회 저장부터 적용되고, 저장하지 않으면 파일은 그대로다. */
        let mut vault =
            Self { conn, key: *key, salt, container, schema_version, format: PhantomData };
        if container < app_container {
            let mut fresh = [0u8; SALT_LEN];
            rand::rng().fill_bytes(&mut fresh);
            let upgraded = derive_key_for(app_container, password, &fresh)?;
            vault.key.zeroize();
            vault.key = *upgraded;
            vault.salt = fresh;
            vault.container = app_container;
        }
        Ok(vault)
    }

    /// 현재 내용을 암호화한 바이트로 생성한다. 파일 기록은 부르는 곳이 수행한다.
    /// 논스는 저장할 때마다 신규 생성한다(동일한 키로 논스를 재사용하지 않는다).
    pub fn seal(&self) -> Result<Vec<u8>> {
        seal_with(&self.conn, F::MAGIC, self.container, &self.key, &self.salt)
    }

    /// **타 연결을 이 봉투의 열쇠로** 봉인한다. 앱의 시험이 변조 파일을 생성할
    /// 때만 사용한다.
    ///
    /// 비밀번호를 인지하는 자가 수행 가능한 작업이 정확히 그 범위다 — 해제해 변조하고
    /// 동일한 비밀번호로 재봉인한다. 그러한 파일이 [`Format::check`] 에 포착되는지
    /// 판정하려면 그 파일을 생성할 수 있어야 한다. 우리 연결은 `DEFENSIVE` 라 스키마를
    /// 변조할 수 없으므로 순수 연결을 수용한다.
    #[doc(hidden)]
    pub fn seal_connection(&self, conn: &Connection) -> Result<Vec<u8>> {
        seal_with(conn, F::MAGIC, self.container, &self.key, &self.salt)
    }

    /// 비밀번호를 변경하고, 그 위치에서 파일 기록까지 수행한다.
    ///
    /// ## 기록을 여기서 수용하는 사유
    ///
    /// 키만 교체하고 저장을 추후 수용하면, **파일은 구 비밀번호로 봉인된 채 앱만
    /// 신규 비밀번호를 보유한 상태**가 발생한다. 변경했다고 인지하는데 파일은
    /// 변경되지 않은 것이다. 그래서 기록까지가 단일 동작이다.
    ///
    /// 순서도 역전할 수 없다. **기록이 종료된 이후에야** `self` 의 키를 교체한다 —
    /// 디스크가 포화해 기록이 실패했는데 앱만 신규 키를 보유하면, 차회 저장이 누구도
    /// 인지하지 못하는 비밀번호로 파일을 피복한다.
    ///
    /// 현행 비밀번호를 재질의하는 사유는, 파일을 개방한 채 자리를 비우는 일이
    /// 빈번해서다. 인접한 자가 비밀번호만 변경해 잠금할 수 있으면 안 된다.
    pub fn rekey(
        &mut self,
        current: &str,
        next: &str,
        write: impl FnOnce(&[u8]) -> std::result::Result<(), F::Error>,
    ) -> std::result::Result<(), F::Error> {
        // **이 금고의 판으로 도출한다.** `self.key` 가 그 판으로 도출된 것이라,
        // 상수로 도출하면 판이 분기한 금고에서 정확한 비밀번호를 오류로 판정하게 된다.
        let proof = derive_key_for(self.container, current, &self.salt)?;
        if !constant_time_eq(&proof, &self.key) {
            return Err(Error::new(ErrorCode::VaultPasswordWrong).into());
        }
        check_password(next)?;

        // 솔트도 신규 생성한다. 동일한 솔트를 계속 사용하면 구 비밀번호로 생성한
        // 무지개표가 그대로 유효하다.
        let mut salt = [0u8; SALT_LEN];
        rand::rng().fill_bytes(&mut salt);
        // **이 금고의 판으로 도출하고 그 판으로 봉인한다.** 상수를 사용하면 판이
        // 분기한 금고에서 도출한 열쇠와 기재한 판이 불일치한다 — 이 위치에서 이미
        // 2회 오류를 범했다.
        let key = derive_key_for(self.container, next, &salt)?;
        let bytes = seal_with(&self.conn, F::MAGIC, self.container, &key, &salt)?;
        write(&bytes)?;

        self.key.zeroize();
        self.key = *key;
        self.salt = salt;
        Ok(())
    }
}

fn seal_with(
    conn: &Connection,
    magic: [u8; MAGIC_LEN],
    container: u8,
    key: &[u8; 32],
    salt: &[u8; SALT_LEN],
) -> Result<Vec<u8>> {
    /* **이 평문은 소거할 수 없다.** `serialize` 는 우선 `NOCOPY` 로 SQLite 가
       보유한 **활성 페이지**를 지시하게 한다. 여기서 소거하면 개방된 DB 가 그
       위치에서 손상된다. 사본이 도달하는 경우에도 `&mut` 로 수용할 경로가 부재한다.

       그래서 앱이 동작하는 동안 수용된 자료는 메모리에 평문으로 존재한다 — 그것이
       설계다(파일만 암호화한다). 잠금 이후 잔존물을 감소시키려면 SQLite 의 할당기를
       교체해야 하고, 그것은 이 그릇이 응답할 수 있는 질의가 아니다. */
    let data = conn
        .serialize("main")
        .map_err(|e| Error::new(ErrorCode::VaultSealFailed).detail(e))?;

    let mut nonce = [0u8; NONCE_LEN];
    rand::rng().fill_bytes(&mut nonce);

    // 머리에 기재하는 판은 **DB 에서 판독한다.** 상수를 기재하면 알맹이가 구 판인
    // 채로 머리만 신판이 되고, 그 불일치는 누구도 포착하지 못한다.
    let version = user_version(conn)?;

    let mut out = Vec::with_capacity(HEADER_LEN + data.len() + TAG_LEN);
    out.extend_from_slice(&magic);
    // **상수가 아니라 이 열쇠를 도출한 판을 기재한다.** 양자가 불일치하면 차회
    // 개방 시 상이한 비용으로 도출해 열쇠가 상이해진다.
    out.push(container);
    out.extend_from_slice(&version.to_le_bytes());
    out.extend_from_slice(salt);
    out.extend_from_slice(&nonce);

    /* **인증 범위는 CRC 직전까지다.** 포함할 수가 없다 — CRC 는 알맹이 생성 이후에야
       산출되는데, 그 알맹이가 머리를 AAD 로 채택해 생성되기 때문이다. 상호 의존한다.

       제외해도 상실하는 것이 없다. CRC 는 **손상과 상이한 비밀번호를 구분하는 안내**
       일 뿐이고, 거기를 변조해 봐야 "손상되었습니다" 와 "비밀번호가 맞지 않습니다" 가
       역전될 뿐이다. 위조는 여전히 태그가 차단한다. */
    let ct = sheetsift_aead::seal(key, &nonce, &out, &data)
        .ok_or_else(|| Error::new(ErrorCode::VaultSealFailed).detail("could not seal"))?;
    let crc = crc32(&[&out, &ct]);
    out.extend_from_slice(&crc.to_le_bytes());
    out.extend_from_slice(&ct);
    Ok(out)
}

/// 길이가 동일한 두 바이트열을 **종단까지** 대조한다. 상이한 위치에서 즉시
/// 복귀하면 소요 시간이 "몇 문자까지 일치했는지" 를 노출한다.
fn constant_time_eq(a: &[u8; 32], b: &[u8; 32]) -> bool {
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    /// 시험용 그릇. 표 하나에 판 하나다.
    struct Notes;

    impl Format for Notes {
        type Error = Error;
        const MAGIC: [u8; MAGIC_LEN] = *b"TESTVLT";
        const SCHEMA_VERSION: u32 = 1;

        fn migrate(conn: &Connection) -> Result<()> {
            if user_version(conn)? == 0 {
                conn.execute_batch(
                    "CREATE TABLE notes(id INTEGER PRIMARY KEY, body TEXT NOT NULL);
                     PRAGMA user_version = 1;",
                )
                .map_err(|e| Error::new(ErrorCode::VaultOpenFailed).detail(e))?;
            }
            Ok(())
        }

        /// 표가 `notes` 하나뿐인지만 판정한다.
        fn check(conn: &Connection) -> Result<()> {
            let n: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE name IS NOT 'notes'",
                    [],
                    |r| r.get(0),
                )
                .map_err(|e| Error::new(ErrorCode::VaultFormatInvalid).detail(e))?;
            if n != 0 {
                return Err(Error::new(ErrorCode::VaultFormatInvalid).detail("unknown objects"));
            }
            Ok(())
        }
    }

    type V = Vault<Notes>;

    const PW: &str = "학교비밀번호2027";
    const NEW_PW: &str = "바뀐비밀번호2027";

    fn seeded() -> V {
        let v = V::create(PW).unwrap();
        v.conn.execute("INSERT INTO notes(body) VALUES('비밀메모')", []).unwrap();
        v
    }

    fn body(v: &V) -> String {
        v.conn.query_row("SELECT body FROM notes", [], |r| r.get(0)).unwrap()
    }

    /// 검산을 재정합한다. 변조하는 측이 수행 가능한 작업이다.
    fn refix_crc(sealed: &mut [u8]) {
        let fixed =
            crc32(&[&sealed[..HEADER_LEN - CRC_LEN], &sealed[HEADER_LEN..]]).to_le_bytes();
        sealed[HEADER_LEN - CRC_LEN..HEADER_LEN].copy_from_slice(&fixed);
    }

    #[test]
    fn round_trip_preserves_contents() {
        let opened = V::open(&seeded().seal().unwrap(), PW).unwrap();
        assert_eq!(body(&opened), "비밀메모");
    }

    #[test]
    fn wrong_password_is_rejected() {
        let e = V::open(&seeded().seal().unwrap(), "틀린비밀번호").unwrap_err();
        assert_eq!(e.code, ErrorCode::VaultPasswordWrong);
    }

    /// **전송 경로의 손상과 비밀번호 불일치를 구분한다.**
    ///
    /// 양자 모두 개방되지 않는 것은 동일하다 — 암호가 그렇게 설계되어 있다. 상이한
    /// 것은 안내 문구다. 정확한 비밀번호를 보유한 자에게 "비밀번호를 확인하세요"
    /// 라고 안내하면 하루를 엉뚱한 곳에서 소모한다.
    #[test]
    fn a_file_damaged_in_transit_says_so_instead_of_blaming_the_password() {
        let mut sealed = seeded().seal().unwrap();
        let last = sealed.len() - 30;
        sealed[last] ^= 0xff;
        assert_eq!(
            V::open(&sealed, PW).unwrap_err().code,
            ErrorCode::VaultFileDamaged,
            "손상된 파일을 비밀번호 사유로 전가한다"
        );

        // 절단되어 도달한 파일도 동일하다(클라우드가 절반만 전송하는 사례가 존재한다).
        let mut cut = seeded().seal().unwrap();
        cut.truncate(cut.len() - 40);
        assert_eq!(V::open(&cut, PW).unwrap_err().code, ErrorCode::VaultFileDamaged);
    }

    /// **검산까지 정합해 온 위조는 태그가 차단한다.** 그때는 손상된 파일이 아니다.
    #[test]
    fn tampering_is_detected() {
        let mut sealed = seeded().seal().unwrap();
        let last = sealed.len() - 30;
        sealed[last] ^= 0xff;
        refix_crc(&mut sealed);
        assert_eq!(V::open(&sealed, PW).unwrap_err().code, ErrorCode::VaultPasswordWrong);
    }

    /// **머리의 손상도 손상이라고 안내한다.**
    ///
    /// 솔트·논스·판 항목은 전송 경로에서 손상되기 쉬운 위치인데, 검산이 알맹이만
    /// 포괄하면 이것이 "비밀번호가 맞지 않습니다" 로 노출된다.
    #[test]
    fn a_damaged_header_says_damaged_not_wrong_password() {
        for at in [8, 12, 20, 28, 39] {
            let mut sealed = seeded().seal().unwrap();
            sealed[at] ^= 0x01;
            assert_eq!(
                V::open(&sealed, PW).unwrap_err().code,
                ErrorCode::VaultFileDamaged,
                "{at}번 바이트가 손상되었는데 손상이라 안내하지 않는다"
            );
        }
    }

    /// 머리를 **검산까지 정합해** 변조해 오면 그때는 태그가 차단한다. AAD 의 역할이다.
    #[test]
    fn header_tampering_is_detected() {
        let mut sealed = seeded().seal().unwrap();
        sealed[20] ^= 0x01; // 솔트를 변경한다 → 키가 상이해지므로 실패
        refix_crc(&mut sealed);
        assert_eq!(V::open(&sealed, PW).unwrap_err().code, ErrorCode::VaultPasswordWrong);
    }

    #[test]
    fn plaintext_never_appears_in_the_file() {
        let sealed = seeded().seal().unwrap();
        assert!(!sealed.starts_with(b"SQLite format 3\0"), "평문 SQLite 헤더가 노출된다");
        let needle = "비밀메모".as_bytes();
        assert!(!sealed.windows(needle.len()).any(|w| w == needle), "수용된 문장이 평문으로 잔존한다");
    }

    #[test]
    fn short_password_is_rejected() {
        assert_eq!(V::create("1234").unwrap_err().code, ErrorCode::VaultPasswordTooShort);
    }

    /// 자모로 기재해 8자를 초과하는 비밀번호는 **정규화한 뒤에** 산정하면 부족하다.
    #[test]
    fn the_length_is_counted_after_normalizing() {
        use unicode_normalization::UnicodeNormalization;
        let decomposed: String = "한글비밀".nfd().collect();
        assert!(decomposed.chars().count() >= MIN_PASSWORD_LEN, "자모로 분해되지 않았다");
        assert_eq!(V::create(&decomposed).unwrap_err().code, ErrorCode::VaultPasswordTooShort);
    }

    #[test]
    fn future_schema_version_is_rejected() {
        let mut sealed = seeded().seal().unwrap();
        sealed[8..12].copy_from_slice(&(Notes::SCHEMA_VERSION + 5).to_le_bytes());
        // 검산을 정합해 둔다 — 그러지 않으면 "손상된 파일" 로 선행 포착된다. 여기서
        // 판정하려는 것은 **온전한 파일이 더 신규 판일 때** 무슨 안내를 하는가다.
        refix_crc(&mut sealed);
        assert_eq!(V::open(&sealed, PW).unwrap_err().code, ErrorCode::VaultVersionTooNew);
    }

    /// 봉인 시 머리에 기재하는 판은 DB 에서 판독한다. 상수를 기재하면 알맹이와
    /// 머리가 불일치하고, 그 불일치는 누구도 포착하지 못한다.
    #[test]
    fn the_header_version_comes_from_the_database() {
        let v = seeded();
        let bare = v.seal_connection(&Connection::open_in_memory().unwrap()).unwrap();
        assert_eq!(u32::from_le_bytes(bare[8..12].try_into().unwrap()), 0);
        // 구 판(0) 파일은 차단하지 않고 상승시켜 개방한다.
        let opened = V::open(&bare, PW).unwrap();
        assert_eq!(opened.schema_version, Notes::SCHEMA_VERSION, "개방하면서 판을 상승시키지 않았다");
    }

    /// 앱이 대조하는 구조에 불일치하면 개방하지 않는다.
    #[test]
    fn what_the_format_does_not_recognise_does_not_open() {
        let v = seeded();
        let plain = Connection::open_in_memory().unwrap();
        plain
            .execute_batch(
                "CREATE TABLE notes(id INTEGER PRIMARY KEY, body TEXT NOT NULL);
                 CREATE TABLE extra(x);
                 PRAGMA user_version = 1;",
            )
            .unwrap();
        let doctored = v.seal_connection(&plain).unwrap();
        assert_eq!(V::open(&doctored, PW).unwrap_err().code, ErrorCode::VaultFormatInvalid);
    }

    /// 비밀번호를 변경하면 **신규 비밀번호로 개방되고 구 비밀번호로는 개방되지 않는다.**
    #[test]
    fn rekeying_changes_which_password_opens_the_file() {
        let mut v = seeded();
        let mut saved = Vec::new();
        v.rekey(PW, NEW_PW, |b| {
            saved = b.to_vec();
            Ok(())
        })
        .unwrap();

        let opened = V::open(&saved, NEW_PW).unwrap();
        assert_eq!(body(&opened), "비밀메모", "알맹이가 함께 이전되지 않았다");
        assert_eq!(
            V::open(&saved, PW).unwrap_err().code,
            ErrorCode::VaultPasswordWrong,
            "구 비밀번호로 여전히 개방된다"
        );
    }

    /// 변경 이후 단순 저장해도 신규 비밀번호로 봉인되어야 한다. 키를 실제로
    /// 교체했는지 판정하는 위치다.
    #[test]
    fn saving_after_a_rekey_uses_the_new_password() {
        let mut v = seeded();
        v.rekey(PW, NEW_PW, |_| Ok(())).unwrap();
        assert!(V::open(&v.seal().unwrap(), NEW_PW).is_ok());
    }

    #[test]
    fn rekeying_needs_the_current_password() {
        let mut v = seeded();
        let e = v.rekey("틀린비밀번호", NEW_PW, |_| panic!("기록까지 도달했다")).unwrap_err();
        assert_eq!(e.code, ErrorCode::VaultPasswordWrong);
        assert!(V::open(&v.seal().unwrap(), PW).is_ok(), "키가 변경되었다");
    }

    #[test]
    fn a_short_new_password_is_rejected() {
        let mut v = seeded();
        let e = v.rekey(PW, "1234", |_| panic!("기록까지 도달했다")).unwrap_err();
        assert_eq!(e.code, ErrorCode::VaultPasswordTooShort);
    }

    /// **기록이 실패하면 키도 그대로다.** 파일은 구 비밀번호로 봉인되어 있는데
    /// 앱만 신규 키를 보유하면, 차회 저장이 누구도 인지하지 못하는 비밀번호로
    /// 피복한다.
    #[test]
    fn a_failed_write_leaves_the_old_password_in_force() {
        let mut v = seeded();
        let e = v
            .rekey(PW, NEW_PW, |_| {
                Err(Error::new(ErrorCode::VaultWriteFailed).detail("디스크가 포화했다"))
            })
            .unwrap_err();
        assert_eq!(e.code, ErrorCode::VaultWriteFailed);

        let sealed = v.seal().unwrap();
        assert!(V::open(&sealed, PW).is_ok(), "기록이 실패했는데 키가 변경되었다");
        assert_eq!(V::open(&sealed, NEW_PW).unwrap_err().code, ErrorCode::VaultPasswordWrong);
    }

    /* ── 이미 배포된 그릇을 고정하는 위치 ───────────────────────

       하기는 **1회 배포되면 변경할 수 없는 것**을 보전한다. 값을 변경하는 측이
       그럴듯한 사유를 보유해도(더 강력한 암호, 더 단축된 머리) 사용자가 보유한
       파일은 그 변경을 추종하지 못한다. 그래서 사람의 판단이 아니라 시험이
       차단한다. 앱은 자체 매직과 이미 배포된 파일로 동일한 것을 재차 판정한다. */

    /// 머리의 구조. **위치 하나하나가 약정이다.**
    ///
    /// 항목을 하나 증가시키거나 순서를 변경하면 이미 배포된 파일은 영구히 개방되지
    /// 않는다. 수치를 여기 기재하는 것은 문서가 아니라 **잠금 장치**다.
    #[test]
    fn the_header_layout_is_pinned() {
        assert_eq!(MAGIC_LEN, 7);
        assert_eq!(CONTAINER_VERSION, 1);
        assert_eq!(SALT_LEN, 16);
        assert_eq!(NONCE_LEN, 12);
        assert_eq!(TAG_LEN, 16);
        assert_eq!(CRC_LEN, 4);
        assert_eq!(HEADER_LEN, 44, "머리 길이가 변경되었습니다");

        // 위치까지 판정한다. 길이의 합이 동일해도 순서가 변경되면 판독하지 못한다.
        let v = seeded();
        let sealed = v.seal().unwrap();
        assert_eq!(&sealed[..7], &Notes::MAGIC);
        assert_eq!(sealed[7], CONTAINER_VERSION);
        assert_eq!(u32::from_le_bytes(sealed[8..12].try_into().unwrap()), Notes::SCHEMA_VERSION);
        assert_eq!(&sealed[12..28], &v.salt, "솔트 위치");
        assert_eq!(
            u32::from_le_bytes(sealed[40..44].try_into().unwrap()),
            crc32(&[&sealed[..HEADER_LEN - CRC_LEN], &sealed[HEADER_LEN..]]),
            "머리에 기재된 검산이 머리·알맹이와 일치하지 않는다"
        );
        let image = v.conn.serialize("main").unwrap().len();
        assert_eq!(sealed.len(), HEADER_LEN + TAG_LEN + image);
    }

    /// **열쇠 도출 방법이 고정되어 있는가.**
    ///
    /// 동일한 비밀번호·동일한 솔트에서 항상 동일한 열쇠가 산출되어야 한다.
    /// 알고리즘·판·비용 어느 하나라도 변경되면 여기서 중단된다 — 크레이트가
    /// 기본값을 변경해도 동일하다. 이 시험이 중단되면 **이미 배포된 파일이 개방되지
    /// 않는다는 의미**이고, 화면에는 "비밀번호가 맞지 않거나…" 가 표시된다. 그
    /// 지점을 여기서 포착한다.
    #[test]
    fn the_key_recipe_is_pinned() {
        let key = derive_key("학교비밀번호2027", b"goe-navi-salt-16").unwrap();
        assert_eq!(
            hex(&*key),
            "dd5dcf8533250488be80b1df35c8df8c4653bec6e14403e20778576549c2993f",
            "\n열쇠 도출 방법이 변경되었습니다. 비용·알고리즘·판 중 무엇이 변경되었는지\n\
             확인하세요. 이미 배포된 파일이 존재하면 그 파일들은 개방되지 않습니다.\n"
        );
        let p = kdf_for(CONTAINER_VERSION).unwrap().params().clone();
        assert_eq!((p.m_cost(), p.t_cost(), p.p_cost()), (KDF_MEMORY_KIB, KDF_PASSES, KDF_LANES));
    }

    /// **자모로 기재한 비밀번호로도 개방된다.** 맥 자판과 맥에서 복제한 문장은 `한`
    /// 을 자모 3개로 기재한다(NFD). 육안으로는 동일한 문자다.
    #[test]
    fn the_same_password_typed_on_a_mac_opens_it_too() {
        use unicode_normalization::UnicodeNormalization;
        let decomposed: String = PW.nfd().collect();
        assert_ne!(decomposed, PW, "선정한 비밀번호에 자모로 분해될 문자가 부재합니다");
        assert!(V::open(&seeded().seal().unwrap(), &decomposed).is_ok());
    }

    /// 그릇의 판이 더 신규이면 **해제 방법을 인지하지 못하는 파일**이다. 매직에
    /// 결합해 두었을 때는 이것이 "우리 파일이 아니다" 로 노출되었다.
    #[test]
    fn a_newer_container_says_so() {
        let mut sealed = seeded().seal().unwrap();
        sealed[7] = CONTAINER_VERSION + 1;
        let e = V::open(&sealed, PW).unwrap_err();
        assert_eq!(e.code, ErrorCode::VaultContainerUnknown);
        // 매직은 그대로이므로 "우리 파일이 아니다" 가 되어서는 안 된다.
        assert_ne!(e.code, ErrorCode::VaultFormatInvalid);

        // 존재한 적 없는 판 0 도 인지하는 판이 아니다. 1바이트가 손상된 파일이 여기 도달한다.
        let mut zeroed = seeded().seal().unwrap();
        zeroed[7] = 0;
        assert_eq!(V::open(&zeroed, PW).unwrap_err().code, ErrorCode::VaultContainerUnknown);
    }

    /// **봉인 시 기재하는 판은 그 열쇠를 도출한 판이다.**
    ///
    /// 상수를 기재하면, 구 판 파일을 신규 앱에서 개방해 1회 저장하는 것만으로
    /// 머리와 알맹이가 불일치해 영구히 개방되지 않는다.
    #[test]
    fn sealing_writes_the_container_the_key_came_from() {
        let mut v = seeded();
        assert_eq!(v.seal().unwrap()[7], v.container);

        // 판을 수동으로 변경하면 머리도 추종해야 한다(상수를 기재하지 않는다는 의미).
        v.container = 7;
        assert_eq!(v.seal().unwrap()[7], 7, "머리에 상수를 기재하고 있다");
    }

    /* ── 판 상승 시점을 사전에 실연한다 ──────────────────────────

       이 경로는 판이 둘이 되기 전에는 도달할 수 없다. 그런데 **여기서 2회 오류를
       범했다** — 개방하는 측이 판을 참조하지 않았고, 봉인하는 측이 상수를 기재했다.
       양자 모두 "그 시점" 에야 발현되고, 그 시점에 발현되면 사용자가 보유한 파일이
       전부 개방되지 않는다. 그래서 시험에서만 판을 하나 더 배치하고 실제로 실행한다. */

    /// 구 판 파일이 **신규 앱에서 개방되고, 저장하면 신판이 되고, 재개방된다.**
    #[test]
    fn an_older_container_opens_and_upgrades_when_saved() {
        const NEXT: u8 = 2;
        let sealed = seeded().seal().unwrap();
        assert_eq!(sealed[7], 1, "생성한 파일이 1판이 아니다");

        let upgraded = V::open_as(&sealed, PW, NEXT).expect("구 판 파일이 개방되지 않는다");
        assert_eq!(upgraded.container, NEXT, "개방하면서 판이 상승하지 않았다");
        assert_eq!(body(&upgraded), "비밀메모", "알맹이가 이전되지 않았다");

        let saved = upgraded.seal().unwrap();
        assert_eq!(saved[7], NEXT, "저장했는데 머리에 구 판이 기재되었다");

        // **그리고 재개방된다.** 머리와 열쇠가 불일치했다면 여기서 중단된다.
        let reopened = V::open_as(&saved, PW, NEXT).expect("저장한 파일이 개방되지 않는다");
        assert_eq!(reopened.container, NEXT);
    }

    /// 구 앱은 신판 파일을 **미지의 파일로 거절한다.** 강제로 해제하지 않는다.
    #[test]
    fn an_older_app_refuses_a_newer_container() {
        const NEXT: u8 = 2;
        let sealed = seeded().seal().unwrap();
        let saved = V::open_as(&sealed, PW, NEXT).unwrap().seal().unwrap();
        assert_eq!(V::open_as(&saved, PW, 1).unwrap_err().code, ErrorCode::VaultContainerUnknown);
    }

    /// 상승 이후 **저장하지 않으면 파일은 그대로**다. 개방했다는 사유로 타인의
    /// 파일이 변경되면 안 된다.
    #[test]
    fn opening_alone_does_not_rewrite_the_file() {
        let sealed = seeded().seal().unwrap();
        let before = sealed.clone();
        let _ = V::open_as(&sealed, PW, 2).unwrap();
        assert_eq!(sealed, before);
        // 구 앱이 여전히 개방한다 — 아무것도 변경되지 않았으므로.
        assert!(V::open_as(&sealed, PW, 1).is_ok());
    }

    /// 비밀번호 변경은 **그 금고의 판**으로 봉인한다. 상수를 사용하면 판이 분기한
    /// 금고에서 정확한 비밀번호를 오류로 판정하게 된다.
    #[test]
    fn rekeying_keeps_the_file_on_its_own_container() {
        const NEXT: u8 = 2;
        let sealed = seeded().seal().unwrap();
        // 실제로 상승시켜 개방한 금고를 사용한다. 판만 수동으로 변경하면 열쇠와
        // 불일치해 "정확한 비밀번호를 오류로 판정" 하는 지점을 포착하지 못한다.
        let mut v = V::open_as(&sealed, PW, NEXT).unwrap();
        let mut saved = Vec::new();
        v.rekey(PW, NEW_PW, |b| {
            saved = b.to_vec();
            Ok(())
        })
        .unwrap();
        assert_eq!(saved[7], NEXT, "변경 이후 판이 하강했다");
        assert_eq!(v.container, NEXT);
        assert!(V::open_as(&saved, NEW_PW, NEXT).is_ok());
    }

    /// 수용하는 DB 는 **임시 파일을 디스크에 생성하지 않는다.**
    #[test]
    fn the_database_keeps_its_scratch_in_memory() {
        let v = V::create(PW).unwrap();
        let mode: i64 = v.conn.query_row("PRAGMA temp_store", [], |r| r.get(0)).unwrap();
        assert_eq!(mode, 2, "임시 파일이 디스크로 반출될 수 있다 (2 = MEMORY)");
    }

    #[test]
    fn garbage_file_is_rejected_as_format_error() {
        assert_eq!(V::open(&[0u8; 200], PW).unwrap_err().code, ErrorCode::VaultFormatInvalid);
    }

    /// 타 앱의 매직을 부착한 파일은 우리 것이 아니다.
    #[test]
    fn another_apps_file_is_not_ours() {
        let mut sealed = seeded().seal().unwrap();
        sealed[..7].copy_from_slice(b"OTHERAP");
        assert_eq!(V::open(&sealed, PW).unwrap_err().code, ErrorCode::VaultFormatInvalid);
    }

    /// **이 기기에서 파일 하나 개방에 소요되는 시간.**
    ///
    /// ```text
    /// cargo test -p sheetsift-core --release opening_takes -- --ignored --nocapture
    /// ```
    ///
    /// 열쇠 도출 비용(`KDF_MEMORY_KIB`·`KDF_PASSES`)은 **모든 기기에서 동일해야
    /// 한다** — 저사양 기기에 맞춰 하향하면 그 값으로 생성한 파일은 어디서나 그만큼
    /// 취약하다. 그래서 값을 결정하기 전에 **최저속 기기**에서 측정한다.
    ///
    /// `--release` 로 측정해야 한다. 그대로 실행하면 사용자가 사용하지 않는 빌드를
    /// 측정하는 셈이다.
    #[test]
    #[ignore = "측정만 수행한다. 값을 결정하기 전에 저속 기기에서 실행한다."]
    fn opening_takes() {
        let v = V::create(PW).unwrap();
        v.conn
            .execute(
                "WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i+1 FROM n WHERE i<20000)
                 INSERT INTO notes(body) SELECT printf('줄 %d', i) FROM n",
                [],
            )
            .unwrap();
        let sealed = v.seal().unwrap();

        let ms = |t: std::time::Instant| t.elapsed().as_secs_f64() * 1000.0;

        let t = std::time::Instant::now();
        let opened = V::open(&sealed, PW).unwrap();
        let whole = ms(t);

        let t = std::time::Instant::now();
        derive_key(PW, &[7u8; SALT_LEN]).unwrap();
        let kdf = ms(t);

        let t = std::time::Instant::now();
        check_image::<Notes>(&opened.conn).unwrap();
        let image = ms(t);

        eprintln!(
            "\n  파일 {} KB · 개방 소요 {whole:.0} ms\n\
               그중 열쇠 도출 {kdf:.0} ms · 알맹이 검사 {image:.0} ms\n\
               (메모리 {} MiB · {}회)\n",
            sealed.len() / 1024,
            KDF_MEMORY_KIB / 1024,
            KDF_PASSES,
        );
    }
}
