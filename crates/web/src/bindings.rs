//! **브라우저와 주고받는 겹.** 글자를 받아 글자를 돌려주는 것이 전부다.
//!
//! 알맹이는 [`crate::Viewer`] 에 있고 이 파일을 모른다. 여기 있는 것은
//! `wasm_bindgen` 이 붙잡을 수 있는 모양으로 바꿔 주는 껍질뿐이다 — 오류를
//! `JsValue` 로, 구조체를 JSON 글자로.
//!
//! **얇게 두는 것이 요점이다.** 여기 로직이 들어오면 그만큼이
//! `cargo test --workspace` 밖으로 빠진다. 브라우저가 있어야 볼 수 있는
//! 코드이기 때문이다.
//!
//! JSON 글자로 주고받는 까닭은, 그 경계를 **한 가지 방법으로만** 넘기 위해서다.
//! 타입마다 다른 길을 내면 어느 쪽이 어떻게 넘어가는지가 자리마다 달라진다.

#![cfg(target_arch = "wasm32")]

use wasm_bindgen::prelude::*;

use crate::{Ask, Limits, ValuesAsk, Viewer};

/// 화면이 손으로 적으면 안 되는 값들 (JSON). 띄울 때 한 번 받는다.
#[wasm_bindgen]
pub fn limits() -> std::result::Result<String, JsValue> {
    json(&Limits::default())
}

/// 바탕의 오류를 브라우저로 넘긴다.
///
/// **사람이 읽을 문구를 여기서 만들지 않는다.** 코드와 치환 값을 그대로
/// 넘기고, 문구는 화면이 붙인다 — 바탕의 약속 그대로다.
fn thrown(e: sheetsift_core::Error) -> JsValue {
    match serde_json::to_string(&e) {
        Ok(json) => JsValue::from_str(&json),
        // 오류를 글자로 못 바꾸는 자리까지 왔으면 코드만이라도 넘긴다.
        Err(_) => JsValue::from_str(r#"{"code":"UNKNOWN"}"#),
    }
}

fn json<T: serde::Serialize>(v: &T) -> std::result::Result<String, JsValue> {
    serde_json::to_string(v).map_err(|e| {
        thrown(sheetsift_core::Error::new(sheetsift_core::ErrorCode::DbQueryFailed).detail(e))
    })
}

/// 열어 둔 양식 하나. 화면이 이것을 들고 있는다.
#[wasm_bindgen]
pub struct Book {
    inner: Viewer,
}

#[wasm_bindgen]
impl Book {
    /// 양식 바이트를 받는다. 돌려주는 것은 시트 이름 목록(JSON)이다.
    #[wasm_bindgen(constructor)]
    pub fn new(bytes: &[u8], name: &str) -> std::result::Result<Book, JsValue> {
        Viewer::open(bytes, name).map(|inner| Book { inner }).map_err(thrown)
    }

    /// 시트 이름을 탭 차례대로 (JSON).
    pub fn sheets(&self) -> std::result::Result<String, JsValue> {
        json(&self.inner.opened())
    }

    /// 시트 하나를 열고 그 열 정의를 돌려준다 (JSON).
    #[wasm_bindgen(js_name = openSheet)]
    pub fn open_sheet(&mut self, at: usize) -> std::result::Result<String, JsValue> {
        let view = self.inner.open_sheet(at).map_err(thrown)?;
        json(&view)
    }

    /// 한 쪽을 읽는다. 받는 것도 돌려주는 것도 JSON 글자다.
    pub fn page(&mut self, request: &str) -> std::result::Result<String, JsValue> {
        let ask: Ask = parsed(request)?;
        let page = self.inner.page(&ask).map_err(thrown)?;
        json(&page)
    }

    /// 값 선택 목록 (JSON 요청 하나).
    pub fn values(&mut self, request: &str) -> std::result::Result<String, JsValue> {
        let ask: ValuesAsk = parsed(request)?;
        let values = self.inner.values(&ask).map_err(thrown)?;
        json(&values)
    }
}

/// 화면이 보낸 요청 글자를 모양으로 바꾼다.
///
/// **깨진 요청은 선다.** 빠진 칸을 기본값으로 메우면 묻지 않은 표의 첫 쪽이
/// 그럴듯하게 돌아간다.
fn parsed<T: serde::de::DeserializeOwned>(request: &str) -> std::result::Result<T, JsValue> {
    serde_json::from_str(request).map_err(|e| {
        thrown(sheetsift_core::Error::new(sheetsift_core::ErrorCode::DbQueryFailed).detail(e))
    })
}
