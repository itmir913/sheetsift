//! **브라우저에서 표를 열어 보는 껍데기.**
//!
//! 바탕([`sheetsift_core`])이 바이트를 받아 SQLite 로 바꾸고 조회까지 하므로,
//! 여기서 하는 일은 그 셋을 **한 자리에 묶어 들고 있는 것**이다 — 바꿔 놓은
//! 연결, 그 표의 열 정의, 그리고 화면이 보낸 요청.
//!
//! ## 알맹이는 브라우저를 모른다
//!
//! 이 파일에는 `wasm_bindgen` 이 없다. 브라우저와 주고받는 겹은
//! [`bindings`] 한 파일이고, 거기 있는 것은 글자를 받아 글자를 돌려주는
//! 얇은 껍질뿐이다.
//!
//! **까닭은 시험이다.** `wasm_bindgen` 으로 쓴 코드는 브라우저나 노드가
//! 있어야 돌지만, 여기 있는 것은 `cargo test --workspace` 가 데스크톱에서
//! 그대로 본다. 관문 하나가 알맹이를 다 보고, 웹에서만 드러나는 자리는
//! 따로 작은 시험을 둔다.

use sheetsift_core::grid::{self, Column, ColumnKind, Table};
use sheetsift_core::sheet::{self, Sheet, TablePlan};
use sheetsift_core::{infer, Connection, Error, ErrorCode, Result};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::BTreeMap;

pub mod bindings;

/// 값이 없는 칸에 적는 글자.
///
/// **고를 수 있어야 한다.** 양식마다 다르고(`-` · `–` · `N/A`), 무엇을 "없음"
/// 으로 볼지가 갈리면 값 선택 목록과 필터가 같은 파일 안에서 어긋난다. 지금은
/// 화면이 고르게 하지 않고 이 값을 쓰되, **한 곳에만 둔다.**
pub const BLANK: &str = "-";

/* ── 화면에 넘기는 모양 ─────────────────────────────────────────── */

/// 머리글이 어디서 몇 줄인가. **사용자가 조작부에서 지정한다.**
///
/// 추정으로 전부 적중시킬 수 없다 — 열마다 단수가 상이하거나(`학년` 은 1단,
/// `국어 등급` 은 2단) 상단에 제목 줄이 부착된 양식이 실재한다. 적중시킬 수
/// 없는 것을 적중한 것처럼 반환하지 않고, 최종 결정을 사용자에게 위임한다.
///
/// 이 지정은 **요청마다 지참한다.** 겹이 기억하면, 지정을 변경한 직후 도착한
/// 지연 응답이 신규 열 구성으로 오인되어 표시된다. 오류는 발생하지 않는다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeaderSpec {
    /// 머리글이 개시되는 줄. 0부터 센다.
    pub at: usize,
    /// 머리글 줄 수. 1 이상이어야 한다.
    pub rows: usize,
}

/// 열 하나를 화면이 아는 모양으로.
///
/// 칸 이름과 뜻은 이 표를 그릴 화면이 쓰는 것과 **같은 벌**이다. 머리글을
/// 누를 수 있는지·값 선택을 띄울 수 있는지를 화면이 열마다 보는데, 그것이
/// 없으면 화면은 "다 된다" 고 짐작하게 되고 그 짐작은 표마다 다르다.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnView {
    pub key: String,
    pub label: String,
    /// `"number"` 또는 `"text"`.
    pub kind: String,
    /// 머리글을 눌러 줄을 세울 수 있는가.
    pub sortable: bool,
    /// 값 선택 목록을 띄울 수 있는가.
    pub filterable: bool,
    /// **알아보지 못한 열인가.** 화면이 그것을 말해 줄 수 있어야 한다 —
    /// 짐작해서 숫자로 정렬해 놓고 가만히 있으면 틀린 차례를 참으로 본다.
    pub guessed: bool,
}

/// **화면이 손으로 적으면 안 되는 값들.**
///
/// 빈칸 표식은 값 선택 목록에 그대로 나오고 **거를 때도 그 글자로 돌려줘야**
/// 한다. 쪽 크기 상한을 모르면 그보다 큰 수를 달라고 하고선 말없이 적게 받은
/// 것을 다 받은 줄 안다. 어느 쪽도 오류가 나지 않는다 — 그래서 화면에 적지
/// 않고 여기서 내준다.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Limits {
    pub blank_token: String,
    pub max_values: u32,
    pub max_page: u32,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            blank_token: grid::BLANK_TOKEN.to_string(),
            max_values: grid::MAX_VALUES,
            max_page: grid::MAX_LIMIT,
        }
    }
}

/// 볼 수 있는 표 하나. **시트 하나가 화면 하나다.**
///
/// 칸 이름과 뜻은 이 표를 그릴 화면이 쓰는 화면 정의와 같은 벌이다. 다른
/// 이름으로 같은 것을 담으면 화면이 두 벌을 알아야 한다.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreenView {
    /// 조회 요청이 들고 오는 이름. 여기서는 시트 이름이다.
    pub id: String,
    /// 무엇으로 그리는 화면인가. 여기는 표뿐이다.
    pub kind: String,
    /// 화면에 적는 이름. 시트 이름 그대로다 — **탭을 보고 찾으라는 것이다.**
    pub title: String,
    pub description: String,
    /// 왼쪽 목록에서 묶이는 자리.
    pub group: String,
    pub columns: Vec<ColumnView>,
    /// 머리글을 찾지 못했으면 열이 없다. **열이 없는 것과 아직 안 본 것은
    /// 다르다** — 화면이 그 까닭을 말할 수 있어야 한다.
    pub header_found: bool,
    /// 적용된 머리글 지정. 조작부가 이 값을 표시하고 변경한다.
    pub header: Option<HeaderSpec>,
    /// 그 지정이 **추정**인가. 참이면 사용자가 확인하지 않은 값이다.
    ///
    /// 화면은 이것을 근거로 "추정했습니다" 를 표시한다. 추정을 확정처럼 두면
    /// 열이 하나 밀린 표가 정상으로 보인다 — 조용히 틀리는 쪽이다.
    pub header_guessed: bool,
    /// 시트의 총 줄 수. 조작부가 지정 가능한 범위의 상한이다.
    pub sheet_rows: usize,
}

/// 양식을 연 직후 화면이 알아야 하는 것 전부.
///
/// 한 번에 다 준다. 표마다 따로 물어야 하면 "무엇을 아직 안 물었는가" 를
/// 화면이 기억해야 하고, 그 기억이 화면마다 달라진다.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreenCatalog {
    /// 왼쪽 목록의 묶음 차례. 양식 하나가 한 묶음이라 이름은 그 파일 이름이다.
    pub groups: Vec<String>,
    /// 프로그램 자체를 다루는 메뉴의 묶음. 보는 겹에는 그런 화면이 없다.
    pub foot_groups: Vec<String>,
    /// 값이 없는 칸에 양식이 적어 두는 글자.
    pub blank: String,
    /// 숫자 칸의 소수 자릿수.
    ///
    /// **모른다.** 보는 겹은 양식의 값을 고치지 않고 그대로 보여 주므로 정할
    /// 자리가 없다. 0 을 보내면 양식의 소수가 조용히 반올림되고, 그것은 화면
    /// 어디에서도 드러나지 않는다. 자릿수를 아는 앱이 그 값을 들고 온다.
    pub decimals: Option<u8>,
    pub screens: Vec<ScreenView>,
}

/// 화면이 보내는 조회 요청. [`grid::Query`] 와 같은 모양이되 글자로 온다.
///
/// **무슨 표를 보는지를 요청이 들고 온다.** 열어 둔 것이 무엇인지 겹 쪽에서
/// 기억하고 요청은 그것을 묻지 않으면, 화면이 표를 바꾼 뒤에 늦게 도착한
/// 앞 요청의 답이 **새 표의 것처럼** 그려진다. 오류는 나지 않는다.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Ask {
    /// 볼 표. 여기서는 시트 이름이다.
    pub screen_id: String,
    /// 적용할 머리글 지정. 없으면 추정한 것을 사용한다.
    #[serde(default)]
    pub header: Option<HeaderSpec>,
    #[serde(default)]
    pub sorts: Vec<grid::Sort>,
    #[serde(default)]
    pub filters: Vec<grid::Filter>,
    #[serde(default)]
    pub offset: u32,
    #[serde(default)]
    pub limit: u32,
}

/// 값 선택 목록 요청.
///
/// 조회 요청과 **같은 길로** 온다 — 칸 하나짜리 요청이라도 자리마다 다른
/// 방법으로 넘기면, 어느 쪽이 어떻게 넘어가는지가 부르는 자리마다 갈린다.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ValuesAsk {
    /// 볼 표. 여기서는 시트 이름이다.
    pub screen_id: String,
    /// 적용할 머리글 지정. 조회와 동일한 값이어야 한다.
    #[serde(default)]
    pub header: Option<HeaderSpec>,
    pub column_key: String,
    /// 지금 걸린 필터들. 이 열 자신의 것은 바탕이 빼고 센다.
    #[serde(default)]
    pub filters: Vec<grid::Filter>,
    /// 후보를 좁히는 글자.
    #[serde(default)]
    pub needle: String,
}

/// 화면 목록 요청.
///
/// 시트별 머리글 지정을 지참한다. 기재하지 않은 시트는 추정한 것을 사용한다.
/// 사용자가 한 시트의 지정을 변경하면 그 시트만 기재해 재요청한다.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogAsk {
    #[serde(default)]
    pub headers: BTreeMap<String, HeaderSpec>,
}

/* ── 들고 있는 것 ───────────────────────────────────────────────── */

/// 열어 둔 표 하나. 바꿔 놓은 연결과 그 열 정의를 함께 든다.
struct Open {
    conn: Connection,
    name: String,
    /// 이 표를 구축할 때 적용한 머리글 지정. 요청의 것과 대조한다.
    header: HeaderSpec,
    columns: Vec<Column>,
    rows: u32,
}

impl Table for Open {
    fn id(&self) -> &str {
        &self.name
    }
    fn base(&self) -> &str {
        "t"
    }
    fn columns(&self) -> Box<dyn Iterator<Item = &Column> + '_> {
        Box::new(self.columns.iter())
    }
}

#[derive(Clone)]
/// 시트 하나와, 지정된 머리글로 산출한 생김새.
///
/// 생김새는 **머리글 지정마다 달라진다.** 사용자가 조작부에서 줄 수를 변경하면
/// 열의 명칭도 개수도 종류도 전부 변경된다. 그래서 하나로 고정하지 않고,
/// 요청이 지참한 지정으로 그때 산출한다.
struct Shape {
    /// 적용된 머리글 지정. 없으면 판별에 실패한 것이다.
    spec: Option<HeaderSpec>,
    /// 추정으로 결정한 것인가. 사용자가 지정한 것이면 거짓이다.
    guessed: bool,
    /// 각 열의 표시 명칭. SQL 열쇠도 이것에서 파생한다.
    labels: Vec<String>,
    /// 조회 엔진이 사용하는 열 정의.
    columns: Vec<Column>,
    /// 화면이 참조하는 열 정의. 동일한 열을 동일한 순서로 보유한다.
    view: Vec<ColumnView>,
}

impl Shape {
    fn empty() -> Self {
        Self { spec: None, guessed: true, labels: Vec::new(), columns: Vec::new(), view: Vec::new() }
    }

    /// 지정된 머리글로 생김새를 산출한다.
    ///
    /// **SQL 열쇠를 머리글 원문이 아니라 표시 명칭에서 파생한다.** 2단 머리글은
    /// 하단 줄이 일부 열에서 공백이라(`학년` 은 상단에만 존재), 원문으로 열쇠를
    /// 지으면 다수의 열이 `_`·`_2`·`_3` 이 된다. 유일하기는 하나 조회 오류가
    /// 발생했을 때 어느 열인지 판독할 수 없다. 표시 명칭은 이미 합성되어 유일하다.
    fn derive(layout: infer::Layout, guessed: bool) -> Self {
        let infer::Header::Found { at, rows } = layout.header else {
            return Self::empty();
        };
        if layout.columns.is_empty() {
            return Self::empty();
        }

        let labels: Vec<String> = layout
            .columns
            .iter()
            .enumerate()
            .map(|(i, c)| match &c.group {
                // 2단 이상은 `묶음 · 명칭` 으로 한 줄에 수용한다. 표의 머리글이
                // 한 줄이라 그렇다. 합성하지 않으면 `등급` 이 여섯 열에 중복된다.
                Some(g) if !g.is_empty() && g != &c.label => format!("{g} · {}", c.label),
                // 명칭이 없는 열. 사용자가 지정한 범위에 병합으로 비어 있던 칸이
                // 포함되면 발생한다. **엑셀의 열 기호를 부여한다** — 공백으로
                // 두면 머리글이 비어 열을 지시할 수 없고, 열쇠도 `_`·`_2` 가
                // 되어 조회 오류에서 어느 열인지 판독할 수 없다.
                _ if c.label.is_empty() => format!("{}열", column_ref(i)),
                _ => c.label.clone(),
            })
            .collect();

        // 열쇠는 **바탕이 짓는다.** 여기서 동일한 규칙을 한 벌 더 기재하면,
        // 바탕이 명명 규칙을 변경하는 시점에 양자가 조용히 상이해진다.
        let head = Sheet { rows: vec![labels.clone()] };
        let keys = sheet::column_names(&head);

        let mut columns = Vec::with_capacity(keys.len());
        let mut view = Vec::with_capacity(keys.len());
        for (i, (_, db)) in keys.iter().enumerate() {
            let guess = &layout.columns[i];
            let number = guess.kind == infer::Kind::Number;
            let label = labels[i].clone();
            columns.push(Column {
                key: db.clone(),
                label: label.clone(),
                column: db.clone(),
                kind: if number { ColumnKind::Number } else { ColumnKind::Text },
                sortable: true,
                filterable: true,
                signed: false,
                dict: false,
                alias: String::new(),
            });
            view.push(ColumnView {
                key: db.clone(),
                label,
                kind: if number { "number" } else { "text" }.to_string(),
                sortable: true,
                filterable: true,
                guessed: guess.kind == infer::Kind::Unknown,
            });
        }
        grid::assign_aliases(columns.iter_mut());
        Self { spec: Some(HeaderSpec { at, rows }), guessed, labels, columns, view }
    }
}

/// 시트 하나를 **판독만 해 둔** 것.
///
/// SQLite 표로 변환하는 작업은 그 표를 최초로 조회할 때 수행한다 — 사전에 전부
/// 변환하면 대형 양식에서 **참조하지도 않을 시트 때문에 최초 화면이 지연된다.**
///
/// 추정으로 산출한 생김새는 개방 시점에 확보한다. 양식의 줄이 그때 이미
/// 메모리에 존재하고, 화면 목록이 **한 번에** 도달하려면 전 시트의 열을 알아야
/// 한다. 사용자가 지정을 변경하면 그 시트만 그 시점에 재산출한다.
struct Tab {
    name: String,
    data: Sheet,
    /// 추정으로 산출한 기본 생김새.
    guess: Shape,
}

impl Tab {
    fn read(name: String, data: Sheet) -> Self {
        let guess = Shape::derive(infer::layout(&data, BLANK), true);
        Self { name, data, guess }
    }

    /// 지정된 머리글의 생김새. 지정이 없거나 추정과 동일하면 산출을 생략한다.
    fn shape(&self, spec: Option<HeaderSpec>) -> Cow<'_, Shape> {
        match spec {
            None => Cow::Borrowed(&self.guess),
            Some(s) if self.guess.spec == Some(s) && self.guess.guessed => Cow::Borrowed(&self.guess),
            Some(s) => Cow::Owned(Shape::derive(infer::layout_at(&self.data, BLANK, s.at, s.rows), false)),
        }
    }

    /// 시트의 총 줄 수. 조작부가 지정할 수 있는 범위의 상한이다.
    fn height(&self) -> usize {
        self.data.rows.len()
    }
}

/// 양식 하나를 열어 둔 상태.
pub struct Viewer {
    /// 왼쪽 목록의 묶음 이름. 양식 하나가 한 묶음이라 그 파일 이름이다.
    group: String,
    tabs: Vec<Tab>,
    open: Option<Open>,
}

impl Viewer {
    /// 양식 바이트를 받는다. xlsx 면 시트가 여럿, csv 면 하나다.
    ///
    /// 무엇인지는 **이름이 아니라 속으로** 가린다. 받아 든 파일의 확장자는
    /// 사람이 고친 것일 수 있고, 그때 조용히 틀린 쪽으로 읽으면 깨진 표가
    /// 그럴듯하게 열린다.
    pub fn open(bytes: &[u8], name: &str) -> Result<Self> {
        let raw = if is_zip(bytes) {
            sheet::from_xlsx(bytes)?
        } else {
            vec![(sheet_name_of(name), sheet::from_csv(bytes)?)]
        };
        let tabs = raw.into_iter().map(|(n, d)| Tab::read(n, d)).collect();
        Ok(Self { group: sheet_name_of(name), tabs, open: None })
    }

    /// 열람 가능한 표 전부와, 화면이 임의로 기재하면 안 되는 값들.
    ///
    /// **탭 순서대로** 반환한다. 열 정의가 함께 도달하므로 화면은 표를 하나씩
    /// 개방하며 열을 수집하지 않는다.
    ///
    /// 요청이 시트별 머리글 지정을 지참한다. 기재하지 않은 시트는 추정값을
    /// 적용한다.
    pub fn list_screens(&self, ask: &CatalogAsk) -> ScreenCatalog {
        ScreenCatalog {
            groups: vec![self.group.clone()],
            foot_groups: Vec::new(),
            blank: BLANK.to_string(),
            decimals: None,
            screens: self
                .tabs
                .iter()
                .map(|t| {
                    let shape = t.shape(ask.headers.get(&t.name).copied());
                    ScreenView {
                        id: t.name.clone(),
                        kind: "grid".to_string(),
                        title: t.name.clone(),
                        description: String::new(),
                        group: self.group.clone(),
                        columns: shape.view.clone(),
                        header_found: shape.spec.is_some(),
                        header: shape.spec,
                        header_guessed: shape.guessed,
                        sheet_rows: t.height(),
                    }
                })
                .collect(),
        }
    }

    /// 시트 하나를 SQLite 표로 변환해 보유한다.
    ///
    /// 머리글 줄을 **합성해서** 본문 앞에 부착한다. 양식의 머리글 원문을 그대로
    /// 사용하지 않는 이유는 2단 머리글이다 — 하단 줄이 일부 열에서 공백이라
    /// 원문으로는 열쇠가 `_`·`_2` 가 된다. 표시 명칭은 이미 합성되어 유일하고,
    /// 목록이 내준 열쇠도 그것에서 파생했으므로 양자가 일치한다.
    fn open_sheet(&mut self, at: usize, spec: Option<HeaderSpec>) -> Result<()> {
        let tab = self
            .tabs
            .get(at)
            .ok_or_else(|| Error::new(ErrorCode::SheetUnreadable).with("sheet", at))?;
        let shape = tab.shape(spec);
        let Some(applied) = shape.spec else {
            self.open = None;
            return Ok(());
        };

        let width = shape.labels.len();
        let mut rows = Vec::with_capacity(1 + tab.data.rows.len());
        rows.push(shape.labels.clone());
        rows.extend(trimmed(&tab.data, applied.at + applied.rows, width));
        let body = Sheet { rows };

        let conn = Connection::open_in_memory()
            .map_err(|e| Error::new(ErrorCode::SheetTableFailed).detail(e))?;
        let plan = TablePlan { table: "t".into(), map: Vec::new(), indexes: Vec::new() };
        let made = sheet::build(&conn, &[(plan, body)], BLANK)?;
        let built = &made["t"];

        // **목록이 내준 열쇠와 표에 생성된 열쇠가 일치해야 한다.** 상이하면
        // 화면이 보유한 열쇠로 정렬·필터가 전부 "그런 열 없음" 이 되거나, 더
        // 불리하게는 **다른 열에 적용된다.**
        let built_keys: Vec<&String> = built.columns.iter().map(|(_, db)| db).collect();
        let listed_keys: Vec<&String> = shape.columns.iter().map(|c| &c.key).collect();
        if built_keys != listed_keys {
            return Err(Error::new(ErrorCode::SheetColumnMissing)
                .with("sheet", &tab.name)
                .detail("listed column keys differ from the built table"));
        }

        self.open = Some(Open {
            conn,
            name: tab.name.clone(),
            header: applied,
            columns: shape.columns.clone(),
            rows: built.rows as u32,
        });
        Ok(())
    }

    /// 요청이 지시하는 표를 개방된 상태로 만든다.
    ///
    /// 동일한 시트가 **동일한 머리글 지정으로** 개방되어 있으면 재사용하고,
    /// 아니면 재구축한다. 시트만 대조하면 지정을 변경해도 구 표로 응답하여,
    /// 화면은 신규 열 구성을 보고 있다고 오인한다 — 오류는 발생하지 않는다.
    ///
    /// **미존재 명칭은 중단한다** — 개방되어 있던 표로 응답하면 화면은 변경된
    /// 표를 열람 중이라고 신뢰한다.
    fn ready(&mut self, screen_id: &str, spec: Option<HeaderSpec>) -> Result<&Open> {
        let at = self
            .tabs
            .iter()
            .position(|t| t.name == screen_id)
            .ok_or_else(|| Error::new(ErrorCode::SheetUnreadable).with("sheet", screen_id))?;
        let wanted = self.tabs[at].shape(spec).spec;
        let same = self
            .open
            .as_ref()
            .is_some_and(|o| o.name == screen_id && Some(o.header) == wanted);
        if !same {
            self.open_sheet(at, spec)?;
        }
        // 머리글을 판별하지 못한 시트는 표가 없다. 빈 쪽을 반환하면 화면이
        // "자료 없음" 으로 판독한다.
        self.open
            .as_ref()
            .ok_or_else(|| Error::new(ErrorCode::SheetEmpty).with("sheet", screen_id))
    }

    /// 한 쪽을 읽는다.
    pub fn query_rows(&mut self, ask: &Ask) -> Result<grid::Page> {
        let open = self.ready(&ask.screen_id, ask.header)?;
        let q = grid::Query {
            sorts: ask.sorts.clone(),
            filters: ask.filters.clone(),
            offset: ask.offset,
            limit: ask.limit,
        };
        grid::run(&open.conn, open, BLANK, &q)
    }

    /// 값 선택 목록. 엑셀 필터 드롭다운에 해당한다.
    ///
    /// `needle` 은 후보를 좁히는 글자다. **받은 목록을 화면이 걸러 보여 주는
    /// 것으로는 안 된다** — 값이 많으면 바탕이 앞에서 잘라 보내므로, 찾는 값이
    /// 그 뒤에 있으면 "없습니다" 가 뜬다.
    pub fn column_values(&mut self, ask: &ValuesAsk) -> Result<grid::ColumnValues> {
        let open = self.ready(&ask.screen_id, ask.header)?;
        grid::column_values(
            &open.conn,
            open,
            BLANK,
            &ask.column_key,
            &ask.filters,
            &ask.needle,
        )
    }

    /// 열어 둔 표의 줄 수. 필터를 걸기 전의 값이다.
    pub fn rows(&self) -> u32 {
        self.open.as_ref().map(|o| o.rows).unwrap_or(0)
    }
}

/* ── 자잘한 것 ──────────────────────────────────────────────────── */

/// 열 번호를 엑셀의 열 기호로. 0 → `A`, 25 → `Z`, 26 → `AA`.
///
/// 명칭이 없는 열을 지시할 때 사용한다. 사람이 원본 엑셀에서 **그 열을 찾아갈
/// 수 있는** 유일한 표현이다.
fn column_ref(mut at: usize) -> String {
    let mut out = Vec::new();
    loop {
        out.push(b'A' + (at % 26) as u8);
        if at < 26 {
            break;
        }
        at = at / 26 - 1;
    }
    out.reverse();
    String::from_utf8(out).expect("ASCII")
}

/// xlsx 는 zip 이다. **이름이 아니라 속으로 가린다.**
fn is_zip(bytes: &[u8]) -> bool {
    bytes.starts_with(b"PK\x03\x04")
}

/// csv 에는 시트 이름이 없다. 파일 이름에서 만든다.
fn sheet_name_of(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let stem = base.rsplit_once('.').map(|(s, _)| s).unwrap_or(base);
    if stem.is_empty() {
        base.to_string()
    } else {
        stem.to_string()
    }
}

/// 머리글 줄과 그 아래만, **열 수만큼만** 남긴다.
///
/// 열 수를 맞추는 까닭이 있다. 본문에 머리글보다 오른쪽으로 삐져 나온 칸이
/// 있는 양식이 흔한데, 그대로 두면 이름 없는 열이 하나 생긴다.
fn trimmed(sheet: &Sheet, from: usize, width: usize) -> Vec<Vec<String>> {
    sheet.rows[from..]
        .iter()
        .map(|row| (0..width).map(|i| Sheet::at(row, i).to_string()).collect())
        .collect()
}


#[cfg(test)]
mod tests {
    use super::*;

    /// 바탕의 시험이 쓰는 그 작은 양식이다. 여기서도 같은 것을 쓴다.
    const TINY_XLSX: &[u8] = include_bytes!("../../core/tests/fixtures/tiny.xlsx");

    /// 깊이가 열마다 상이한 머리글. 좌측 둘은 1단, 우측 넷은 2단이다.
    /// **실재하는 진학결과 양식의 형태**이고, 추정은 여기서 실패한다.
    const MIXED_CSV: &[u8] = "학년,반,국어,,,영어
        ,,선택과목,표준점수,등급,등급
        3,1,언어와매체,104,3,2
        3,2,화법과작문,99,4,1
"
        .as_bytes();

    fn spec(at: usize, rows: usize) -> CatalogAsk {
        let mut headers = BTreeMap::new();
        headers.insert("진학결과".to_string(), HeaderSpec { at, rows });
        CatalogAsk { headers }
    }


    /// 목록에서 표 하나를 집어 온다. **열 정의는 목록에 함께 온다** — 시험도
    /// 화면과 같은 길로 본다.
    fn screen(v: &Viewer, at: usize) -> ScreenView {
        v.list_screens(&CatalogAsk::default()).screens[at].clone()
    }

    /// 그 표의 줄 수. **목록은 줄 수를 말하지 않는다** — 조회가 말하는 수와
    /// 두 벌이 되면 화면이 어느 것을 믿어야 할지 알 수 없다.
    fn total(v: &mut Viewer, sheet: &str) -> u32 {
        v.query_rows(&ask(sheet, 1)).unwrap().total
    }

    /// 표 이름을 탭 차례대로.
    fn names(v: &Viewer) -> Vec<String> {
        v.list_screens(&CatalogAsk::default()).screens.iter().map(|s| s.id.clone()).collect()
    }

    /// 조회 요청 하나. **무슨 표를 보는지는 요청이 들고 온다.**
    fn ask(sheet: &str, limit: u32) -> Ask {
        Ask { screen_id: sheet.to_string(), limit, ..Ask::default() }
    }

    /// 값 선택 요청 하나.
    fn values_ask(sheet: &str, key: &str) -> ValuesAsk {
        ValuesAsk {
            screen_id: sheet.to_string(),
            column_key: key.to_string(),
            ..ValuesAsk::default()
        }
    }

    #[test]
    fn a_workbook_lists_its_sheets_in_tab_order() {
        let v = Viewer::open(TINY_XLSX, "tiny.xlsx").unwrap();
        assert_eq!(names(&v), ["사람", "안내"]);
    }

    /// **목록 한 번에 열 정의까지 온다.** 표를 하나씩 열어 보며 열을 모으게
    /// 두면, 무엇을 아직 안 물었는지를 화면이 기억해야 한다.
    #[test]
    fn the_catalog_carries_every_table_s_columns_at_once() {
        let v = Viewer::open(TINY_XLSX, "tiny.xlsx").unwrap();
        let cat = v.list_screens(&CatalogAsk::default());
        let people = &cat.screens[0];
        assert!(people.header_found);
        assert!(!people.columns.is_empty(), "열을 열어 봐야 알려 준다");
        assert_eq!(people.kind, "grid");
        assert_eq!(people.title, people.id, "화면 이름은 탭 이름 그대로다");

        // 표가 아닌 시트도 목록에 있다. **열이 없는 것과 아직 안 본 것은
        // 다르다** — 빠뜨리면 화면이 탭을 그릴 수 없다.
        let guide = &cat.screens[1];
        assert!(!guide.header_found);
        assert!(guide.columns.is_empty());
    }

    /// 묶음은 양식 하나다. 화면이 왼쪽 목록을 그릴 때 쓴다.
    #[test]
    fn the_catalog_puts_one_workbook_in_one_group() {
        let v = Viewer::open(TINY_XLSX, "/어딘가/tiny.xlsx").unwrap();
        let cat = v.list_screens(&CatalogAsk::default());
        assert_eq!(cat.groups, ["tiny"]);
        assert!(cat.screens.iter().all(|s| s.group == "tiny"));
        assert!(cat.foot_groups.is_empty(), "보는 겹에는 프로그램 메뉴가 없다");
    }

    /// **빈칸 글자는 목록이 말한다.** 화면이 그 글자를 손으로 적으면 두 벌이
    /// 되어, 한쪽만 바뀌는 날 멀쩡한 자료가 "없음" 으로 보인다.
    #[test]
    fn the_catalog_says_what_a_blank_looks_like() {
        let v = Viewer::open(TINY_XLSX, "tiny.xlsx").unwrap();
        assert_eq!(v.list_screens(&CatalogAsk::default()).blank, BLANK);
    }

    /// **소수 자릿수는 모른다고 말한다.** 0 을 보내면 양식의 소수가 조용히
    /// 반올림되고, 그것은 화면 어디에서도 드러나지 않는다.
    #[test]
    fn the_catalog_does_not_guess_how_many_decimals() {
        let v = Viewer::open(TINY_XLSX, "tiny.xlsx").unwrap();
        assert_eq!(v.list_screens(&CatalogAsk::default()).decimals, None);
    }

    /// **목록의 열쇠로 바로 조회된다.** 열쇠는 표를 만들 때 바탕이 짓는데
    /// 목록은 그 전에 나간다 — 두 자리가 다른 이름을 지으면 화면이 든 열쇠로
    /// 정렬·필터가 모두 "그런 열 없다" 가 되거나 다른 열에 걸린다.
    #[test]
    fn a_key_from_the_catalog_works_without_opening_anything() {
        let mut v = Viewer::open(TINY_XLSX, "tiny.xlsx").unwrap();
        let view = screen(&v, 0);
        let key = view.columns[0].key.clone();
        let page = v
            .query_rows(&Ask {
                sorts: vec![serde_json::from_value(serde_json::json!({
                    "key": key, "dir": "asc"
                }))
                .unwrap()],
                ..ask(&view.id, 10)
            })
            .unwrap();
        assert_eq!(page.total, 3);
        assert_eq!(page.rows[0][0].as_str().unwrap(), "가람");
    }

    #[test]
    fn opening_a_sheet_gives_its_columns_and_rows() {
        let mut v = Viewer::open(TINY_XLSX, "tiny.xlsx").unwrap();
        let view = screen(&v, 0);
        assert!(view.header_found);
        assert_eq!(view.id, "사람");
        assert_eq!(total(&mut v, &view.id), 3);
        let labels: Vec<&str> = view.columns.iter().map(|c| c.label.as_str()).collect();
        assert_eq!(labels, ["이름", "지역", "점수"]);
    }

    /// **알아보지 못한 열은 그렇다고 말한다.** 점수 열에 "-" 가 섞여 있지만
    /// 그것은 "없음" 표기라 숫자로 갈린다. 짐작한 자리는 화면이 알아야 한다.
    #[test]
    fn a_column_says_whether_its_kind_was_guessed() {
        let v = Viewer::open(TINY_XLSX, "tiny.xlsx").unwrap();
        let view = screen(&v, 0);
        let by = |k: &str| view.columns.iter().find(|c| c.label == k).unwrap().clone();
        assert_eq!(by("이름").kind, "text");
        assert_eq!(by("점수").kind, "number");
        assert!(!by("점수").guessed, "'-' 는 없음 표기라 섞인 것이 아니다");
    }

    #[test]
    fn a_page_comes_back_sorted() {
        let mut v = Viewer::open(TINY_XLSX, "tiny.xlsx").unwrap();
        let view = screen(&v, 0);
        let name = view.columns[0].key.clone();
        let page = v
            .query_rows(&Ask {
                sorts: vec![serde_json::from_value(serde_json::json!({
                    "key": name, "dir": "asc"
                }))
                .unwrap()],
                ..ask(&view.id, 10)
            })
            .unwrap();
        assert_eq!(page.total, 3);
        let first = page.rows[0][0].as_str().unwrap();
        assert_eq!(first, "가람", "가나다순이면 '가람' 이 먼저다");
    }

    /// **숫자 열은 숫자로 줄을 세운다.** 글자로 세면 1 · 10 · 2 가 되고,
    /// 값 없음(`-`)은 방향과 무관하게 뒤로 간다. 알아본 종류를 바탕에 제대로
    /// 넘기지 않으면 둘 다 조용히 틀린다.
    #[test]
    fn a_number_column_sorts_as_numbers_with_blanks_last() {
        let csv = "점수\n9\n-\n100\n20\n".as_bytes();
        let mut v = Viewer::open(csv, "점수.csv").unwrap();
        let view = screen(&v, 0);
        assert_eq!(view.columns[0].kind, "number");
        let key = view.columns[0].key.clone();

        let mut sorted = |dir: &str| {
            let page = v
                .query_rows(&Ask {
                    sorts: vec![serde_json::from_value(serde_json::json!({
                        "key": key, "dir": dir
                    }))
                    .unwrap()],
                    ..ask("점수", 10)
                })
                .unwrap();
            page.rows
                .iter()
                .map(|r| r[0].as_str().unwrap_or_default().to_string())
                .collect::<Vec<_>>()
        };
        assert_eq!(sorted("asc"), ["9", "20", "100", "-"]);
        assert_eq!(sorted("desc"), ["100", "20", "9", "-"]);
    }

    #[test]
    fn a_filter_narrows_the_page() {
        let mut v = Viewer::open(TINY_XLSX, "tiny.xlsx").unwrap();
        let view = screen(&v, 0);
        let region = view.columns[1].key.clone();
        let filters: Vec<grid::Filter> = vec![serde_json::from_value(serde_json::json!({
            "key": region, "values": ["서울"]
        }))
        .unwrap()];
        let page = v.query_rows(&Ask { filters: filters.clone(), ..ask(&view.id, 10) }).unwrap();
        assert_eq!(page.total, 2);

        // 값 목록은 **그 열 자신의 필터를 빼고** 센다. 그래야 체크를 풀었다
        // 걸었다 해도 후보가 사라지지 않는다.
        let values = v
            .column_values(&ValuesAsk { filters, ..values_ask(&view.id, &region) })
            .unwrap();
        let got: Vec<&str> = values.values.iter().map(|v| v.value.as_str()).collect();
        assert!(got.contains(&"부산"), "제 필터 때문에 후보가 사라졌다");
    }

    /// 표를 바꾸면 앞의 표가 남지 않는다. **바꾸다 선 자리도 그렇다** —
    /// 앞의 표가 열린 채로 남으면 그 뒤의 물음이 그 표로 답한다.
    #[test]
    fn switching_tables_leaves_nothing_of_the_previous_one() {
        let mut v = Viewer::open(TINY_XLSX, "tiny.xlsx").unwrap();
        assert_eq!(total(&mut v, "사람"), 3);
        assert_eq!(v.rows(), 3);
        // '안내' 는 표가 아니다 — 머리글이 없다.
        assert_eq!(v.query_rows(&ask("안내", 10)).unwrap_err().code, ErrorCode::SheetEmpty);
        assert_eq!(v.rows(), 0, "앞 시트의 표가 남아 있다");
    }

    #[test]
    fn a_sheet_that_is_not_there_stops() {
        let mut v = Viewer::open(TINY_XLSX, "tiny.xlsx").unwrap();
        assert_eq!(
            v.query_rows(&ask("없는시트", 10)).unwrap_err().code,
            ErrorCode::SheetUnreadable
        );
    }

    /// 요청이 가리킨 표를 **열지 않아도 조회가 된다.** 시트를 먼저 열어 두는
    /// 일을 화면이 기억해야 하면, 그 기억이 화면마다 달라진다.
    #[test]
    fn asking_opens_the_sheet_the_request_names() {
        let mut v = Viewer::open(TINY_XLSX, "tiny.xlsx").unwrap();
        assert_eq!(v.query_rows(&ask("사람", 10)).unwrap().total, 3);
    }

    /// **없는 표를 물으면 선다.** 열려 있던 표로 답하면 화면은 바꾼 표를 보고
    /// 있다고 믿는다 — 줄 수도 열 이름도 그럴듯하다.
    #[test]
    fn asking_for_a_sheet_that_is_not_there_stops() {
        let mut v = Viewer::open(TINY_XLSX, "tiny.xlsx").unwrap();
        v.open_sheet(0, None).unwrap();
        let e = v.query_rows(&ask("없는시트", 10)).unwrap_err();
        assert_eq!(e.code, ErrorCode::SheetUnreadable);
        assert_eq!(e.params.get("sheet").map(String::as_str), Some("없는시트"));
    }

    /// 머리글을 못 찾은 시트를 물으면 **빈 쪽이 아니라 오류**다. 빈 쪽은
    /// 화면이 "자료가 없다" 로 읽는다.
    #[test]
    fn asking_for_a_sheet_without_a_header_stops() {
        let csv = "1,2\n3,4\n".as_bytes();
        let mut v = Viewer::open(csv, "숫자.csv").unwrap();
        assert_eq!(v.query_rows(&ask("숫자", 10)).unwrap_err().code, ErrorCode::SheetEmpty);
    }

    /* ── 값 선택 목록 ───────────────────────────────────────────── */

    /// 목록은 **세어서** 온다. 화면이 "몇 건" 을 보여 줄 수 있어야 고를 때
    /// 무엇이 큰 덩이인지 안다.
    #[test]
    fn a_value_list_comes_back_counted_and_sorted() {
        let csv = "지역\n서울\n부산\n서울\n서울\n".as_bytes();
        let mut v = Viewer::open(csv, "지역.csv").unwrap();
        let view = screen(&v, 0);
        let key = view.columns[0].key.clone();
        let got = v.column_values(&values_ask(&view.id, &key)).unwrap();
        let pairs: Vec<(&str, u32)> =
            got.values.iter().map(|x| (x.value.as_str(), x.count)).collect();
        assert_eq!(pairs, [("부산", 1), ("서울", 3)]);
        assert!(!got.truncated);
    }

    /// **값이 없는 칸도 고를 수 있어야 한다.** 바탕이 쓰는 표식 그대로 오고,
    /// 그 글자로 되돌려 주면 걸린다. 화면이 그 글자를 손으로 적으면 두 벌이
    /// 되어 한쪽만 바뀌는 날 0건이 된다.
    #[test]
    fn a_blank_can_be_picked_with_the_token_core_gives() {
        let csv = "점수\n90\n-\n75\n".as_bytes();
        let mut v = Viewer::open(csv, "점수.csv").unwrap();
        let view = screen(&v, 0);
        let key = view.columns[0].key.clone();

        let token = Limits::default().blank_token;
        let got = v.column_values(&values_ask(&view.id, &key)).unwrap();
        assert!(got.values.iter().any(|x| x.value == token), "빈칸이 목록에 없다");

        let picked: Vec<grid::Filter> = vec![serde_json::from_value(serde_json::json!({
            "key": key, "values": [token]
        }))
        .unwrap()];
        let page = v.query_rows(&Ask { filters: picked, ..ask(&view.id, 10) }).unwrap();
        assert_eq!(page.total, 1, "빈칸 표식으로 걸리지 않았다");
    }

    /// **좁히는 글자는 잘린 뒤가 아니라 열 전체를 본다.**
    ///
    /// 값이 상한을 넘으면 바탕이 앞에서 잘라 보낸다. 받은 목록을 화면이 걸러
    /// 보여 주면, 찾는 값이 그 뒤에 있을 때 "없습니다" 가 뜬다 — 사람은 그런
    /// 값이 없는 줄 안다. 그래서 좁히는 일을 바탕이 한다.
    #[test]
    fn the_needle_looks_past_where_the_list_was_cut() {
        let max = Limits::default().max_values as usize;
        // 상한을 넘기도록 값을 만들고, 찾을 값은 **가나다순으로 맨 뒤**에 둔다.
        let mut csv = String::from("이름\n");
        for i in 0..(max + 50) {
            csv.push_str(&format!("가{i:06}\n"));
        }
        csv.push_str("힣찾을값\n");
        let mut v = Viewer::open(csv.as_bytes(), "많은값.csv").unwrap();
        let view = screen(&v, 0);
        let key = view.columns[0].key.clone();

        // 좁히지 않으면 잘린다. 그리고 찾는 값은 그 뒤에 있다.
        let all = v.column_values(&values_ask(&view.id, &key)).unwrap();
        assert!(all.truncated, "상한을 넘기지 못했다 — 시험 자료를 늘리세요");
        assert!(!all.values.iter().any(|x| x.value == "힣찾을값"), "잘린 뒤에 있어야 한다");

        // 좁히면 찾아진다.
        let found = v
            .column_values(&ValuesAsk { needle: "찾을값".into(), ..values_ask(&view.id, &key) })
            .unwrap();
        assert_eq!(found.values.len(), 1);
        assert_eq!(found.values[0].value, "힣찾을값");
        assert!(!found.truncated);
    }

    /// 좁히는 글자는 **고른 값을 지우지 않는다.** 둘은 다른 일이다 — 좁히는
    /// 것은 후보를 보는 창이고, 고른 것은 표에 걸리는 조건이다.
    #[test]
    fn the_needle_does_not_disturb_what_is_picked() {
        let csv = "지역\n서울\n부산\n대구\n".as_bytes();
        let mut v = Viewer::open(csv, "지역.csv").unwrap();
        let view = screen(&v, 0);
        let key = view.columns[0].key.clone();
        let picked: Vec<grid::Filter> = vec![serde_json::from_value(serde_json::json!({
            "key": key, "values": ["서울"]
        }))
        .unwrap()];
        // 제 열의 선택은 후보 목록에서 빠진다(엑셀과 같다). 좁히기는 그 위에 건다.
        let got = v
            .column_values(&ValuesAsk {
                filters: picked.clone(),
                needle: "부".into(),
                ..values_ask(&view.id, &key)
            })
            .unwrap();
        let names: Vec<&str> = got.values.iter().map(|x| x.value.as_str()).collect();
        assert_eq!(names, ["부산"]);
        // 표에 걸린 것은 그대로 '서울' 이다.
        assert_eq!(v.query_rows(&Ask { filters: picked, ..ask(&view.id, 10) }).unwrap().total, 1);
    }

    /// 고른 값으로 좁혀진다. 여럿 고르면 OR 다.
    #[test]
    fn picking_values_narrows_the_page() {
        let csv = "지역\n서울\n부산\n대구\n서울\n".as_bytes();
        let mut v = Viewer::open(csv, "지역.csv").unwrap();
        let view = screen(&v, 0);
        let key = view.columns[0].key.clone();
        let pick = |names: &[&str]| -> Vec<grid::Filter> {
            vec![serde_json::from_value(serde_json::json!({ "key": key, "values": names })).unwrap()]
        };
        let sheet = view.id.clone();
        assert_eq!(v.query_rows(&Ask { filters: pick(&["서울"]), ..ask(&sheet, 10) }).unwrap().total, 2);
        assert_eq!(
            v.query_rows(&Ask { filters: pick(&["서울", "대구"]), ..ask(&sheet, 10) }).unwrap().total,
            3
        );
    }

    /// **화면이 손으로 적으면 안 되는 값들**이 실제 바탕의 것과 같아야 한다.
    /// 여기서 베껴 두면 바탕이 바뀌는 날 조용히 갈린다.
    #[test]
    fn the_limits_come_from_core_not_from_here() {
        let l = Limits::default();
        assert_eq!(l.blank_token, grid::BLANK_TOKEN);
        assert_eq!(l.max_values, grid::MAX_VALUES);
        assert_eq!(l.max_page, grid::MAX_LIMIT);
        assert!(l.max_page > 0 && !l.blank_token.is_empty());
    }

    /* ── csv ────────────────────────────────────────────────────── */

    #[test]
    fn a_csv_opens_as_one_sheet_named_after_the_file() {
        let csv = "이름,점수\n가람,90\n하늘,75\n".as_bytes();
        let mut v = Viewer::open(csv, "/어딘가/성적표.csv").unwrap();
        assert_eq!(names(&v), ["성적표"]);
        let view = screen(&v, 0);
        assert_eq!(total(&mut v, &view.id), 2);
        assert_eq!(view.columns[1].kind, "number");
    }

    /// **이름이 아니라 속으로 가린다.** 확장자는 사람이 고친 것일 수 있고,
    /// 그때 조용히 틀린 쪽으로 읽으면 깨진 표가 그럴듯하게 열린다.
    #[test]
    fn the_kind_of_file_is_told_by_its_bytes_not_its_name() {
        // 이름은 csv 인데 속은 xlsx 다.
        let v = Viewer::open(TINY_XLSX, "거짓말.csv").unwrap();
        assert_eq!(names(&v), ["사람", "안내"]);
    }

    /* ── 조용히 틀리느니 선다 ───────────────────────────────────── */

    /// 머리글을 찾지 못하면 **첫 줄을 열 이름으로 쓰지 않는다.** 화면이 그
    /// 까닭을 말할 수 있게 비워서 돌려준다.
    #[test]
    fn a_sheet_without_a_header_opens_empty_and_says_so() {
        let csv = "1,2\n3,4\n".as_bytes();
        let v = Viewer::open(csv, "숫자.csv").unwrap();
        let view = screen(&v, 0);
        assert!(!view.header_found);
        assert!(view.columns.is_empty(), "머리글을 못 찾았는데 열을 돌려주었다");
    }

    /// 두 줄 머리글은 한 줄로 합쳐 보여 준다. 화면이 한 줄이라 그렇다.
    #[test]
    fn a_two_row_header_becomes_one_label() {
        let csv = "전형개요,,모집\n지역,대학,인원\n서울,가대,30\n부산,나대,20\n".as_bytes();
        let mut v = Viewer::open(csv, "두줄.csv").unwrap();
        let view = screen(&v, 0);
        let labels: Vec<&str> = view.columns.iter().map(|c| c.label.as_str()).collect();
        assert_eq!(labels, ["전형개요 · 지역", "전형개요 · 대학", "모집 · 인원"]);
        assert_eq!(total(&mut v, &view.id), 2, "머리글 두 줄이 자료로 들어갔다");
    }

    /// 머리글 위의 안내 줄은 자료가 아니다. 그것까지 담으면 줄 수가 틀린다.
    #[test]
    fn notes_above_the_header_do_not_become_rows() {
        let csv = "이 표는 보기용입니다\n\n이름,점수\n가람,90\n하늘,75\n".as_bytes();
        let mut v = Viewer::open(csv, "안내.csv").unwrap();
        let view = screen(&v, 0);
        assert_eq!(total(&mut v, &view.id), 2);
        assert_eq!(view.columns.len(), 2);
    }

    /// 본문에 삐져 나온 칸이 있어도 이름 없는 열이 생기지 않는다.
    #[test]
    fn a_stray_cell_does_not_add_a_nameless_column() {
        let csv = "이름,점수\n가람,90,여기 왜 있지\n".as_bytes();
        let v = Viewer::open(csv, "삐죽.csv").unwrap();
        let view = screen(&v, 0);
        assert_eq!(view.columns.len(), 2);
        assert!(view.columns.iter().all(|c| !c.label.is_empty()));
    }
    /* ── 사용자가 지정하는 머리글 ──────────────────────────────── */

    /// 추정은 **거부한다.** 상단 줄 중간에 이름 없는 열이 존재해서다.
    /// 화면은 그 사실을 전달받아야 조작부로 물을 수 있다.
    #[test]
    fn a_header_of_differing_depth_is_not_guessed() {
        let v = Viewer::open(MIXED_CSV, "진학결과.csv").expect("양식을 열지 못했습니다");
        let s = &v.list_screens(&CatalogAsk::default()).screens[0];
        assert!(!s.header_found, "추정이 성공해 버렸습니다");
        assert!(s.header_guessed, "추정이라고 표시되지 않았습니다");
        assert_eq!(s.header, None);
        assert_eq!(s.sheet_rows, 4, "조작부가 지정할 범위를 알 수 없습니다");
    }

    /// **2줄이라고 지정하면 열린다.** 명칭은 묶음과 합성해 유일해진다 —
    /// `등급` 이 두 열에 존재하므로 합성하지 않으면 한쪽을 지시할 수 없다.
    #[test]
    fn the_user_can_say_the_header_is_two_rows() {
        let v = Viewer::open(MIXED_CSV, "진학결과.csv").expect("양식을 열지 못했습니다");
        let s = &v.list_screens(&spec(0, 2)).screens[0];
        assert!(s.header_found);
        assert!(!s.header_guessed, "사용자 지정인데 추정으로 표시됩니다");
        assert_eq!(s.header, Some(HeaderSpec { at: 0, rows: 2 }));
        let labels: Vec<&str> = s.columns.iter().map(|c| c.label.as_str()).collect();
        assert_eq!(
            labels,
            ["학년", "반", "국어 · 선택과목", "국어 · 표준점수", "국어 · 등급", "영어 · 등급"]
        );
        let keys: Vec<&str> = s.columns.iter().map(|c| c.key.as_str()).collect();
        assert_eq!(keys.len(), 6);
        assert!(keys.iter().all(|k| !k.is_empty() && *k != "_"), "열쇠가 비었습니다: {keys:?}");
    }

    /// 지정한 머리글로 **조회까지 도달한다.** 열 정의만 바뀌고 자료가 따라오지
    /// 않으면 표는 비어 보인다.
    #[test]
    fn rows_come_back_under_the_header_the_user_chose() {
        let mut v = Viewer::open(MIXED_CSV, "진학결과.csv").expect("양식을 열지 못했습니다");
        let header = Some(HeaderSpec { at: 0, rows: 2 });
        let ask = Ask {
            screen_id: "진학결과".to_string(),
            header,
            limit: 10,
            ..Default::default()
        };
        let page = v.query_rows(&ask).expect("조회하지 못했습니다");
        assert_eq!(page.total, 2, "머리글 두 줄이 자료로 섞였습니다");
        assert_eq!(page.rows.len(), 2);
    }

    /// **지정을 변경하면 표가 재구축된다.** 시트 명칭만 대조하면 구 표로
    /// 응답하고, 화면은 신규 열 구성을 열람 중이라고 오인한다.
    #[test]
    fn changing_the_header_rebuilds_the_table() {
        let mut v = Viewer::open(MIXED_CSV, "진학결과.csv").expect("양식을 열지 못했습니다");
        let two = Ask {
            screen_id: "진학결과".to_string(),
            header: Some(HeaderSpec { at: 0, rows: 2 }),
            limit: 10,
            ..Default::default()
        };
        assert_eq!(v.query_rows(&two).expect("조회 실패").total, 2);

        let one = Ask { header: Some(HeaderSpec { at: 0, rows: 1 }), ..two.clone() };
        // 1줄이면 둘째 줄이 자료가 되므로 한 줄이 늘어난다.
        assert_eq!(v.query_rows(&one).expect("조회 실패").total, 3);

        // 되돌리면 다시 둘이다. 한 번 구축한 표가 눌러앉지 않는다.
        assert_eq!(v.query_rows(&two).expect("조회 실패").total, 2);
    }

    /// 범위를 벗어난 지정은 **중단한다.** 보정해서 다른 표를 표시하지 않는다.
    #[test]
    fn a_header_past_the_end_of_the_sheet_stops() {
        let mut v = Viewer::open(MIXED_CSV, "진학결과.csv").expect("양식을 열지 못했습니다");
        let ask = Ask {
            screen_id: "진학결과".to_string(),
            header: Some(HeaderSpec { at: 0, rows: 99 }),
            limit: 10,
            ..Default::default()
        };
        assert!(v.query_rows(&ask).is_err(), "범위를 벗어난 지정이 통과했습니다");
    }

    /// 명칭이 없는 열에는 **엑셀의 열 기호**를 부여한다. 공백으로 두면 머리글이
    /// 비어 사람이 그 열을 지시할 수 없다.
    #[test]
    fn a_nameless_column_gets_its_spreadsheet_letter() {
        let v = Viewer::open(MIXED_CSV, "진학결과.csv").expect("양식을 열지 못했습니다");
        let s = &v.list_screens(&spec(0, 1)).screens[0];
        let labels: Vec<&str> = s.columns.iter().map(|c| c.label.as_str()).collect();
        assert_eq!(labels, ["학년", "반", "국어", "D열", "E열", "영어"]);
    }

    #[test]
    fn column_refs_follow_the_spreadsheet() {
        assert_eq!(column_ref(0), "A");
        assert_eq!(column_ref(25), "Z");
        assert_eq!(column_ref(26), "AA");
        assert_eq!(column_ref(27), "AB");
        assert_eq!(column_ref(51), "AZ");
        assert_eq!(column_ref(52), "BA");
    }

}
