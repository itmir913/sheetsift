//! **스키마의 판을 상승시키고, 그 구조를 지문으로 동결한다.**
//!
//! 여기 소재하는 것은 **기법뿐이다.** 판마다 실행할 조각과 판마다 기록한 지문은
//! 그릇을 사용하는 앱의 것이라, 부르는 곳이 [`Schema`] 로 구성해 주입한다. 이
//! 저장소는 어떤 표도 인지하지 않는다.
//!
//! ## 판은 DB 내부에 기재한다
//!
//! `PRAGMA user_version` 이 유일한 근거다. 그릇의 머리에도 동일한 값이 기재되나
//! 그것은 **복호화 이전에 사전 여과하려는** 사본이고, 봉인할 때 DB 에서 판독해
//! 기재한다([`crate::vault`]).
//!
//! ## 조각과 지문을 분리해 보관하는 사유
//!
//! 조각은 *무엇을 수행했는가*(`ALTER`·`CREATE`)이고 지문은 *그 결과 무엇이
//! 되었는가*다. 양자를 대조해야 "구 판 파일을 상승시킨 구조가 신규 생성한 파일과
//! 동일한가" 에 답할 수 있다. 상이하면 동일한 판 번호를 진술하는 파일이 두 구조가
//! 되는데, 앱은 그 차이를 영영 인지하지 못한다.
//!
//! ## 조각이 외래 키를 비활성화해야 하는 경우
//!
//! [`Schema::run`] 은 판마다 조각을 트랜잭션으로 포장한다. `PRAGMA foreign_keys` 는
//! **트랜잭션 내부에서 변경되지 않고, 변경하려 해도 아무 진술이 없다.** 표를 삭제했다
//! 재생성하는 조각이면 부르는 곳이 **이 함수를 호출하기 이전에** 비활성화해야 한다.

use rusqlite::Connection;

use crate::error::{Error, ErrorCode, Result};

/// 한 앱의 스키마. 판마다 조각 묶음 하나와 지문 하나.
///
/// `steps[i]` 가 v(i) → v(i+1) 을 수행하고, `fingerprints[i]` 가 v(i+1) 의 스키마
/// 전문이다. **배열의 차례가 실행 차례다** — 여기서 정렬하지 않는다.
#[derive(Debug, Clone, Copy)]
pub struct Schema<'a> {
    steps: &'a [&'a [&'a str]],
    fingerprints: &'a [&'a str],
}

impl<'a> Schema<'a> {
    /// **판 수와 지문 수가 불일치하면 정지한다.** `const` 로 구성하면 그 정지가
    /// 컴파일 시점에 발생한다 — 지문을 누락하고 판을 상승시키는 작업이 시험 이전에
    /// 차단된다.
    pub const fn new(steps: &'a [&'a [&'a str]], fingerprints: &'a [&'a str]) -> Self {
        assert!(
            steps.len() == fingerprints.len(),
            "every schema version needs exactly one recorded fingerprint"
        );
        assert!(!steps.is_empty(), "a schema needs at least one version");
        Self { steps, fingerprints }
    }

    /// 이 앱이 생성하고 판독하는 최고 판. 조각 묶음의 수와 동일하다.
    pub const fn version(&self) -> u32 {
        self.steps.len() as u32
    }

    /// 판마다의 조각. 구 판 파일을 재현하는 시험이 사용한다.
    pub fn steps(&self) -> &'a [&'a [&'a str]] {
        self.steps
    }

    /// 판 `v` 가 보유해야 할 지문. 판 0 은 표가 하나도 부재하다.
    pub fn expected(&self, v: u32) -> Option<&'a str> {
        if v == 0 {
            return Some("");
        }
        self.fingerprints.get(v as usize - 1).copied()
    }

    /// 해제한 알맹이가 **기록한 구조 그대로인지** 판정한다.
    ///
    /// `integrity_check` 는 b-tree 의 정합만 판정한다 — 표를 뷰로 교체하거나
    /// 트리거를 삽입하거나 열을 추가해도 `ok` 다. 여기서 차단하는 것은 **프로그램이
    /// 인지하지 못하는 구조**가 수용되는 것이고, 수용된 **값**의 변경은 판정 대상이
    /// 아니다.
    pub fn check(&self, conn: &Connection) -> Result<()> {
        let v = crate::vault::user_version(conn)?;
        let Some(want) = self.expected(v) else {
            return Err(Error::new(ErrorCode::VaultFormatInvalid)
                .detail(format!("no recorded schema for version {v}")));
        };
        if same_lines(&fingerprint(conn)?) != same_lines(want) {
            return Err(Error::new(ErrorCode::VaultFormatInvalid)
                .detail("schema does not match the recorded one"));
        }
        Ok(())
    }

    /// 판을 이 앱이 인지하는 최고 판까지 상승시킨다.
    ///
    /// **상위 판은 변경하기 이전에 거절한다.** 미지의 구조에 기지의 SQL 을 적용하면
    /// 그 시점에 파일이 훼손된다.
    ///
    /// 판 하나씩, 각각 트랜잭션 내부에서 상승시킨다. 도중에 실패하면 그 판은 전부
    /// 철회되므로 절반만 상승한 파일이 존치되지 않는다 — `user_version` 도 DB 머리의
    /// 값이라 동일한 트랜잭션과 함께 철회된다.
    pub fn run(&self, conn: &Connection) -> Result<()> {
        let current = crate::vault::user_version(conn)?;
        let newest = self.version();

        if current > newest {
            return Err(Error::new(ErrorCode::VaultVersionTooNew)
                .with("fileVersion", current)
                .with("appVersion", newest));
        }

        for (i, fragments) in self.steps.iter().enumerate() {
            let target = i as u32 + 1;
            if current >= target {
                continue;
            }
            let failed = |e: rusqlite::Error| {
                Error::new(ErrorCode::VaultMigrationFailed)
                    .with("fileVersion", current)
                    .with("appVersion", target)
                    .detail(e)
            };
            let tx = conn.unchecked_transaction().map_err(failed)?;
            for sql in *fragments {
                tx.execute_batch(sql).map_err(failed)?;
            }
            tx.execute_batch(&format!("PRAGMA user_version = {target}")).map_err(failed)?;
            tx.commit().map_err(failed)?;
        }
        Ok(())
    }
}

/// 행 종단을 단일 형태로 정합한다. 윈도에서 수령하면 git 이 지문 파일을 CRLF 로
/// 전개하는데, 조각이 생성하는 스키마는 LF 다. 아무것도 변경하지 않았는데 불일치로
/// 판정된다.
///
/// **행 종단만 정돈한다.** `trim()` 은 선두의 공백 행을, `trim_end()` 는 후미의
/// 공백 행을 전부 소거한다 — 지문 원문끼리는 상이한데 **실제로 대조하는 위치에서만**
/// 동일해진다. 정돈할 사유는 기록한 지문 파일의 최종 행 종단 하나뿐이므로 그것만
/// 제거한다.
pub fn same_lines(text: &str) -> String {
    text.replace("\r\n", "\n").trim_end_matches('\n').to_string()
}

/// 이 DB 의 스키마를 행 단위로 나열한다. **판마다 기록한 지문과 대조하는 문자다.**
///
/// **명칭으로 제외하지 않는다.** SQLite 가 `sqlite_` 명칭을 차단하는 것은 SQL 로
/// **생성할 때**뿐이다. `writable_schema` 로 삽입한 `sqlite_wipe` 라는 트리거는
/// 정상 호출되면서 지문에서는 누락된다.
///
/// 제외하는 것은 둘뿐이다.
///
/// - **내부에 부수하는 색인** — `type` 이 `index` 이고 `sql` 이 부재한 것. 그
///   근거가 되는 표의 문자는 이미 지문에 존재한다.
/// - **질의 계획용 통계 표**(`sqlite_stat*`) — `ANALYZE` 가 생성하는 것이라 수용된
///   자료와 무관하다. 제외하지 않으면 누군가 `ANALYZE` 한 행을 추가하는 시점에 **이미
///   배포된 파일이 전부 개방되지 않는다.**
///
/// 통계 표는 **문자 하나까지 동일해야** 제외한다. `LIKE 'sqlite_stat%'` 은 `_` 가
/// 와일드카드라 `sqliteZstat1` 까지 제외했고, 여는 괄호 후방을 자유로 둔 패턴은
/// `sqlite_stat1(tbl,idx,stat,payload)` 라는 **임의의 표**를 은폐했다. SQLite 가
/// `ANALYZE` 에서 기재하는 문자 그대로만 제외한다. 이 저장소는 SQLite 를 함께
/// 빌드하므로(`bundled`) 그 문자는 잠금 파일에 고정되고, 판이 상승해 문자가
/// 변경되면 `analyzing_does_not_change_the_fingerprint` 가 정지한다.
///
/// NULL 인 항목은 전부 공백 문자로 수령한다. `AND`·`NOT` 의 3값 논리에서 NULL 은
/// 묵시적으로 누락되는데, **누락되는 행이 존재한다는 것 자체가 은폐할 위치**다.
pub fn fingerprint(conn: &Connection) -> Result<String> {
    let mut stmt = conn.prepare(
        "SELECT COALESCE(type, ''), COALESCE(name, ''), COALESCE(sql, '') FROM sqlite_master
         WHERE (type IS NOT 'index' OR sql IS NOT NULL)
           AND NOT (type IS 'table' AND COALESCE(sql, '') IN (
                      'CREATE TABLE sqlite_stat1(tbl,idx,stat)',
                      'CREATE TABLE sqlite_stat4(tbl,idx,neq,nlt,ndlt,sample)'))
         ORDER BY type, name",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(format!(
            "{} {}\n{}",
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?
        ))
    })?;
    Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?.join("\n\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /* 시험용 스키마. 이 저장소는 어떤 앱의 표도 인지하지 않으므로, 기법을 판정할
       최소한의 구조를 여기서 구성한다. */

    const V1: &[&str] = &["CREATE TABLE notes(id INTEGER PRIMARY KEY, body TEXT NOT NULL);"];
    const V2: &[&str] = &["ALTER TABLE notes ADD COLUMN tag TEXT;"];

    const FP1: &str = "table notes\nCREATE TABLE notes(id INTEGER PRIMARY KEY, body TEXT NOT NULL)";
    const FP2: &str = "table notes\nCREATE TABLE notes(id INTEGER PRIMARY KEY, body TEXT NOT NULL, tag TEXT)";

    const SCHEMA: Schema<'static> = Schema::new(&[V1, V2], &[FP1, FP2]);

    fn fresh() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        SCHEMA.run(&conn).unwrap();
        conn
    }

    /// 판 `v` 까지만 상승시킨 DB. **구 판 파일을 재현하는 위치다.**
    fn at_version(v: u32) -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        for fragments in SCHEMA.steps().iter().take(v as usize) {
            for sql in *fragments {
                conn.execute_batch(sql).unwrap();
            }
        }
        conn.execute_batch(&format!("PRAGMA user_version = {v}")).unwrap();
        conn
    }

    fn version(conn: &Connection) -> u32 {
        crate::vault::user_version(conn).unwrap()
    }

    #[test]
    fn a_new_file_is_at_the_newest_version_and_matches_it() {
        let conn = fresh();
        assert_eq!(version(&conn), SCHEMA.version());
        SCHEMA.check(&conn).unwrap();
    }

    /// 2회 실행해도 동일하다. 파일을 개방할 때마다 실행되는 경로라 반드시 그래야 한다.
    #[test]
    fn running_twice_changes_nothing() {
        let conn = fresh();
        let before = fingerprint(&conn).unwrap();
        SCHEMA.run(&conn).unwrap();
        assert_eq!(fingerprint(&conn).unwrap(), before);
    }

    /// **상위 판은 변경하기 이전에 거절한다.**
    #[test]
    fn a_newer_file_is_refused_untouched() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(&format!("PRAGMA user_version = {}", SCHEMA.version() + 1)).unwrap();
        let e = SCHEMA.run(&conn).unwrap_err();
        assert_eq!(e.code, ErrorCode::VaultVersionTooNew);
        assert_eq!(e.params["appVersion"], SCHEMA.version().to_string());
        assert_eq!(fingerprint(&conn).unwrap(), "", "상위 판 파일을 변경했다");
    }

    /// 구 판마다 상승시킨 구조가 신규 생성한 것과 동일한가. 조각을 "공백에서
    /// 생성하는 전문" 으로 기재하는 실수가 여기서 포착된다.
    #[test]
    fn every_older_file_climbs_to_the_same_shape() {
        let newest = fingerprint(&fresh()).unwrap();
        for v in 0..SCHEMA.version() {
            let conn = at_version(v);
            SCHEMA.run(&conn).unwrap_or_else(|e| panic!("v{v} 를 상승시키지 못했다: {e:?}"));
            assert_eq!(version(&conn), SCHEMA.version());
            assert_eq!(fingerprint(&conn).unwrap(), newest, "v{v} 를 상승시킨 구조가 상이하다");
        }
    }

    /// 판마다 그 판의 구조가 기록과 동일한가.
    #[test]
    fn every_version_matches_its_recorded_fingerprint() {
        for v in 0..=SCHEMA.version() {
            let got = fingerprint(&at_version(v)).unwrap();
            assert_eq!(same_lines(&got), same_lines(SCHEMA.expected(v).unwrap()), "v{v}");
        }
        assert_eq!(SCHEMA.expected(SCHEMA.version() + 1), None);
    }

    /// **실패한 판은 전부 철회된다.** 판 번호도 철회되므로 절반만 상승한 파일이
    /// 존치되지 않는다.
    #[test]
    fn a_failing_step_leaves_nothing_behind() {
        const BAD: &[&str] = &["CREATE TABLE half(x);", "THIS IS NOT SQL;"];
        let broken = Schema::new(&[V1, BAD], &[FP1, ""]);
        let conn = at_version(1);
        let e = broken.run(&conn).unwrap_err();
        assert_eq!(e.code, ErrorCode::VaultMigrationFailed);
        assert_eq!(e.params["fileVersion"], "1");
        assert_eq!(e.params["appVersion"], "2");
        assert_eq!(version(&conn), 1, "판 번호가 철회되지 않았다");
        assert_eq!(same_lines(&fingerprint(&conn).unwrap()), FP1, "절반만 상승한 구조가 존치되었다");
    }

    /// 기록과 상이한 구조는 [`Schema::check`] 가 거절한다. 기록이 부재한 판도 동일하다.
    #[test]
    fn a_shape_that_is_not_recorded_is_refused() {
        let widened = fresh();
        widened.execute_batch("ALTER TABLE notes ADD COLUMN secret TEXT").unwrap();
        assert_eq!(SCHEMA.check(&widened).unwrap_err().code, ErrorCode::VaultFormatInvalid);

        let trapped = fresh();
        trapped
            .execute_batch("CREATE TRIGGER wipe AFTER INSERT ON notes BEGIN UPDATE notes SET body = 'X'; END;")
            .unwrap();
        assert_eq!(SCHEMA.check(&trapped).unwrap_err().code, ErrorCode::VaultFormatInvalid);

        let unknown = fresh();
        unknown.execute_batch(&format!("PRAGMA user_version = {}", SCHEMA.version() + 1)).unwrap();
        assert_eq!(SCHEMA.check(&unknown).unwrap_err().code, ErrorCode::VaultFormatInvalid);
    }

    /// 지문 파일이 CRLF 로 전개되어도, 최종 행 종단이 부착되어도 동일하다.
    #[test]
    fn line_endings_do_not_count() {
        assert_eq!(same_lines("a\r\nb\r\n"), same_lines("a\nb"));
    }

    /// 대조하는 위치가 **선두의 공백 행을 소거하지 않는다.**
    #[test]
    fn the_comparison_does_not_swallow_a_blank_first_row() {
        let empty_row = format!("{} {}\n{}", "", "", "");
        assert_ne!(
            same_lines(&format!("{empty_row}\n\ntable notes\nCREATE …")),
            same_lines("table notes\nCREATE …"),
        );
    }

    /// `writable_schema` 로 행 하나를 삽입한 연결. 우리 연결은 그것을 차단하므로
    /// 순수 연결에서 수행한다 — 파일을 수령한 사람이 수행할 수 있는 작업이 그만큼이다.
    fn planted(row: &str) -> (String, String) {
        let conn = fresh();
        let before = fingerprint(&conn).unwrap();
        conn.execute_batch(&format!(
            "PRAGMA writable_schema = ON;
             INSERT INTO sqlite_master(type, name, tbl_name, rootpage, sql) VALUES({row});
             PRAGMA writable_schema = OFF;"
        ))
        .unwrap();
        (same_lines(&before), same_lines(&fingerprint(&conn).unwrap()))
    }

    /// 종류·명칭이 부재한 행도, 예약 명칭 후방의 트리거도 지문에 출현한다.
    #[test]
    fn nothing_planted_slips_past_the_fingerprint() {
        for row in [
            "NULL, 'ghost', 'notes', 0, NULL",
            "'table', NULL, 'notes', 0, NULL",
            "'trigger', 'sqlite_wipe', 'notes', 0, 'CREATE TRIGGER sqlite_wipe AFTER INSERT ON notes BEGIN UPDATE notes SET body = ''X''; END'",
        ] {
            let (before, after) = planted(row);
            assert_ne!(before, after, "지문에서 누락되었다: {row}");
        }
    }

    /// **통계 표의 명칭 후방에 은폐할 수 없다.** 명칭만 동일하고 문자가 상이하면
    /// 지문에 출현한다.
    #[test]
    fn a_table_wearing_the_statistics_name_is_seen() {
        for (name, sql) in [
            ("sqlite_stat1", "CREATE TABLE sqlite_stat1(tbl,idx,stat,payload TEXT)"),
            ("sqlite_stat1", "CREATE TABLE sqlite_stat1(a,b,c)"),
            ("sqliteZstat1", "CREATE TABLE sqliteZstat1(tbl,idx,stat)"),
            ("SQLITE_STAT1", "CREATE TABLE SQLITE_STAT1(tbl,idx,stat)"),
        ] {
            let (before, after) = planted(&format!("'table', '{name}', '{name}', 0, '{sql}'"));
            assert_ne!(before, after, "통계 표 명칭 후방에 은폐되었다: {sql}");
        }
    }

    /// **`ANALYZE` 를 실행해도 지문은 동일하다.** 상이하면 그 한 행에 배포된 파일이
    /// 전부 개방되지 않는다. SQLite 가 기재하는 문자가 변경되면 여기서 정지한다.
    #[test]
    fn analyzing_does_not_change_the_fingerprint() {
        let conn = fresh();
        conn.execute_batch("INSERT INTO notes(body) VALUES('a'); CREATE INDEX by_body ON notes(body);")
            .unwrap();
        let before = fingerprint(&conn).unwrap();
        conn.execute_batch("ANALYZE").unwrap();
        assert_eq!(fingerprint(&conn).unwrap(), before);
    }
}
