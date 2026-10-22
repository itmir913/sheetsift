//! **웹에서도 동작하는지 판정한다.** 컴파일되는 것과 동작하는 것은 상이하다.
//!
//! 이 바탕은 데스크톱과 웹이 동일한 코드를 사용하려고 바이트만 수수한다. 그런데
//! `wasm32-unknown-unknown` 에는 운영체제가 부재해, 컴파일은 정상 수행되고
//! **호출하는 즉시 패닉하는** 함수가 존재한다. 시계가 그러하다. 글자로는 노출되지
//! 않고 데스크톱 시험도 판정하지 못한다. 그래서 거기서 1회 실행한다.
//!
//! ```bash
//! cargo test -p sheetsift-core --target wasm32-unknown-unknown
//! ```
//!
//! 실행하는 것은 `.cargo/config.toml` 이 기재한 `wasm-bindgen-test-runner` 다.
//! `cargo test --workspace` 는 이 파일을 판정하지 않는다 — 데스크톱 target 에서는
//! 하기 `cfg` 가 전체를 공백화한다.
#![cfg(target_arch = "wasm32")]

use sheetsift_core::grid::{self, Column, ColumnKind, Query, Table};
use sheetsift_core::sheet::{self, ColumnMap, Sheet, TablePlan, ValueType};
use wasm_bindgen_test::wasm_bindgen_test;

/// 극소 양식. 웹에는 파일 시스템이 부재하므로 **실행 파일 내부에 수용**한다.
/// 명칭을 아스키로 설정하는 사유는 맥이 한글 명칭을 NFD 로 변환하면
/// `include_bytes!` 가 빌드 시점에 탐색하지 못하기 때문이다.
const TINY_XLSX: &[u8] = include_bytes!("fixtures/tiny.xlsx");

/// 단일 열 표. 조회의 동작 여부만 판정하려는 것이라 최소로 설정한다.
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

/// **SQLite 가 웹에서 개방되는가.** `rusqlite` 는 wasm 에서 `sqlite-wasm-rs` 를
/// 경유한다. 개방만 되어도 알맹이가 전부 포함된 것이다.
#[wasm_bindgen_test]
fn sqlite_opens_in_the_browser() {
    let conn = rusqlite::Connection::open_in_memory().expect("메모리 DB 를 개방하지 못했습니다");
    conn.execute_batch("CREATE TABLE rows (name TEXT); INSERT INTO rows VALUES ('가'), ('나');")
        .expect("표를 생성하지 못했습니다");
    let count: u32 = conn
        .query_row("SELECT count(*) FROM rows", [], |r| r.get(0))
        .expect("산정하지 못했습니다");
    assert_eq!(count, 2);
}

/// **조회 엔진이 웹에서 동작하는가.** 여기가 시계를 사용하는 위치다.
#[wasm_bindgen_test]
fn the_query_engine_runs_in_the_browser() {
    let conn = rusqlite::Connection::open_in_memory().expect("메모리 DB 를 개방하지 못했습니다");
    conn.execute_batch("CREATE TABLE rows (name TEXT); INSERT INTO rows VALUES ('가'), ('나');")
        .expect("표를 생성하지 못했습니다");
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
    .expect("조회가 동작하지 않았습니다");
    assert_eq!(page.total, 2);
    assert_eq!(page.rows.len(), 2);
}

/// **값 목록도 동작하는가.** 조회와 상이한 위치에서 시계를 사용한다.
#[wasm_bindgen_test]
fn column_values_run_in_the_browser() {
    let conn = rusqlite::Connection::open_in_memory().expect("메모리 DB 를 개방하지 못했습니다");
    conn.execute_batch("CREATE TABLE rows (name TEXT); INSERT INTO rows VALUES ('가'), ('가'), ('나');")
        .expect("표를 생성하지 못했습니다");
    let table = one();
    let values = grid::column_values(&conn, &table, "-", "name", &[], "")
        .expect("값 목록이 동작하지 않았습니다");
    assert_eq!(values.values.len(), 2);
}

/// **봉투를 웹에서 봉인하고 개방할 수 있는가.**
///
/// Argon2 는 64 MiB 를 확보한다. wasm 의 메모리는 확장되어야 하는 것이라,
/// 데스크톱에서 정상이던 것이 여기서는 차단될 수 있다. 난수도 여기서 최초로
/// 브라우저에게 수령한다 — `.cargo/config.toml` 이 기재한 그 위치다.
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

    // 누구나 인지하는 값이다. 시험에만 사용한다.
    const PASSWORD: &str = "열려라참깨1234";

    let vault = Vault::<Plain>::create(PASSWORD).expect("봉투를 생성하지 못했습니다");
    vault
        .conn
        .execute("INSERT INTO rows VALUES (?1)", ["가"])
        .expect("삽입하지 못했습니다");
    let sealed = vault.seal().expect("봉인하지 못했습니다");
    drop(vault);

    let opened = Vault::<Plain>::open(&sealed, PASSWORD).expect("개방하지 못했습니다");
    let name: String = opened
        .conn
        .query_row("SELECT name FROM rows", [], |r| r.get(0))
        .expect("판독하지 못했습니다");
    assert_eq!(name, "가");

    // 상이한 비밀번호로는 개방되지 않는다.
    assert!(Vault::<Plain>::open(&sealed, "틀린비밀번호").is_err());
}

/// **양식 바이트에서 DB 바이트까지 웹에서 일순.**
///
/// 이 바탕이 웹에서 수행하려는 작업 전부가 이 일순이다 — 양식을 수용해 SQLite 로
/// 변환하고, 조회하고, 바이트로 추출한다. 어느 한 구간이 데스크톱에서만 동작하면
/// 웹 껍데기는 구축할 수 없다.
#[wasm_bindgen_test]
fn a_workbook_becomes_a_queryable_db_in_the_browser() {
    let sheets = sheet::from_xlsx(TINY_XLSX).expect("양식을 열지 못했습니다");
    // 탭 순서대로 도달한다. 안내 시트는 표로 생성하지 않는다 — 무엇을 생성할지는
    // 부르는 곳이 결정한다.
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

    let conn = rusqlite::Connection::open_in_memory().expect("메모리 DB 를 개방하지 못했습니다");
    let made = sheet::build(&conn, &[(plan, people)], "-").expect("변환하지 못했습니다");
    assert_eq!(made["people"].rows, 3);

    // **조회가 동작한다.** 사전 열은 조인한 문자로 산출된다.
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
        .expect("조회가 동작하지 않았습니다");
    assert_eq!(page.total, 3);

    // **바이트로 추출해 재개방된다.** 반출이 이 위치에서 산출된다.
    let bytes = sheet::serialize(&conn).expect("바이트로 추출하지 못했습니다");
    assert!(bytes.starts_with(b"SQLite format 3\0"), "SQLite 머리가 아니다");
}

/// **서명한 그릇이 웹에서 개방되는가.**
///
/// 데스크톱에서 작동하는 것이 여기서 작동한다는 진술이 아니다. Ed25519 의
/// 검증은 `curve25519-dalek` 이 수행하는데, 그 상자는 플랫폼마다 상이한 구현을
/// 선택한다(`simd`·`u64`·`u32`). wasm 은 32비트라 선택되는 것이 데스크톱과
/// 상이하고, **그 선택은 컴파일 시점에 묵시적으로 수행된다.**
///
/// 열쇠는 전부 시험 전용이다. **누구나 인지하는 값**이라 이 저장소에 기재해도
/// 은닉하는 것이 감소하지 않는다 — 실제 열쇠는 부르는 곳이 주입한다.
#[wasm_bindgen_test]
fn a_signed_container_opens_in_the_browser() {
    use sheetsift_core::seal;

    const MAGIC: &[u8; seal::MAGIC_LEN] = b"TESTSEAL";
    const KEY: [u8; 32] = [5u8; 32];
    const IMAGE: &[u8] = b"SQLite format 3\0-- pretend this is a database --";

    let signing = seal::SigningKey::from_bytes(&[7u8; 32]);
    let verify = vec![signing.verifying_key().to_bytes()];

    let sealed = seal::seal(IMAGE, MAGIC, "260801", [0, 5, 3], &KEY, &signing)
        .expect("봉인하지 못했습니다");

    let verified = seal::parse(&sealed, MAGIC, &verify).expect("서명을 확인하지 못했습니다");
    let opened = seal::open(&verified, &KEY).expect("개방하지 못했습니다");
    assert_eq!(opened, IMAGE, "알맹이가 상이합니다");

    // **위조 교체를 차단하는가.** 타 열쇠로 서명한 것은 개방되지 않는다.
    let other = seal::SigningKey::from_bytes(&[9u8; 32]);
    let forged = seal::seal(b"forged", MAGIC, "260801", [0, 5, 3], &KEY, &other)
        .expect("봉인하지 못했습니다");
    assert!(seal::parse(&forged, MAGIC, &verify).is_err(), "타 열쇠의 서명이 통과했습니다");

    // **훼손을 차단하는가.** 한 바이트만 변경해도 개방되지 않는다.
    let mut torn = sealed.clone();
    let at = sealed.len() / 2;
    torn[at] ^= 0xFF;
    assert!(
        seal::parse(&torn, MAGIC, &verify).is_err(),
        "훼손한 바이트가 통과했습니다"
    );
}
