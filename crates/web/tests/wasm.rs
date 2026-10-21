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

#[wasm_bindgen_test]
fn a_workbook_opens_and_lists_its_sheets() {
    let book = Book::new(TINY_XLSX, "tiny.xlsx").expect("양식을 열지 못했습니다");
    let sheets = book.sheets().expect("시트 목록을 받지 못했습니다");
    assert!(sheets.contains("사람"), "{sheets}");
    assert!(sheets.contains("안내"), "{sheets}");
}

#[wasm_bindgen_test]
fn a_sheet_opens_and_a_page_comes_back() {
    let mut book = Book::new(TINY_XLSX, "tiny.xlsx").expect("양식을 열지 못했습니다");
    let view = book.open_sheet(0).expect("시트를 열지 못했습니다");
    assert!(view.contains("\"headerFound\":true"), "{view}");
    assert!(view.contains("\"rows\":3"), "{view}");

    let page = book.page(r#"{"limit":10}"#).expect("조회가 돌지 않았습니다");
    assert!(page.contains("\"total\":3"), "{page}");
    assert!(page.contains("가람"), "{page}");
}

#[wasm_bindgen_test]
fn a_value_list_comes_back() {
    let mut book = Book::new(TINY_XLSX, "tiny.xlsx").expect("양식을 열지 못했습니다");
    let view = book.open_sheet(0).expect("시트를 열지 못했습니다");
    // 둘째 열이 지역이다. key 는 머리글에서 고른 DB 열 이름이다.
    let key = view.split("\"key\":\"").nth(2).unwrap().split('"').next().unwrap().to_string();
    let values = book.values(&key, "[]", "").expect("값 목록이 오지 않았습니다");
    assert!(values.contains("서울") && values.contains("부산"), "{values}");
}

/// **오류는 코드로 던진다.** 사람이 읽을 문구를 여기서 만들지 않는다 —
/// 문구는 화면이 붙인다.
#[wasm_bindgen_test]
fn an_error_is_thrown_as_a_code() {
    let mut book = Book::new(TINY_XLSX, "tiny.xlsx").expect("양식을 열지 못했습니다");
    let err = book.open_sheet(9).expect_err("없는 시트가 열렸습니다");
    let json = text(&err);
    assert!(json.contains("SHEET_UNREADABLE"), "{json}");
}

/// 시트를 열기 전에 조회하면 선다. 빈 쪽을 돌려주면 화면이 "자료가 없다" 로
/// 읽는다.
#[wasm_bindgen_test]
fn asking_before_opening_a_sheet_is_thrown() {
    let book = Book::new(TINY_XLSX, "tiny.xlsx").expect("양식을 열지 못했습니다");
    assert!(book.page(r#"{"limit":10}"#).is_err());
}

/// 깨진 요청도 조용히 기본값으로 떨어지지 않는다.
#[wasm_bindgen_test]
fn a_broken_request_is_thrown() {
    let mut book = Book::new(TINY_XLSX, "tiny.xlsx").expect("양식을 열지 못했습니다");
    book.open_sheet(0).expect("시트를 열지 못했습니다");
    assert!(book.page("이건 JSON 이 아니다").is_err());
}
