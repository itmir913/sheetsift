//! **브라우저와 수수하는 겹이 실제로 동작하는가.**
//!
//! 알맹이는 `cargo test --workspace` 가 데스크톱에서 전부 판정한다. 여기서
//! 판정하는 것은 그 외의 것 — `wasm_bindgen` 이 부착한 껍질이 문자를 수용해
//! 문자를 반환하는가, 오류가 발생하는가.
//!
//! ```bash
//! cargo test -p sheetsift-web --target wasm32-unknown-unknown
//! ```
#![cfg(target_arch = "wasm32")]

use sheetsift_web::bindings::Book;
use wasm_bindgen::JsValue;
use wasm_bindgen_test::wasm_bindgen_test;

/// 웹에는 파일 시스템이 부재하므로 실행 파일 내부에 수용한다. 명칭을 아스키로
/// 설정하는 사유는 맥이 한글 명칭을 NFD 로 변환하면 `include_bytes!` 가 빌드
/// 시점에 탐색하지 못하기 때문이다.
const TINY_XLSX: &[u8] = include_bytes!("../../core/tests/fixtures/tiny.xlsx");

fn text(v: &JsValue) -> String {
    v.as_string().expect("오류가 문자로 도달하지 않았습니다")
}

/// 목록 문자에서 열쇠를 순서대로 추출한다. 화면이 수행하는 작업(JSON 을 해석해
/// 열쇠를 사용하는 것)을 여기서는 문자로 수행한다 — 이 시험이 판정하는 것은
/// 구조가 아니라 **껍질이 동작하는가** 다.
fn key_at(catalog: &str, nth: usize) -> String {
    catalog
        .split("\"key\":\"")
        .nth(nth + 1)
        .expect("열쇠가 해당 수만큼 부재합니다")
        .split('"')
        .next()
        .unwrap()
        .to_string()
}

#[wasm_bindgen_test]
fn a_workbook_opens_and_lists_its_tables() {
    let book = Book::new(TINY_XLSX, "tiny.xlsx").expect("양식을 개방하지 못했습니다");
    let catalog = book.list_screens("{}").expect("목록을 수령하지 못했습니다");
    assert!(catalog.contains("사람"), "{catalog}");
    assert!(catalog.contains("안내"), "{catalog}");
    // 열 정의가 목록에 함께 도달한다.
    assert!(catalog.contains("\"headerFound\":true"), "{catalog}");
    assert!(catalog.contains("\"columns\""), "{catalog}");
}

#[wasm_bindgen_test]
fn a_page_comes_back() {
    let mut book = Book::new(TINY_XLSX, "tiny.xlsx").expect("양식을 개방하지 못했습니다");
    let page = book
        .query_rows(r#"{"screenId":"사람","limit":10}"#)
        .expect("조회가 동작하지 않았습니다");
    assert!(page.contains("\"total\":3"), "{page}");
    assert!(page.contains("가람"), "{page}");
}

#[wasm_bindgen_test]
fn a_value_list_comes_back() {
    let mut book = Book::new(TINY_XLSX, "tiny.xlsx").expect("양식을 개방하지 못했습니다");
    let catalog = book.list_screens("{}").expect("목록을 수령하지 못했습니다");
    // 2번째 열이 지역이다. 열쇠는 목록이 제시한다.
    let key = key_at(&catalog, 1);
    let values = book
        .column_values(&format!(r#"{{"screenId":"사람","columnKey":"{key}"}}"#))
        .expect("값 목록이 도달하지 않았습니다");
    assert!(values.contains("서울") && values.contains("부산"), "{values}");
}

/// **오류는 코드로 발생시킨다.** 사람이 읽을 문구를 여기서 생성하지 않는다 —
/// 문구는 화면이 부여한다.
#[wasm_bindgen_test]
fn an_error_is_thrown_as_a_code() {
    let mut book = Book::new(TINY_XLSX, "tiny.xlsx").expect("양식을 개방하지 못했습니다");
    let err = book
        .query_rows(r#"{"screenId":"없는시트","limit":10}"#)
        .expect_err("부재하는 표가 개방되었습니다");
    let json = text(&err);
    assert!(json.contains("SHEET_UNREADABLE"), "{json}");
}

/// 표가 아닌 시트를 질의하면 **공백 페이지가 아니라 오류를 발생시킨다.** 공백
/// 페이지는 화면이 "자료 없음" 으로 판독한다.
#[wasm_bindgen_test]
fn asking_for_a_sheet_without_a_header_is_thrown() {
    let mut book = Book::new(TINY_XLSX, "tiny.xlsx").expect("양식을 개방하지 못했습니다");
    let err = book
        .query_rows(r#"{"screenId":"안내","limit":10}"#)
        .expect_err("표가 아닌 시트가 개방되었습니다");
    assert!(text(&err).contains("SHEET_EMPTY"), "{}", text(&err));
}

/// 훼손된 요청도 묵시적으로 기본값으로 귀결되지 않는다.
#[wasm_bindgen_test]
fn a_broken_request_is_thrown() {
    let mut book = Book::new(TINY_XLSX, "tiny.xlsx").expect("양식을 개방하지 못했습니다");
    assert!(book.query_rows("이건 JSON 이 아니다").is_err());
}

/// 화면이 임의로 기재하면 안 되는 값들이 문자로 도달한다.
#[wasm_bindgen_test]
fn the_limits_come_across() {
    let l = sheetsift_web::bindings::limits().expect("상한을 수령하지 못했습니다");
    assert!(l.contains("blankToken") && l.contains("maxPage"), "{l}");
}
