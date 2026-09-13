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
//!     40     …  AES-256-GCM(직렬화된 SQLite) + 인증 태그 16 B
//! ```
//! 앞 40바이트(헤더)는 AAD 로 함께 인증한다. 헤더 한 바이트만 고쳐도 복호화가 실패한다.
//!
//! 비밀번호는 담당교사가 정하고, 담임에게 파일과 **다른 경로로** 전달한다.
//! (zip 은 메일, 비밀번호는 문자 — 파일만 새어 나가도 열리지 않는다.)

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
const HEADER_LEN: usize = 7 + 1 + 4 + SALT_LEN + NONCE_LEN;
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

   값은 OWASP 가 말하는 Argon2id 최소치다. 올리려면 그릇의 판을 함께 올려야
   한다 — 옛 파일은 옛 비용으로 뽑아야 열린다. */
const KDF_MEMORY_KIB: u32 = 19_456;
const KDF_PASSES: u32 = 2;
const KDF_LANES: u32 = 1;

fn kdf() -> AppResult<Argon2<'static>> {
    let params = argon2::Params::new(KDF_MEMORY_KIB, KDF_PASSES, KDF_LANES, None)
        .map_err(|e| AppError::new(ErrorCode::VaultKeyDerivationFailed).detail(e))?;
    Ok(Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params))
}

fn derive_key(password: &str, salt: &[u8]) -> AppResult<[u8; 32]> {
    let mut key = [0u8; 32];
    kdf()?
        .hash_password_into(password.as_bytes(), salt, &mut key)
        .map_err(|e| AppError::new(ErrorCode::VaultKeyDerivationFailed).detail(e))?;
    Ok(key)
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
    conn.execute_batch("PRAGMA temp_store = MEMORY;")
        .map_err(|e| AppError::new(ErrorCode::VaultOpenFailed).detail(e))?;
    Ok(conn)
}

fn check_password(password: &str) -> AppResult<()> {
    if password.chars().count() < MIN_PASSWORD_LEN {
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

        Ok(Self { conn, key, salt, schema_version: SCHEMA_VERSION })
    }

    /// 파일 내용을 복호화해 메모리에서 연다.
    pub fn open(bytes: &[u8], password: &str) -> AppResult<Self> {
        if bytes.len() < HEADER_LEN + TAG_LEN || &bytes[..7] != MAGIC {
            return Err(AppError::new(ErrorCode::VaultFormatInvalid)
                .detail("magic mismatch or file too short"));
        }
        // 그릇의 판. 더 새것이면 **푸는 방법 자체를 모르는** 파일이다.
        if bytes[7] > CONTAINER_VERSION {
            return Err(AppError::new(ErrorCode::VaultVersionTooNew)
                .with("fileVersion", bytes[7])
                .with("appVersion", CONTAINER_VERSION));
        }
        // 머리에 적힌 스키마 판. **알맹이를 풀기 전에** 상위 판을 걸러 낸다 —
        // 모르는 구조를 애써 풀어 놓고 그제야 막을 까닭이 없다.
        //
        // 머리는 AAD 로 함께 인증되므로 **위조는 못 한다** — 한 바이트만 고쳐도
        // 복호화가 반드시 실패한다. 다만 이 미리 거르기는 인증보다 앞서 돌므로,
        // 망가진 파일이 `더 새 판입니다` 로 잘못 갈릴 수는 있다. 그 값으로 하는
        // 일이 거절뿐이라 더 나빠질 것은 없다. 근거는 어디까지나 복호화 뒤에 읽는
        // DB 안의 `user_version` 이고, 이건 미리 거르려고 두는 사본이다.
        let header_version =
            u32::from_le_bytes(bytes[8..12].try_into().expect("length checked above"));
        if header_version > SCHEMA_VERSION {
            return Err(AppError::new(ErrorCode::VaultVersionTooNew)
                .with("fileVersion", header_version)
                .with("appVersion", SCHEMA_VERSION));
        }

        let mut salt = [0u8; SALT_LEN];
        salt.copy_from_slice(&bytes[12..12 + SALT_LEN]);
        let nonce: [u8; NONCE_LEN] = bytes[12 + SALT_LEN..HEADER_LEN]
            .try_into()
            .expect("length checked above");
        let header = &bytes[..HEADER_LEN];
        let ciphertext = &bytes[HEADER_LEN..];

        let key = derive_key(password, &salt)?;
        let cipher = Aes256Gcm::new((&key).into());
        let mut plain = cipher
            .decrypt(&Nonce::from(nonce), Payload { msg: ciphertext, aad: header })
            // 비밀번호 오류와 파일 손상은 구분할 수 없다. 구분해서도 안 된다.
            .map_err(|_| AppError::new(ErrorCode::VaultPasswordWrong))?;

        let mut conn = memory_db()?;
        let len = plain.len();
        conn.deserialize_read_exact("main", std::io::Cursor::new(&plain), len, false)
            .map_err(|e| AppError::new(ErrorCode::VaultOpenFailed).detail(e))?;
        plain.zeroize();

        // 여기서부터 근거는 DB 안의 판이다. 옛 파일은 막지 않고 올린다 —
        // 3월에 만든 파일을 11월에 여는 일이 이 프로그램에서는 보통이다.
        crate::migrate::run(&conn)?;
        let schema_version = crate::migrate::user_version(&conn)?;

        Ok(Self { conn, key, salt, schema_version })
    }

    /// 현재 내용을 암호화한 바이트로 만든다. 파일 쓰기는 호출 쪽이 한다.
    /// 논스는 저장할 때마다 새로 뽑는다(같은 키로 논스를 재사용하지 않는다).
    pub fn seal(&self) -> AppResult<Vec<u8>> {
        self.seal_with(&self.key, &self.salt)
    }

    fn seal_with(&self, key: &[u8; 32], salt: &[u8; SALT_LEN]) -> AppResult<Vec<u8>> {
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
        out.push(CONTAINER_VERSION);
        out.extend_from_slice(&version.to_le_bytes());
        out.extend_from_slice(salt);
        out.extend_from_slice(&nonce);

        let cipher = Aes256Gcm::new(key.into());
        let ct = cipher
            .encrypt(&Nonce::from(nonce), Payload { msg: &data, aad: &out })
            .map_err(|e| AppError::new(ErrorCode::VaultSealFailed).detail(e))?;
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
        let mut proof = derive_key(current, &self.salt)?;
        let ok = constant_time_eq(&proof, &self.key);
        proof.zeroize();
        if !ok {
            return Err(AppError::new(ErrorCode::VaultPasswordWrong));
        }
        check_password(next)?;

        // 솔트도 새로 뽑는다. 같은 솔트를 이어 쓰면 옛 비밀번호로 만든 무지개표가
        // 그대로 먹힌다.
        let mut salt = [0u8; SALT_LEN];
        rand::rng().fill_bytes(&mut salt);
        let mut key = derive_key(next, &salt)?;

        let result = self.seal_with(&key, &salt).and_then(|bytes| write(&bytes));
        if let Err(e) = result {
            key.zeroize();
            return Err(e);
        }

        self.key.zeroize();
        self.key = key;
        self.salt = salt;
        // `[u8; 32]` 는 Copy 라 옮겨도 지역 사본이 스택에 남는다.
        key.zeroize();
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

    #[test]
    fn tampering_is_detected() {
        let mut sealed = seeded().seal().unwrap();
        // 본문 한 바이트를 바꾼다 → GCM 태그 검증 실패
        let last = sealed.len() - 30;
        sealed[last] ^= 0xff;
        assert_eq!(Vault::open(&sealed, PW).unwrap_err().code, ErrorCode::VaultPasswordWrong);
    }

    #[test]
    fn header_tampering_is_detected() {
        let mut sealed = seeded().seal().unwrap();
        sealed[20] ^= 0x01; // 솔트를 건드린다 → 키가 달라지므로 실패
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
            key: derive_key(PW, &salt).unwrap(),
            salt,
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
            hex(&key),
            "8febb810076c7f36afb488444be4e63346d68940d16533b446bfe1e584f856c5",
            "\n열쇠를 뽑는 방법이 달라졌습니다. 비용·알고리즘·판 중 무엇이 바뀌었는지\n\
             확인하세요. 이미 나간 파일이 있으면 그 파일들은 열리지 않습니다.\n"
        );
        let p = kdf().unwrap().params().clone();
        assert_eq!((p.m_cost(), p.t_cost(), p.p_cost()), (KDF_MEMORY_KIB, KDF_PASSES, KDF_LANES));
    }

    /// 그릇의 판이 더 새것이면 **푸는 방법을 모르는 파일**이다. 매직에 붙여
    /// 두었을 때는 이것이 "우리 파일이 아니다" 로 나왔다.
    #[test]
    fn a_newer_container_says_so() {
        let mut sealed = seeded().seal().unwrap();
        sealed[7] = CONTAINER_VERSION + 1;
        let e = Vault::open(&sealed, PW).unwrap_err();
        assert_eq!(e.code, ErrorCode::VaultVersionTooNew);
        // 매직은 그대로이므로 "우리 파일이 아니다" 가 되어서는 안 된다.
        assert_ne!(e.code, ErrorCode::VaultFormatInvalid);
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
