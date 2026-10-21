//! **웹에서도 도는지 본다.** 컴파일되는 것과 도는 것은 다르다.
//!
//! 이 바탕은 데스크톱과 웹이 같은 코드를 쓰려고 바이트만 주고받는다. 그런데
//! `wasm32-unknown-unknown` 에는 운영체제가 없어서, 컴파일은 멀쩡히 되고
//! **부르는 순간 패닉하는** 함수가 있다. 시계가 그렇다. 글자로는 보이지 않고
//! 데스크톱 시험도 보지 못한다. 그래서 거기서 한 번 돌려 본다.
//!
//! ```bash
//! cargo test -p sheetsift-core --target wasm32-unknown-unknown
//! ```
//!
//! 돌리는 것은 `.cargo/config.toml` 이 적어 둔 `wasm-bindgen-test-runner` 다.
//! `cargo test --workspace` 는 이 파일을 보지 않는다 — 데스크톱 target 에서는
//! 아래 `cfg` 가 통째로 비운다.
#![cfg(target_arch = "wasm32")]

use sheetsift_core::grid::{self, Column, ColumnKind, Query, Table};
use sheetsift_core::sheet::{self, ColumnMap, Sheet, TablePlan, ValueType};
use wasm_bindgen_test::wasm_bindgen_test;

/// 아주 작은 양식. 웹에는 파일 시스템이 없으므로 **실행 파일 안에 넣어** 둔다.
/// 이름을 아스키로 두는 까닭은 맥이 한글 이름을 NFD 로 바꿔 두면
/// `include_bytes!` 가 빌드 때 못 찾기 때문이다.
const TINY_XLSX: &[u8] = include_bytes!("fixtures/tiny.xlsx");

/// 열 하나짜리 표. 조회가 도는지만 보려는 것이라 가장 작게 둔다.
struct One {
    base: &'static str,
    columns: Vec<Column>,
}

impl Table for One {
    fn id(&self) -> &str {
        "one"
    }
    fn base(&self) -> &str {
        self.base
    }
    fn columns(&self) -> Box<dyn Iterator<Item = &Column> + '_> {
        Box::new(self.columns.iter())
    }
}

fn one() -> One {
    One {
        base: "rows",
        columns: vec![Column {
            key: "name".into(),
            label: "이름".into(),
            column: "name".into(),
            kind: ColumnKind::Text,
            sortable: true,
            filterable: true,
            signed: false,
            dict: false,
            alias: String::new(),
        }],
    }
}

/// **SQLite 가 웹에서 열리는가.** `rusqlite` 는 wasm 에서 `sqlite-wasm-rs` 를
/// 거친다. 열리기만 해도 알맹이가 다 들어온 것이다.
#[wasm_bindgen_test]
fn sqlite_opens_in_the_browser() {
    let conn = rusqlite::Connection::open_in_memory().expect("메모리 DB 를 열지 못했습니다");
    conn.execute_batch("CREATE TABLE rows (name TEXT); INSERT INTO rows VALUES ('가'), ('나');")
        .expect("표를 만들지 못했습니다");
    let count: u32 = conn
        .query_row("SELECT count(*) FROM rows", [], |r| r.get(0))
        .expect("세지 못했습니다");
    assert_eq!(count, 2);
}

/// **조회 엔진이 웹에서 도는가.** 여기가 시계를 만지는 자리다.
#[wasm_bindgen_test]
fn the_query_engine_runs_in_the_browser() {
    let conn = rusqlite::Connection::open_in_memory().expect("메모리 DB 를 열지 못했습니다");
    conn.execute_batch("CREATE TABLE rows (name TEXT); INSERT INTO rows VALUES ('가'), ('나');")
        .expect("표를 만들지 못했습니다");
    let table = one();
    let page = grid::run(
        &conn,
        &table,
        "-",
        &Query {
            limit: 10,
            ..Query::default()
        },
    )
    .expect("조회가 돌지 않았습니다");
    assert_eq!(page.total, 2);
    assert_eq!(page.rows.len(), 2);
}

/// **값 목록도 도는가.** 조회와 다른 자리에서 시계를 만진다.
#[wasm_bindgen_test]
fn column_values_run_in_the_browser() {
    let conn = rusqlite::Connection::open_in_memory().expect("메모리 DB 를 열지 못했습니다");
    conn.execute_batch("CREATE TABLE rows (name TEXT); INSERT INTO rows VALUES ('가'), ('가'), ('나');")
        .expect("표를 만들지 못했습니다");
    let table = one();
    let values = grid::column_values(&conn, &table, "-", "name", &[], "")
        .expect("값 목록이 돌지 않았습니다");
    assert_eq!(values.values.len(), 2);
}

/// **봉투를 웹에서 봉하고 열 수 있는가.**
///
/// Argon2 는 64 MiB 를 잡는다. wasm 의 메모리는 자라야 하는 것이라, 데스크톱에서
/// 아무 일 없던 것이 여기서는 막힐 수 있다. 난수도 여기서 처음 브라우저에게
/// 받는다 — `.cargo/config.toml` 이 적어 둔 그 자리다.
#[wasm_bindgen_test]
fn the_vault_seals_and_opens_in_the_browser() {
    use sheetsift_core::vault::{self, Vault};

    struct Plain;
    impl vault::Format for Plain {
        type Error = sheetsift_core::Error;
        const MAGIC: [u8; vault::MAGIC_LEN] = *b"SIFTWSM";
        const SCHEMA_VERSION: u32 = 1;
        fn migrate(conn: &rusqlite::Connection) -> sheetsift_core::Result<()> {
            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS rows (name TEXT); PRAGMA user_version = 1;",
            )
            .map_err(|e| {
                sheetsift_core::Error::new(sheetsift_core::ErrorCode::VaultOpenFailed)
                    .detail(e)
            })
        }
        fn check(_conn: &rusqlite::Connection) -> sheetsift_core::Result<()> {
            Ok(())
        }
    }

    // 누구나 아는 값이다. 시험에만 쓴다.
    const PASSWORD: &str = "열려라참깨1234";

    let vault = Vault::<Plain>::create(PASSWORD).expect("봉투를 만들지 못했습니다");
    vault
        .conn
        .execute("INSERT INTO rows VALUES (?1)", ["가"])
        .expect("넣지 못했습니다");
    let sealed = vault.seal().expect("봉하지 못했습니다");
    drop(vault);

    let opened = Vault::<Plain>::open(&sealed, PASSWORD).expect("열지 못했습니다");
    let name: String = opened
        .conn
        .query_row("SELECT name FROM rows", [], |r| r.get(0))
        .expect("읽지 못했습니다");
    assert_eq!(name, "가");

    // 틀린 비밀번호로는 열리지 않는다.
    assert!(Vault::<Plain>::open(&sealed, "틀린비밀번호").is_err());
}

/// **양식 바이트에서 DB 바이트까지 웹에서 한 바퀴.**
///
/// 이 바탕이 웹에서 하려는 일 전부가 이 한 바퀴다 — 양식을 받아 SQLite 로
/// 바꾸고, 조회하고, 바이트로 꺼낸다. 어느 한 토막이 데스크톱에서만 되면
/// 웹 껍데기는 지을 수 없다.
#[wasm_bindgen_test]
fn a_workbook_becomes_a_queryable_db_in_the_browser() {
    let sheets = sheet::from_xlsx(TINY_XLSX).expect("양식을 열지 못했습니다");
    // 탭 차례대로 온다. 안내 시트는 표로 만들지 않는다 — 무엇을 만들지는
    // 부르는 쪽이 정한다.
    assert_eq!(sheets.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>(), ["사람", "안내"]);

    let people: Sheet = sheets.into_iter().find(|(n, _)| n == "사람").unwrap().1;
    let plan = TablePlan {
        table: "people".into(),
        map: vec![
            ColumnMap { sheet: "이름".into(), column: "name".into(), ty: ValueType::Dict },
            ColumnMap { sheet: "지역".into(), column: "region".into(), ty: ValueType::Dict },
            ColumnMap { sheet: "점수".into(), column: "score".into(), ty: ValueType::Int },
        ],
        indexes: vec![vec!["region".into()]],
    };

    let conn = rusqlite::Connection::open_in_memory().expect("메모리 DB 를 열지 못했습니다");
    let made = sheet::build(&conn, &[(plan, people)], "-").expect("바꾸지 못했습니다");
    assert_eq!(made["people"].rows, 3);

    // **조회가 돈다.** 사전 열은 조인한 글자로 나온다.
    let mut table = One {
        base: "people",
        columns: vec![Column {
            key: "name".into(),
            label: "이름".into(),
            column: "name".into(),
            kind: ColumnKind::Text,
            sortable: true,
            filterable: true,
            signed: false,
            dict: true,
            alias: String::new(),
        }],
    };
    grid::assign_aliases(table.columns.iter_mut());
    let page = grid::run(&conn, &table, "-", &Query { limit: 10, ..Query::default() })
        .expect("조회가 돌지 않았습니다");
    assert_eq!(page.total, 3);

    // **바이트로 꺼내 다시 열린다.** 내보내기가 이 자리에서 나온다.
    let bytes = sheet::serialize(&conn).expect("바이트로 꺼내지 못했습니다");
    assert!(bytes.starts_with(b"SQLite format 3\0"), "SQLite 머리가 아니다");
}
