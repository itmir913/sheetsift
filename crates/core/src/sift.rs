//! **`.sift` — 1회 변환해 둔 표를 수용하는 파일.**
//!
//! xlsx 를 개방해 SQLite 로 변환하는 데는 시간이 소요된다. 동일한 표를 재열람할
//! 때마다 그 비용을 지불할 사유가 없으므로, 변환한 것을 파일로 수용한다.
//!
//! ## 수용되는 것
//!
//! 알맹이는 [`sheet`](crate::sheet) 가 생성한 SQLite 그대로다. 거기에 표 하나를
//! 추가 배치한다 — 어디서 유래한 자료이고 어떤 열이 존재하는지([`Meta`]).
//!
//! **지문을 함께 수용하는 사유**가 존재한다. `.sift` 는 원본의 사본이라, 원본이
//! 변경되면 노후한다. 명칭·용량·해시를 기재해 두면 재개방 시 **동일한 원본에서
//! 유래한 것인지**를 그 위치에서 판별할 수 있다. 노후한 사본을 최신으로 표시하는
//! 것이 이 형식이 범할 수 있는 최악의 행위다.
//!
//! ## 비밀번호가 기본이다
//!
//! [`vault`](crate::vault) 봉투에 수용한다. 표에는 사람의 성명이나 성적이 포함될
//! 수 있고, **수용 시점에 선택하게 하면 누구도 선택하지 않는다.** 평문으로
//! 수용하는 경로([`seal_plain`])는 별도로 존재하고, 쓰는 곳이 의도적으로
//! 호출해야 한다.
//!
//! ## 판을 상승시키는 방법
//!
//! 머리의 매직과 스키마 판은 [`vault`] 가 판정한다. 알맹이 내부의 [`Meta`] 판은
//! [`FORMAT_VERSION`] 이다. 양자 모두 **고정값 시험이 고정한다** — 그 시험이
//! 중단되면 이미 배포된 파일이 개방되지 않는다는 의미다. 값을 변경하지 말고 판을
//! 상승시킨다.

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::vault::{self, Vault};
use crate::{Error, ErrorCode, Result};

/// 파일 최선두의 7바이트. **이것이 `.sift` 라는 표식이다.**
///
/// 열쇠가 아니라 **형식의 일부**라 여기 기재한다. 봉인 대칭키와 서명 열쇠는
/// 부르는 앱이 주입하지만(그래야 공개 저장소에 배치할 수 있다), 표식을 앱마다
/// 상이하게 배치하면 동일한 `.sift` 가 앱마다 상이한 파일이 된다.
pub const MAGIC: [u8; vault::MAGIC_LEN] = *b"SIFT\0v1";

/// 알맹이의 스키마 판. 표가 증가하거나 변경되면 상승시킨다.
pub const SCHEMA_VERSION: u32 = 1;

/// [`Meta`] 의 판. 수용되는 값이 증가하거나 의미가 변경되면 상승시킨다.
pub const FORMAT_VERSION: u32 = 1;

/// 지문과 열 정보를 수용하는 표. **단일 행만 수용된다** — `CHECK` 가 SQLite 에서
/// 그것을 보장한다. 2행이 되는 경로를 코드로 차단하면 언젠가 그 경로가 발생한다.
const META_DDL: &str =
    "CREATE TABLE sift_meta(id INTEGER PRIMARY KEY CHECK (id = 1), json TEXT NOT NULL)";

/* ── 담기는 것 ──────────────────────────────────────────────────── */

/// 어디서 유래한 자료인가. **사본의 노후 여부를 판정하는 데 사용한다.**
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Source {
    /// 원본 파일의 명칭. 경로가 아니라 명칭이다 — 경로는 개인 컴퓨터의 사정이고,
    /// 파일을 수수하면 의미가 소멸한다.
    pub name: String,
    /// 원본의 바이트 수. 해시보다 선행해 판정하는 값이다.
    pub bytes: u64,
    /// 원본의 sha256, 소문자 16진수.
    ///
    /// 문자로 수용하는 사유는 사람이 `sha256sum` 으로 단독 대조할 수 있어서다.
    pub sha256: String,
}

impl Source {
    /// 원본 바이트에서 지문을 산출한다.
    pub fn of(name: &str, bytes: &[u8]) -> Self {
        use sha2::Digest;
        let digest = sha2::Sha256::digest(bytes);
        Self {
            name: name.to_string(),
            bytes: bytes.len() as u64,
            sha256: digest.iter().map(|b| format!("{b:02x}")).collect(),
        }
    }

    /// 이 지문이 그 바이트에서 유래한 것인가.
    pub fn matches(&self, bytes: &[u8]) -> bool {
        self.bytes == bytes.len() as u64 && self.sha256 == Self::of(&self.name, bytes).sha256
    }
}

/// 열 하나. 조회 화면이 무엇을 묘사할지 판단하는 데 필요한 만큼만 수용한다.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnInfo {
    /// DB 열 명칭.
    pub column: String,
    /// 화면에 노출되는 명칭. 원본 머리글이다.
    pub label: String,
    /// 숫자 열인가. **값의 구조로 추정하지 않는다** — 변환 시점에 판명된 것을
    /// 기재한다.
    pub number: bool,
    /// 사전으로 압축한 열인가. 해당하면 값은 정수 id 이고 `dict` 를 조인한다.
    pub dict: bool,
}

/// 표 하나.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableInfo {
    pub table: String,
    /// 원본에서 이 표로 전환된 시트의 명칭.
    pub sheet: String,
    pub rows: u64,
    pub columns: Vec<ColumnInfo>,
}

/// `.sift` 가 알맹이와 함께 수용하는 것.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Meta {
    /// [`FORMAT_VERSION`]. **선행 기재한다** — 미지의 판이면 나머지를 판독하지 않는다.
    pub format: u32,
    pub source: Source,
    pub tables: Vec<TableInfo>,
}

/* ── 그릇 ───────────────────────────────────────────────────────── */

/// `.sift` 의 그릇 약정. [`vault`] 가 이것을 참조해 봉인하고 해제한다.
pub struct Sift;

impl vault::Format for Sift {
    type Error = Error;
    const MAGIC: [u8; vault::MAGIC_LEN] = MAGIC;
    const SCHEMA_VERSION: u32 = SCHEMA_VERSION;

    /// 지문 표를 생성하거나 구 판에서 상승시킨다. **알맹이의 표는 생성하지
    /// 않는다** — 무엇이 유입될지 인지하지 못하고, 그것은
    /// [`sheet`](crate::sheet) 가 생성한다.
    ///
    /// **공백 DB 에도, 해제한 구 파일에도 호출된다.** 그래서 기재된 판을 참조해
    /// 분기한다. `IF NOT EXISTS` 로 배치하지 않는 사유는 그것이 "이미 존재하면
    /// 충분하다" 는 의미여서다 — 열이 누락된 구 표가 존재해도 그대로 통과한다.
    fn migrate(conn: &Connection) -> Result<()> {
        let bad = |e: rusqlite::Error| Error::new(ErrorCode::SiftSchemaUnexpected).detail(e);
        let at: u32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).map_err(bad)?;
        match at {
            // 공백 DB. 최초 생성한다.
            0 => conn
                .execute_batch(&format!("{META_DDL}; PRAGMA user_version = {SCHEMA_VERSION};"))
                .map_err(bad),
            // 이미 해당 판이다. 수행할 작업이 없다.
            SCHEMA_VERSION => Ok(()),
            // **미지의 판이면 추정하지 않는다.** 판이 둘 이상이 되는 시점에 여기에
            // 한 행이 증가하고, 구 파일은 구 행을 경유해 상승한다.
            other => Err(Error::new(ErrorCode::SiftSchemaUnexpected)
                .with("fileVersion", other)
                .with("appVersion", SCHEMA_VERSION)
                .detail("no migration path from this schema version")),
        }
    }

    /// 해제한 알맹이가 **우리가 생성한 구조인지** 판정한다.
    ///
    /// 표를 뷰로 교체하거나 트리거를 삽입한 것은 `integrity_check` 가 포착하지
    /// 못한다. 비밀번호를 인지하는 자가 해제해 변조하고 재봉인할 수 있으므로,
    /// 개방하는 곳이 판정한다.
    fn check(conn: &Connection) -> Result<()> {
        let kind: String = conn
            .query_row("SELECT type FROM sqlite_master WHERE name = 'sift_meta'", [], |r| r.get(0))
            .map_err(|e| Error::new(ErrorCode::SiftSchemaUnexpected).detail(e))?;
        if kind != "table" {
            return Err(Error::new(ErrorCode::SiftSchemaUnexpected)
                .with("found", &kind)
                .detail("sift_meta is not a table"));
        }
        // 단일 행이어야 한다. `CHECK` 가 차단하지만, 변조된 파일은 그 `CHECK` 가
        // 부재하는 표로 교체할 수 있다.
        let rows: u32 = conn
            .query_row("SELECT count(*) FROM sift_meta", [], |r| r.get(0))
            .map_err(|e| Error::new(ErrorCode::SiftSchemaUnexpected).detail(e))?;
        if rows != 1 {
            return Err(Error::new(ErrorCode::SiftSchemaUnexpected)
                .with("rows", rows)
                .detail("sift_meta must hold exactly one row"));
        }
        Ok(())
    }
}

/* ── 담고 꺼내기 ────────────────────────────────────────────────── */

fn put_meta(conn: &Connection, meta: &Meta) -> Result<()> {
    let json =
        serde_json::to_string(meta).map_err(|e| Error::new(ErrorCode::SiftMetaInvalid).detail(e))?;
    conn.execute("INSERT OR REPLACE INTO sift_meta(id, json) VALUES (1, ?1)", [&json])
        .map_err(|e| Error::new(ErrorCode::SiftSchemaUnexpected).detail(e))?;
    Ok(())
}

/// 수용된 지문과 열 정보를 판독한다.
///
/// **미지의 판이면 중단한다.** 신판의 파일을 구 앱이 판독하면, 누락된 항목이
/// 기본값으로 충전되어 그럴듯한 허위가 산출된다.
pub fn read_meta(conn: &Connection) -> Result<Meta> {
    let json: String = conn
        .query_row("SELECT json FROM sift_meta WHERE id = 1", [], |r| r.get(0))
        .map_err(|e| Error::new(ErrorCode::SiftMetaInvalid).detail(e))?;
    let meta: Meta = serde_json::from_str(&json)
        .map_err(|e| Error::new(ErrorCode::SiftMetaInvalid).detail(e))?;
    if meta.format > FORMAT_VERSION {
        return Err(Error::new(ErrorCode::SiftVersionTooNew)
            .with("fileVersion", meta.format)
            .with("appVersion", FORMAT_VERSION));
    }
    Ok(meta)
}

/// **비밀번호로 봉인한 `.sift` 바이트를 생성한다.** 기본 경로다.
///
/// `built` 는 [`sheet::build`](crate::sheet::build) 가 표를 생성해 둔 연결이다.
/// 그 알맹이를 봉투 내부로 이전하고 지문을 함께 수용한다.
pub fn seal(built: &Connection, meta: &Meta, password: &str) -> Result<Vec<u8>> {
    let vault = Vault::<Sift>::create(password)?;
    copy_into(built, &vault.conn)?;
    put_meta(&vault.conn, meta)?;
    vault.seal()
}

/// 비밀번호 없이 수용한다. **쓰는 곳이 의도적으로 호출해야 한다.**
///
/// 반출되는 것이 순수 SQLite 라, 수령한 누구나 개방한다. 그것이 적합한 위치가
/// 존재하므로 경로는 배치하되, 기본으로 설정하지 않는다.
pub fn seal_plain(built: &Connection, meta: &Meta) -> Result<Vec<u8>> {
    let conn = Connection::open_in_memory()
        .map_err(|e| Error::new(ErrorCode::SiftSchemaUnexpected).detail(e))?;
    <Sift as vault::Format>::migrate(&conn)?;
    copy_into(built, &conn)?;
    put_meta(&conn, meta)?;
    crate::sheet::serialize(&conn)
}

/// 봉인한 `.sift` 를 개방해 메모리에 배치한다.
pub fn open(bytes: &[u8], password: &str) -> Result<Vault<Sift>> {
    Vault::<Sift>::open(bytes, password)
}

/// 순수 `.sift` 를 개방해 메모리에 배치한다. [`seal_plain`] 으로 수용한 것이다.
pub fn open_plain(bytes: &[u8]) -> Result<Connection> {
    let mut conn = Connection::open_in_memory()
        .map_err(|e| Error::new(ErrorCode::SiftSchemaUnexpected).detail(e))?;
    conn.deserialize_read_exact("main", std::io::Cursor::new(bytes), bytes.len(), true)
        .map_err(|e| Error::new(ErrorCode::SiftFormatInvalid).detail(e))?;
    <Sift as vault::Format>::check(&conn)?;
    Ok(conn)
}

/// 생성해 둔 표들을 그릇 내부의 연결로 이전한다.
///
/// SQL 로 복제한다. 바이트를 그대로 이전하면 그릇의 표(`sift_meta`)가 피복되고,
/// 그릇의 `PRAGMA` 가 알맹이의 것으로 변경된다.
fn copy_into(from: &Connection, to: &Connection) -> Result<()> {
    let bad = |e: rusqlite::Error| Error::new(ErrorCode::SiftSchemaUnexpected).detail(e);
    // 임시 파일 없이 이전하려면 연결해 복제한다. `serialize` 한 바이트를 메모리
    // DB 로 연결하는 경로가 rusqlite 에 부재하므로, 송신 측을 수신 측에 연결한다.
    let image = crate::sheet::serialize(from)?;
    let mut src = Connection::open_in_memory().map_err(bad)?;
    src.deserialize_read_exact("main", std::io::Cursor::new(&image), image.len(), true)
        .map_err(bad)?;

    let mut sql = Vec::new();
    {
        let mut stmt = src
            .prepare("SELECT type, name, sql FROM sqlite_master WHERE sql IS NOT NULL ORDER BY rowid")
            .map_err(bad)?;
        let mut rows = stmt.query([]).map_err(bad)?;
        while let Some(r) = rows.next().map_err(bad)? {
            let (kind, name, ddl): (String, String, String) =
                (r.get(0).map_err(bad)?, r.get(1).map_err(bad)?, r.get(2).map_err(bad)?);
            sql.push((kind, name, ddl));
        }
    }

    for (_, _, ddl) in sql.iter().filter(|(k, _, _)| k == "table") {
        to.execute_batch(ddl).map_err(bad)?;
    }
    for (_, name, _) in sql.iter().filter(|(k, _, _)| k == "table") {
        let names: Vec<String> = {
            let stmt = src.prepare(&format!("SELECT * FROM \"{name}\" LIMIT 0")).map_err(bad)?;
            stmt.column_names().iter().map(|s| s.to_string()).collect()
        };
        let cols = names.iter().map(|c| format!("\"{c}\"")).collect::<Vec<_>>().join(",");
        let holes = vec!["?"; names.len()].join(",");
        let mut read = src.prepare(&format!("SELECT {cols} FROM \"{name}\"")).map_err(bad)?;
        let mut write = to
            .prepare(&format!("INSERT INTO \"{name}\"({cols}) VALUES ({holes})"))
            .map_err(bad)?;
        let mut rows = read.query([]).map_err(bad)?;
        while let Some(r) = rows.next().map_err(bad)? {
            let vals: Vec<rusqlite::types::Value> = (0..names.len())
                .map(|i| r.get(i).map_err(bad))
                .collect::<Result<Vec<_>>>()?;
            write.execute(rusqlite::params_from_iter(vals)).map_err(bad)?;
        }
    }
    // 색인은 알맹이를 전부 삽입한 뒤에 생성한다. 삽입과 동시에 생성하면 행마다
    // 색인을 갱신하느라 수 배의 비용이 발생한다.
    for (_, _, ddl) in sql.iter().filter(|(k, _, _)| k == "index") {
        to.execute_batch(ddl).map_err(bad)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sheet;

    /// 누구나 인지하는 값이다. 시험에만 사용한다.
    const PASSWORD: &str = "열려라참깨1234";

    fn built() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        let s = sheet::Sheet {
            rows: vec![
                vec!["이름".into(), "지역".into()],
                vec!["하늘".into(), "서울".into()],
                vec!["가람".into(), "부산".into()],
            ],
        };
        let plan = sheet::TablePlan {
            table: "people".into(),
            map: vec![
                sheet::ColumnMap {
                    sheet: "이름".into(),
                    column: "name".into(),
                    ty: sheet::ValueType::Dict,
                },
                sheet::ColumnMap {
                    sheet: "지역".into(),
                    column: "region".into(),
                    ty: sheet::ValueType::Dict,
                },
            ],
            indexes: vec![vec!["region".into()]],
        };
        sheet::build(&conn, &[(plan, s)], "-").unwrap();
        conn
    }

    fn meta() -> Meta {
        Meta {
            format: FORMAT_VERSION,
            source: Source::of("people.xlsx", b"\x01\x02\x03"),
            tables: vec![TableInfo {
                table: "people".into(),
                sheet: "사람".into(),
                rows: 2,
                columns: vec![
                    ColumnInfo {
                        column: "name".into(),
                        label: "이름".into(),
                        number: false,
                        dict: true,
                    },
                    ColumnInfo {
                        column: "region".into(),
                        label: "지역".into(),
                        number: false,
                        dict: true,
                    },
                ],
            }],
        }
    }

    /* ── 이미 배포된 형식은 고정값이 고정한다 ─────────────────────────
       하기 값이 상이해지면 **이미 배포된 파일이 개방되지 않는다는 의미**다. 값을
       변경하지 말고 판을 상승시킨다. 상승 경로는 매직·스키마 판·Meta 판 셋이고,
       어느 것을 상승시킬지는 무엇이 변경되었는지가 결정한다. */

    #[test]
    fn the_name_tag_is_pinned() {
        assert_eq!(&MAGIC, b"SIFT\0v1", "매직이 변경되었습니다 — 배포된 파일이 개방되지 않습니다");
        assert_eq!(MAGIC.len(), vault::MAGIC_LEN);
    }

    #[test]
    fn the_versions_are_pinned() {
        assert_eq!(SCHEMA_VERSION, 1, "알맹이 스키마 판이 변경되었습니다");
        assert_eq!(FORMAT_VERSION, 1, "Meta 판이 변경되었습니다");
    }

    /// 지문 표의 구조도 형식이다. 열 명칭이 변경되면 구 파일에서 지문을 판독하지
    /// 못한다.
    #[test]
    fn the_meta_table_is_pinned() {
        assert_eq!(
            META_DDL,
            "CREATE TABLE sift_meta(id INTEGER PRIMARY KEY CHECK (id = 1), json TEXT NOT NULL)",
            "지문 표의 구조가 변경되었습니다"
        );
    }

    /// **Meta 의 문자 형태도 형식이다.** 항목 명칭이 변경되면 구 파일의 지문을
    /// 판독하지 못하고, 그러면 사본의 노후 여부를 판별할 수 없다.
    #[test]
    fn the_meta_json_is_pinned() {
        let json = serde_json::to_string(&meta()).unwrap();
        assert_eq!(
            json,
            r#"{"format":1,"source":{"name":"people.xlsx","bytes":3,"sha256":"039058c6f2c0cb492c533b0a4d14ef77cc0f78abccced5287d84a1a2011cfb81"},"tables":[{"table":"people","sheet":"사람","rows":2,"columns":[{"column":"name","label":"이름","number":false,"dict":true},{"column":"region","label":"지역","number":false,"dict":true}]}]}"#,
            "Meta 의 문자 형태가 변경되었습니다 — 배포된 파일의 지문을 판독하지 못합니다"
        );
    }

    /// 머리 7바이트가 해당 위치에 존재해야 한다. 수령한 곳이 그것으로 `.sift`
    /// 인지 식별한다.
    #[test]
    fn a_sealed_file_starts_with_the_name_tag() {
        let bytes = seal(&built(), &meta(), PASSWORD).unwrap();
        assert_eq!(&bytes[..vault::MAGIC_LEN], &MAGIC);
    }

    /* ── 동작하는가 ──────────────────────────────────────────────── */

    #[test]
    fn a_sealed_file_comes_back_with_its_rows_and_fingerprint() {
        let bytes = seal(&built(), &meta(), PASSWORD).unwrap();
        let opened = open(&bytes, PASSWORD).unwrap();
        assert_eq!(read_meta(&opened.conn).unwrap(), meta());

        // 알맹이가 수반되었다. 사전 조인까지 그대로다.
        let name: String = opened
            .conn
            .query_row(
                "SELECT d.s FROM people p JOIN dict d ON d.id = p.name
                 JOIN dict r ON r.id = p.region WHERE r.s = '부산'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(name, "가람");

        // 색인도 수반되었다. 부재하면 대형 표에서 조회가 지연된다.
        let indexes: u32 = opened
            .conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type = 'index' AND tbl_name = 'people'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(indexes, 1);
    }

    #[test]
    fn a_wrong_password_does_not_open_it() {
        let bytes = seal(&built(), &meta(), PASSWORD).unwrap();
        assert_eq!(open(&bytes, "틀린비밀번호다").unwrap_err().code, ErrorCode::VaultPasswordWrong);
    }

    #[test]
    fn another_kind_of_file_is_refused_by_its_name_tag() {
        let mut bytes = seal(&built(), &meta(), PASSWORD).unwrap();
        bytes[0] = b'X';
        assert_eq!(open(&bytes, PASSWORD).unwrap_err().code, ErrorCode::VaultFormatInvalid);
    }

    /// 평문으로 수용하는 경로는 **의도적으로 호출해야** 존재한다. 그래도 동일한 것이 수용된다.
    #[test]
    fn the_plain_path_holds_the_same_thing() {
        let bytes = seal_plain(&built(), &meta()).unwrap();
        assert!(bytes.starts_with(b"SQLite format 3\0"), "순수 SQLite 가 아니다");
        let conn = open_plain(&bytes).unwrap();
        assert_eq!(read_meta(&conn).unwrap(), meta());
        let rows: u32 = conn.query_row("SELECT count(*) FROM people", [], |r| r.get(0)).unwrap();
        assert_eq!(rows, 2);
    }

    /* ── 사본의 노후 여부를 판별한다 ──────────────────────────────── */

    #[test]
    fn the_fingerprint_knows_the_original_it_came_from() {
        let source = Source::of("people.xlsx", b"\x01\x02\x03");
        assert!(source.matches(b"\x01\x02\x03"));
        // 1바이트만 상이해도 아니다.
        assert!(!source.matches(b"\x01\x02\x04"));
        // 길이만 상이해도 아니다 — 해시 산출 이전에 여과된다.
        assert!(!source.matches(b"\x01\x02"));
    }

    /* ── 묵시적으로 오류를 범하느니 중단한다 ──────────────────────── */

    /// **미지의 판이면 판독하지 않는다.** 누락된 항목이 기본값으로 충전되면
    /// 그럴듯한 허위가 산출되고, 노후한 사본을 최신으로 표시하게 된다.
    #[test]
    fn a_newer_format_version_stops() {
        let conn = Connection::open_in_memory().unwrap();
        <Sift as vault::Format>::migrate(&conn).unwrap();
        let mut newer = meta();
        newer.format = FORMAT_VERSION + 1;
        put_meta(&conn, &newer).unwrap();
        let err = read_meta(&conn).unwrap_err();
        assert_eq!(err.code, ErrorCode::SiftVersionTooNew);
        assert_eq!(err.params["fileVersion"], (FORMAT_VERSION + 1).to_string());
    }

    /// 지문 표는 **단일 행만** 수용한다. 2행이면 어느 쪽이 참인지 판별할 수 없다.
    #[test]
    fn a_second_fingerprint_row_cannot_be_added() {
        let conn = Connection::open_in_memory().unwrap();
        <Sift as vault::Format>::migrate(&conn).unwrap();
        put_meta(&conn, &meta()).unwrap();
        assert!(
            conn.execute("INSERT INTO sift_meta(id, json) VALUES (2, '{}')", []).is_err(),
            "2번째 행이 삽입되었다"
        );
    }

    /// 지문 표를 뷰로 교체한 파일은 `integrity_check` 가 포착하지 못한다.
    /// 개방하는 곳이 판정한다.
    #[test]
    fn a_doctored_meta_table_is_caught() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE VIEW sift_meta AS SELECT 1 AS id, '{}' AS json").unwrap();
        let err = <Sift as vault::Format>::check(&conn).unwrap_err();
        assert_eq!(err.code, ErrorCode::SiftSchemaUnexpected);
        assert_eq!(err.params["found"], "view");
    }

    /// 지문이 전무한 파일도 `.sift` 가 아니다.
    #[test]
    fn a_file_without_a_fingerprint_is_not_a_sift() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE people(name TEXT)").unwrap();
        let bytes = sheet::serialize(&conn).unwrap();
        assert_eq!(open_plain(&bytes).unwrap_err().code, ErrorCode::SiftSchemaUnexpected);
    }
}
