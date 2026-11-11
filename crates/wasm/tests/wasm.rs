//! **브라우저와 수수하는 겹이 실제로 동작하는가.**
//!
//! 알맹이는 `cargo test --workspace` 가 데스크톱에서 전부 판정한다. 여기서
//! 판정하는 것은 그 외의 것 — `wasm_bindgen` 이 부착한 껍질이 문자를 수용해
//! 문자를 반환하는가, 오류가 발생하는가.
//!
//! ```bash
//! cargo test -p sheetsift-wasm --target wasm32-unknown-unknown
//! ```
#![cfg(target_arch = "wasm32")]

use sheetsift_wasm::Book;
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
    let l = sheetsift_wasm::limits().expect("상한을 수령하지 못했습니다");
    assert!(l.contains("blankToken") && l.contains("maxPage"), "{l}");
}

/* ── 화면이 보내는 JSON 이 이 겹을 통과하는가 ──────────────────────
   알맹이는 `sheetsift-shell` 이 JSON 을 역직렬화해 `Viewer` 에 넘기는 것까지
   판정한다. 여기서만 판정할 수 있는 것은 **그 사이의 겹** 이다 — `parsed` 가
   요청을 받고, `json` 이 응답을 문자로 환원하고, `thrown` 이 오류를 코드로
   전달하는 자리. 그 겹에서 항목 하나가 탈락해도 양측의 시험은 전부 초록이다.

   2026-11-11 에 실제로 겪은 것이 이 경로다. 「값 목록이 범위를 반영하지
   않는다」는 결함이 보고되었고, 엔진에도 껍질에도 결함이 부재했다 — 낡은
   산출물이었다. 그때 이 겹을 지나는 시험이 하나도 없었다. */

/// 숫자 한 열. 범위를 거는 경로를 판정하는 데 필요한 최소다. `-` 는 숫자로
/// 판독되지 않는 값이고, 범위가 걸리면 탈락해야 한다.
const SCORES_CSV: &[u8] = "점수\n9\n-\n100\n20\n".as_bytes();

/// 목록 문자에서 공백 표식을 취한다. 화면도 이것을 `limits()` 에서 수령한다.
fn blank_token() -> String {
    let l = sheetsift_wasm::limits().expect("상한을 수령하지 못했습니다");
    l.split("\"blankToken\":\"")
        .nth(1)
        .expect("공백 표식이 부재합니다")
        .split('"')
        .next()
        .unwrap()
        .to_string()
}

fn scores() -> (Book, String) {
    let book = Book::new(SCORES_CSV, "점수.csv").expect("csv 를 개방하지 못했습니다");
    let catalog = book.list_screens("{}").expect("목록을 수령하지 못했습니다");
    assert!(catalog.contains("\"kind\":\"number\""), "숫자 열로 판독되지 않았습니다: {catalog}");
    let key = key_at(&catalog, 0);
    (book, key)
}

/// **범위가 화면이 보내는 JSON 그대로 이 겹을 통과한다.** 항목 명칭(`min`·`max`)이
/// 어긋나거나 중간에서 탈락하면 조건이 **묵시적으로 누락된 채** 질의된다 —
/// 전부 통과하는 페이지가 도달하고, 아무 곳에서도 오류가 발생하지 않는다.
#[wasm_bindgen_test]
fn a_range_crosses_the_boundary_as_json() {
    let (mut book, key) = scores();
    let page = book
        .query_rows(&format!(
            r#"{{"screenId":"점수","limit":10,"filters":[{{"key":"{key}","min":10,"max":100}}]}}"#
        ))
        .expect("범위 조회가 동작하지 않았습니다");
    assert!(page.contains("\"total\":2"), "20 과 100 만 통과해야 합니다: {page}");
}

/// **값 목록이 범위를 반영한다.** 목록이 범위 밖의 값을 담으면, 그것을 선택하는
/// 순간 범위와 AND 로 겹쳐 **조용히 0건**이 된다. 실제로 보고된 결함이 이것이다.
#[wasm_bindgen_test]
fn the_value_list_follows_the_range_across_the_boundary() {
    let (mut book, key) = scores();
    let ranged = book
        .column_values(&format!(
            r#"{{"screenId":"점수","columnKey":"{key}","filters":[{{"key":"{key}","min":10,"max":100}}]}}"#
        ))
        .expect("값 목록이 도달하지 않았습니다");
    assert!(ranged.contains("\"20\"") && ranged.contains("\"100\""), "{ranged}");
    assert!(!ranged.contains("\"9\""), "범위 밖의 값이 목록에 있습니다: {ranged}");
    assert!(!ranged.contains(&blank_token()), "범위가 걸렸는데 공백이 있습니다: {ranged}");

    // **자료의 범위는 걸어 둔 범위에 좁아지지 않는다.** 손잡이의 양 끝이 이것이라,
    // 좁아지면 범위를 조정할 때마다 조작 가능한 폭이 축소된다.
    assert!(ranged.contains("\"range\""), "자료의 범위가 부재합니다: {ranged}");
    assert!(ranged.contains("\"min\":9"), "{ranged}");

    // 범위를 풀면 공백까지 전부로 복귀한다.
    let all = book
        .column_values(&format!(r#"{{"screenId":"점수","columnKey":"{key}"}}"#))
        .expect("값 목록이 도달하지 않았습니다");
    assert!(all.contains("\"9\"") && all.contains(&blank_token()), "{all}");
}

/// **문자 열에 범위를 걸면 중단한다.** 그대로 수행하면 모든 행이 탈락하는데,
/// 그것은 오류로 출현하지 않고 「자료 없음」으로 표시된다.
#[wasm_bindgen_test]
fn a_range_on_a_text_column_is_thrown() {
    let mut book = Book::new(TINY_XLSX, "tiny.xlsx").expect("양식을 개방하지 못했습니다");
    let catalog = book.list_screens("{}").expect("목록을 수령하지 못했습니다");
    // 2번째 열이 지역이다 — 문자 열이다.
    let key = key_at(&catalog, 1);
    let err = book.query_rows(&format!(
        r#"{{"screenId":"사람","limit":10,"filters":[{{"key":"{key}","min":1}}]}}"#
    ));
    assert!(err.is_err(), "문자 열에 범위가 적용되었습니다");
}

/// **상한이 하한에 미달하면 중단한다.** 그 조건은 아무 행도 통과시키지 못한다.
#[wasm_bindgen_test]
fn an_upside_down_range_is_thrown() {
    let (mut book, key) = scores();
    let err = book.query_rows(&format!(
        r#"{{"screenId":"점수","limit":10,"filters":[{{"key":"{key}","min":100,"max":10}}]}}"#
    ));
    assert!(err.is_err(), "뒤집힌 범위가 적용되었습니다");
}
