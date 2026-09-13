//! 암호화된 학생 데이터 파일(`.navi`).
//!
//! ## 왜 SQLCipher 를 쓰지 않았나
//! SQLCipher 는 Windows 에서 OpenSSL 빌드가 얽혀 CI 구성이 까다롭다.
//! 우리는 이미 내장 DB 를 `sqlite3_deserialize` 로 메모리에서 여는 구조라,
//! 같은 방식으로 **직렬화된 SQLite 바이트 전체를 AES-256-GCM 으로 감싸면** 된다.
//! 순수 Rust 로 끝나고, GCM 이라 위·변조 탐지도 따라온다.
//! 평문 SQLite 가 디스크에 남는 순간이 없다.
//!
//! ## 파일 형식
//! ```text
//! offset  size  내용
//!      0     7  "GOENAVI"     매직
//!      7     1  container_version  그릇의 판 — 푸는 방법이 바뀌면 오른다
//!      8     4  schema_version (little endian)  안의 표가 바뀌면 오른다
//!     12    16  Argon2id 솔트
//!     28    12  AES-GCM 논스
//!     40     4  알맹이의 CRC-32 — **깨진 것과 비밀번호를 가르는 데만 쓴다**
//!     44     …  AES-256-GCM(직렬화된 SQLite) + 인증 태그 16 B
//! ```
//!
//! ## 깨진 파일과 틀린 비밀번호를 가른다
//!
//! 인증 태그는 **한 바이트만 달라도** 복호화를 거절한다. 그것이 암호가 하는 일이라
//! 어떤 설계로도 깨진 파일을 열 수는 없다. 그런데 그때 담임에게 "비밀번호가 맞지
//! 않거나 파일이 손상되었습니다" 라고만 말하면, **맞는 비밀번호를 하루 종일 다시
//! 쳐 보게 된다.** 학교에서 그 하루가 비싸다.
//!
//! 그래서 알맹이의 CRC 를 머리에 적어 둔다. 열 때 먼저 대 보아 다르면 **전송 중에
//! 깨졌다**고 말한다 — 파일을 다시 받으라는 말이 비밀번호를 의심하라는 말보다 훨씬
//! 쓸모 있다. CRC 는 암호문에서 계산한 값이라 비밀번호에 대해 아무것도 말하지
//! 않는다(누구나 다시 계산할 수 있다). 손보는 쪽이 CRC 까지 맞춰 오면 그때는 태그가
//! 막고, 그건 이미 "깨진 파일" 이 아니다.
//! **앞 40바이트(셈 앞까지)가 AAD 로 함께 인증된다.** 한 바이트만 고쳐도 복호화가
//! 실패한다. 셈이 든 네 바이트는 인증 밖이다 — 까닭은 아래에 적는다.
//!
//! 비밀번호는 담당교사가 정하고, 담임에게 파일과 **다른 경로로** 전달한다.
//! (zip 은 메일, 비밀번호는 문자 — 파일만 새어 나가도 열리지 않는다.)
//!
//! ## 열어 둔 채로는 잠그지 않는다 — 정한 것이다
//!
//! 얼마쯤 지나면 저절로 잠그는 길을 두지 않는다. 상담은 학생을 앞에 두고 30분씩
//! 이어지고, 그 사이에 비밀번호를 다시 묻는 프로그램은 선생님이 창을 닫아 두거나
//! 더 쉬운 비밀번호를 고르게 만든다. 파일을 열어 둔 자리는 교무실이고, 그 자리의
//! 보안은 화면 잠금이 진다.

use aes_gcm::{
    aead::{Aead, KeyInit, Payload},
    Aes256Gcm, Nonce,
};
use argon2::Argon2;
use rand::RngCore;
use rusqlite::Connection;
use zeroize::Zeroize;

use crate::error::{AppError, AppResult, ErrorCode};

const MAGIC: &[u8; 7] = b"GOENAVI";

/// **그릇의 판.** 머리 모양이나 암호 방식을 바꾸면 이것을 올린다. 안의 스키마
/// 판(`schema_version`)과 다른 것이다 — 그쪽은 표가 바뀌는 것이고, 이쪽은 파일을
/// 푸는 방법이 바뀌는 것이다.
///
/// **매직에 붙여 두지 않는다.** 예전에는 `GOENAVI\x01` 여덟 자를 통째로 견주어,
/// 판을 올리는 순간 옛 파일이 `우리 파일이 아니다` 가 됐다 — 판으로 쓸 수 없는
/// 판 자리였다. 지금은 따로 읽어, 더 새것이면 그렇게 말해 준다.
const CONTAINER_VERSION: u8 = 1;

const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 12;
const CRC_LEN: usize = 4;
const HEADER_LEN: usize = 7 + 1 + 4 + SALT_LEN + NONCE_LEN + CRC_LEN;
/// GCM 인증 태그 길이.
const TAG_LEN: usize = 16;

/// 스키마 판은 `migrate.rs` 가 정한다. 여기서는 그 값을 그대로 쓴다.
pub use crate::migrate::SCHEMA_VERSION;

/// 비밀번호를 너무 짧게 잡으면 암호화가 의미를 잃는다.
pub const MIN_PASSWORD_LEN: usize = 8;

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
   걸리는 시간은 담당자가 파일을 열 때 한 번이고, 추측하는 쪽은 그 시간을 수억 번
   치러야 한다. 그 거래는 언제나 우리 쪽이 남는다.

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

fn kdf_for(container: u8) -> AppResult<Argon2<'static>> {
    let Some(cost) = container.checked_sub(1).and_then(|i| CONTAINERS.get(i as usize)) else {
        return Err(AppError::new(ErrorCode::VaultContainerUnknown)
            .with("fileVersion", container)
            .with("appVersion", CONTAINER_VERSION));
    };
    let params = argon2::Params::new(cost.memory_kib, cost.passes, cost.lanes, None)
        .map_err(|e| AppError::new(ErrorCode::VaultKeyDerivationFailed).detail(e))?;
    Ok(Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params))
}

/// 비밀번호의 유니코드 표기를 한 가지로 맞춘다.
///
/// **같은 글자가 두 바이트열로 적힌다.** `한` 은 윈도우 자판에서 완성형 한 자
/// (NFC, 3바이트)로, 맥 자판·맥에서 복사한 글에서는 자모 셋(NFD, 9바이트)으로
/// 온다. 눈으로는 같은데 열쇠가 다르다. 맥에서 만든 파일을 윈도우에서 못 열고,
/// 그때 뜨는 말은 `비밀번호가 맞지 않거나 파일이 손상되었습니다` 다.
///
/// 담당교사가 비밀번호를 **문자로 보내는** 구조라(파일과 다른 경로) 그 글이
/// 어느 표기로 도착할지 우리가 정할 수 없다. 그래서 받는 쪽에서 맞춘다.
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

fn derive_key(password: &str, salt: &[u8]) -> AppResult<zeroize::Zeroizing<[u8; 32]>> {
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
) -> AppResult<zeroize::Zeroizing<[u8; 32]>> {
    let mut key = zeroize::Zeroizing::new([0u8; 32]);
    let mut normalized = normalize(password);
    let hashed = kdf_for(container)?.hash_password_into(normalized.as_bytes(), salt, key.as_mut());
    normalized.zeroize();
    hashed.map_err(|e| AppError::new(ErrorCode::VaultKeyDerivationFailed).detail(e))?;
    Ok(key)
}

/// 풀어 낸 SQLite 알맹이가 **성한 것인지** 본다.
///
/// 태그가 맞았다는 것은 "이 파일을 만든 사람이 비밀번호를 알았다" 까지만 말한다.
/// 그런데 이 프로그램은 **파일과 비밀번호를 함께 건네는 것이 정상 흐름**이라,
/// 받은 사람이 손본 파일을 되돌려 주는 길이 열려 있다. SQLite 는 조작된 b-tree
/// 이미지에 대해 스스로 안전을 보장하지 않는다고 문서에 적어 두었다.
///
/// `DEFENSIVE` 는 SQL 로 스키마를 직접 건드리는 길을 막고, `integrity_check` 는
/// 장부가 앞뒤가 맞는지 훑는다. 학생 수백 명짜리 파일에서 눈에 띄는 시간이 들지
/// 않는다.
fn check_image(conn: &Connection) -> AppResult<()> {
    let verdict: String = conn
        .query_row("PRAGMA integrity_check", [], |r| r.get(0))
        .map_err(|e| AppError::new(ErrorCode::VaultFormatInvalid).detail(e))?;
    if verdict != "ok" {
        return Err(AppError::new(ErrorCode::VaultFormatInvalid).detail(verdict));
    }

    /* **표의 생김새가 우리가 만든 그대로인가.**

       장부가 성한 것과 담긴 것이 우리 것인 것은 다른 말이다. `integrity_check` 는
       b-tree 가 앞뒤 맞는지만 본다 — 표를 뷰로 바꿔치우거나, 트리거를 심거나,
       열을 하나 더해도 `ok` 다. 비밀번호를 아는 쪽이 만든 파일이면 태그도 맞는다.

       막을 재료는 이미 있었다. 판마다 스키마 전문을 박아 두고(판 폴더의
       schema.txt) 시험에서만 쓰고 있었다. 여는 길에서도 그것과 견준다.

       담긴 **값**이 바뀐 것은 여전히 못 잡는다. 그건 암호가 답할 수 있는 물음이
       아니다 — 비밀번호를 가진 사람은 언제나 제 파일을 고쳐 쓸 수 있다. 여기서
       막는 것은 **프로그램이 모르는 구조**가 실려 오는 것이다. */
    let v = crate::migrate::user_version(conn)?;
    let Some(want) = crate::migrate::expected_fingerprint(v) else {
        return Err(AppError::new(ErrorCode::VaultFormatInvalid)
            .detail(format!("no recorded schema for version {v}")));
    };
    let got = crate::migrate::fingerprint(conn)?;
    if crate::migrate::same_lines(&got) != crate::migrate::same_lines(want) {
        return Err(AppError::new(ErrorCode::VaultFormatInvalid)
            .detail("schema does not match the recorded one"));
    }
    Ok(())
}

/// 학생 자료를 담는 메모리 DB. **임시 파일을 디스크에 만들지 않는다.**
///
/// SQLite 는 정렬이나 임시 인덱스가 캐시를 넘기면 `%TEMP%` 에 임시 파일을 만든다.
/// 그 안에는 학생 이름과 성적이 평문으로 들어간다. 내장 DB 에는 진작 걸어 두었는데
/// (`db.rs`) 정작 개인정보가 든 이쪽에 빠져 있었다. 이 파일이 통째로 암호화된다는
/// 말은 이 한 줄이 있어야 참이 된다.
fn memory_db() -> AppResult<Connection> {
    let conn = Connection::open_in_memory()
        .map_err(|e| AppError::new(ErrorCode::VaultOpenFailed).detail(e))?;
    /* `foreign_keys` 는 SQLite 에서 **기본이 꺼짐**이다. 스키마는 성적이 학생을
       따라 지워지도록(`ON DELETE CASCADE`) 적어 두었는데, 켜지 않으면 그 선언이
       아무 일도 하지 않는다. 지금은 학생을 지우는 길이 없어 틀린 것이 아니지만,
       삭제가 생기는 날 **지운 학생의 성적이 파일 안에 고아로 남는다** — 개인정보
       파일에서 삭제가 삭제가 아니게 된다. 그날 아무도 이 한 줄을 떠올리지 못한다.

       이미 든 자료에는 영향이 없다(이 설정은 앞으로의 쓰기에만 걸린다). 그래서
       손에 있는 파일이 이것 때문에 안 열리는 일은 없다. */
    conn.execute_batch("PRAGMA temp_store = MEMORY; PRAGMA foreign_keys = ON;")
        .map_err(|e| AppError::new(ErrorCode::VaultOpenFailed).detail(e))?;

    // **만드는 길에도 건다.** 예전에는 여는 길에만 걸어, 같은 학생 자료를 담는
    // 같은 연결인데 어느 길로 왔느냐에 따라 깃발이 달랐다. 켜졌는지도 확인한다 —
    // `set_db_config` 는 **적용된 값**을 돌려주는데 그냥 버리고 있었다.
    let on = conn
        .set_db_config(rusqlite::config::DbConfig::SQLITE_DBCONFIG_DEFENSIVE, true)
        .map_err(|e| AppError::new(ErrorCode::VaultOpenFailed).detail(e))?;
    if !on {
        return Err(AppError::new(ErrorCode::VaultOpenFailed).detail("defensive mode refused"));
    }
    Ok(conn)
}

/// 파일이 오는 길에 깨졌는지 보는 셈. **암호가 아니라 진단이다.**
///
/// CRC-32(IEEE). 바이트가 바뀌거나 잘려 나가면 사실상 언제나 걸린다. 일부러 맞춰
/// 오는 것을 막는 장치가 아니다 — 그건 인증 태그가 한다.
///
/// **머리와 알맹이를 함께 센다.** 알맹이만 세던 때에는 솔트나 논스가 깨진 파일이
/// "비밀번호가 맞지 않습니다" 로, 판 칸이 깨진 파일이 "최신 프로그램을 요청하세요"
/// 로 나갔다 — 이 셈을 넣은 이유가 바로 그 두 문구를 없애자는 것이었는데 정작
/// 머리 서른두 바이트를 비워 두고 있었다. 제 셈이 든 네 바이트만 뺀다.
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

fn check_password(password: &str) -> AppResult<()> {
    // 길이도 **맞춘 뒤에** 센다. NFD 로 온 `한글비밀번호` 는 자모 하나하나가
    // 글자로 세어져, 같은 비밀번호가 표기에 따라 되기도 하고 안 되기도 한다.
    let mut normalized = normalize(password);
    let count = normalized.chars().count();
    normalized.zeroize();
    if count < MIN_PASSWORD_LEN {
        return Err(AppError::new(ErrorCode::VaultPasswordTooShort)
            .with("min", MIN_PASSWORD_LEN));
    }
    Ok(())
}

/// 열려 있는 학생 데이터. 메모리에만 존재한다.
pub struct Vault {
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
}

impl Drop for Vault {
    fn drop(&mut self) {
        self.key.zeroize();
    }
}

/// 키·솔트가 로그나 패닉 메시지로 새지 않도록 직접 구현한다.
impl std::fmt::Debug for Vault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Vault")
            .field("schema_version", &self.schema_version)
            .field("key", &"<redacted>")
            .finish()
    }
}

impl Vault {
    /// 빈 학생 데이터를 새로 만든다(담당교사가 처음 가져오기 할 때).
    pub fn create(password: &str) -> AppResult<Self> {
        check_password(password)?;
        let mut salt = [0u8; SALT_LEN];
        rand::rng().fill_bytes(&mut salt);
        let key = derive_key(password, &salt)?;

        let conn = memory_db()?;
        crate::migrate::run(&conn)?;

        Ok(Self {
            conn,
            key: *key,
            salt,
            container: CONTAINER_VERSION,
            schema_version: SCHEMA_VERSION,
        })
    }

    /// 파일 내용을 복호화해 메모리에서 연다.
    pub fn open(bytes: &[u8], password: &str) -> AppResult<Self> {
        Self::open_as(bytes, password, CONTAINER_VERSION)
    }

    /// 여는 길. **이 앱이 몇 판인지를 받는다.**
    ///
    /// 늘 `CONTAINER_VERSION` 이다. 판을 올리는 길(옛 그릇을 열어 새 그릇으로
    /// 올리는 것)은 판이 둘이 되기 전에는 도달할 수 없어, 그 길만 시험이 지나갈 수
    /// 있게 자리를 열어 둔다. **두 번이나 이 자리에서 틀렸다** — 한 번은 여는 쪽이
    /// 판을 안 보았고, 한 번은 봉하는 쪽이 상수를 적었다. 다음에 판을 올리는 사람이
    /// 손으로 확인하기를 기대하지 않는다.
    fn open_as(bytes: &[u8], password: &str, app_container: u8) -> AppResult<Self> {
        if bytes.len() < HEADER_LEN + TAG_LEN || &bytes[..7] != MAGIC {
            return Err(AppError::new(ErrorCode::VaultFormatInvalid)
                .detail("magic mismatch or file too short"));
        }
        /* 그릇의 판. **아는 판이 아니면 푸는 방법을 모르는 파일이다.**

           `>` 가 아니라 `!=` 다. 판 0 인 그릇은 존재한 적이 없으므로, 0 을 적어 둔
           파일은 더 새것도 옛것도 아니고 그냥 우리 것이 아니다. 한 바이트가 망가진
           파일도 여기서 걸린다 — 그때 "더 새 프로그램을 구하세요" 라고 말하면
           담당자는 있지도 않은 것을 찾으러 간다. */
        let container = bytes[7];
        if container == 0 || container > app_container {
            return Err(AppError::new(ErrorCode::VaultContainerUnknown)
                .with("fileVersion", container)
                .with("appVersion", app_container));
        }
        // 인증하는 머리는 셈 앞까지다(봉하는 쪽과 같아야 한다).
        let header = &bytes[..HEADER_LEN - CRC_LEN];
        let ciphertext = &bytes[HEADER_LEN..];

        /* **오는 길에 깨졌는지 먼저 본다.** 태그가 막는 것과 같은 결과지만 말이
           다르다 — 여기서 걸리면 "파일을 다시 받으세요" 이고, 태그에서 걸리면
           "비밀번호를 확인하세요" 다. 맞는 비밀번호를 쥔 사람이 비밀번호를 의심하며
           하루를 보내지 않게 하는 것이 이 네 바이트가 하는 일의 전부다.

           **그릇 판 검사 뒤에 둔다.** 판이 오르면 머리 모양이 달라질 수 있는데, 그때
           옛 앱이 새 파일을 "손상" 이라 부르면 안 된다. 그건 모르는 파일이지 깨진
           파일이 아니다. 반대로 **스키마 판 검사보다는 앞이다** — 그 칸이 깨진 것을
           "더 새 프로그램을 구하세요" 라고 말하면 없는 것을 찾으러 간다. */
        let recorded = u32::from_le_bytes(
            bytes[HEADER_LEN - CRC_LEN..HEADER_LEN].try_into().expect("length checked above"),
        );
        if crc32(&[header, ciphertext]) != recorded {
            return Err(AppError::new(ErrorCode::VaultFileDamaged));
        }

        // 머리에 적힌 스키마 판. **알맹이를 풀기 전에** 상위 판을 걸러 낸다 —
        // 모르는 구조를 애써 풀어 놓고 그제야 막을 까닭이 없다.
        //
        // 함께 인증되므로 **위조는 못 한다** — 한 바이트만 고쳐도
        // 복호화가 반드시 실패한다. 깨진 파일이 이 자리에서 `더 새 판입니다` 로
        // 잘못 갈리던 것은 위의 셈이 먼저 잡는다. 근거는 어디까지나 복호화 뒤에
        // 읽는 DB 안의 `user_version` 이고, 이건 미리 거르려고 두는 사본이다.
        let header_version =
            u32::from_le_bytes(bytes[8..12].try_into().expect("length checked above"));
        if header_version > SCHEMA_VERSION {
            return Err(AppError::new(ErrorCode::VaultVersionTooNew)
                .with("fileVersion", header_version)
                .with("appVersion", SCHEMA_VERSION));
        }

        let mut salt = [0u8; SALT_LEN];
        salt.copy_from_slice(&bytes[12..12 + SALT_LEN]);
        let nonce: [u8; NONCE_LEN] = bytes[12 + SALT_LEN..12 + SALT_LEN + NONCE_LEN]
            .try_into()
            .expect("length checked above");
        // 인증하는 머리는 CRC 앞까지다(봉하는 쪽과 같아야 한다).


        // **그 파일의 판으로 뽑는다.** 비용이 판마다 다를 수 있다.
        let key = derive_key_for(container, password, &salt)?;
        let cipher = Aes256Gcm::new((&*key).into());
        let mut plain = cipher
            .decrypt(&Nonce::from(nonce), Payload { msg: ciphertext, aad: header })
            // 비밀번호 오류와 파일 손상은 구분할 수 없다. 구분해서도 안 된다.
            .map_err(|_| AppError::new(ErrorCode::VaultPasswordWrong))?;

        let mut conn = memory_db()?;
        let len = plain.len();
        /* **성패와 무관하게 지운다.** 예전에는 `?` 를 먼저 써서, SQLite 가 이미지를
           거절하면 지우는 줄에 닿지도 못했다. 그때 힙에 남는 것이 학생 전원의 평문
           DB 다 — 지우려고 둔 줄이 정작 지워야 할 때 건너뛰어지고 있었다. */
        let loaded =
            conn.deserialize_read_exact("main", std::io::Cursor::new(&plain), len, false);
        plain.zeroize();
        loaded.map_err(|e| AppError::new(ErrorCode::VaultOpenFailed).detail(e))?;

        check_image(&conn)?;

        // 여기서부터 근거는 DB 안의 판이다. 옛 파일은 막지 않고 올린다 —
        // 3월에 만든 파일을 11월에 여는 일이 이 프로그램에서는 보통이다.
        crate::migrate::run(&conn)?;
        let schema_version = crate::migrate::user_version(&conn)?;

        /* **옛 그릇은 열면서 새 그릇으로 올린다.** 비밀번호가 여기 있을 때만 할 수
           있는 일이다 — 저장할 때는 이미 없다. 올린 열쇠는 다음 저장부터 쓰이고,
           저장하지 않으면 파일은 그대로다. */
        let mut vault = Self { conn, key: *key, salt, container, schema_version };
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
    pub fn seal(&self) -> AppResult<Vec<u8>> {
        self.seal_with(self.container, &self.key, &self.salt)
    }

    fn seal_with(
        &self,
        container: u8,
        key: &[u8; 32],
        salt: &[u8; SALT_LEN],
    ) -> AppResult<Vec<u8>> {
        /* **이 평문은 지울 수 없다.** `serialize` 는 먼저 `NOCOPY` 로 SQLite 가 쥐고
           있는 **살아 있는 페이지**를 가리키게 한다. 여기서 지우면 열려 있는 DB 가
           그 자리에서 깨진다. 사본이 오는 경우에도 `&mut` 로 받을 길이 없다.

           그래서 이 앱이 도는 동안 학생 자료는 메모리에 평문으로 있다 — 그것이
           설계다(파일만 암호화한다). 잠근 뒤에 남는 것을 줄이려면 SQLite 의 할당기를
           갈아야 하고, 그건 이 그릇이 답할 수 있는 물음이 아니다. */
        let data = self
            .conn
            .serialize("main")
            .map_err(|e| AppError::new(ErrorCode::VaultSealFailed).detail(e))?;

        let mut nonce = [0u8; NONCE_LEN];
        rand::rng().fill_bytes(&mut nonce);

        // 머리에 적는 판은 **DB 에서 읽어 온다.** 상수를 적어 넣으면 알맹이가
        // 옛 판인 채로 머리만 새 판이 되고, 그 어긋남은 아무도 못 본다.
        let version = crate::migrate::user_version(&self.conn)?;

        let mut out = Vec::with_capacity(HEADER_LEN + data.len() + TAG_LEN);
        out.extend_from_slice(MAGIC);
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
        let cipher = Aes256Gcm::new(key.into());
        let ct = cipher
            .encrypt(&Nonce::from(nonce), Payload { msg: &data, aad: &out })
            .map_err(|e| AppError::new(ErrorCode::VaultSealFailed).detail(e))?;
        let crc = crc32(&[&out, &ct]);
        out.extend_from_slice(&crc.to_le_bytes());
        out.extend_from_slice(&ct);
        Ok(out)
    }

    /// 비밀번호를 바꾸고, 그 자리에서 파일까지 쓴다.
    ///
    /// ## 왜 쓰기를 여기서 받나
    ///
    /// 키만 갈아 끼우고 저장을 나중에 받으면, **파일은 옛 비밀번호로 봉해진 채
    /// 앱만 새 비밀번호를 쥔 상태**가 생긴다. 담당자는 바꿨다고 알고 있는데
    /// 파일은 안 바뀐 것이다. 그래서 쓰기까지가 한 동작이다.
    ///
    /// 차례도 뒤집을 수 없다. **쓰기가 끝난 뒤에야** `self` 의 키를 간다 —
    /// 디스크가 가득 차 쓰기가 실패했는데 앱만 새 키를 쥐고 있으면, 다음
    /// 저장이 담당자가 모르는 비밀번호로 파일을 덮어쓴다.
    ///
    /// 현재 비밀번호를 다시 묻는 까닭은, 파일을 열어 둔 채 자리를 비우는 일이
    /// 흔해서다. 옆에 앉은 사람이 비밀번호만 바꿔 잠가 버릴 수 있으면 안 된다.
    pub fn rekey(
        &mut self,
        current: &str,
        next: &str,
        write: impl FnOnce(&[u8]) -> AppResult<()>,
    ) -> AppResult<()> {
        // **이 금고의 판으로 뽑는다.** `self.key` 가 그 판으로 뽑힌 것이라, 상수로
        // 뽑으면 판이 갈린 금고에서 맞는 비밀번호를 틀렸다고 하게 된다.
        let proof = derive_key_for(self.container, current, &self.salt)?;
        let ok = constant_time_eq(&proof, &self.key);
        if !ok {
            return Err(AppError::new(ErrorCode::VaultPasswordWrong));
        }
        check_password(next)?;

        // 솔트도 새로 뽑는다. 같은 솔트를 이어 쓰면 옛 비밀번호로 만든 무지개표가
        // 그대로 먹힌다.
        let mut salt = [0u8; SALT_LEN];
        rand::rng().fill_bytes(&mut salt);
        // **이 금고의 판으로 뽑고 그 판으로 봉한다.** 상수를 쓰면 판이 갈린
        // 금고에서 뽑은 열쇠와 적은 판이 어긋난다 — 이 자리에서 이미 두 번 틀렸다.
        let key = derive_key_for(self.container, next, &salt)?;
        self.seal_with(self.container, &key, &salt)
            .and_then(|bytes| write(&bytes))?;

        self.key.zeroize();
        self.key = *key;
        self.salt = salt;
        Ok(())
    }
}

/// 길이가 같은 두 바이트열을 **끝까지** 견준다. 다른 자리를 만나 곧바로
/// 돌아가면 걸린 시간이 "몇 글자까지 맞았는지" 를 말해 준다.
fn constant_time_eq(a: &[u8; 32], b: &[u8; 32]) -> bool {
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// 파일로 안전하게 쓴다. 임시 파일에 쓴 뒤 바꿔치기해서,
/// 저장 도중 전원이 꺼져도 기존 파일이 반쯤 망가지지 않게 한다.
///
/// 임시 이름은 **확장자를 갈지 않고 뒤에 덧붙인다.** `with_extension("tmp")`
/// 는 `학생데이터.navi` 를 `학생데이터.tmp` 로 만드는데, 같은 자리에 담당자가
/// 둔 `학생데이터.tmp` 가 있으면 그것을 말없이 덮어쓴다.
pub fn write_atomically(path: &std::path::Path, bytes: &[u8]) -> AppResult<()> {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".tmp");
    let tmp = path.with_file_name(name);
    // **알맹이가 디스크에 닿은 뒤에** 이름을 바꾼다. 이름 바꾸기는 파일 체계가
    // 지켜 주지만 알맹이는 지켜 주지 않는다. 그냥 쓰고 바꾸면, 전원이 꺼졌을 때
    // 이름만 남고 속이 빈 파일이 될 수 있다 — 옛 파일은 이미 사라진 뒤다.
    // 담당자가 USB 로 건네는 흐름이라 꺼내기 전에 빼는 일이 실제로 일어난다.
    let write = || -> std::io::Result<()> {
        use std::io::Write;
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()
    };
    write().map_err(|e| {
        // 쓰다 만 임시 파일을 남기지 않는다.
        let _ = std::fs::remove_file(&tmp);
        AppError::new(ErrorCode::VaultWriteFailed).detail(e)
    })?;
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        AppError::new(ErrorCode::VaultWriteFailed).detail(e)
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    use unicode_normalization::UnicodeNormalization;

    /// 저장소에 둔 고정 파일의 비밀번호. **한글을 넣는다** — 자모로 갈리는
    /// 글자가 있어야 표기를 맞추는 일까지 붙잡을 수 있다.
    const FIXTURE_PW: &str = "학교비밀번호2027";

    const PW: &str = "학교비밀번호2027";

    fn seeded() -> Vault {
        let v = Vault::create(PW).unwrap();
        v.conn
            .execute(
                "INSERT INTO students(grade, class_no, number, name) VALUES(3, 1, 9, '김우현')",
                [],
            )
            .unwrap();
        v
    }

    #[test]
    fn round_trip_preserves_contents() {
        let sealed = seeded().seal().unwrap();
        let opened = Vault::open(&sealed, PW).unwrap();
        let name: String = opened
            .conn
            .query_row("SELECT name FROM students", [], |r| r.get(0))
            .unwrap();
        assert_eq!(name, "김우현");
    }

    #[test]
    fn wrong_password_is_rejected() {
        let sealed = seeded().seal().unwrap();
        let e = Vault::open(&sealed, "틀린비밀번호").unwrap_err();
        assert_eq!(e.code, ErrorCode::VaultPasswordWrong);
    }

    /// **오는 길에 깨진 것과 비밀번호가 틀린 것을 가른다.**
    ///
    /// 둘 다 열리지 않는 것은 같다 — 암호가 그렇게 만들어져 있다. 다른 것은 담임에게
    /// 하는 말이다. 맞는 비밀번호를 쥔 사람에게 "비밀번호를 확인하세요" 라고 하면
    /// 하루를 엉뚱한 데서 보낸다.
    #[test]
    fn a_file_damaged_in_transit_says_so_instead_of_blaming_the_password() {
        let mut sealed = seeded().seal().unwrap();
        let last = sealed.len() - 30;
        sealed[last] ^= 0xff;
        assert_eq!(
            Vault::open(&sealed, PW).unwrap_err().code,
            ErrorCode::VaultFileDamaged,
            "깨진 파일을 비밀번호 탓으로 돌린다"
        );

        // 잘려서 온 파일도 마찬가지다(클라우드가 반만 내려 주는 일이 있다).
        let mut cut = seeded().seal().unwrap();
        cut.truncate(cut.len() - 40);
        assert_eq!(Vault::open(&cut, PW).unwrap_err().code, ErrorCode::VaultFileDamaged);
    }

    /// **셈까지 맞춰 온 위조는 태그가 막는다.** 그때는 깨진 파일이 아니다.
    #[test]
    fn tampering_is_detected() {
        let mut sealed = seeded().seal().unwrap();
        let last = sealed.len() - 30;
        sealed[last] ^= 0xff;
        // 손보는 쪽이 셈을 다시 맞춰 온 상황.
        let fixed =
            crc32(&[&sealed[..HEADER_LEN - CRC_LEN], &sealed[HEADER_LEN..]]).to_le_bytes();
        sealed[40..44].copy_from_slice(&fixed);
        assert_eq!(Vault::open(&sealed, PW).unwrap_err().code, ErrorCode::VaultPasswordWrong);
    }

    /// 성한 파일에 틀린 비밀번호는 **비밀번호 문제로** 말한다.
    #[test]
    fn a_wrong_password_on_an_intact_file_is_not_called_damage() {
        let sealed = seeded().seal().unwrap();
        assert_eq!(
            Vault::open(&sealed, "틀린비밀번호").unwrap_err().code,
            ErrorCode::VaultPasswordWrong
        );
    }

    /// **머리가 깨진 것도 깨졌다고 말한다.**
    ///
    /// 솔트·논스·판 칸은 오는 길에 깨지기 쉬운 자리인데, 셈이 알맹이만 덮던 때에는
    /// 이것이 "비밀번호가 맞지 않습니다" 로 나갔다. 맞는 비밀번호를 쥔 담임이
    /// 하루를 엉뚱한 데서 보낸다.
    #[test]
    fn a_damaged_header_says_damaged_not_wrong_password() {
        for at in [8, 12, 20, 28, 39] {
            let mut sealed = seeded().seal().unwrap();
            sealed[at] ^= 0x01;
            assert_eq!(
                Vault::open(&sealed, PW).unwrap_err().code,
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
        let fixed =
            crc32(&[&sealed[..HEADER_LEN - CRC_LEN], &sealed[HEADER_LEN..]]).to_le_bytes();
        sealed[40..44].copy_from_slice(&fixed);
        assert_eq!(Vault::open(&sealed, PW).unwrap_err().code, ErrorCode::VaultPasswordWrong);
    }

    #[test]
    fn plaintext_never_appears_in_the_file() {
        let sealed = seeded().seal().unwrap();
        assert!(!sealed.starts_with(b"SQLite format 3\0"), "평문 SQLite 헤더가 보인다");
        let needle = "김우현".as_bytes();
        assert!(
            !sealed.windows(needle.len()).any(|w| w == needle),
            "학생 이름이 평문으로 남아 있다"
        );
    }

    #[test]
    fn short_password_is_rejected() {
        assert_eq!(
            Vault::create("1234").unwrap_err().code,
            ErrorCode::VaultPasswordTooShort
        );
    }

    #[test]
    fn future_schema_version_is_rejected() {
        let mut sealed = seeded().seal().unwrap();
        sealed[8..12].copy_from_slice(&(SCHEMA_VERSION + 5).to_le_bytes());
        // 셈을 맞춰 둔다 — 그러지 않으면 "깨진 파일" 로 먼저 걸린다. 여기서 보려는
        // 것은 **성한 파일이 더 새 판일 때** 무슨 말을 하는가다.
        let fixed =
            crc32(&[&sealed[..HEADER_LEN - CRC_LEN], &sealed[HEADER_LEN..]]).to_le_bytes();
        sealed[40..44].copy_from_slice(&fixed);
        assert_eq!(Vault::open(&sealed, PW).unwrap_err().code, ErrorCode::VaultVersionTooNew);
    }

    /// 옛 판 파일은 **막지 않고 올려서 연다.** 담당자가 3월에 만든 파일을
    /// 11월에 여는 일이 이 프로그램에서는 보통이다.
    #[test]
    fn an_older_file_opens_after_being_migrated() {
        // 판을 매기기 전(0판) 파일을 그대로 만든다 — 표가 하나도 없다.
        let mut salt = [0u8; SALT_LEN];
        rand::rng().fill_bytes(&mut salt);
        let old = Vault {
            conn: Connection::open_in_memory().unwrap(),
            key: *derive_key(PW, &salt).unwrap(),
            salt,
            container: CONTAINER_VERSION,
            schema_version: 0,
        };
        let sealed = old.seal().unwrap();
        assert_eq!(u32::from_le_bytes(sealed[8..12].try_into().unwrap()), 0);

        // 막지 않고 올려서 연다. 3월에 만든 파일을 11월에 여는 일이 보통이다.
        let opened = Vault::open(&sealed, PW).unwrap();
        assert_eq!(opened.schema_version, SCHEMA_VERSION, "열면서 판을 올리지 않았다");
        opened
            .conn
            .execute(
                "INSERT INTO students(grade, class_no, number, name) VALUES(3, 1, 9, '김우현')",
                [],
            )
            .expect("판을 올렸는데 표가 없다");
    }

    /// 봉할 때 머리에 적는 판은 DB 에서 읽는다. 상수를 적어 넣으면 알맹이와
    /// 머리가 어긋나고, 그 어긋남은 아무도 못 본다.
    #[test]
    fn the_header_version_comes_from_the_database() {
        let sealed = seeded().seal().unwrap();
        assert_eq!(
            u32::from_le_bytes(sealed[8..12].try_into().unwrap()),
            SCHEMA_VERSION
        );
    }

    const NEW_PW: &str = "바뀐비밀번호2027";

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

        let opened = Vault::open(&saved, NEW_PW).unwrap();
        let name: String = opened
            .conn
            .query_row("SELECT name FROM students", [], |r| r.get(0))
            .unwrap();
        assert_eq!(name, "김우현", "알맹이가 함께 넘어오지 않았다");
        assert_eq!(
            Vault::open(&saved, PW).unwrap_err().code,
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
        let sealed = v.seal().unwrap();
        assert!(Vault::open(&sealed, NEW_PW).is_ok());
    }

    #[test]
    fn rekeying_needs_the_current_password() {
        let mut v = seeded();
        let e = v.rekey("틀린비밀번호", NEW_PW, |_| panic!("쓰기까지 갔다")).unwrap_err();
        assert_eq!(e.code, ErrorCode::VaultPasswordWrong);
        assert!(Vault::open(&v.seal().unwrap(), PW).is_ok(), "키가 바뀌었다");
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
                Err(AppError::new(ErrorCode::VaultWriteFailed).detail("디스크가 가득 찼다"))
            })
            .unwrap_err();
        assert_eq!(e.code, ErrorCode::VaultWriteFailed);

        let sealed = v.seal().unwrap();
        assert!(Vault::open(&sealed, PW).is_ok(), "쓰기가 실패했는데 키가 바뀌었다");
        assert_eq!(
            Vault::open(&sealed, NEW_PW).unwrap_err().code,
            ErrorCode::VaultPasswordWrong
        );
    }

    /// 임시 이름은 확장자를 갈지 않고 덧붙인다. 갈아 버리면 같은 자리의
    /// `<같은이름>.tmp` 를 말없이 덮어쓴다.
    #[test]
    fn the_temporary_name_does_not_eat_a_neighbours_file() {
        let dir = std::env::temp_dir().join("goe-navi-tmp-name-test");
        std::fs::create_dir_all(&dir).unwrap();
        let neighbour = dir.join("학생데이터.tmp");
        std::fs::write(&neighbour, "담당자가 둔 파일").unwrap();

        let target = dir.join("학생데이터.navi");
        write_atomically(&target, "새로 쓴 것".as_bytes()).unwrap();

        assert_eq!(
            std::fs::read(&neighbour).unwrap(),
            "담당자가 둔 파일".as_bytes(),
            "옆의 .tmp 를 덮어썼다"
        );
        assert_eq!(std::fs::read(&target).unwrap(), "새로 쓴 것".as_bytes());
        std::fs::remove_dir_all(&dir).ok();
    }

    /* ── 이미 나간 그릇을 못 박는 자리 ───────────────────────────

       아래 둘은 **한 번 배포되면 손댈 수 없는 것**을 지킨다. 값을 고치는 손이
       그럴듯한 까닭을 갖고 있어도(더 센 암호, 더 짧은 머리) 담당자 손의 파일은
       그 손을 따라가지 못한다. 그래서 사람의 판단이 아니라 시험이 막는다. */

    /// 머리의 생김새. **자리 하나하나가 약속이다.**
    ///
    /// 칸을 하나 늘리거나 차례를 바꾸면 이미 나간 파일은 영영 안 열린다. 숫자를
    /// 여기 적어 두는 것은 문서가 아니라 **자물쇠**다.
    /// **이 기계에서 파일 하나 여는 데 얼마나 걸리는가.**
    ///
    /// ```text
    /// cargo test --manifest-path src-tauri/Cargo.toml --release opening_takes -- --ignored --nocapture
    /// ```
    ///
    /// 열쇠 뽑는 비용(`KDF_MEMORY_KIB`·`KDF_PASSES`)은 **모든 기계에서 같아야 한다** —
    /// 낮은 기계에 맞춰 낮추면 그 값으로 만든 파일은 어디서나 그만큼 약하다. 그래서
    /// 값을 정하기 전에 **가장 느린 기계**에서 재 본다. 학교 업무용 PC 가 그것이다.
    ///
    /// `--release` 로 재야 한다. 그냥 돌리면 담당자가 쓰지 않는 빌드를 재는 셈이다.
    #[test]
    #[ignore = "재기만 한다. 값을 정하기 전에 느린 기계에서 돌려 본다."]
    fn opening_takes() {
        // 학교 하나 크기로 채운다.
        let v = Vault::create(PW).unwrap();
        v.conn
            .execute(
                "WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i+1 FROM n WHERE i<336)
                 INSERT INTO students(id, grade, class_no, number, name)
                   SELECT i, 3, (i%10)+1, i, ?1 FROM n",
                rusqlite::params!["아무개"],
            )
            .unwrap();
        for set in crate::exam::SUBJECT_SETS {
            v.conn
                .execute(
                    "WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i+1 FROM n WHERE i<336)
                     INSERT INTO naesin(student_id, subject_set, term, grade_avg)
                       SELECT i, ?1, '1-1', 1.5 FROM n",
                    rusqlite::params![set],
                )
                .unwrap();
        }
        let sealed = v.seal().unwrap();

        let ms = |t: std::time::Instant| t.elapsed().as_secs_f64() * 1000.0;

        let t = std::time::Instant::now();
        let opened = Vault::open(&sealed, PW).unwrap();
        let whole = ms(t);

        let t = std::time::Instant::now();
        derive_key(PW, &[7u8; SALT_LEN]).unwrap();
        let kdf = ms(t);

        let t = std::time::Instant::now();
        check_image(&opened.conn).unwrap();
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

    #[test]
    fn the_header_layout_is_pinned() {
        assert_eq!(MAGIC, b"GOENAVI", "매직이 달라졌습니다");
        assert_eq!(CONTAINER_VERSION, 1);
        assert_eq!(SALT_LEN, 16);
        assert_eq!(NONCE_LEN, 12);
        assert_eq!(TAG_LEN, 16);
        assert_eq!(CRC_LEN, 4);
        assert_eq!(HEADER_LEN, 44, "머리 길이가 달라졌습니다");

        // 자리까지 본다. 길이의 합이 같아도 차례가 바뀌면 못 읽는다.
        let sealed = seeded().seal().unwrap();
        assert_eq!(&sealed[..7], MAGIC);
        assert_eq!(sealed[7], CONTAINER_VERSION);
        assert_eq!(
            u32::from_le_bytes(sealed[8..12].try_into().unwrap()),
            SCHEMA_VERSION
        );
        assert_eq!(
            u32::from_le_bytes(sealed[40..44].try_into().unwrap()),
            crc32(&[&sealed[..HEADER_LEN - CRC_LEN], &sealed[HEADER_LEN..]]),
            "머리에 적힌 셈이 머리·알맹이와 맞지 않는다"
        );
        assert_eq!(sealed.len(), HEADER_LEN + TAG_LEN + serialized_len());
    }

    /// 봉인한 알맹이의 길이. 머리·태그를 뺀 나머지가 이것과 맞아야 한다.
    fn serialized_len() -> usize {
        seeded().conn.serialize("main").unwrap().len()
    }

    /**
     * **이미 나간 파일이 아직 열리는가.**
     *
     * 저장소에 넣어 둔 `.navi` 를 그대로 연다. 이 파일 하나가 그릇의 모든
     * 약속을 한꺼번에 붙잡는다 — 매직·판·솔트·논스의 자리, AES-256-GCM 과 AAD,
     * Argon2id 의 갈래·횟수·메모리, 비밀번호의 유니코드 표기까지. 그중 **무엇
     * 하나만 달라져도 이 시험이 선다.**
     *
     * 지문(`the_key_recipe_is_pinned`)이 이미 비용을 붙잡지만, 그것은 우리가 아는
     * 곳만 본다. 이 파일은 **아무도 생각하지 못한 곳**까지 본다.
     *
     * 담긴 것은 지어낸 학생 둘이다. 실제 성적은 저장소에 들어가지 않는다.
     *
     * 고쳐야 한다면(아직 아무에게도 나가지 않았다면) 아래 `write_the_fixture` 를
     * 손으로 돌려 다시 뜬다. 나간 뒤라면 **고칠 수 없다** — 그릇의 판을 올려라.
     */
    #[test]
    fn a_file_sealed_by_the_released_format_still_opens() {
        let bytes = include_bytes!("../fixtures/released-v1.navi");
        let v = Vault::open(bytes, FIXTURE_PW).unwrap_or_else(|e| {
            panic!(
                "\n이미 나간 그릇으로 봉한 파일이 열리지 않습니다: {e:?}\n\
                 머리 모양·암호 방식·열쇠 뽑는 비용·비밀번호 표기 중 하나가 달라졌습니다.\n\
                 담당자 손의 파일은 그 변경을 따라가지 못합니다. 그릇의 판을 올리세요.\n"
            )
        });
        assert_eq!(v.schema_version, SCHEMA_VERSION);

        let names: Vec<String> = {
            let mut stmt = v.conn.prepare("SELECT name FROM students ORDER BY number").unwrap();
            let rows = stmt.query_map([], |r| r.get(0)).unwrap();
            rows.map(|r| r.unwrap()).collect()
        };
        assert_eq!(names, ["가나다", "라마바"], "알맹이가 달라졌습니다");

        let grade: f64 = v
            .conn
            .query_row("SELECT grade_avg FROM naesin WHERE student_id = 1", [], |r| r.get(0))
            .unwrap();
        assert!((grade - 1.25).abs() < 1e-9);
    }

    /// **자모로 적은 비밀번호로도 열린다.** 맥 자판과 맥에서 복사한 글은 `한` 을
    /// 자모 셋으로 적는다(NFD). 눈으로는 같은 글자다.
    ///
    /// 이 시험이 서면 맥에서 만든 파일이 윈도우에서 안 열린다는 뜻이고, 그때 뜨는
    /// 말은 "비밀번호가 맞지 않거나 파일이 손상되었습니다" 다. 아무도 원인을 못 찾는다.
    #[test]
    fn the_same_password_typed_on_a_mac_opens_it_too() {
        let bytes = include_bytes!("../fixtures/released-v1.navi");
        let decomposed: String = FIXTURE_PW.nfd().collect();
        assert_ne!(decomposed, FIXTURE_PW, "고른 비밀번호에 자모로 갈릴 글자가 없습니다");
        assert!(
            Vault::open(bytes, &decomposed).is_ok(),
            "자모로 적은 같은 비밀번호로 열리지 않습니다"
        );
    }

    /// 저장소에 둘 `.navi` 를 다시 뜬다. **평소에는 돌지 않는다.**
    ///
    /// ```text
    /// cargo test --manifest-path src-tauri/Cargo.toml write_the_fixture -- --ignored
    /// ```
    ///
    /// 나가기 전에 그릇을 고쳤을 때만 돌린다. 나간 뒤에는 이 파일이 곧 약속이라
    /// 다시 뜨면 안 된다 — 지켜야 할 것을 지우는 셈이다.
    #[test]
    #[ignore = "저장소의 고정 파일을 덮어쓴다. 그릇을 일부러 고쳤을 때만 돌린다."]
    fn write_the_fixture() {
        /* **두 겹으로 막는다.** `#[ignore]` 하나로는 모자라다 — 이 파일 안에 `--ignored`
           로 돌리라고 적어 둔 다른 연장(`opening_takes`)이 있어서, 필터 없이 한 번
           치면 함께 돌아 고정 파일이 새로 떠진다. 그 뒤로 그릇 고정 시험은 무엇을
           바꾸든 영원히 통과한다. 일부러 켜야만 쓴다. */
        if std::env::var("GOE_NAVI_WRITE_FIXTURE").unwrap_or_default() != "1" {
            eprintln!("  GOE_NAVI_WRITE_FIXTURE=1 이 없어 다시 뜨지 않았다");
            return;
        }
        // **나간 뒤에는 다시 뜨지 않는다.** `#[ignore]` 하나로는 모자란다 —
        // 누군가 `cargo test -- --ignored` 를 한 번 치면 고정 파일이 새로 떠지고,
        // 그 뒤로 고정 시험은 무엇을 바꾸든 영원히 통과한다.
        #[allow(clippy::assertions_on_constants)] // 값이 상수인 것이 요점이다.
        {
            assert!(
                !crate::migrate::RELEASED,
                "이미 나간 뒤입니다. 고정 파일은 그때의 약속이라 다시 뜨면 안 됩니다."
            );
        }
        let v = Vault::create(FIXTURE_PW).unwrap();
        // 어휘는 `exam.rs` 에서 가져온다. 글자로 적으면 자료 검사가 막고,
        // 어휘가 바뀌면 고정 파일만 옛말로 남는다.
        let set = crate::exam::SUBJECT_SETS[0];
        let area = crate::exam::AREA_KOR;
        v.conn
            .execute(
                "INSERT INTO students(id, grade, class_no, number, name) VALUES
                   (1, 3, 1, 1, ?1), (2, 3, 1, 2, ?2)",
                rusqlite::params!["가나다", "라마바"],
            )
            .unwrap();
        v.conn
            .execute(
                "INSERT INTO naesin(student_id, subject_set, term, grade_avg) VALUES
                   (1, ?1, '1-1', 1.25), (2, ?1, '1-1', 2.5)",
                rusqlite::params![set],
            )
            .unwrap();
        v.conn
            .execute(
                "INSERT INTO mock(student_id, month, area, subject, percentile, grade_band)
                   VALUES(1, '9월', ?1, ?2, 92.0, 2)",
                rusqlite::params![area, "언어와매체"],
            )
            .unwrap();

        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("released-v1.navi"), v.seal().unwrap()).unwrap();
        eprintln!("  다시 떴다: {}", dir.join("released-v1.navi").display());
    }

    /// **비밀번호를 아는 쪽이 손본 파일을 그냥 받아들이지 않는다.**
    ///
    /// 태그가 맞았다는 것은 "만든 사람이 비밀번호를 알았다" 까지만 말한다. 이
    /// 프로그램은 파일과 비밀번호를 함께 건네는 것이 정상 흐름이라, 받은 사람이
    /// 손본 파일을 되돌려 주는 길이 열려 있다.
    ///
    /// 아래 셋은 감사에서 **실제로 만들어 통과시킨** 것들이다. `integrity_check` 는
    /// 셋 다 `ok` 를 내고 `DEFENSIVE` 도 막지 않는다. 스키마 지문이 막는다.
    #[test]
    fn a_doctored_schema_does_not_open() {
        // 표를 뷰로 바꿔치운다. 이름은 그대로라 화면은 아무것도 모른다.
        let swapped = doctored(|c| {
            c.execute_batch(
                "PRAGMA writable_schema = ON;
                 UPDATE sqlite_master SET type='view', sql='CREATE VIEW students AS
                   SELECT 1 AS id, 3 AS grade, 1 AS class_no, 1 AS number, ''가짜'' AS name'
                 WHERE name='students';
                 PRAGMA writable_schema = OFF;",
            )
            .unwrap()
        });
        assert_eq!(
            Vault::open(&swapped, PW).unwrap_err().code,
            ErrorCode::VaultFormatInvalid,
            "표가 뷰로 바뀐 파일이 열렸다"
        );

        // 트리거를 심는다. 담당자가 학생을 하나 넣는 순간 돈다.
        let trapped = doctored(|c| {
            c.execute_batch(
                "CREATE TRIGGER wipe AFTER INSERT ON students BEGIN
                   UPDATE students SET name = 'X';
                 END;",
            )
            .unwrap()
        });
        assert_eq!(
            Vault::open(&trapped, PW).unwrap_err().code,
            ErrorCode::VaultFormatInvalid,
            "트리거가 심긴 파일이 열렸다"
        );

        // 열을 하나 더한다. 우리가 모르는 자리다.
        let widened = doctored(|c| {
            c.execute_batch("ALTER TABLE students ADD COLUMN secret TEXT").unwrap()
        });
        assert_eq!(
            Vault::open(&widened, PW).unwrap_err().code,
            ErrorCode::VaultFormatInvalid,
            "열이 늘어난 파일이 열렸다"
        );
    }

    /// **통계 표 이름 뒤에도 숨을 수 없다.**
    ///
    /// 통계 표는 지문에서 빼는데, 그 예외를 이름으로만 두면 아무 표나 그 이름을
    /// 달고 사라진다. `LIKE` 였을 때는 더 나빴다 — `_` 가 와일드카드라
    /// `sqliteZstat1` 도 함께 빠졌고, 대소문자도 안 가렸다.
    #[test]
    fn a_table_wearing_the_statistics_name_does_not_open() {
        for name in ["sqlite_stat1", "sqliteZstat1", "SQLITE_STAT1"] {
            let hidden = doctored(|c| {
                c.execute_batch(&format!(
                    "CREATE TABLE zzz(secret TEXT);
                     PRAGMA writable_schema = ON;
                     UPDATE sqlite_master SET name = '{name}', tbl_name = '{name}'
                       WHERE name = 'zzz';
                     PRAGMA writable_schema = OFF;"
                ))
                .unwrap()
            });
            assert_eq!(
                Vault::open(&hidden, PW).unwrap_err().code,
                ErrorCode::VaultFormatInvalid,
                "'{name}' 이름 뒤에 숨은 표가 지나갔다"
            );
        }
    }

    /// **통계 표 이름에 짐을 실어 숨길 수 없다.**
    ///
    /// 이름만 보던 시절에는 `sqliteZstat1` 이 빠졌고, 글의 앞머리만 보던 시절에는
    /// `sqlite_stat1(tbl,idx,stat,payload)` 가 빠졌다 — 여는 괄호 뒤가 자유였다.
    /// 이제 `ANALYZE` 가 적는 글과 **글자 하나까지** 같아야 빠진다.
    #[test]
    fn a_statistics_table_carrying_cargo_does_not_open() {
        // 그 이름은 SQL 로 못 만든다(예약어다). 손보는 쪽은 스키마를 직접 고친다.
        let hidden = doctored(|c| {
            c.execute_batch(
                "CREATE TABLE zzz(tbl,idx,stat,payload TEXT);
                 INSERT INTO zzz VALUES('a','b','c','숨긴자료');
                 PRAGMA writable_schema = ON;
                 UPDATE sqlite_master
                    SET name = 'sqlite_stat1', tbl_name = 'sqlite_stat1',
                        sql = 'CREATE TABLE sqlite_stat1(tbl,idx,stat,payload TEXT)'
                  WHERE name = 'zzz';
                 PRAGMA writable_schema = OFF;",
            )
            .unwrap()
        });
        assert_eq!(
            Vault::open(&hidden, PW).unwrap_err().code,
            ErrorCode::VaultFormatInvalid,
            "통계 표 이름 뒤에 짐을 실은 표가 지나갔다"
        );

        // 열 이름만 바꾼 것도 마찬가지다.
        let renamed = doctored(|c| {
            c.execute_batch(
                "CREATE TABLE zzz(a,b,c);
                 PRAGMA writable_schema = ON;
                 UPDATE sqlite_master
                    SET name = 'sqlite_stat1', tbl_name = 'sqlite_stat1',
                        sql = 'CREATE TABLE sqlite_stat1(a,b,c)'
                  WHERE name = 'zzz';
                 PRAGMA writable_schema = OFF;",
            )
            .unwrap()
        });
        assert_eq!(
            Vault::open(&renamed, PW).unwrap_err().code,
            ErrorCode::VaultFormatInvalid
        );
    }

    /// 견주는 자리가 **앞의 빈 줄을 지우지 않는다.**
    ///
    /// `trim()` 이던 시절, 종류도 이름도 글도 없는 줄은 한 칸으로 그려져 맨 앞에
    /// 서는데 그 다듬기가 통째로 지웠다. 지문 원문끼리는 달라 보이는데 실제로
    /// 견주는 자리에서만 같아졌다 — 시험이 원문을 보고 있어 못 잡았다.
    #[test]
    fn the_comparison_does_not_swallow_a_blank_first_row() {
        let empty_row = format!("{} {}\n{}", "", "", "");
        assert_ne!(
            crate::migrate::same_lines(&format!("{empty_row}\n\ntable students\nCREATE …")),
            crate::migrate::same_lines("table students\nCREATE …"),
            "앞의 빈 줄이 견주는 자리에서 지워진다"
        );
    }

    /// 이름이 없는 줄도 지문에 오른다. NULL 은 `AND`·`NOT` 에서 조용히 빠진다.
    #[test]
    fn a_row_with_no_name_does_not_slip_past_the_fingerprint() {
        let conn = Connection::open_in_memory().unwrap();
        crate::migrate::run(&conn).unwrap();
        let before = crate::migrate::fingerprint(&conn).unwrap();
        conn.execute_batch(
            "PRAGMA writable_schema = ON;
             INSERT INTO sqlite_master(type, name, tbl_name, rootpage, sql)
               VALUES('table', NULL, 'students', 0, NULL);
             PRAGMA writable_schema = OFF;",
        )
        .unwrap();
        assert_ne!(
            crate::migrate::same_lines(&crate::migrate::fingerprint(&conn).unwrap()),
            crate::migrate::same_lines(&before),
            "이름 없는 줄이 지문에서 빠졌다"
        );
    }

    /// **`sqlite_` 로 시작하는 이름으로 지문을 피해 갈 수 없다.**
    ///
    /// SQLite 가 그 이름을 막는 것은 SQL 로 만들 때뿐이다. `writable_schema` 로
    /// 심으면 멀쩡히 불려 나간다. 재감사가 이 이름의 트리거로 실제로 통과했다.
    #[test]
    fn a_trigger_hiding_behind_a_reserved_name_does_not_open() {
        let hidden = doctored(|c| {
            c.execute_batch(
                "PRAGMA writable_schema = ON;
                 INSERT INTO sqlite_master(type, name, tbl_name, rootpage, sql)
                   VALUES('trigger', 'sqlite_wipe', 'students', 0,
                     'CREATE TRIGGER sqlite_wipe AFTER INSERT ON students BEGIN
                        UPDATE students SET name = ''X''; END');
                 PRAGMA writable_schema = OFF;",
            )
            .unwrap()
        });
        assert_eq!(
            Vault::open(&hidden, PW).unwrap_err().code,
            ErrorCode::VaultFormatInvalid,
            "`sqlite_` 이름 뒤에 숨은 트리거가 지나갔다"
        );
    }

    /// **봉할 때 적는 판은 그 열쇠를 뽑은 판이다.**
    ///
    /// 상수를 적어 넣으면, 옛 판 파일을 새 앱에서 열어 한 번 저장하는 것만으로
    /// 머리와 알맹이가 어긋나 영영 안 열린다. 재감사가 짚은 자리다.
    #[test]
    fn sealing_writes_the_container_the_key_came_from() {
        let mut v = seeded();
        assert_eq!(v.seal().unwrap()[7], v.container);

        // 판을 손으로 바꿔 두면 머리도 따라가야 한다(상수를 적지 않는다는 뜻).
        v.container = 7;
        assert_eq!(v.seal().unwrap()[7], 7, "머리에 상수를 적고 있다");
    }

    /// **`ANALYZE` 를 해도 파일은 열린다.**
    ///
    /// 통계 표는 질의 계획에만 쓰이고 담긴 자료와 무관하다. 지문에 올리면 누군가
    /// "여는 게 느리다" 며 한 줄 넣는 순간 **담임 전원이 제 학생 자료에서 잠긴다.**
    /// 지금은 부르는 곳이 없지만, 없다는 것과 넣어도 괜찮다는 것은 다른 말이다.
    #[test]
    fn analyzing_the_database_does_not_lock_it() {
        let analyzed = doctored(|c| c.execute_batch("ANALYZE").unwrap());
        assert!(
            Vault::open(&analyzed, PW).is_ok(),
            "ANALYZE 한 줄에 파일이 안 열린다"
        );
    }

    /// **종류가 없는 줄도 지문에 오른다.**
    ///
    /// `NOT (type = 'index' AND sql IS NULL)` 은 `type` 이 NULL 이면 통째로 NULL 이
    /// 되어 그 줄이 조용히 빠진다. 지금 그것이 악용되지 않는 까닭은 SQLite 파서가
    /// 그런 줄을 거절해서지 우리 검사가 막아서가 아니었다.
    #[test]
    fn a_row_with_no_type_does_not_slip_past_the_fingerprint() {
        let conn = Connection::open_in_memory().unwrap();
        crate::migrate::run(&conn).unwrap();
        let before = crate::migrate::fingerprint(&conn).unwrap();

        conn.execute_batch(
            "PRAGMA writable_schema = ON;
             INSERT INTO sqlite_master(type, name, tbl_name, rootpage, sql)
               VALUES(NULL, 'ghost', 'students', 0, NULL);
             PRAGMA writable_schema = OFF;",
        )
        .unwrap();

        // **`same_lines` 를 거쳐 견준다.** 원문끼리 견주면 production 이 쓰는 자리를
        // 시험하지 않는 셈이고, 실제로 그 다듬기가 줄을 지우고 있었다.
        assert_ne!(
            crate::migrate::same_lines(&crate::migrate::fingerprint(&conn).unwrap()),
            crate::migrate::same_lines(&before),
            "종류가 없는 줄이 지문에서 빠졌다"
        );
    }

    /* ── 판을 올리는 날을 미리 치러 본다 ─────────────────────────

       이 길은 판이 둘이 되기 전에는 도달할 수 없다. 그런데 **여기서 두 번 틀렸다** —
       여는 쪽이 판을 안 보았고, 봉하는 쪽이 상수를 적었다. 둘 다 "그날" 에야 드러나고,
       그날 드러나면 담당자 손의 파일이 전부 안 열린다. 그래서 시험에서만 판을 하나 더
       두고 실제로 태워 본다. */

    /// 옛 판 파일이 **새 앱에서 열리고, 저장하면 새 판이 되고, 다시 열린다.**
    #[test]
    fn an_older_container_opens_and_upgrades_when_saved() {
        const NEXT: u8 = 2;
        let sealed = seeded().seal().unwrap();
        assert_eq!(sealed[7], 1, "만든 파일이 1판이 아니다");

        // 새 앱(2판)이 옛 파일을 연다.
        let upgraded = Vault::open_as(&sealed, PW, NEXT).expect("옛 판 파일이 안 열린다");
        assert_eq!(upgraded.container, NEXT, "열면서 판이 올라가지 않았다");
        let name: String = upgraded
            .conn
            .query_row("SELECT name FROM students", [], |r| r.get(0))
            .unwrap();
        assert_eq!(name, "김우현", "알맹이가 넘어오지 않았다");

        // 저장하면 새 판으로 적힌다.
        let saved = upgraded.seal().unwrap();
        assert_eq!(saved[7], NEXT, "저장했는데 머리에 옛 판이 적혔다");

        // **그리고 다시 열린다.** 머리와 열쇠가 어긋났다면 여기서 선다.
        let reopened = Vault::open_as(&saved, PW, NEXT).expect("저장한 파일이 안 열린다");
        assert_eq!(reopened.container, NEXT);
    }

    /// 옛 앱은 새 판 파일을 **모르는 파일로 거절한다.** 억지로 풀지 않는다.
    #[test]
    fn an_older_app_refuses_a_newer_container() {
        const NEXT: u8 = 2;
        let sealed = seeded().seal().unwrap();
        let saved = Vault::open_as(&sealed, PW, NEXT).unwrap().seal().unwrap();
        assert_eq!(
            Vault::open_as(&saved, PW, 1).unwrap_err().code,
            ErrorCode::VaultContainerUnknown
        );
    }

    /// 올린 뒤 **저장하지 않으면 파일은 그대로**다. 열었다는 이유로 남의 파일이
    /// 바뀌면 안 된다.
    #[test]
    fn opening_alone_does_not_rewrite_the_file() {
        let sealed = seeded().seal().unwrap();
        let before = sealed.clone();
        let _ = Vault::open_as(&sealed, PW, 2).unwrap();
        assert_eq!(sealed, before);
        // 옛 앱이 아직 연다 — 아무것도 안 바뀌었으므로.
        assert!(Vault::open_as(&sealed, PW, 1).is_ok());
    }

    /// 비밀번호 바꾸기는 **그 금고의 판**으로 봉한다. 상수를 쓰면 판이 갈린 금고에서
    /// 맞는 비밀번호를 틀렸다고 하게 된다.
    #[test]
    fn rekeying_keeps_the_file_on_its_own_container() {
        const NEXT: u8 = 2;
        let sealed = seeded().seal().unwrap();
        // 진짜로 올려서 연 금고를 쓴다. 판만 손으로 바꿔 두면 열쇠와 어긋나
        // "맞는 비밀번호를 틀렸다" 고 하는 자리를 못 본다.
        let mut v = Vault::open_as(&sealed, PW, NEXT).unwrap();
        let mut saved = Vec::new();
        v.rekey(PW, "바뀐비밀번호2027", |b| {
            saved = b.to_vec();
            Ok(())
        })
        .unwrap();
        assert_eq!(saved[7], NEXT, "바꾼 뒤에 판이 내려갔다");
        assert_eq!(v.container, NEXT);
        // 바꾼 비밀번호로 다시 열린다.
        assert!(Vault::open_as(&saved, "바뀐비밀번호2027", NEXT).is_ok());
    }

    /// 비밀번호를 아는 쪽이 손본 파일을 만든다. 태그는 맞는다.
    ///
    /// **우리 연결로는 만들 수 없다.** `DEFENSIVE` 를 걸어 둔 뒤로 `writable_schema`
    /// 가 막힌다(`table sqlite_master may not be modified`). 그래서 맨 연결에서
    /// 손본 뒤, 같은 열쇠로 봉해 되돌려 준다 — 파일을 받아 든 사람이 할 수 있는
    /// 일이 꼭 그만큼이다.
    fn doctored(change: impl FnOnce(&Connection)) -> Vec<u8> {
        let v = seeded();
        let image = v.conn.serialize("main").unwrap().to_vec();

        let mut plain = Connection::open_in_memory().unwrap();
        plain
            .deserialize_read_exact("main", std::io::Cursor::new(&image), image.len(), false)
            .unwrap();
        change(&plain);

        let doctored = Vault {
            conn: plain,
            key: v.key,
            salt: v.salt,
            container: v.container,
            schema_version: v.schema_version,
        };
        doctored.seal().unwrap()
    }

    /// **앱을 거쳐 간 수정은 적법하다.** 6월에 만든 파일에 9월 성적을 더해
    /// 저장하고, 다른 자리에서 다시 여는 것이 이 프로그램의 보통 하루다.
    ///
    /// 스키마 지문은 표의 **생김새**만 본다. 줄을 넣고 고치고 지우는 것은 지문을
    /// 건드리지 않는다. 이 시험이 서면 담당자가 제 파일에 성적을 못 넣는다는 뜻이고,
    /// 그건 막으려던 어떤 공격보다 나쁘다.
    #[test]
    fn adding_scores_later_and_saving_again_is_fine() {
        // 6월 — 파일을 만들고 내신을 넣는다.
        let june = seeded();
        june.conn
            .execute(
                "INSERT INTO naesin(student_id, subject_set, term, grade_avg)
                   VALUES(1, ?1, '1-1', 2.5)",
                rusqlite::params![crate::exam::SUBJECT_SETS[0]],
            )
            .unwrap();
        let sealed = june.seal().unwrap();

        // 9월 — 다른 자리에서 열어 모의고사를 더하고 다시 저장한다.
        let september = Vault::open(&sealed, PW).expect("6월 파일이 안 열린다");
        september
            .conn
            .execute(
                "INSERT INTO mock(student_id, month, area, percentile, grade_band)
                   VALUES(1, '9월', ?1, 92.0, 2)",
                rusqlite::params![crate::exam::AREA_KOR],
            )
            .unwrap();
        // 고치는 것도, 지우는 것도 마찬가지다.
        september
            .conn
            .execute("UPDATE naesin SET grade_avg = 2.1 WHERE student_id = 1", [])
            .unwrap();
        september.conn.execute("DELETE FROM mock WHERE month = '3월'", []).unwrap();
        let again = september.seal().unwrap();

        // 11월 — 또 연다.
        let november = Vault::open(&again, PW).expect("9월에 저장한 파일이 안 열린다");
        let n: i64 = november
            .conn
            .query_row("SELECT COUNT(*) FROM mock WHERE month = '9월'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1, "더한 성적이 없다");
    }

    /// **값이 바뀐 것까지 막지는 못한다.** 막을 수 없는 것이라 적어 둔다 —
    /// 비밀번호를 가진 사람은 언제나 제 파일을 고쳐 쓸 수 있다. 여기서 지키는
    /// 것은 프로그램이 **모르는 구조**가 실려 오지 않는 것뿐이다.
    #[test]
    fn a_changed_grade_is_not_something_the_container_can_catch() {
        let changed = doctored(|c| {
            c.execute_batch(
                &format!(
                    "INSERT INTO naesin(student_id, subject_set, term, grade_avg)
                       SELECT id, '{}', '1-1', 9.0 FROM students",
                    crate::exam::SUBJECT_SETS[0]
                ),
            )
            .unwrap()
        });
        assert!(Vault::open(&changed, PW).is_ok());
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

    /// 그릇의 판이 더 새것이면 **푸는 방법을 모르는 파일**이다. 매직에 붙여
    /// 두었을 때는 이것이 "우리 파일이 아니다" 로 나왔다.
    #[test]
    fn a_newer_container_says_so() {
        let mut sealed = seeded().seal().unwrap();
        sealed[7] = CONTAINER_VERSION + 1;
        let e = Vault::open(&sealed, PW).unwrap_err();
        assert_eq!(e.code, ErrorCode::VaultContainerUnknown);
        // 매직은 그대로이므로 "우리 파일이 아니다" 가 되어서는 안 된다.
        assert_ne!(e.code, ErrorCode::VaultFormatInvalid);

        // 있은 적 없는 판 0 도 아는 판이 아니다. 한 바이트가 망가진 파일이 여기 온다.
        let mut zeroed = seeded().seal().unwrap();
        zeroed[7] = 0;
        assert_eq!(
            Vault::open(&zeroed, PW).unwrap_err().code,
            ErrorCode::VaultContainerUnknown
        );
    }

    /// **선언해 둔 `ON DELETE CASCADE` 가 실제로 돈다.**
    ///
    /// SQLite 는 외래 키를 기본으로 끄고 연다. 켜지 않으면 스키마에 적어 둔 그 말이
    /// 아무 일도 하지 않아, 학생을 지워도 성적이 파일 안에 고아로 남는다 — 개인정보
    /// 파일에서 삭제가 삭제가 아니게 된다. 지금은 지우는 길이 없지만, 생기는 날
    /// 아무도 이 한 줄을 떠올리지 못한다.
    #[test]
    fn deleting_a_student_takes_their_scores_with_them() {
        let v = seeded();
        v.conn
            .execute(
                "INSERT INTO naesin(student_id, subject_set, term, grade_avg)
                   VALUES(1, ?1, '1-1', 2.0)",
                rusqlite::params![crate::exam::SUBJECT_SETS[0]],
            )
            .unwrap();
        v.conn.execute("DELETE FROM students WHERE id = 1", []).unwrap();

        let left: i64 = v
            .conn
            .query_row("SELECT COUNT(*) FROM naesin WHERE student_id = 1", [], |r| r.get(0))
            .unwrap();
        assert_eq!(left, 0, "지운 학생의 성적이 파일에 남는다");

        // 파일을 거쳐 와도 마찬가지다(설정은 연결마다 다시 걸어야 한다).
        let reopened = Vault::open(&v.seal().unwrap(), PW).unwrap();
        reopened
            .conn
            .execute(
                "INSERT INTO students(id, grade, class_no, number, name) VALUES(2, 3, 1, 2, ?1)",
                rusqlite::params!["아무개"],
            )
            .unwrap();
        reopened
            .conn
            .execute(
                "INSERT INTO mock(student_id, month, area, percentile) VALUES(2, '9월', ?1, 90.0)",
                rusqlite::params![crate::exam::AREA_KOR],
            )
            .unwrap();
        reopened.conn.execute("DELETE FROM students WHERE id = 2", []).unwrap();
        let left: i64 = reopened
            .conn
            .query_row("SELECT COUNT(*) FROM mock WHERE student_id = 2", [], |r| r.get(0))
            .unwrap();
        assert_eq!(left, 0, "다시 연 파일에서는 따라 지워지지 않는다");
    }

    /// 없는 학생의 성적은 **애초에 들어가지 않는다.** 외래 키를 켠 덕이다.
    #[test]
    fn scores_cannot_point_at_a_student_who_is_not_there() {
        let v = seeded();
        let e = v.conn.execute(
            "INSERT INTO naesin(student_id, subject_set, term, grade_avg)
               VALUES(9999, ?1, '1-1', 2.0)",
            rusqlite::params![crate::exam::SUBJECT_SETS[0]],
        );
        assert!(e.is_err(), "없는 학생에게 성적이 붙었다");
    }

    /// 학생 자료가 든 DB 는 **임시 파일을 디스크에 만들지 않는다.**
    #[test]
    fn the_student_database_keeps_its_scratch_in_memory() {
        let v = Vault::create(PW).unwrap();
        let mode: i64 = v.conn.query_row("PRAGMA temp_store", [], |r| r.get(0)).unwrap();
        assert_eq!(mode, 2, "임시 파일이 디스크로 나갈 수 있다 (2 = MEMORY)");
    }

    #[test]
    fn garbage_file_is_rejected_as_format_error() {
        let junk = vec![0u8; 200];
        assert_eq!(Vault::open(&junk, PW).unwrap_err().code, ErrorCode::VaultFormatInvalid);
    }
}
