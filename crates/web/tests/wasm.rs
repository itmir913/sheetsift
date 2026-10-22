//! **브라우저와 주고받는 겹이 실제로 도는가.**
//!
//! 알맹이는 `cargo test --workspace` 가 데스크톱에서 다 본다. 여기서 보는
//! 것은 그 밖의 것 — `wasm_bindgen` 이 붙인 껍질이 글자를 받아 글자를
//! 돌려주는가, 오류가 던져지는가.
//!
//! ```bash
//! cargo test -p sheetsift-web --target wasm32-unknown-unknown
//! ```
#![cfg(target_arch = "wasm32")]

use sheetsift_web::bindings::Book;
use wasm_bindgen::JsValue;
use wasm_bindgen_test::wasm_bindgen_test;

/// 웹에는 파일 시스템이 없으므로 실행 파일 안에 넣어 둔다. 이름을 아스키로
/// 두는 까닭은 맥이 한글 이름을 NFD 로 바꿔 두면 `include_bytes!` 가 빌드 때
/// 못 찾기 때문이다.
const TINY_XLSX: &[u8] = include_bytes!("../../core/tests/fixtures/tiny.xlsx");

fn text(v: &JsValue) -> String {
    v.as_string().expect("오류가 글자로 오지 않았습니다")
}

/// 목록 글자에서 열쇠를 차례대로 집는다. 화면이 하는 일(JSON 을 풀어 열쇠를
/// 쓰는 것)을 여기서는 글자로 한다 — 이 시험이 보는 것은 모양이 아니라
/// **껍질이 도는가** 다.
fn key_at(catalog: &str, nth: usize) -> String {
    catalog
        .split("\"key\":\"")
        .nth(nth + 1)
        .expect("열쇠가 그만큼 없습니다")
        .split('"')
        .next()
        .unwrap()
        .to_string()
}

#[wasm_bindgen_test]
fn a_workbook_opens_and_lists_its_tables() {
    let book = Book::new(TINY_XLSX, "tiny.xlsx").expect("양식을 열지 못했습니다");
    let catalog = book.list_screens("{}").expect("목록을 받지 못했습니다");
    assert!(catalog.contains("사람"), "{catalog}");
    assert!(catalog.contains("안내"), "{catalog}");
    // 열 정의가 목록에 함께 온다.
    assert!(catalog.contains("\"headerFound\":true"), "{catalog}");
    assert!(catalog.contains("\"columns\""), "{catalog}");
}

#[wasm_bindgen_test]
fn a_page_comes_back() {
    let mut book = Book::new(TINY_XLSX, "tiny.xlsx").expect("양식을 열지 못했습니다");
    let page = book
        .query_rows(r#"{"screenId":"사람","limit":10}"#)
        .expect("조회가 돌지 않았습니다");
    assert!(page.contains("\"total\":3"), "{page}");
    assert!(page.contains("가람"), "{page}");
}

#[wasm_bindgen_test]
fn a_value_list_comes_back() {
    let mut book = Book::new(TINY_XLSX, "tiny.xlsx").expect("양식을 열지 못했습니다");
    let catalog = book.list_screens("{}").expect("목록을 받지 못했습니다");
    // 둘째 열이 지역이다. 열쇠는 목록이 말해 준다.
    let key = key_at(&catalog, 1);
    let values = book
        .column_values(&format!(r#"{{"screenId":"사람","columnKey":"{key}"}}"#))
        .expect("값 목록이 오지 않았습니다");
    assert!(values.contains("서울") && values.contains("부산"), "{values}");
}

/// **오류는 코드로 던진다.** 사람이 읽을 문구를 여기서 만들지 않는다 —
/// 문구는 화면이 붙인다.
#[wasm_bindgen_test]
fn an_error_is_thrown_as_a_code() {
    let mut book = Book::new(TINY_XLSX, "tiny.xlsx").expect("양식을 열지 못했습니다");
    let err = book
        .query_rows(r#"{"screenId":"없는시트","limit":10}"#)
        .expect_err("없는 표가 열렸습니다");
    let json = text(&err);
    assert!(json.contains("SHEET_UNREADABLE"), "{json}");
}

/// 표가 아닌 시트를 물으면 **빈 쪽이 아니라 던진다.** 빈 쪽은 화면이
/// "자료가 없다" 로 읽는다.
#[wasm_bindgen_test]
fn asking_for_a_sheet_without_a_header_is_thrown() {
    let mut book = Book::new(TINY_XLSX, "tiny.xlsx").expect("양식을 열지 못했습니다");
    let err = book
        .query_rows(r#"{"screenId":"안내","limit":10}"#)
        .expect_err("표가 아닌 시트가 열렸습니다");
    assert!(text(&err).contains("SHEET_EMPTY"), "{}", text(&err));
}

/// 깨진 요청도 조용히 기본값으로 떨어지지 않는다.
#[wasm_bindgen_test]
fn a_broken_request_is_thrown() {
    let mut book = Book::new(TINY_XLSX, "tiny.xlsx").expect("양식을 열지 못했습니다");
    assert!(book.query_rows("이건 JSON 이 아니다").is_err());
}

/// 화면이 손으로 적으면 안 되는 값들이 글자로 온다.
#[wasm_bindgen_test]
fn the_limits_come_across() {
    let l = sheetsift_web::bindings::limits().expect("상한을 받지 못했습니다");
    assert!(l.contains("blankToken") && l.contains("maxPage"), "{l}");
}
