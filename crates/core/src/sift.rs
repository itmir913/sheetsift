//! **`.sift` — 한 번 바꿔 둔 표를 담는 파일.**
//!
//! xlsx 를 열어 SQLite 로 바꾸는 데는 시간이 든다. 같은 표를 다시 볼 때마다 그
//! 값을 치를 까닭이 없으므로, 바꿔 둔 것을 파일로 담는다.
//!
//! ## 담기는 것
//!
//! 알맹이는 [`sheet`](crate::sheet) 가 만든 SQLite 그대로다. 거기에 표 하나를
//! 더 둔다 — 어디서 온 자료이고 어떤 열이 있는지([`Meta`]).
//!
//! **지문을 함께 담는 까닭**이 있다. `.sift` 는 원본의 사본이라, 원본이 바뀌면
//! 낡는다. 이름·크기·해시를 적어 두면 다시 열 때 **같은 원본에서 나온
//! 것인지**를 그 자리에서 알 수 있다. 낡은 사본을 최신으로 보여 주는 것이
//! 이 형식이 할 수 있는 가장 나쁜 일이다.
//!
//! ## 비밀번호가 기본이다
//!
//! [`vault`](crate::vault) 봉투에 넣는다. 표에는 사람 이름이나 성적이 들어
//! 있을 수 있고, **담을 때 고르게 하면 아무도 고르지 않는다.** 평문으로
//! 담는 길([`seal_plain`])은 따로 있고, 쓰는 쪽이 일부러 불러야 한다.
//!
//! ## 판을 올리는 법
//!
//! 머리의 매직과 스키마 판은 [`vault`] 가 본다. 알맹이 안의 [`Meta`] 판은
//! [`FORMAT_VERSION`] 이다. 둘 다 **고정값 시험이 못 박는다** — 그 시험이
//! 서면 이미 나간 파일이 안 열린다는 뜻이다. 값을 고치지 말고 판을 올린다.

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::vault::{self, Vault};
use crate::{Error, ErrorCode, Result};

/// 파일 맨 앞의 일곱 바이트. **이것이 `.sift` 라는 이름표다.**
///
/// 열쇠가 아니라 **형식의 일부**라 여기 적는다. 봉인 대칭키와 서명 열쇠는
/// 부르는 앱이 넣지만(그래야 공개 저장소에 둘 수 있다), 이름표를 앱마다 다르게
/// 두면 같은 `.sift` 가 앱마다 다른 파일이 된다.
pub const MAGIC: [u8; vault::MAGIC_LEN] = *b"SIFT\0v1";

/// 알맹이의 스키마 판. 표가 늘거나 바뀌면 올린다.
pub const SCHEMA_VERSION: u32 = 1;

/// [`Meta`] 의 판. 담기는 값이 늘거나 뜻이 바뀌면 올린다.
pub const FORMAT_VERSION: u32 = 1;

/// 지문과 열 정보를 담는 표. **한 줄만 담긴다** — `CHECK` 가 SQLite 에서
/// 그것을 지킨다. 두 줄이 되는 길을 코드로 막으면 언젠가 그 길이 생긴다.
const META_DDL: &str =
    "CREATE TABLE sift_meta(id INTEGER PRIMARY KEY CHECK (id = 1), json TEXT NOT NULL)";

/* ── 담기는 것 ──────────────────────────────────────────────────── */

/// 어디서 온 자료인가. **사본이 낡았는지 보는 데 쓴다.**
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Source {
    /// 원본 파일의 이름. 경로가 아니라 이름이다 — 경로는 사람의 컴퓨터 사정이고,
    /// 파일을 주고받으면 뜻이 없어진다.
    pub name: String,
    /// 원본의 바이트 수. 해시보다 먼저 보는 값이다.
    pub bytes: u64,
    /// 원본의 sha256, 소문자 열여섯 진수.
    ///
    /// 글자로 담는 까닭은 사람이 `sha256sum` 으로 혼자 맞춰 볼 수 있어서다.
    pub sha256: String,
}

impl Source {
    /// 원본 바이트에서 지문을 뜬다.
    pub fn of(name: &str, bytes: &[u8]) -> Self {
        use sha2::Digest;
        let digest = sha2::Sha256::digest(bytes);
        Self {
            name: name.to_string(),
            bytes: bytes.len() as u64,
            sha256: digest.iter().map(|b| format!("{b:02x}")).collect(),
        }
    }

    /// 이 지문이 그 바이트에서 나온 것인가.
    pub fn matches(&self, bytes: &[u8]) -> bool {
        self.bytes == bytes.len() as u64 && self.sha256 == Self::of(&self.name, bytes).sha256
    }
}

/// 열 하나. 조회 화면이 무엇을 그릴지 아는 데 필요한 만큼만 담는다.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnInfo {
    /// DB 열 이름.
    pub column: String,
    /// 화면에 보이는 이름. 원본 머리글이다.
    pub label: String,
    /// 숫자 열인가. **값의 생김새로 짐작하지 않는다** — 바꿀 때 알았던 것을
    /// 적어 둔다.
    pub number: bool,
    /// 사전으로 눌러 담은 열인가. 그렇다면 값은 정수 id 이고 `dict` 를 조인한다.
    pub dict: bool,
}

/// 표 하나.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableInfo {
    pub table: String,
    /// 원본에서 이 표가 된 시트의 이름.
    pub sheet: String,
    pub rows: u64,
    pub columns: Vec<ColumnInfo>,
}

/// `.sift` 가 알맹이와 함께 담는 것.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Meta {
    /// [`FORMAT_VERSION`]. **먼저 적는다** — 모르는 판이면 나머지를 읽지 않는다.
    pub format: u32,
    pub source: Source,
    pub tables: Vec<TableInfo>,
}

/* ── 그릇 ───────────────────────────────────────────────────────── */

/// `.sift` 의 그릇 약속. [`vault`] 가 이것을 보고 봉하고 푼다.
pub struct Sift;

impl vault::Format for Sift {
    type Error = Error;
    const MAGIC: [u8; vault::MAGIC_LEN] = MAGIC;
    const SCHEMA_VERSION: u32 = SCHEMA_VERSION;

    /// 지문 표를 만들거나 옛 판에서 올린다. **알맹이의 표는 만들지 않는다** —
    /// 무엇이 들어올지 모르고, 그것은 [`sheet`](crate::sheet) 가 만든다.
    ///
    /// **빈 DB 에도, 풀어 낸 옛 파일에도 불린다.** 그래서 적힌 판을 보고
    /// 가른다. `IF NOT EXISTS` 로 두지 않는 까닭은 그것이 "이미 있으면 됐다"
    /// 는 말이어서다 — 열이 빠진 옛 표가 있어도 그냥 지나간다.
    fn migrate(conn: &Connection) -> Result<()> {
        let bad = |e: rusqlite::Error| Error::new(ErrorCode::SiftSchemaUnexpected).detail(e);
        let at: u32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).map_err(bad)?;
        match at {
            // 빈 DB. 처음 만든다.
            0 => conn
                .execute_batch(&format!("{META_DDL}; PRAGMA user_version = {SCHEMA_VERSION};"))
                .map_err(bad),
            // 이미 이 판이다. 할 일이 없다.
            SCHEMA_VERSION => Ok(()),
            // **모르는 판이면 짐작하지 않는다.** 판이 둘 이상이 되는 날 여기에
            // 한 줄이 늘고, 옛 파일은 옛 줄을 거쳐 올라온다.
            other => Err(Error::new(ErrorCode::SiftSchemaUnexpected)
                .with("fileVersion", other)
                .with("appVersion", SCHEMA_VERSION)
                .detail("no migration path from this schema version")),
        }
    }

    /// 풀어 낸 알맹이가 **우리가 만든 생김새인지** 본다.
    ///
    /// 표를 뷰로 바꿔치우거나 트리거를 심은 것은 `integrity_check` 가 못 잡는다.
    /// 비밀번호를 아는 사람이 풀어서 손보고 다시 봉할 수 있으므로, 여는 쪽이 본다.
    fn check(conn: &Connection) -> Result<()> {
        let kind: String = conn
            .query_row("SELECT type FROM sqlite_master WHERE name = 'sift_meta'", [], |r| r.get(0))
            .map_err(|e| Error::new(ErrorCode::SiftSchemaUnexpected).detail(e))?;
        if kind != "table" {
            return Err(Error::new(ErrorCode::SiftSchemaUnexpected)
                .with("found", &kind)
                .detail("sift_meta is not a table"));
        }
        // 한 줄이어야 한다. `CHECK` 가 막지만, 손본 파일은 그 `CHECK` 가 없는
        // 표로 바꿔치울 수 있다.
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

/// 담긴 지문과 열 정보를 읽는다.
///
/// **모르는 판이면 선다.** 새 판의 파일을 옛 앱이 읽으면, 빠진 칸이 기본값으로
/// 채워져 그럴듯한 거짓이 나온다.
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

/// **비밀번호로 봉한 `.sift` 바이트를 만든다.** 기본 길이다.
///
/// `built` 는 [`sheet::build`](crate::sheet::build) 가 표를 만들어 둔 연결이다.
/// 그 알맹이를 봉투 안으로 옮기고 지문을 함께 담는다.
pub fn seal(built: &Connection, meta: &Meta, password: &str) -> Result<Vec<u8>> {
    let vault = Vault::<Sift>::create(password)?;
    copy_into(built, &vault.conn)?;
    put_meta(&vault.conn, meta)?;
    vault.seal()
}

/// 비밀번호 없이 담는다. **쓰는 쪽이 일부러 불러야 한다.**
///
/// 나가는 것이 맨 SQLite 라, 받아 든 누구나 열어 본다. 그것이 맞는 자리가
/// 있으므로 길은 두지만, 기본으로 두지 않는다.
pub fn seal_plain(built: &Connection, meta: &Meta) -> Result<Vec<u8>> {
    let conn = Connection::open_in_memory()
        .map_err(|e| Error::new(ErrorCode::SiftSchemaUnexpected).detail(e))?;
    <Sift as vault::Format>::migrate(&conn)?;
    copy_into(built, &conn)?;
    put_meta(&conn, meta)?;
    crate::sheet::serialize(&conn)
}

/// 봉한 `.sift` 를 열어 메모리에 둔다.
pub fn open(bytes: &[u8], password: &str) -> Result<Vault<Sift>> {
    Vault::<Sift>::open(bytes, password)
}

/// 맨 `.sift` 를 열어 메모리에 둔다. [`seal_plain`] 으로 담은 것이다.
pub fn open_plain(bytes: &[u8]) -> Result<Connection> {
    let mut conn = Connection::open_in_memory()
        .map_err(|e| Error::new(ErrorCode::SiftSchemaUnexpected).detail(e))?;
    conn.deserialize_read_exact("main", std::io::Cursor::new(bytes), bytes.len(), true)
        .map_err(|e| Error::new(ErrorCode::SiftFormatInvalid).detail(e))?;
    <Sift as vault::Format>::check(&conn)?;
    Ok(conn)
}

/// 만들어 둔 표들을 그릇 안의 연결로 옮긴다.
///
/// SQL 로 베낀다. 바이트를 그대로 옮기면 그릇의 표(`sift_meta`)가 덮이고,
/// 그릇의 `PRAGMA` 가 알맹이의 것으로 바뀐다.
fn copy_into(from: &Connection, to: &Connection) -> Result<()> {
    let bad = |e: rusqlite::Error| Error::new(ErrorCode::SiftSchemaUnexpected).detail(e);
    // 임시 파일 없이 옮기려면 붙여서 베낀다. `serialize` 한 바이트를 메모리
    // DB 로 붙이는 길이 rusqlite 에 없으므로, 보낼 쪽을 받는 쪽에 붙인다.
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
    // 색인은 알맹이를 다 넣은 뒤에 만든다. 넣으면서 만들면 줄마다 색인을
    // 고치느라 몇 배가 든다.
    for (_, _, ddl) in sql.iter().filter(|(k, _, _)| k == "index") {
        to.execute_batch(ddl).map_err(bad)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sheet;

    /// 누구나 아는 값이다. 시험에만 쓴다.
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

    /* ── 이미 나간 형식은 고정값이 못 박는다 ──────────────────────────
       아래 값이 달라지면 **이미 나간 파일이 안 열린다는 뜻**이다. 값을 고치지
       말고 판을 올린다. 올리는 길은 매직·스키마 판·Meta 판 셋이고, 어느 것을
       올려야 하는지는 무엇이 바뀌었는지가 정한다. */

    #[test]
    fn the_name_tag_is_pinned() {
        assert_eq!(&MAGIC, b"SIFT\0v1", "매직이 달라졌습니다 — 나간 파일이 안 열립니다");
        assert_eq!(MAGIC.len(), vault::MAGIC_LEN);
    }

    #[test]
    fn the_versions_are_pinned() {
        assert_eq!(SCHEMA_VERSION, 1, "알맹이 스키마 판이 달라졌습니다");
        assert_eq!(FORMAT_VERSION, 1, "Meta 판이 달라졌습니다");
    }

    /// 지문 표의 생김새도 형식이다. 열 이름이 바뀌면 옛 파일에서 지문을 읽지
    /// 못한다.
    #[test]
    fn the_meta_table_is_pinned() {
        assert_eq!(
            META_DDL,
            "CREATE TABLE sift_meta(id INTEGER PRIMARY KEY CHECK (id = 1), json TEXT NOT NULL)",
            "지문 표의 생김새가 달라졌습니다"
        );
    }

    /// **Meta 의 글자 모양도 형식이다.** 칸 이름이 바뀌면 옛 파일의 지문을
    /// 읽지 못하고, 그러면 사본이 낡았는지 알 수 없다.
    #[test]
    fn the_meta_json_is_pinned() {
        let json = serde_json::to_string(&meta()).unwrap();
        assert_eq!(
            json,
            r#"{"format":1,"source":{"name":"people.xlsx","bytes":3,"sha256":"039058c6f2c0cb492c533b0a4d14ef77cc0f78abccced5287d84a1a2011cfb81"},"tables":[{"table":"people","sheet":"사람","rows":2,"columns":[{"column":"name","label":"이름","number":false,"dict":true},{"column":"region","label":"지역","number":false,"dict":true}]}]}"#,
            "Meta 의 글자 모양이 달라졌습니다 — 나간 파일의 지문을 읽지 못합니다"
        );
    }

    /// 머리 일곱 바이트가 그 자리에 있어야 한다. 받아 든 쪽이 그것으로
    /// `.sift` 인지 가린다.
    #[test]
    fn a_sealed_file_starts_with_the_name_tag() {
        let bytes = seal(&built(), &meta(), PASSWORD).unwrap();
        assert_eq!(&bytes[..vault::MAGIC_LEN], &MAGIC);
    }

    /* ── 도는가 ──────────────────────────────────────────────────── */

    #[test]
    fn a_sealed_file_comes_back_with_its_rows_and_fingerprint() {
        let bytes = seal(&built(), &meta(), PASSWORD).unwrap();
        let opened = open(&bytes, PASSWORD).unwrap();
        assert_eq!(read_meta(&opened.conn).unwrap(), meta());

        // 알맹이가 따라왔다. 사전 조인까지 그대로다.
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

        // 색인도 따라왔다. 없으면 큰 표에서 조회가 기어간다.
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

    /// 평문으로 담는 길은 **일부러 불러야** 있다. 그래도 같은 것이 담긴다.
    #[test]
    fn the_plain_path_holds_the_same_thing() {
        let bytes = seal_plain(&built(), &meta()).unwrap();
        assert!(bytes.starts_with(b"SQLite format 3\0"), "맨 SQLite 가 아니다");
        let conn = open_plain(&bytes).unwrap();
        assert_eq!(read_meta(&conn).unwrap(), meta());
        let rows: u32 = conn.query_row("SELECT count(*) FROM people", [], |r| r.get(0)).unwrap();
        assert_eq!(rows, 2);
    }

    /* ── 사본이 낡았는지 안다 ─────────────────────────────────────── */

    #[test]
    fn the_fingerprint_knows_the_original_it_came_from() {
        let source = Source::of("people.xlsx", b"\x01\x02\x03");
        assert!(source.matches(b"\x01\x02\x03"));
        // 한 바이트만 달라도 아니다.
        assert!(!source.matches(b"\x01\x02\x04"));
        // 길이만 달라도 아니다 — 해시를 뜨기 전에 걸러진다.
        assert!(!source.matches(b"\x01\x02"));
    }

    /* ── 조용히 틀리느니 선다 ─────────────────────────────────────── */

    /// **모르는 판이면 읽지 않는다.** 빠진 칸이 기본값으로 채워지면 그럴듯한
    /// 거짓이 나오고, 낡은 사본을 최신으로 보여 주게 된다.
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

    /// 지문 표는 **한 줄만** 담는다. 두 줄이면 어느 쪽이 참인지 알 수 없다.
    #[test]
    fn a_second_fingerprint_row_cannot_be_added() {
        let conn = Connection::open_in_memory().unwrap();
        <Sift as vault::Format>::migrate(&conn).unwrap();
        put_meta(&conn, &meta()).unwrap();
        assert!(
            conn.execute("INSERT INTO sift_meta(id, json) VALUES (2, '{}')", []).is_err(),
            "두 번째 줄이 들어갔다"
        );
    }

    /// 지문 표를 뷰로 바꿔치운 파일은 `integrity_check` 가 못 잡는다. 여는
    /// 쪽이 본다.
    #[test]
    fn a_doctored_meta_table_is_caught() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE VIEW sift_meta AS SELECT 1 AS id, '{}' AS json").unwrap();
        let err = <Sift as vault::Format>::check(&conn).unwrap_err();
        assert_eq!(err.code, ErrorCode::SiftSchemaUnexpected);
        assert_eq!(err.params["found"], "view");
    }

    /// 지문이 아예 없는 파일도 `.sift` 가 아니다.
    #[test]
    fn a_file_without_a_fingerprint_is_not_a_sift() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE people(name TEXT)").unwrap();
        let bytes = sheet::serialize(&conn).unwrap();
        assert_eq!(open_plain(&bytes).unwrap_err().code, ErrorCode::SiftSchemaUnexpected);
    }
}
