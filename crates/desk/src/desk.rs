//! **창이 호출하는 명령.**
//!
//! 명칭과 요청의 항목 명칭은 브라우저 껍질(`sheetsift-wasm`)의 것과 **동일한
//! 벌**이다. 화면이 단일 벌이고 호출 대상만 교체되기 때문이다 — 두 벌이 되면
//! 한쪽만 수정된다.
//!
//! | 창(여기) | 브라우저(`sheetsift-wasm`) |
//! |---|---|
//! | `open_workbook` | `new Book(bytes, name)` |
//! | `list_screens` | `listScreens(request)` |
//! | `query_rows` | `queryRows(request)` |
//! | `column_values` | `columnValues(request)` |
//! | `limits` | `limits()` |
//!
//! 상이한 것은 **개방하는 위치 하나**다. 브라우저는 사용자가 선택한 파일의
//! 바이트를 수령하고, 창은 경로를 수령해 직접 판독한다. 그 차이가 이 껍질이
//! 실재하는 사유다.
//!
//! ## 요청은 구조 그대로 수수한다
//!
//! 브라우저 껍질은 JSON 문자로 수수한다 — `wasm_bindgen` 이 임의의 구조를
//! 포착하지 못하기 때문이다. 창은 그 제약이 부재하므로 구조 그대로 수수하고,
//! 직렬화는 명령 겹이 수행한다. **요청의 항목 명칭은 동일하다** — 화면이 송신
//! 하는 것이 동일한 묶음이라는 것이 요점이고, 그것이 문자인지 구조인지는 그
//! 아래의 사안이다.

use std::path::Path;

use sheetsift_core::grid::{ColumnValues, Page};
use sheetsift_shell::{Ask, CatalogAsk, Limits, ScreenCatalog, ValuesAsk, Viewer};

use crate::error::{ErrorCode, Result, ShellError};
use crate::file;

/// 개방한 양식 하나와 그 출처.
struct Book {
    viewer: Viewer,
    /// 판독한 파일의 명칭. 창의 제목과 `.sift` 의 지문에 사용한다.
    name: String,
}

/// 창이 보유하는 상태. **양식 하나**다.
///
/// 복수를 동시에 개방하지 않는 사유는 화면이 단일 양식을 전제하기 때문이다.
/// 신규로 개방하면 직전의 것을 해제한다 — 묵시적으로 적재하면 어느 것을 조회
/// 중인지가 요청마다 상이해진다.
#[derive(Default)]
pub struct Desk {
    open: Option<Book>,
}

impl Desk {
    pub fn new() -> Self {
        Self::default()
    }

    /// **경로의 양식을 판독해 개방한다.**
    ///
    /// 목록을 함께 반환하지 않는다. 화면은 개방 직후 [`Desk::list_screens`] 를
    /// 호출하는데, 여기서도 반환하면 목록을 산출하는 경로가 두 벌이 된다.
    ///
    /// 개방하지 못하면 **직전의 양식을 존치한다.** 교체한 이후에 실패하면 조회
    /// 중이던 표가 소멸하고, 사용자에게는 오류 문구 하나만 도달한다.
    pub fn open_workbook(&mut self, path: &Path) -> Result<()> {
        let name = file::name_of(path);
        let bytes = file::read(path)?;
        self.open_bytes(&bytes, &name)
    }

    /// **수령한 바이트로 개방한다.** 창에 투입(drag-drop)한 파일이 그것이다 —
    /// 그 경로는 웹과 동일하게 바이트로 도달한다.
    ///
    /// 경로로 개방하는 쪽을 존치하는 사유는 **용량**이다. 바이트로 수령하면
    /// 화면이 파일 전체를 1회 적재하므로, 데스크톱이 해소하려는 제약이 그대로
    /// 잔존한다. 선택 대화상자는 경로만 전달한다.
    pub fn open_bytes(&mut self, bytes: &[u8], name: &str) -> Result<()> {
        let viewer = Viewer::open(bytes, name)?;
        self.open = Some(Book { viewer, name: name.to_string() });
        Ok(())
    }

    /// 개방한 양식의 파일 명칭. 부재하면 `None` 이다.
    pub fn name(&self) -> Option<&str> {
        self.open.as_ref().map(|b| b.name.as_str())
    }

    /// 열람 가능한 표 전부와 그 열 정의.
    pub fn list_screens(&self, ask: &CatalogAsk) -> Result<ScreenCatalog> {
        Ok(self.book()?.viewer.list_screens(ask))
    }

    /// 한 페이지를 판독한다.
    pub fn query_rows(&mut self, ask: &Ask) -> Result<Page> {
        Ok(self.book_mut()?.viewer.query_rows(ask)?)
    }

    /// 값 선택 목록.
    pub fn column_values(&mut self, ask: &ValuesAsk) -> Result<ColumnValues> {
        Ok(self.book_mut()?.viewer.column_values(ask)?)
    }

    /// 화면이 임의로 기재하면 안 되는 값들. 양식이 부재해도 응답한다 — 기동 시
    /// 1회 수령하는 값이라 개방 이전에 도달한다.
    pub fn limits(&self) -> Limits {
        Limits::default()
    }

    /// **개방한 양식이 부재하면 중단한다.** 공백 목록을 반환하면 화면은 "표가
    /// 부재하는 양식" 을 개방한 것으로 묘사하고, 사용자는 파일이 손상된 것으로
    /// 판독한다.
    fn book(&self) -> Result<&Book> {
        self.open.as_ref().ok_or_else(|| ShellError::new(ErrorCode::NoWorkbookOpen).into())
    }

    fn book_mut(&mut self) -> Result<&mut Book> {
        self.open.as_mut().ok_or_else(|| ShellError::new(ErrorCode::NoWorkbookOpen).into())
    }
}
