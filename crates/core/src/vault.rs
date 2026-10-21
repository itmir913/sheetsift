//! 비밀번호로 봉한 SQLite.
//!
//! 내 파일을 내가 봉해 내가(또는 비밀번호를 건네받은 사람이) 다시 연다. 발행자가
//! 없으므로 서명이 없다 — 그쪽은 [`seal`](crate::seal) 이다.
//!
//! ## 왜 SQLCipher 를 쓰지 않았나
//! SQLCipher 는 Windows 에서 OpenSSL 빌드가 얽혀 CI 구성이 까다롭다.
//! **직렬화된 SQLite 바이트 전체를 AES-256-GCM 으로 감싸면** 된다.
//! 순수 Rust 로 끝나고, GCM 이라 위·변조 탐지도 따라온다.
//! 평문 SQLite 가 디스크에 남는 순간이 없다.
//!
//! ## 파일 형식
//! ```text
//! offset  size  내용
//!      0     7  매직 — 앱이 정한다([`Format::MAGIC`])
//!      7     1  container_version  그릇의 판 — 푸는 방법이 바뀌면 오른다
//!      8     4  schema_version (little endian)  안의 표가 바뀌면 오른다
//!     12    16  Argon2id 솔트
//!     28    12  AES-GCM 논스
//!     40     4  머리·알맹이의 CRC-32 — **깨진 것과 비밀번호를 가르는 데만 쓴다**
//!     44     …  AES-256-GCM(직렬화된 SQLite) + 인증 태그 16 B
//! ```
//!
//! **앞 40바이트(셈 앞까지)가 AAD 로 함께 인증된다.** 한 바이트만 고쳐도 복호화가
//! 실패한다. 셈이 든 네 바이트는 인증 밖이다 — 까닭은 봉하는 자리에 적는다.
//!
//! ## 깨진 파일과 틀린 비밀번호를 가른다
//!
//! 인증 태그는 **한 바이트만 달라도** 복호화를 거절한다. 그것이 암호가 하는 일이라
//! 어떤 설계로도 깨진 파일을 열 수는 없다. 그런데 그때 "비밀번호가 맞지 않거나
//! 파일이 손상되었습니다" 라고만 말하면, **맞는 비밀번호를 하루 종일 다시 쳐 보게
//! 된다.**
//!
//! 그래서 CRC 를 머리에 적어 둔다. 열 때 먼저 대 보아 다르면 **전송 중에 깨졌다**고
//! 말한다 — 파일을 다시 받으라는 말이 비밀번호를 의심하라는 말보다 훨씬 쓸모 있다.
//! CRC 는 암호문에서 계산한 값이라 비밀번호에 대해 아무것도 말하지 않는다(누구나
//! 다시 계산할 수 있다). 손보는 쪽이 CRC 까지 맞춰 오면 그때는 태그가 막고, 그건
//! 이미 "깨진 파일" 이 아니다.
//!
//! ## 표의 생김새는 앱이 안다
//!
//! 그릇은 SQLite 가 무엇을 담는지 모른다. 표를 만들고 올리는 법(마이그레이션)과
//! 풀어 낸 알맹이가 **우리가 만든 생김새인지** 견주는 법은 [`Format`] 으로 앱이
//! 넣는다. 그릇이 직접 보는 것은 장부가 앞뒤 맞는지(`integrity_check`)뿐이다.
//!
//! ## 열어 둔 채로는 잠그지 않는다 — 정한 것이다
//!
//! 얼마쯤 지나면 저절로 잠그는 길을 두지 않는다. 그 사이에 비밀번호를 다시 묻는
//! 프로그램은 쓰는 사람이 창을 닫아 두거나 더 쉬운 비밀번호를 고르게 만든다. 열어
//! 둔 자리의 보안은 화면 잠금이 진다.

use std::marker::PhantomData;

use argon2::Argon2;
use rand::RngCore;
use rusqlite::Connection;
use zeroize::Zeroize;

use crate::{Error, ErrorCode, Result};

/// 매직의 길이. 머리 배치가 이 값에 묶여 있다.
pub const MAGIC_LEN: usize = 7;

/// **그릇의 판.** 머리 모양이나 열쇠 뽑는 법이 바뀌면 오른다.
pub const CONTAINER_VERSION: u8 = 1;

const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 12;
const CRC_LEN: usize = 4;
const HEADER_LEN: usize = MAGIC_LEN + 1 + 4 + SALT_LEN + NONCE_LEN + CRC_LEN;
/// GCM 태그. 알맹이가 비어 있어도 이만큼은 있다.
const TAG_LEN: usize = 16;

/// 비밀번호의 최소 길이(표기를 맞춘 뒤의 글자 수).
pub const MIN_PASSWORD_LEN: usize = 8;

/// **이 그릇에 무엇이 담기는가.** 앱이 정한다.
///
/// 매직과 스키마 판, 그리고 표를 다루는 두 길이다. 그릇의 코드는 한 벌이고, 담는
/// 것만 앱마다 다르다.
pub trait Format {
    /// 앱의 오류. 그릇이 내는 [`Error`] 를 받아들일 수 있어야 한다.
    type Error: From<Error>;

    /// 파일 맨 앞의 일곱 바이트. 사람이 주고받는 파일이라 이름표 노릇을 한다.
    const MAGIC: [u8; MAGIC_LEN];

    /// 이 앱이 아는 가장 높은 스키마 판. 머리에 적힌 판이 이보다 높으면 **풀기
    /// 전에** 거절한다.
    const SCHEMA_VERSION: u32;

    /// 표를 만들거나 옛 판에서 올린다. 빈 DB 에도, 풀어 낸 옛 파일에도 불린다.
    /// 끝나면 `PRAGMA user_version` 이 [`Self::SCHEMA_VERSION`] 이어야 한다.
    fn migrate(conn: &Connection) -> std::result::Result<(), Self::Error>;

    /// 풀어 낸 알맹이가 **우리가 만든 생김새인지** 본다. 표를 뷰로 바꿔치우거나
    /// 트리거를 심은 것은 `integrity_check` 가 못 잡는다.
    fn check(conn: &Connection) -> std::result::Result<(), Self::Error>;
}

/* 열쇠를 뽑는 비용. **크레이트 기본값에 기대지 않는다.**

   `Argon2::default()` 는 그때그때의 기본값을 쓴다. argon2 는 실제로 한 번 기본을
   바꿨고(m=4096·t=3 → m=19456·t=2), 또 바뀌면 **같은 비밀번호에서 다른 열쇠가
   나온다.** 3월에 만든 파일을 11월에 열면 `비밀번호가 맞지 않거나 파일이
   손상되었습니다` 가 뜨는데 둘 다 아니다. 아무도 원인을 찾지 못한다.

   64 MiB · 3회는 RFC 9106 이 두 번째로 권하는 값이고, OWASP 최소치(12 MiB·3회)의
   다섯 배가 넘는다. 갈래를 하나로 두는 까닭은 우리가 한 줄로 계산하기 때문이다 —
   갈래를 늘리면 우리는 그만큼 느려지지 않는데 여러 코어를 가진 공격자만 이득을
   본다.

   **새어 나간 파일에 대한 현실적인 공격은 오프라인 추측 하나뿐이다.** 여는 데
   걸리는 시간은 파일을 열 때 한 번이고, 추측하는 쪽은 그 시간을 수억 번 치러야
   한다. 그 거래는 언제나 우리 쪽이 남는다.

   올리려면 그릇의 판을 함께 올려야 한다 — 옛 파일은 옛 비용으로 뽑아야 열린다. */
const KDF_MEMORY_KIB: u32 = 65_536;
const KDF_PASSES: u32 = 3;
const KDF_LANES: u32 = 1;

/// 그릇 판마다의 열쇠 뽑는 법. **`CONTAINERS[v-1]` 이 판 v 의 것이다.**
///
/// 비용을 못 박아 두는 것만으로는 부족하다. 나중에 비용을 올리려면 판을 올려야
/// 하는데, **여는 쪽이 판을 보고 갈라 쓰지 않으면** 이미 나간 파일이 새 비용으로
/// 풀려 전부 안 열린다. 그때 화면에 뜨는 말은 "비밀번호가 맞지 않거나…" 다.
///
/// 그래서 판을 올리는 길을 지금 뚫어 둔다. 올릴 때 하는 일은 이 배열에 한 줄
/// 더하고 `CONTAINER_VERSION` 을 올리는 것뿐이고, **옛 파일은 옛 줄로 열린다.**
#[cfg(not(test))]
const CONTAINERS: &[Kdf] = &[Kdf { memory_kib: KDF_MEMORY_KIB, passes: KDF_PASSES, lanes: KDF_LANES }];

/// 시험에서는 판이 하나 더 있다. **판을 올리는 날을 미리 치러 보려는 것**이다.
///
/// 둘째 줄의 값은 첫째와 달라야 뜻이 있다(열쇠가 실제로 달라져야 한다). 비용은
/// 일부러 낮게 잡는다 — 시험이 기다릴 까닭이 없다.
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

/// 판 수와 비용 줄 수가 어긋나면 **컴파일이 선다.** (시험에서는 한 줄이 더 있다.)
#[cfg(not(test))]
const _: () = assert!(
    CONTAINER_VERSION as usize == CONTAINERS.len(),
    "그릇 판마다 열쇠 뽑는 법이 한 줄씩 있어야 한다"
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

/// 비밀번호의 유니코드 표기를 한 가지로 맞춘다.
///
/// **같은 글자가 두 바이트열로 적힌다.** `한` 은 윈도우 자판에서 완성형 한 자
/// (NFC, 3바이트)로, 맥 자판·맥에서 복사한 글에서는 자모 셋(NFD, 9바이트)으로
/// 온다. 눈으로는 같은데 열쇠가 다르다. 맥에서 만든 파일을 윈도우에서 못 열고,
/// 그때 뜨는 말은 `비밀번호가 맞지 않거나 파일이 손상되었습니다` 다.
///
/// 비밀번호는 흔히 **파일과 다른 경로로**(문자·메신저) 건너간다. 그 글이 어느
/// 표기로 도착할지 우리가 정할 수 없다. 그래서 받는 쪽에서 맞춘다.
fn normalize(password: &str) -> String {
    use unicode_normalization::UnicodeNormalization;
    // **자리를 한 번에 잡는다.** `collect()` 에 맡기면 8→16→32→… 로 옮겨 다니며,
    // 옮기기 전 버퍼에 비밀번호 앞부분이 **지워지지 않은 채** 해제된다.
    //
    // 세 배로 잡는 까닭: NFC 는 대개 줄이지만 늘리는 글자도 있다(`U+0344` 는 두
    // 배가 된다). 모자라면 다시 옮겨 다녀 막으려던 일이 그대로 일어난다.
    let mut out = String::with_capacity(password.len() * 3);
    out.extend(password.nfc());
    out
}

fn derive_key(password: &str, salt: &[u8]) -> Result<zeroize::Zeroizing<[u8; 32]>> {
    derive_key_for(CONTAINER_VERSION, password, salt)
}

/// 판 `container` 의 법으로 열쇠를 뽑는다. 옛 파일은 옛 법으로 푼다.
///
/// **감싸서 돌려준다.** `[u8; 32]` 는 Copy 라 그냥 돌려주면 이 함수의 자리에 사본이
/// 남는다 — 잠근 뒤에도 스택에 열쇠가 있는 셈이다. `Zeroizing` 은 떨어질 때 지운다.
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

/// `PRAGMA user_version`. 머리에 적는 스키마 판의 근거다.
pub fn user_version(conn: &Connection) -> Result<u32> {
    let v: i64 = conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .map_err(|e| Error::new(ErrorCode::VaultOpenFailed).detail(e))?;
    Ok(v.max(0) as u32)
}

/// 풀어 낸 SQLite 알맹이가 **성한 것인지** 본다.
///
/// 태그가 맞았다는 것은 "이 파일을 만든 사람이 비밀번호를 알았다" 까지만 말한다.
/// 그런데 **파일과 비밀번호를 함께 건네는 것이 정상 흐름**이라, 받은 사람이 손본
/// 파일을 되돌려 주는 길이 열려 있다. SQLite 는 조작된 b-tree 이미지에 대해 스스로
/// 안전을 보장하지 않는다고 문서에 적어 두었다.
///
/// `DEFENSIVE` 는 SQL 로 스키마를 직접 건드리는 길을 막고, `integrity_check` 는
/// 장부가 앞뒤가 맞는지 훑는다. **담긴 것이 우리 것인지**는 그다음 [`Format::check`]
/// 가 본다 — 장부가 성한 것과 생김새가 우리 것인 것은 다른 말이다.
fn check_image<F: Format>(conn: &Connection) -> std::result::Result<(), F::Error> {
    let verdict: String = conn
        .query_row("PRAGMA integrity_check", [], |r| r.get(0))
        .map_err(|e| Error::new(ErrorCode::VaultFormatInvalid).detail(e))?;
    if verdict != "ok" {
        return Err(Error::new(ErrorCode::VaultFormatInvalid).detail(verdict).into());
    }
    F::check(conn)
}

/// 알맹이를 담는 메모리 DB. **임시 파일을 디스크에 만들지 않는다.**
///
/// SQLite 는 정렬이나 임시 인덱스가 캐시를 넘기면 `%TEMP%` 에 임시 파일을 만든다.
/// 그 안에는 담긴 것이 평문으로 들어간다. 이 파일이 통째로 암호화된다는 말은 이 한
/// 줄이 있어야 참이 된다.
fn memory_db() -> Result<Connection> {
    let conn = Connection::open_in_memory()
        .map_err(|e| Error::new(ErrorCode::VaultOpenFailed).detail(e))?;
    /* `foreign_keys` 는 SQLite 에서 **기본이 꺼짐**이다. 스키마에 `ON DELETE
       CASCADE` 를 적어 두어도 켜지 않으면 그 선언이 아무 일도 하지 않는다. 지운
       줄에 딸린 줄이 파일 안에 고아로 남는다 — 개인정보 파일에서 삭제가 삭제가
       아니게 된다.

       이미 든 자료에는 영향이 없다(이 설정은 앞으로의 쓰기에만 걸린다). 그래서
       손에 있는 파일이 이것 때문에 안 열리는 일은 없다. */
    conn.execute_batch("PRAGMA temp_store = MEMORY; PRAGMA foreign_keys = ON;")
        .map_err(|e| Error::new(ErrorCode::VaultOpenFailed).detail(e))?;

    // **만드는 길에도 건다.** 여는 길에만 걸면, 같은 자료를 담는 같은 연결인데
    // 어느 길로 왔느냐에 따라 깃발이 달라진다. 켜졌는지도 확인한다 —
    // `set_db_config` 는 **적용된 값**을 돌려준다.
    let on = conn
        .set_db_config(rusqlite::config::DbConfig::SQLITE_DBCONFIG_DEFENSIVE, true)
        .map_err(|e| Error::new(ErrorCode::VaultOpenFailed).detail(e))?;
    if !on {
        return Err(Error::new(ErrorCode::VaultOpenFailed).detail("defensive mode refused"));
    }
    Ok(conn)
}

/// 파일이 오는 길에 깨졌는지 보는 셈. **암호가 아니라 진단이다.**
///
/// CRC-32(IEEE). 바이트가 바뀌거나 잘려 나가면 사실상 언제나 걸린다. 일부러 맞춰
/// 오는 것을 막는 장치가 아니다 — 그건 인증 태그가 한다.
///
/// **머리와 알맹이를 함께 센다.** 알맹이만 세면 솔트나 논스가 깨진 파일이
/// "비밀번호가 맞지 않습니다" 로, 판 칸이 깨진 파일이 "최신 프로그램을 요청하세요"
/// 로 나간다 — 이 셈을 넣은 이유가 바로 그 두 문구를 없애자는 것이다. 제 셈이 든
/// 네 바이트만 뺀다.
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
    // 길이도 **맞춘 뒤에** 센다. NFD 로 온 `한글비밀번호` 는 자모 하나하나가
    // 글자로 세어져, 같은 비밀번호가 표기에 따라 되기도 하고 안 되기도 한다.
    let mut normalized = normalize(password);
    let count = normalized.chars().count();
    normalized.zeroize();
    if count < MIN_PASSWORD_LEN {
        return Err(Error::new(ErrorCode::VaultPasswordTooShort).with("min", MIN_PASSWORD_LEN));
    }
    Ok(())
}

/// 열려 있는 봉투. 메모리에만 존재한다.
pub struct Vault<F: Format> {
    pub conn: Connection,
    /// 다시 봉인할 때 쓰는 키. Drop 에서 지운다.
    key: [u8; 32],
    /// 키를 유도한 솔트. 같은 키를 계속 쓰려면 함께 보관해야 한다.
    salt: [u8; SALT_LEN],
    /// **이 열쇠를 뽑은 그릇의 판.** 봉할 때 머리에 적는 값이다.
    ///
    /// 상수를 적어 넣으면 안 된다. 옛 판 파일을 새 앱에서 열면 열쇠는 옛 비용으로
    /// 뽑혀 있는데 머리에는 새 판이 적히고, 다음에 열 때 새 비용으로 뽑아 열쇠가
    /// 달라진다 — 한 번 저장하는 것만으로 파일이 영영 안 열린다.
    container: u8,
    pub schema_version: u32,
    format: PhantomData<F>,
}

impl<F: Format> Drop for Vault<F> {
    fn drop(&mut self) {
        self.key.zeroize();
    }
}

/// 키·솔트가 로그나 패닉 메시지로 새지 않도록 직접 구현한다.
impl<F: Format> std::fmt::Debug for Vault<F> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Vault")
            .field("schema_version", &self.schema_version)
            .field("key", &"<redacted>")
            .finish()
    }
}

impl<F: Format> Vault<F> {
    /// 빈 봉투를 새로 만든다.
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

    /// 파일 내용을 복호화해 메모리에서 연다.
    pub fn open(bytes: &[u8], password: &str) -> std::result::Result<Self, F::Error> {
        Self::open_as(bytes, password, CONTAINER_VERSION)
    }

    /// 여는 길. **이 앱이 몇 판인지를 받는다.**
    ///
    /// 늘 `CONTAINER_VERSION` 이다. 판을 올리는 길(옛 그릇을 열어 새 그릇으로
    /// 올리는 것)은 판이 둘이 되기 전에는 도달할 수 없어, 그 길만 시험이 지나갈 수
    /// 있게 자리를 열어 둔다. **두 번이나 이 자리에서 틀렸다** — 한 번은 여는 쪽이
    /// 판을 안 보았고, 한 번은 봉하는 쪽이 상수를 적었다. 다음에 판을 올리는 사람이
    /// 손으로 확인하기를 기대하지 않는다.
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
        /* 그릇의 판. **아는 판이 아니면 푸는 방법을 모르는 파일이다.**

           판 0 인 그릇은 존재한 적이 없으므로, 0 을 적어 둔 파일은 더 새것도
           옛것도 아니고 그냥 우리 것이 아니다. 한 바이트가 망가진 파일도 여기서
           걸린다 — 그때 "더 새 프로그램을 구하세요" 라고 말하면 있지도 않은 것을
           찾으러 간다. */
        let container = bytes[MAGIC_LEN];
        if container == 0 || container > app_container {
            return Err(Error::new(ErrorCode::VaultContainerUnknown)
                .with("fileVersion", container)
                .with("appVersion", app_container)
                .into());
        }
        // 인증하는 머리는 셈 앞까지다(봉하는 쪽과 같아야 한다).
        let header = &bytes[..HEADER_LEN - CRC_LEN];
        let ciphertext = &bytes[HEADER_LEN..];

        /* **오는 길에 깨졌는지 먼저 본다.** 태그가 막는 것과 같은 결과지만 말이
           다르다 — 여기서 걸리면 "파일을 다시 받으세요" 이고, 태그에서 걸리면
           "비밀번호를 확인하세요" 다.

           **그릇 판 검사 뒤에 둔다.** 판이 오르면 머리 모양이 달라질 수 있는데, 그때
           옛 앱이 새 파일을 "손상" 이라 부르면 안 된다. 그건 모르는 파일이지 깨진
           파일이 아니다. 반대로 **스키마 판 검사보다는 앞이다** — 그 칸이 깨진 것을
           "더 새 프로그램을 구하세요" 라고 말하면 없는 것을 찾으러 간다. */
        let recorded = u32::from_le_bytes(
            bytes[HEADER_LEN - CRC_LEN..HEADER_LEN].try_into().expect("length checked above"),
        );
        if crc32(&[header, ciphertext]) != recorded {
            return Err(Error::new(ErrorCode::VaultFileDamaged).into());
        }

        // 머리에 적힌 스키마 판. **알맹이를 풀기 전에** 상위 판을 걸러 낸다 —
        // 모르는 구조를 애써 풀어 놓고 그제야 막을 까닭이 없다.
        //
        // 함께 인증되므로 **위조는 못 한다.** 근거는 어디까지나 복호화 뒤에 읽는
        // DB 안의 `user_version` 이고, 이건 미리 거르려고 두는 사본이다.
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
        // 담을 빈 그릇을 먼저 연다.
        let mut conn = memory_db()?;

        // **그 파일의 판으로 뽑는다.** 비용이 판마다 다를 수 있다.
        let key = derive_key_for(container, password, &salt)?;
        /* **스스로 지워지는 그릇에 담는다.** 여기 든 것은 평문 DB 다. 손으로 지우는
           줄을 두었을 때 두 번 틀렸다 — 한 번은 `?` 가 그 줄 앞에서 돌아서, 한 번은
           그릇을 여는 `?` 가 그 줄 앞에서 돌아서. 어느 쪽도 시험이 보지 못한다.
           지우는 자리를 옳게 고르는 대신 **고를 일을 없앤다**: `Zeroizing` 은 어떤
           길로 함수를 빠져나가든, 되감기는 패닉에서도 지운다. */
        let plain = zeroize::Zeroizing::new(
            sheetsift_aead::open(&key, &nonce, header, ciphertext)
                // 비밀번호 오류와 파일 손상은 구분할 수 없다. 구분해서도 안 된다.
                .ok_or_else(|| Error::new(ErrorCode::VaultPasswordWrong))?,
        );

        let len = plain.len();
        conn.deserialize_read_exact("main", std::io::Cursor::new(&*plain), len, false)
            .map_err(|e| Error::new(ErrorCode::VaultOpenFailed).detail(e))?;

        check_image::<F>(&conn)?;

        // 여기서부터 근거는 DB 안의 판이다. 옛 파일은 막지 않고 올린다 —
        // 오래전에 만든 파일을 여는 일이 보통이다.
        F::migrate(&conn)?;
        let schema_version = user_version(&conn)?;

        /* **옛 그릇은 열면서 새 그릇으로 올린다.** 비밀번호가 여기 있을 때만 할 수
           있는 일이다 — 저장할 때는 이미 없다. 올린 열쇠는 다음 저장부터 쓰이고,
           저장하지 않으면 파일은 그대로다. */
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

    /// 현재 내용을 암호화한 바이트로 만든다. 파일 쓰기는 호출 쪽이 한다.
    /// 논스는 저장할 때마다 새로 뽑는다(같은 키로 논스를 재사용하지 않는다).
    pub fn seal(&self) -> Result<Vec<u8>> {
        seal_with(&self.conn, F::MAGIC, self.container, &self.key, &self.salt)
    }

    /// **다른 연결을 이 봉투의 열쇠로** 봉한다. 앱의 시험이 손본 파일을 만들 때만
    /// 쓴다.
    ///
    /// 비밀번호를 아는 사람이 할 수 있는 일이 꼭 그만큼이다 — 풀어서 손보고 같은
    /// 비밀번호로 다시 봉한다. 그런 파일이 [`Format::check`] 에 걸리는지 보려면
    /// 그 파일을 만들 수 있어야 한다. 우리 연결은 `DEFENSIVE` 라 스키마를 손볼 수
    /// 없으므로 맨 연결을 받는다.
    #[doc(hidden)]
    pub fn seal_connection(&self, conn: &Connection) -> Result<Vec<u8>> {
        seal_with(conn, F::MAGIC, self.container, &self.key, &self.salt)
    }

    /// 비밀번호를 바꾸고, 그 자리에서 파일까지 쓴다.
    ///
    /// ## 왜 쓰기를 여기서 받나
    ///
    /// 키만 갈아 끼우고 저장을 나중에 받으면, **파일은 옛 비밀번호로 봉해진 채
    /// 앱만 새 비밀번호를 쥔 상태**가 생긴다. 바꿨다고 알고 있는데 파일은 안 바뀐
    /// 것이다. 그래서 쓰기까지가 한 동작이다.
    ///
    /// 차례도 뒤집을 수 없다. **쓰기가 끝난 뒤에야** `self` 의 키를 간다 —
    /// 디스크가 가득 차 쓰기가 실패했는데 앱만 새 키를 쥐고 있으면, 다음
    /// 저장이 아무도 모르는 비밀번호로 파일을 덮어쓴다.
    ///
    /// 현재 비밀번호를 다시 묻는 까닭은, 파일을 열어 둔 채 자리를 비우는 일이
    /// 흔해서다. 옆에 앉은 사람이 비밀번호만 바꿔 잠가 버릴 수 있으면 안 된다.
    pub fn rekey(
        &mut self,
        current: &str,
        next: &str,
        write: impl FnOnce(&[u8]) -> std::result::Result<(), F::Error>,
    ) -> std::result::Result<(), F::Error> {
        // **이 금고의 판으로 뽑는다.** `self.key` 가 그 판으로 뽑힌 것이라, 상수로
        // 뽑으면 판이 갈린 금고에서 맞는 비밀번호를 틀렸다고 하게 된다.
        let proof = derive_key_for(self.container, current, &self.salt)?;
        if !constant_time_eq(&proof, &self.key) {
            return Err(Error::new(ErrorCode::VaultPasswordWrong).into());
        }
        check_password(next)?;

        // 솔트도 새로 뽑는다. 같은 솔트를 이어 쓰면 옛 비밀번호로 만든 무지개표가
        // 그대로 먹힌다.
        let mut salt = [0u8; SALT_LEN];
        rand::rng().fill_bytes(&mut salt);
        // **이 금고의 판으로 뽑고 그 판으로 봉한다.** 상수를 쓰면 판이 갈린
        // 금고에서 뽑은 열쇠와 적은 판이 어긋난다 — 이 자리에서 이미 두 번 틀렸다.
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
    /* **이 평문은 지울 수 없다.** `serialize` 는 먼저 `NOCOPY` 로 SQLite 가 쥐고
       있는 **살아 있는 페이지**를 가리키게 한다. 여기서 지우면 열려 있는 DB 가
       그 자리에서 깨진다. 사본이 오는 경우에도 `&mut` 로 받을 길이 없다.

       그래서 앱이 도는 동안 담긴 자료는 메모리에 평문으로 있다 — 그것이 설계다
       (파일만 암호화한다). 잠근 뒤에 남는 것을 줄이려면 SQLite 의 할당기를 갈아야
       하고, 그건 이 그릇이 답할 수 있는 물음이 아니다. */
    let data = conn
        .serialize("main")
        .map_err(|e| Error::new(ErrorCode::VaultSealFailed).detail(e))?;

    let mut nonce = [0u8; NONCE_LEN];
    rand::rng().fill_bytes(&mut nonce);

    // 머리에 적는 판은 **DB 에서 읽어 온다.** 상수를 적어 넣으면 알맹이가
    // 옛 판인 채로 머리만 새 판이 되고, 그 어긋남은 아무도 못 본다.
    let version = user_version(conn)?;

    let mut out = Vec::with_capacity(HEADER_LEN + data.len() + TAG_LEN);
    out.extend_from_slice(&magic);
    // **상수가 아니라 이 열쇠를 뽑은 판을 적는다.** 둘이 어긋나면 다음에 열 때
    // 다른 비용으로 뽑아 열쇠가 달라진다.
    out.push(container);
    out.extend_from_slice(&version.to_le_bytes());
    out.extend_from_slice(salt);
    out.extend_from_slice(&nonce);

    /* **인증 범위는 CRC 앞까지다.** 넣을 수가 없다 — CRC 는 알맹이를 만든 뒤에야
       나오는데, 그 알맹이가 머리를 AAD 로 삼아 만들어지기 때문이다. 서로가 서로를
       기다린다.

       빼도 잃는 것이 없다. CRC 는 **깨진 것과 틀린 비밀번호를 가르는 말**일 뿐이고,
       거기를 손대 봐야 "손상되었습니다" 와 "비밀번호가 맞지 않습니다" 가 뒤바뀔
       뿐이다. 위조는 여전히 태그가 막는다. */
    let ct = sheetsift_aead::seal(key, &nonce, &out, &data)
        .ok_or_else(|| Error::new(ErrorCode::VaultSealFailed).detail("could not seal"))?;
    let crc = crc32(&[&out, &ct]);
    out.extend_from_slice(&crc.to_le_bytes());
    out.extend_from_slice(&ct);
    Ok(out)
}

/// 길이가 같은 두 바이트열을 **끝까지** 견준다. 다른 자리를 만나 곧바로
/// 돌아가면 걸린 시간이 "몇 글자까지 맞았는지" 를 말해 준다.
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

        /// 표가 `notes` 하나뿐인지만 본다.
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

    /// 셈을 다시 맞춘다. 손보는 쪽이 할 수 있는 일이다.
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

    /// **오는 길에 깨진 것과 비밀번호가 틀린 것을 가른다.**
    ///
    /// 둘 다 열리지 않는 것은 같다 — 암호가 그렇게 만들어져 있다. 다른 것은 하는
    /// 말이다. 맞는 비밀번호를 쥔 사람에게 "비밀번호를 확인하세요" 라고 하면 하루를
    /// 엉뚱한 데서 보낸다.
    #[test]
    fn a_file_damaged_in_transit_says_so_instead_of_blaming_the_password() {
        let mut sealed = seeded().seal().unwrap();
        let last = sealed.len() - 30;
        sealed[last] ^= 0xff;
        assert_eq!(
            V::open(&sealed, PW).unwrap_err().code,
            ErrorCode::VaultFileDamaged,
            "깨진 파일을 비밀번호 탓으로 돌린다"
        );

        // 잘려서 온 파일도 마찬가지다(클라우드가 반만 내려 주는 일이 있다).
        let mut cut = seeded().seal().unwrap();
        cut.truncate(cut.len() - 40);
        assert_eq!(V::open(&cut, PW).unwrap_err().code, ErrorCode::VaultFileDamaged);
    }

    /// **셈까지 맞춰 온 위조는 태그가 막는다.** 그때는 깨진 파일이 아니다.
    #[test]
    fn tampering_is_detected() {
        let mut sealed = seeded().seal().unwrap();
        let last = sealed.len() - 30;
        sealed[last] ^= 0xff;
        refix_crc(&mut sealed);
        assert_eq!(V::open(&sealed, PW).unwrap_err().code, ErrorCode::VaultPasswordWrong);
    }

    /// **머리가 깨진 것도 깨졌다고 말한다.**
    ///
    /// 솔트·논스·판 칸은 오는 길에 깨지기 쉬운 자리인데, 셈이 알맹이만 덮으면
    /// 이것이 "비밀번호가 맞지 않습니다" 로 나간다.
    #[test]
    fn a_damaged_header_says_damaged_not_wrong_password() {
        for at in [8, 12, 20, 28, 39] {
            let mut sealed = seeded().seal().unwrap();
            sealed[at] ^= 0x01;
            assert_eq!(
                V::open(&sealed, PW).unwrap_err().code,
                ErrorCode::VaultFileDamaged,
                "{at}번 바이트가 깨졌는데 손상이라 말하지 않는다"
            );
        }
    }

    /// 머리를 **셈까지 맞춰** 고쳐 오면 그때는 태그가 막는다. AAD 가 하는 일이다.
    #[test]
    fn header_tampering_is_detected() {
        let mut sealed = seeded().seal().unwrap();
        sealed[20] ^= 0x01; // 솔트를 건드린다 → 키가 달라지므로 실패
        refix_crc(&mut sealed);
        assert_eq!(V::open(&sealed, PW).unwrap_err().code, ErrorCode::VaultPasswordWrong);
    }

    #[test]
    fn plaintext_never_appears_in_the_file() {
        let sealed = seeded().seal().unwrap();
        assert!(!sealed.starts_with(b"SQLite format 3\0"), "평문 SQLite 헤더가 보인다");
        let needle = "비밀메모".as_bytes();
        assert!(!sealed.windows(needle.len()).any(|w| w == needle), "담긴 글이 평문으로 남아 있다");
    }

    #[test]
    fn short_password_is_rejected() {
        assert_eq!(V::create("1234").unwrap_err().code, ErrorCode::VaultPasswordTooShort);
    }

    /// 자모로 적어 여덟 자를 넘기는 비밀번호는 **맞춘 뒤에** 세면 모자란다.
    #[test]
    fn the_length_is_counted_after_normalizing() {
        use unicode_normalization::UnicodeNormalization;
        let decomposed: String = "한글비밀".nfd().collect();
        assert!(decomposed.chars().count() >= MIN_PASSWORD_LEN, "자모로 갈리지 않았다");
        assert_eq!(V::create(&decomposed).unwrap_err().code, ErrorCode::VaultPasswordTooShort);
    }

    #[test]
    fn future_schema_version_is_rejected() {
        let mut sealed = seeded().seal().unwrap();
        sealed[8..12].copy_from_slice(&(Notes::SCHEMA_VERSION + 5).to_le_bytes());
        // 셈을 맞춰 둔다 — 그러지 않으면 "깨진 파일" 로 먼저 걸린다. 여기서 보려는
        // 것은 **성한 파일이 더 새 판일 때** 무슨 말을 하는가다.
        refix_crc(&mut sealed);
        assert_eq!(V::open(&sealed, PW).unwrap_err().code, ErrorCode::VaultVersionTooNew);
    }

    /// 봉할 때 머리에 적는 판은 DB 에서 읽는다. 상수를 적어 넣으면 알맹이와
    /// 머리가 어긋나고, 그 어긋남은 아무도 못 본다.
    #[test]
    fn the_header_version_comes_from_the_database() {
        let v = seeded();
        let bare = v.seal_connection(&Connection::open_in_memory().unwrap()).unwrap();
        assert_eq!(u32::from_le_bytes(bare[8..12].try_into().unwrap()), 0);
        // 옛 판(0) 파일은 막지 않고 올려서 연다.
        let opened = V::open(&bare, PW).unwrap();
        assert_eq!(opened.schema_version, Notes::SCHEMA_VERSION, "열면서 판을 올리지 않았다");
    }

    /// 앱이 견주는 생김새에 어긋나면 열지 않는다.
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

    /// 비밀번호를 바꾸면 **새 것으로 열리고 옛 것으로는 안 열린다.**
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
        assert_eq!(body(&opened), "비밀메모", "알맹이가 함께 넘어오지 않았다");
        assert_eq!(
            V::open(&saved, PW).unwrap_err().code,
            ErrorCode::VaultPasswordWrong,
            "옛 비밀번호로 아직 열린다"
        );
    }

    /// 바꾼 뒤 그냥 저장해도 새 비밀번호로 봉해져야 한다. 키를 실제로
    /// 갈아 끼웠는지 보는 자리다.
    #[test]
    fn saving_after_a_rekey_uses_the_new_password() {
        let mut v = seeded();
        v.rekey(PW, NEW_PW, |_| Ok(())).unwrap();
        assert!(V::open(&v.seal().unwrap(), NEW_PW).is_ok());
    }

    #[test]
    fn rekeying_needs_the_current_password() {
        let mut v = seeded();
        let e = v.rekey("틀린비밀번호", NEW_PW, |_| panic!("쓰기까지 갔다")).unwrap_err();
        assert_eq!(e.code, ErrorCode::VaultPasswordWrong);
        assert!(V::open(&v.seal().unwrap(), PW).is_ok(), "키가 바뀌었다");
    }

    #[test]
    fn a_short_new_password_is_rejected() {
        let mut v = seeded();
        let e = v.rekey(PW, "1234", |_| panic!("쓰기까지 갔다")).unwrap_err();
        assert_eq!(e.code, ErrorCode::VaultPasswordTooShort);
    }

    /// **쓰기가 실패하면 키도 그대로다.** 파일은 옛 비밀번호로 봉해져 있는데
    /// 앱만 새 키를 쥐고 있으면, 다음 저장이 아무도 모르는 비밀번호로 덮는다.
    #[test]
    fn a_failed_write_leaves_the_old_password_in_force() {
        let mut v = seeded();
        let e = v
            .rekey(PW, NEW_PW, |_| {
                Err(Error::new(ErrorCode::VaultWriteFailed).detail("디스크가 가득 찼다"))
            })
            .unwrap_err();
        assert_eq!(e.code, ErrorCode::VaultWriteFailed);

        let sealed = v.seal().unwrap();
        assert!(V::open(&sealed, PW).is_ok(), "쓰기가 실패했는데 키가 바뀌었다");
        assert_eq!(V::open(&sealed, NEW_PW).unwrap_err().code, ErrorCode::VaultPasswordWrong);
    }

    /* ── 이미 나간 그릇을 못 박는 자리 ───────────────────────────

       아래는 **한 번 배포되면 손댈 수 없는 것**을 지킨다. 값을 고치는 손이
       그럴듯한 까닭을 갖고 있어도(더 센 암호, 더 짧은 머리) 사람 손의 파일은
       그 손을 따라가지 못한다. 그래서 사람의 판단이 아니라 시험이 막는다.
       앱은 제 매직과 이미 나간 파일로 같은 것을 한 번 더 본다. */

    /// 머리의 생김새. **자리 하나하나가 약속이다.**
    ///
    /// 칸을 하나 늘리거나 차례를 바꾸면 이미 나간 파일은 영영 안 열린다. 숫자를
    /// 여기 적어 두는 것은 문서가 아니라 **자물쇠**다.
    #[test]
    fn the_header_layout_is_pinned() {
        assert_eq!(MAGIC_LEN, 7);
        assert_eq!(CONTAINER_VERSION, 1);
        assert_eq!(SALT_LEN, 16);
        assert_eq!(NONCE_LEN, 12);
        assert_eq!(TAG_LEN, 16);
        assert_eq!(CRC_LEN, 4);
        assert_eq!(HEADER_LEN, 44, "머리 길이가 달라졌습니다");

        // 자리까지 본다. 길이의 합이 같아도 차례가 바뀌면 못 읽는다.
        let v = seeded();
        let sealed = v.seal().unwrap();
        assert_eq!(&sealed[..7], &Notes::MAGIC);
        assert_eq!(sealed[7], CONTAINER_VERSION);
        assert_eq!(u32::from_le_bytes(sealed[8..12].try_into().unwrap()), Notes::SCHEMA_VERSION);
        assert_eq!(&sealed[12..28], &v.salt, "솔트 자리");
        assert_eq!(
            u32::from_le_bytes(sealed[40..44].try_into().unwrap()),
            crc32(&[&sealed[..HEADER_LEN - CRC_LEN], &sealed[HEADER_LEN..]]),
            "머리에 적힌 셈이 머리·알맹이와 맞지 않는다"
        );
        let image = v.conn.serialize("main").unwrap().len();
        assert_eq!(sealed.len(), HEADER_LEN + TAG_LEN + image);
    }

    /// **열쇠를 뽑는 방법이 굳어 있는가.**
    ///
    /// 같은 비밀번호·같은 솔트에서 늘 같은 열쇠가 나와야 한다. 알고리즘·판·비용
    /// 어느 하나라도 바뀌면 여기서 선다 — 크레이트가 기본값을 바꿔도 마찬가지다.
    /// 이 시험이 서면 **이미 나간 파일이 안 열린다는 뜻**이고, 화면에는 "비밀번호가
    /// 맞지 않거나…" 가 뜬다. 그 자리를 여기서 잡는다.
    #[test]
    fn the_key_recipe_is_pinned() {
        let key = derive_key("학교비밀번호2027", b"goe-navi-salt-16").unwrap();
        assert_eq!(
            hex(&*key),
            "dd5dcf8533250488be80b1df35c8df8c4653bec6e14403e20778576549c2993f",
            "\n열쇠를 뽑는 방법이 달라졌습니다. 비용·알고리즘·판 중 무엇이 바뀌었는지\n\
             확인하세요. 이미 나간 파일이 있으면 그 파일들은 열리지 않습니다.\n"
        );
        let p = kdf_for(CONTAINER_VERSION).unwrap().params().clone();
        assert_eq!((p.m_cost(), p.t_cost(), p.p_cost()), (KDF_MEMORY_KIB, KDF_PASSES, KDF_LANES));
    }

    /// **자모로 적은 비밀번호로도 열린다.** 맥 자판과 맥에서 복사한 글은 `한` 을
    /// 자모 셋으로 적는다(NFD). 눈으로는 같은 글자다.
    #[test]
    fn the_same_password_typed_on_a_mac_opens_it_too() {
        use unicode_normalization::UnicodeNormalization;
        let decomposed: String = PW.nfd().collect();
        assert_ne!(decomposed, PW, "고른 비밀번호에 자모로 갈릴 글자가 없습니다");
        assert!(V::open(&seeded().seal().unwrap(), &decomposed).is_ok());
    }

    /// 그릇의 판이 더 새것이면 **푸는 방법을 모르는 파일**이다. 매직에 붙여
    /// 두었을 때는 이것이 "우리 파일이 아니다" 로 나왔다.
    #[test]
    fn a_newer_container_says_so() {
        let mut sealed = seeded().seal().unwrap();
        sealed[7] = CONTAINER_VERSION + 1;
        let e = V::open(&sealed, PW).unwrap_err();
        assert_eq!(e.code, ErrorCode::VaultContainerUnknown);
        // 매직은 그대로이므로 "우리 파일이 아니다" 가 되어서는 안 된다.
        assert_ne!(e.code, ErrorCode::VaultFormatInvalid);

        // 있은 적 없는 판 0 도 아는 판이 아니다. 한 바이트가 망가진 파일이 여기 온다.
        let mut zeroed = seeded().seal().unwrap();
        zeroed[7] = 0;
        assert_eq!(V::open(&zeroed, PW).unwrap_err().code, ErrorCode::VaultContainerUnknown);
    }

    /// **봉할 때 적는 판은 그 열쇠를 뽑은 판이다.**
    ///
    /// 상수를 적어 넣으면, 옛 판 파일을 새 앱에서 열어 한 번 저장하는 것만으로
    /// 머리와 알맹이가 어긋나 영영 안 열린다.
    #[test]
    fn sealing_writes_the_container_the_key_came_from() {
        let mut v = seeded();
        assert_eq!(v.seal().unwrap()[7], v.container);

        // 판을 손으로 바꿔 두면 머리도 따라가야 한다(상수를 적지 않는다는 뜻).
        v.container = 7;
        assert_eq!(v.seal().unwrap()[7], 7, "머리에 상수를 적고 있다");
    }

    /* ── 판을 올리는 날을 미리 치러 본다 ─────────────────────────

       이 길은 판이 둘이 되기 전에는 도달할 수 없다. 그런데 **여기서 두 번 틀렸다** —
       여는 쪽이 판을 안 보았고, 봉하는 쪽이 상수를 적었다. 둘 다 "그날" 에야 드러나고,
       그날 드러나면 사람 손의 파일이 전부 안 열린다. 그래서 시험에서만 판을 하나 더
       두고 실제로 태워 본다. */

    /// 옛 판 파일이 **새 앱에서 열리고, 저장하면 새 판이 되고, 다시 열린다.**
    #[test]
    fn an_older_container_opens_and_upgrades_when_saved() {
        const NEXT: u8 = 2;
        let sealed = seeded().seal().unwrap();
        assert_eq!(sealed[7], 1, "만든 파일이 1판이 아니다");

        let upgraded = V::open_as(&sealed, PW, NEXT).expect("옛 판 파일이 안 열린다");
        assert_eq!(upgraded.container, NEXT, "열면서 판이 올라가지 않았다");
        assert_eq!(body(&upgraded), "비밀메모", "알맹이가 넘어오지 않았다");

        let saved = upgraded.seal().unwrap();
        assert_eq!(saved[7], NEXT, "저장했는데 머리에 옛 판이 적혔다");

        // **그리고 다시 열린다.** 머리와 열쇠가 어긋났다면 여기서 선다.
        let reopened = V::open_as(&saved, PW, NEXT).expect("저장한 파일이 안 열린다");
        assert_eq!(reopened.container, NEXT);
    }

    /// 옛 앱은 새 판 파일을 **모르는 파일로 거절한다.** 억지로 풀지 않는다.
    #[test]
    fn an_older_app_refuses_a_newer_container() {
        const NEXT: u8 = 2;
        let sealed = seeded().seal().unwrap();
        let saved = V::open_as(&sealed, PW, NEXT).unwrap().seal().unwrap();
        assert_eq!(V::open_as(&saved, PW, 1).unwrap_err().code, ErrorCode::VaultContainerUnknown);
    }

    /// 올린 뒤 **저장하지 않으면 파일은 그대로**다. 열었다는 이유로 남의 파일이
    /// 바뀌면 안 된다.
    #[test]
    fn opening_alone_does_not_rewrite_the_file() {
        let sealed = seeded().seal().unwrap();
        let before = sealed.clone();
        let _ = V::open_as(&sealed, PW, 2).unwrap();
        assert_eq!(sealed, before);
        // 옛 앱이 아직 연다 — 아무것도 안 바뀌었으므로.
        assert!(V::open_as(&sealed, PW, 1).is_ok());
    }

    /// 비밀번호 바꾸기는 **그 금고의 판**으로 봉한다. 상수를 쓰면 판이 갈린 금고에서
    /// 맞는 비밀번호를 틀렸다고 하게 된다.
    #[test]
    fn rekeying_keeps_the_file_on_its_own_container() {
        const NEXT: u8 = 2;
        let sealed = seeded().seal().unwrap();
        // 진짜로 올려서 연 금고를 쓴다. 판만 손으로 바꿔 두면 열쇠와 어긋나
        // "맞는 비밀번호를 틀렸다" 고 하는 자리를 못 본다.
        let mut v = V::open_as(&sealed, PW, NEXT).unwrap();
        let mut saved = Vec::new();
        v.rekey(PW, NEW_PW, |b| {
            saved = b.to_vec();
            Ok(())
        })
        .unwrap();
        assert_eq!(saved[7], NEXT, "바꾼 뒤에 판이 내려갔다");
        assert_eq!(v.container, NEXT);
        assert!(V::open_as(&saved, NEW_PW, NEXT).is_ok());
    }

    /// 담는 DB 는 **임시 파일을 디스크에 만들지 않는다.**
    #[test]
    fn the_database_keeps_its_scratch_in_memory() {
        let v = V::create(PW).unwrap();
        let mode: i64 = v.conn.query_row("PRAGMA temp_store", [], |r| r.get(0)).unwrap();
        assert_eq!(mode, 2, "임시 파일이 디스크로 나갈 수 있다 (2 = MEMORY)");
    }

    #[test]
    fn garbage_file_is_rejected_as_format_error() {
        assert_eq!(V::open(&[0u8; 200], PW).unwrap_err().code, ErrorCode::VaultFormatInvalid);
    }

    /// 다른 앱의 매직을 단 파일은 우리 것이 아니다.
    #[test]
    fn another_apps_file_is_not_ours() {
        let mut sealed = seeded().seal().unwrap();
        sealed[..7].copy_from_slice(b"OTHERAP");
        assert_eq!(V::open(&sealed, PW).unwrap_err().code, ErrorCode::VaultFormatInvalid);
    }

    /// **이 기계에서 파일 하나 여는 데 얼마나 걸리는가.**
    ///
    /// ```text
    /// cargo test -p sheetsift-core --release opening_takes -- --ignored --nocapture
    /// ```
    ///
    /// 열쇠 뽑는 비용(`KDF_MEMORY_KIB`·`KDF_PASSES`)은 **모든 기계에서 같아야 한다** —
    /// 낮은 기계에 맞춰 낮추면 그 값으로 만든 파일은 어디서나 그만큼 약하다. 그래서
    /// 값을 정하기 전에 **가장 느린 기계**에서 재 본다.
    ///
    /// `--release` 로 재야 한다. 그냥 돌리면 쓰는 사람이 쓰지 않는 빌드를 재는 셈이다.
    #[test]
    #[ignore = "재기만 한다. 값을 정하기 전에 느린 기계에서 돌려 본다."]
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
            "\n  파일 {} KB · 여는 데 {whole:.0} ms\n\
               그중 열쇠 뽑기 {kdf:.0} ms · 알맹이 검사 {image:.0} ms\n\
               (메모리 {} MiB · {}회)\n",
            sealed.len() / 1024,
            KDF_MEMORY_KIB / 1024,
            KDF_PASSES,
        );
    }
}
