//! 암호화된 학생 데이터 파일(`.navi`).
//!
//! ## 그릇은 `sheetsift-core` 의 것이다
//!
//! 머리 배치·열쇠 뽑는 비용·깨진 파일과 틀린 비밀번호를 가르는 셈·비밀번호
//! 바꾸기는 [`sheetsift_core::vault`] 에 있다. 여기서 넣는 것은 **이 앱이 담는
//! 것**이다 — 매직(`GOENAVI`), 학생 스키마의 판, 표를 만들고 올리는 법
//! ([`crate::migrate`]), 그리고 풀어 낸 알맹이가 우리가 만든 생김새인지 견주는 법.
//!
//! 이미 나간 파일이 아직 열리는지는 여기서 본다(`fixtures/released-v1.navi`).
//! 그릇의 약속을 시험용 그릇으로 못 박는 시험은 core 에 있고, 이쪽은 **실제로
//! 나간 파일**로 한 번 더 본다.
//!
//! 비밀번호는 담당교사가 정하고, 담임에게 파일과 **다른 경로로** 전달한다.
//! (zip 은 메일, 비밀번호는 문자 — 파일만 새어 나가도 열리지 않는다.)
//!
//! ## 열어 둔 채로는 잠그지 않는다 — 정한 것이다
//!
//! 상담은 학생을 앞에 두고 30분씩 이어지고, 그 사이에 비밀번호를 다시 묻는
//! 프로그램은 선생님이 창을 닫아 두거나 더 쉬운 비밀번호를 고르게 만든다. 파일을
//! 열어 둔 자리는 교무실이고, 그 자리의 보안은 화면 잠금이 진다.

use rusqlite::Connection;
use sheetsift_core::vault::{self, Format};

use crate::error::{AppError, AppResult, ErrorCode};

pub use crate::migrate::SCHEMA_VERSION;
pub use vault::MIN_PASSWORD_LEN;

/// 열려 있는 학생 데이터. 메모리에만 존재한다.
pub type Vault = vault::Vault<StudentFile>;

/// 학생 데이터 파일이 담는 것.
pub struct StudentFile;

impl Format for StudentFile {
    type Error = AppError;

    /// 사람이 주고받는 파일이라 매직이 이름표 노릇을 한다.
    const MAGIC: [u8; vault::MAGIC_LEN] = *b"GOENAVI";

    const SCHEMA_VERSION: u32 = SCHEMA_VERSION;

    fn migrate(conn: &Connection) -> AppResult<()> {
        crate::migrate::run(conn)
    }

    /* **표의 생김새가 우리가 만든 그대로인가.**

       장부가 성한 것과 담긴 것이 우리 것인 것은 다른 말이다. `integrity_check` 는
       b-tree 가 앞뒤 맞는지만 본다 — 표를 뷰로 바꿔치우거나, 트리거를 심거나,
       열을 하나 더해도 `ok` 다. 비밀번호를 아는 쪽이 만든 파일이면 태그도 맞는다.

       판마다 스키마 전문을 박아 두고(판 폴더의 schema.txt) 여는 길에서 그것과
       견준다.

       담긴 **값**이 바뀐 것은 여전히 못 잡는다. 그건 암호가 답할 수 있는 물음이
       아니다 — 비밀번호를 가진 사람은 언제나 제 파일을 고쳐 쓸 수 있다. 여기서
       막는 것은 **프로그램이 모르는 구조**가 실려 오는 것이다. */
    fn check(conn: &Connection) -> AppResult<()> {
        let invalid = || ErrorCode::Core(sheetsift_core::ErrorCode::VaultFormatInvalid);
        let v = crate::migrate::user_version(conn)?;
        let Some(want) = crate::migrate::expected_fingerprint(v) else {
            return Err(AppError::new(invalid())
                .detail(format!("no recorded schema for version {v}")));
        };
        let got = crate::migrate::fingerprint(conn)?;
        if crate::migrate::same_lines(&got) != crate::migrate::same_lines(want) {
            return Err(AppError::new(invalid()).detail("schema does not match the recorded one"));
        }
        Ok(())
    }
}

/// 파일로 안전하게 쓴다. 임시 파일에 쓴 뒤 바꿔치기해서,
/// 저장 도중 전원이 꺼져도 기존 파일이 반쯤 망가지지 않게 한다.
///
/// 임시 이름은 **확장자를 갈지 않고 뒤에 덧붙인다.** `with_extension("tmp")`
/// 는 `학생데이터.navi` 를 `학생데이터.tmp` 로 만드는데, 같은 자리에 담당자가
/// 둔 `학생데이터.tmp` 가 있으면 그것을 말없이 덮어쓴다.
pub fn write_atomically(path: &std::path::Path, bytes: &[u8]) -> AppResult<()> {
    let failed = || AppError::new(ErrorCode::Core(sheetsift_core::ErrorCode::VaultWriteFailed));
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
        failed().detail(e)
    })?;
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        failed().detail(e)
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sheetsift_core::ErrorCode as Core;
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

    /// **매직은 이미 나간 약속이다.** 담당자 손의 `.navi` 가 이 일곱 글자로 시작한다.
    #[test]
    fn the_magic_is_pinned() {
        assert_eq!(&StudentFile::MAGIC, b"GOENAVI", "매직이 달라졌습니다");
        assert_eq!(&seeded().seal().unwrap()[..7], b"GOENAVI");
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
    fn plaintext_never_appears_in_the_file() {
        let sealed = seeded().seal().unwrap();
        assert!(!sealed.starts_with(b"SQLite format 3\0"), "평문 SQLite 헤더가 보인다");
        let needle = "김우현".as_bytes();
        assert!(
            !sealed.windows(needle.len()).any(|w| w == needle),
            "학생 이름이 평문으로 남아 있다"
        );
    }

    /// 옛 판 파일은 **막지 않고 올려서 연다.** 담당자가 3월에 만든 파일을
    /// 11월에 여는 일이 이 프로그램에서는 보통이다.
    #[test]
    fn an_older_file_opens_after_being_migrated() {
        // 판을 매기기 전(0판) 파일을 그대로 만든다 — 표가 하나도 없다.
        let old = Vault::create(PW).unwrap();
        let sealed = old.seal_connection(&Connection::open_in_memory().unwrap()).unwrap();
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

    /// 임시 이름은 확장자를 갈지 않고 덧붙인다. 갈아 버리면 같은 자리의
    /// `<같은이름>.tmp` 를 말없이 덮어쓴다.
    #[test]
    fn the_temporary_name_does_not_eat_a_neighbours_file() {
        let dir = crate::testing::temp_dir("tmp-name");
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
            ErrorCode::Core(Core::VaultFormatInvalid),
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
            ErrorCode::Core(Core::VaultFormatInvalid),
            "트리거가 심긴 파일이 열렸다"
        );

        // 열을 하나 더한다. 우리가 모르는 자리다.
        let widened = doctored(|c| {
            c.execute_batch("ALTER TABLE students ADD COLUMN secret TEXT").unwrap()
        });
        assert_eq!(
            Vault::open(&widened, PW).unwrap_err().code,
            ErrorCode::Core(Core::VaultFormatInvalid),
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
                ErrorCode::Core(Core::VaultFormatInvalid),
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
            ErrorCode::Core(Core::VaultFormatInvalid),
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
            ErrorCode::Core(Core::VaultFormatInvalid)
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
            ErrorCode::Core(Core::VaultFormatInvalid),
            "`sqlite_` 이름 뒤에 숨은 트리거가 지나갔다"
        );
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

        v.seal_connection(&plain).unwrap()
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
}
