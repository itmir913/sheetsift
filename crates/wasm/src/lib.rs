//! **브라우저와 수수하는 겹.** 문자를 수용해 문자를 반환하는 것이 전부다.
//!
//! 알맹이는 [`sheetsift_shell::Viewer`] 에 소재하고 이 상자를 인지하지 않는다.
//! 여기 소재하는 것은 `wasm_bindgen` 이 포착할 수 있는 형태로 변환하는 껍질뿐
//! 이다 — 오류를 `JsValue` 로, 구조체를 JSON 문자로.
//!
//! **박막으로 유지하는 것이 요점이다.** 여기 논리가 유입되면 그만큼이
//! `cargo test --workspace` 외부로 이탈한다. 브라우저가 존재해야 판정 가능한
//! 코드이기 때문이다.
//!
//! JSON 문자로 수수하는 사유는, 그 경계를 **단일 방법으로만** 통과하기
//! 위해서다. 타입마다 상이한 경로를 개설하면 어느 쪽이 어떻게 전달되는지가
//! 위치마다 상이해진다.
//!
//! ## 명칭과 요청은 부르는 곳의 것과 동일하다
//!
//! 문 명칭(`listScreens`·`queryRows`·`columnValues`)과 요청의 항목 명칭은 이
//! 바탕을 사용하는 앱의 명령 명칭과 동일한 벌로 배치한다. 화면을 웹으로 이전할
//! 때 호출부가 두 벌이 되지 않게 하는 것이다 — 두 벌이 되면 한쪽만 수정된다.

#![cfg(target_arch = "wasm32")]

use wasm_bindgen::prelude::*;

use sheetsift_shell::{Ask, CatalogAsk, Limits, ValuesAsk, Viewer};

/// 화면이 임의로 기재하면 안 되는 값들 (JSON). 기동 시 1회 수령한다.
#[wasm_bindgen]
pub fn limits() -> std::result::Result<String, JsValue> {
    json(&Limits::default())
}

/// 바탕의 오류를 브라우저로 전달한다.
///
/// **사람이 읽을 문구를 여기서 생성하지 않는다.** 코드와 치환 값을 그대로
/// 전달하고, 문구는 화면이 부여한다 — 바탕의 약정 그대로다.
fn thrown(e: sheetsift_core::Error) -> JsValue {
    match serde_json::to_string(&e) {
        Ok(json) => JsValue::from_str(&json),
        // 오류를 문자로 변환하지 못하는 지점까지 도달했으면 코드만이라도 전달한다.
        Err(_) => JsValue::from_str(r#"{"code":"UNKNOWN"}"#),
    }
}

fn json<T: serde::Serialize>(v: &T) -> std::result::Result<String, JsValue> {
    serde_json::to_string(v).map_err(|e| {
        thrown(sheetsift_core::Error::new(sheetsift_core::ErrorCode::DbQueryFailed).detail(e))
    })
}

/// 개방한 양식 하나. 화면이 이것을 보유한다.
#[wasm_bindgen]
pub struct Book {
    inner: Viewer,
}

#[wasm_bindgen]
impl Book {
    /// 양식 바이트를 수용한다. 무엇이 수록되었는지는 [`Book::list_screens`] 가 제시한다.
    #[wasm_bindgen(constructor)]
    pub fn new(bytes: &[u8], name: &str) -> std::result::Result<Book, JsValue> {
        Viewer::open(bytes, name).map(|inner| Book { inner }).map_err(thrown)
    }

    /// 열람 가능한 표 전부와 그 열 정의 (JSON).
    ///
    /// 요청이 시트별 머리글 지정을 지참한다. 빈 요청(`{}`)이면 전부 추정값이다.
    #[wasm_bindgen(js_name = listScreens)]
    pub fn list_screens(&self, request: &str) -> std::result::Result<String, JsValue> {
        let ask: CatalogAsk = parsed(request)?;
        json(&self.inner.list_screens(&ask))
    }

    /// 한 페이지를 판독한다. 수용하는 것도 반환하는 것도 JSON 문자다.
    #[wasm_bindgen(js_name = queryRows)]
    pub fn query_rows(&mut self, request: &str) -> std::result::Result<String, JsValue> {
        let ask: Ask = parsed(request)?;
        let page = self.inner.query_rows(&ask).map_err(thrown)?;
        json(&page)
    }

    /// 값 선택 목록 (JSON 요청 하나).
    #[wasm_bindgen(js_name = columnValues)]
    pub fn column_values(&mut self, request: &str) -> std::result::Result<String, JsValue> {
        let ask: ValuesAsk = parsed(request)?;
        let values = self.inner.column_values(&ask).map_err(thrown)?;
        json(&values)
    }
}

/// 화면이 송신한 요청 문자를 구조로 변환한다.
///
/// **훼손된 요청은 중단한다.** 누락된 항목을 기본값으로 충전하면 질의하지 않은
/// 표의 첫 페이지가 그럴듯하게 반환된다.
fn parsed<T: serde::de::DeserializeOwned>(request: &str) -> std::result::Result<T, JsValue> {
    serde_json::from_str(request).map_err(|e| {
        thrown(sheetsift_core::Error::new(sheetsift_core::ErrorCode::DbQueryFailed).detail(e))
    })
}
