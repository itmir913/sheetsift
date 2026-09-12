//! 암호화된 학생 데이터 파일(`students.db`).
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
//!      0     8  "GOENAVI\x01"  매직 + 컨테이너 버전
//!      8     4  schema_version (little endian)
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

const MAGIC: &[u8; 8] = b"GOENAVI\x01";
const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 12;
const HEADER_LEN: usize = 8 + 4 + SALT_LEN + NONCE_LEN;
/// GCM 인증 태그 길이.
const TAG_LEN: usize = 16;

/// 학생 데이터 스키마 버전. 구조를 바꾸면 반드시 올린다.
/// 파일 버전이 앱이 아는 것보다 높으면 열지 않고 안내한다.
pub const SCHEMA_VERSION: u32 = 1;

/// 비밀번호를 너무 짧게 잡으면 암호화가 의미를 잃는다.
pub const MIN_PASSWORD_LEN: usize = 8;

fn derive_key(password: &str, salt: &[u8]) -> AppResult<[u8; 32]> {
    let mut key = [0u8; 32];
    Argon2::default()
        .hash_password_into(password.as_bytes(), salt, &mut key)
        .map_err(|e| AppError::new(ErrorCode::VaultKeyDerivationFailed).detail(e))?;
    Ok(key)
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

        let conn = Connection::open_in_memory()
            .map_err(|e| AppError::new(ErrorCode::VaultOpenFailed).detail(e))?;
        crate::students::init_schema(&conn)?;

        Ok(Self { conn, key, salt, schema_version: SCHEMA_VERSION })
    }

    /// 파일 내용을 복호화해 메모리에서 연다.
    pub fn open(bytes: &[u8], password: &str) -> AppResult<Self> {
        if bytes.len() < HEADER_LEN + TAG_LEN || &bytes[..8] != MAGIC {
            return Err(AppError::new(ErrorCode::VaultFormatInvalid)
                .detail("magic mismatch or file too short"));
        }
        let schema_version =
            u32::from_le_bytes(bytes[8..12].try_into().expect("length checked above"));
        if schema_version > SCHEMA_VERSION {
            return Err(AppError::new(ErrorCode::VaultVersionTooNew)
                .with("fileVersion", schema_version)
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

        let mut conn = Connection::open_in_memory()
            .map_err(|e| AppError::new(ErrorCode::VaultOpenFailed).detail(e))?;
        let len = plain.len();
        conn.deserialize_read_exact("main", std::io::Cursor::new(&plain), len, false)
            .map_err(|e| AppError::new(ErrorCode::VaultOpenFailed).detail(e))?;
        plain.zeroize();

        Ok(Self { conn, key, salt, schema_version })
    }

    /// 현재 내용을 암호화한 바이트로 만든다. 파일 쓰기는 호출 쪽이 한다.
    /// 논스는 저장할 때마다 새로 뽑는다(같은 키로 논스를 재사용하지 않는다).
    pub fn seal(&self) -> AppResult<Vec<u8>> {
        let data = self
            .conn
            .serialize("main")
            .map_err(|e| AppError::new(ErrorCode::VaultSealFailed).detail(e))?;

        let mut nonce = [0u8; NONCE_LEN];
        rand::rng().fill_bytes(&mut nonce);

        let mut out = Vec::with_capacity(HEADER_LEN + data.len() + TAG_LEN);
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&SCHEMA_VERSION.to_le_bytes());
        out.extend_from_slice(&self.salt);
        out.extend_from_slice(&nonce);

        let cipher = Aes256Gcm::new((&self.key).into());
        let ct = cipher
            .encrypt(&Nonce::from(nonce), Payload { msg: &data, aad: &out })
            .map_err(|e| AppError::new(ErrorCode::VaultSealFailed).detail(e))?;
        out.extend_from_slice(&ct);
        Ok(out)
    }
}

/// 파일로 안전하게 쓴다. 임시 파일에 쓴 뒤 바꿔치기해서,
/// 저장 도중 전원이 꺼져도 기존 파일이 반쯤 망가지지 않게 한다.
pub fn write_atomically(path: &std::path::Path, bytes: &[u8]) -> AppResult<()> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes)
        .map_err(|e| AppError::new(ErrorCode::VaultWriteFailed).detail(e))?;
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        AppError::new(ErrorCode::VaultWriteFailed).detail(e)
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn garbage_file_is_rejected_as_format_error() {
        let junk = vec![0u8; 200];
        assert_eq!(Vault::open(&junk, PW).unwrap_err().code, ErrorCode::VaultFormatInvalid);
    }
}
