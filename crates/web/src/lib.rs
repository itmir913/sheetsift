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

pub mod bindings;

/// 값이 없는 칸에 적는 글자.
///
/// **고를 수 있어야 한다.** 양식마다 다르고(`-` · `–` · `N/A`), 무엇을 "없음"
/// 으로 볼지가 갈리면 값 선택 목록과 필터가 같은 파일 안에서 어긋난다. 지금은
/// 화면이 고르게 하지 않고 이 값을 쓰되, **한 곳에만 둔다.**
pub const BLANK: &str = "-";

/* ── 화면에 넘기는 모양 ─────────────────────────────────────────── */

/// 열 하나를 화면이 아는 모양으로.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnView {
    pub key: String,
    pub label: String,
    /// `"number"` 또는 `"text"`.
    pub kind: String,
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

/// 양식을 연 직후 화면이 알아야 하는 것.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Opened {
    /// 시트 이름을 **탭 차례대로**.
    pub sheets: Vec<String>,
}

/// 시트 하나를 연 결과.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SheetView {
    pub sheet: String,
    pub rows: u32,
    pub columns: Vec<ColumnView>,
    /// 머리글을 찾지 못했으면 열이 없다. 화면이 그 까닭을 말해야 한다.
    pub header_found: bool,
}

/// 화면이 보내는 조회 요청. [`grid::Query`] 와 같은 모양이되 글자로 온다.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Ask {
    #[serde(default)]
    pub sorts: Vec<grid::Sort>,
    #[serde(default)]
    pub filters: Vec<grid::Filter>,
    #[serde(default)]
    pub offset: u32,
    #[serde(default)]
    pub limit: u32,
}

/* ── 들고 있는 것 ───────────────────────────────────────────────── */

/// 열어 둔 표 하나. 바꿔 놓은 연결과 그 열 정의를 함께 든다.
struct Open {
    conn: Connection,
    name: String,
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

/// 양식 하나를 열어 둔 상태.
///
/// 시트를 바꿀 때마다 바꾸는 일을 다시 한다. 미리 다 바꿔 두지 않는 까닭은
/// 큰 양식에서 **보지도 않을 시트 때문에 첫 화면이 늦어지기** 때문이다.
pub struct Viewer {
    sheets: Vec<(String, Sheet)>,
    open: Option<Open>,
}

impl Viewer {
    /// 양식 바이트를 받는다. xlsx 면 시트가 여럿, csv 면 하나다.
    ///
    /// 무엇인지는 **이름이 아니라 속으로** 가린다. 받아 든 파일의 확장자는
    /// 사람이 고친 것일 수 있고, 그때 조용히 틀린 쪽으로 읽으면 깨진 표가
    /// 그럴듯하게 열린다.
    pub fn open(bytes: &[u8], name: &str) -> Result<Self> {
        let sheets = if is_zip(bytes) {
            sheet::from_xlsx(bytes)?
        } else {
            vec![(sheet_name_of(name), sheet::from_csv(bytes)?)]
        };
        Ok(Self { sheets, open: None })
    }

    /// 시트 이름을 탭 차례대로.
    pub fn opened(&self) -> Opened {
        Opened { sheets: self.sheets.iter().map(|(n, _)| n.clone()).collect() }
    }

    /// 시트 하나를 SQLite 표로 바꾸고 열어 둔다.
    ///
    /// 열의 종류는 [`infer`] 가 알아본다. **알아보지 못한 열은 글자로 담되,
    /// 그렇다고 화면에 알린다**([`ColumnView::guessed`]) — 짐작한 것을 아는
    /// 것처럼 두면 틀린 차례가 참으로 보인다.
    pub fn open_sheet(&mut self, at: usize) -> Result<SheetView> {
        let (name, data) = self
            .sheets
            .get(at)
            .ok_or_else(|| Error::new(ErrorCode::SheetUnreadable).with("sheet", at))?;
        let name = name.clone();
        let layout = infer::layout(data, BLANK);

        let header_found = layout.header != infer::Header::Unknown;
        if !header_found {
            self.open = None;
            return Ok(SheetView { sheet: name, rows: 0, columns: Vec::new(), header_found });
        }

        // 머리글 아래만 바꾼다. 머리글 위의 안내 줄은 자료가 아니다.
        let from = match layout.header {
            infer::Header::One(at) => at,
            infer::Header::Two { sub, .. } => sub,
            infer::Header::Unknown => unreachable!("바로 위에서 걸렀다"),
        };
        let body = Sheet { rows: trimmed(data, from, layout.columns.len()) };

        let conn = Connection::open_in_memory()
            .map_err(|e| Error::new(ErrorCode::SheetTableFailed).detail(e))?;
        let plan = TablePlan { table: "t".into(), map: Vec::new(), indexes: Vec::new() };
        let made = sheet::build(&conn, &[(plan, body)], BLANK)?;
        let built = &made["t"];

        let mut columns = Vec::with_capacity(built.columns.len());
        let mut view = Vec::with_capacity(built.columns.len());
        for (i, (_, db)) in built.columns.iter().enumerate() {
            let guess = &layout.columns[i];
            let number = guess.kind == infer::Kind::Number;
            let label = match &guess.group {
                // 두 줄 머리글은 `묶음 · 이름` 으로 한 줄에 담는다. 화면이
                // 한 줄이라 그렇다.
                Some(group) if !group.is_empty() && group != &guess.label => {
                    format!("{group} · {}", guess.label)
                }
                _ => guess.label.clone(),
            };
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
                guessed: guess.kind == infer::Kind::Unknown,
            });
        }
        grid::assign_aliases(columns.iter_mut());

        let rows = built.rows as u32;
        self.open = Some(Open { conn, name: name.clone(), columns, rows });
        Ok(SheetView { sheet: name, rows, columns: view, header_found })
    }

    fn table(&self) -> Result<&Open> {
        self.open.as_ref().ok_or_else(|| {
            Error::new(ErrorCode::SheetUnreadable).detail("no sheet is open yet")
        })
    }

    /// 한 쪽을 읽는다.
    pub fn page(&self, ask: &Ask) -> Result<grid::Page> {
        let open = self.table()?;
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
    pub fn values(
        &self,
        key: &str,
        filters: &[grid::Filter],
        needle: &str,
    ) -> Result<grid::ColumnValues> {
        let open = self.table()?;
        grid::column_values(&open.conn, open, BLANK, key, filters, needle)
    }

    /// 열어 둔 표의 줄 수. 필터를 걸기 전의 값이다.
    pub fn rows(&self) -> u32 {
        self.open.as_ref().map(|o| o.rows).unwrap_or(0)
    }
}

/* ── 자잘한 것 ──────────────────────────────────────────────────── */

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

    fn ask(limit: u32) -> Ask {
        Ask { limit, ..Ask::default() }
    }

    #[test]
    fn a_workbook_lists_its_sheets_in_tab_order() {
        let v = Viewer::open(TINY_XLSX, "tiny.xlsx").unwrap();
        assert_eq!(v.opened().sheets, ["사람", "안내"]);
    }

    #[test]
    fn opening_a_sheet_gives_its_columns_and_rows() {
        let mut v = Viewer::open(TINY_XLSX, "tiny.xlsx").unwrap();
        let view = v.open_sheet(0).unwrap();
        assert!(view.header_found);
        assert_eq!(view.sheet, "사람");
        assert_eq!(view.rows, 3);
        let labels: Vec<&str> = view.columns.iter().map(|c| c.label.as_str()).collect();
        assert_eq!(labels, ["이름", "지역", "점수"]);
    }

    /// **알아보지 못한 열은 그렇다고 말한다.** 점수 열에 "-" 가 섞여 있지만
    /// 그것은 "없음" 표기라 숫자로 갈린다. 짐작한 자리는 화면이 알아야 한다.
    #[test]
    fn a_column_says_whether_its_kind_was_guessed() {
        let mut v = Viewer::open(TINY_XLSX, "tiny.xlsx").unwrap();
        let view = v.open_sheet(0).unwrap();
        let by = |k: &str| view.columns.iter().find(|c| c.label == k).unwrap().clone();
        assert_eq!(by("이름").kind, "text");
        assert_eq!(by("점수").kind, "number");
        assert!(!by("점수").guessed, "'-' 는 없음 표기라 섞인 것이 아니다");
    }

    #[test]
    fn a_page_comes_back_sorted() {
        let mut v = Viewer::open(TINY_XLSX, "tiny.xlsx").unwrap();
        let view = v.open_sheet(0).unwrap();
        let name = view.columns[0].key.clone();
        let page = v
            .page(&Ask {
                sorts: vec![serde_json::from_value(serde_json::json!({
                    "key": name, "dir": "asc"
                }))
                .unwrap()],
                ..ask(10)
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
        let view = v.open_sheet(0).unwrap();
        assert_eq!(view.columns[0].kind, "number");
        let key = view.columns[0].key.clone();

        let sorted = |dir: &str| {
            let page = v
                .page(&Ask {
                    sorts: vec![serde_json::from_value(serde_json::json!({
                        "key": key, "dir": dir
                    }))
                    .unwrap()],
                    ..ask(10)
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
        let view = v.open_sheet(0).unwrap();
        let region = view.columns[1].key.clone();
        let filters: Vec<grid::Filter> = vec![serde_json::from_value(serde_json::json!({
            "key": region, "values": ["서울"]
        }))
        .unwrap()];
        let page = v.page(&Ask { filters: filters.clone(), ..ask(10) }).unwrap();
        assert_eq!(page.total, 2);

        // 값 목록은 **그 열 자신의 필터를 빼고** 센다. 그래야 체크를 풀었다
        // 걸었다 해도 후보가 사라지지 않는다.
        let values = v.values(&region, &filters, "").unwrap();
        let got: Vec<&str> = values.values.iter().map(|v| v.value.as_str()).collect();
        assert!(got.contains(&"부산"), "제 필터 때문에 후보가 사라졌다");
    }

    /// 시트를 바꾸면 그 시트가 열린다. 앞의 표가 남아 있으면 화면과 자료가
    /// 어긋난다.
    #[test]
    fn switching_sheets_replaces_what_is_open() {
        let mut v = Viewer::open(TINY_XLSX, "tiny.xlsx").unwrap();
        v.open_sheet(0).unwrap();
        assert_eq!(v.rows(), 3);
        let guide = v.open_sheet(1).unwrap();
        assert_eq!(guide.sheet, "안내");
        assert_ne!(v.rows(), 3, "앞 시트의 표가 남아 있다");
    }

    #[test]
    fn a_sheet_that_is_not_there_stops() {
        let mut v = Viewer::open(TINY_XLSX, "tiny.xlsx").unwrap();
        assert_eq!(v.open_sheet(9).unwrap_err().code, ErrorCode::SheetUnreadable);
    }

    /// 시트를 열기 전에 조회하면 선다. 빈 쪽을 돌려주면 화면이 "자료가 없다"
    /// 로 읽는다.
    #[test]
    fn asking_before_opening_a_sheet_stops() {
        let v = Viewer::open(TINY_XLSX, "tiny.xlsx").unwrap();
        assert!(v.page(&ask(10)).is_err());
    }

    /* ── 값 선택 목록 ───────────────────────────────────────────── */

    /// 목록은 **세어서** 온다. 화면이 "몇 건" 을 보여 줄 수 있어야 고를 때
    /// 무엇이 큰 덩이인지 안다.
    #[test]
    fn a_value_list_comes_back_counted_and_sorted() {
        let csv = "지역\n서울\n부산\n서울\n서울\n".as_bytes();
        let mut v = Viewer::open(csv, "지역.csv").unwrap();
        let view = v.open_sheet(0).unwrap();
        let key = view.columns[0].key.clone();
        let got = v.values(&key, &[], "").unwrap();
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
        let view = v.open_sheet(0).unwrap();
        let key = view.columns[0].key.clone();

        let token = Limits::default().blank_token;
        let got = v.values(&key, &[], "").unwrap();
        assert!(got.values.iter().any(|x| x.value == token), "빈칸이 목록에 없다");

        let picked: Vec<grid::Filter> = vec![serde_json::from_value(serde_json::json!({
            "key": key, "values": [token]
        }))
        .unwrap()];
        let page = v.page(&Ask { filters: picked, ..ask(10) }).unwrap();
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
        let view = v.open_sheet(0).unwrap();
        let key = view.columns[0].key.clone();

        // 좁히지 않으면 잘린다. 그리고 찾는 값은 그 뒤에 있다.
        let all = v.values(&key, &[], "").unwrap();
        assert!(all.truncated, "상한을 넘기지 못했다 — 시험 자료를 늘리세요");
        assert!(!all.values.iter().any(|x| x.value == "힣찾을값"), "잘린 뒤에 있어야 한다");

        // 좁히면 찾아진다.
        let found = v.values(&key, &[], "찾을값").unwrap();
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
        let view = v.open_sheet(0).unwrap();
        let key = view.columns[0].key.clone();
        let picked: Vec<grid::Filter> = vec![serde_json::from_value(serde_json::json!({
            "key": key, "values": ["서울"]
        }))
        .unwrap()];
        // 제 열의 선택은 후보 목록에서 빠진다(엑셀과 같다). 좁히기는 그 위에 건다.
        let got = v.values(&key, &picked, "부").unwrap();
        let names: Vec<&str> = got.values.iter().map(|x| x.value.as_str()).collect();
        assert_eq!(names, ["부산"]);
        // 표에 걸린 것은 그대로 '서울' 이다.
        assert_eq!(v.page(&Ask { filters: picked, ..ask(10) }).unwrap().total, 1);
    }

    /// 고른 값으로 좁혀진다. 여럿 고르면 OR 다.
    #[test]
    fn picking_values_narrows_the_page() {
        let csv = "지역\n서울\n부산\n대구\n서울\n".as_bytes();
        let mut v = Viewer::open(csv, "지역.csv").unwrap();
        let view = v.open_sheet(0).unwrap();
        let key = view.columns[0].key.clone();
        let pick = |names: &[&str]| -> Vec<grid::Filter> {
            vec![serde_json::from_value(serde_json::json!({ "key": key, "values": names })).unwrap()]
        };
        assert_eq!(v.page(&Ask { filters: pick(&["서울"]), ..ask(10) }).unwrap().total, 2);
        assert_eq!(v.page(&Ask { filters: pick(&["서울", "대구"]), ..ask(10) }).unwrap().total, 3);
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
        assert_eq!(v.opened().sheets, ["성적표"]);
        let view = v.open_sheet(0).unwrap();
        assert_eq!(view.rows, 2);
        assert_eq!(view.columns[1].kind, "number");
    }

    /// **이름이 아니라 속으로 가린다.** 확장자는 사람이 고친 것일 수 있고,
    /// 그때 조용히 틀린 쪽으로 읽으면 깨진 표가 그럴듯하게 열린다.
    #[test]
    fn the_kind_of_file_is_told_by_its_bytes_not_its_name() {
        // 이름은 csv 인데 속은 xlsx 다.
        let v = Viewer::open(TINY_XLSX, "거짓말.csv").unwrap();
        assert_eq!(v.opened().sheets, ["사람", "안내"]);
    }

    /* ── 조용히 틀리느니 선다 ───────────────────────────────────── */

    /// 머리글을 찾지 못하면 **첫 줄을 열 이름으로 쓰지 않는다.** 화면이 그
    /// 까닭을 말할 수 있게 비워서 돌려준다.
    #[test]
    fn a_sheet_without_a_header_opens_empty_and_says_so() {
        let csv = "1,2\n3,4\n".as_bytes();
        let mut v = Viewer::open(csv, "숫자.csv").unwrap();
        let view = v.open_sheet(0).unwrap();
        assert!(!view.header_found);
        assert!(view.columns.is_empty(), "머리글을 못 찾았는데 열을 돌려주었다");
    }

    /// 두 줄 머리글은 한 줄로 합쳐 보여 준다. 화면이 한 줄이라 그렇다.
    #[test]
    fn a_two_row_header_becomes_one_label() {
        let csv = "전형개요,,모집\n지역,대학,인원\n서울,가대,30\n부산,나대,20\n".as_bytes();
        let mut v = Viewer::open(csv, "두줄.csv").unwrap();
        let view = v.open_sheet(0).unwrap();
        let labels: Vec<&str> = view.columns.iter().map(|c| c.label.as_str()).collect();
        assert_eq!(labels, ["전형개요 · 지역", "전형개요 · 대학", "모집 · 인원"]);
        assert_eq!(view.rows, 2, "머리글 두 줄이 자료로 들어갔다");
    }

    /// 머리글 위의 안내 줄은 자료가 아니다. 그것까지 담으면 줄 수가 틀린다.
    #[test]
    fn notes_above_the_header_do_not_become_rows() {
        let csv = "이 표는 보기용입니다\n\n이름,점수\n가람,90\n하늘,75\n".as_bytes();
        let mut v = Viewer::open(csv, "안내.csv").unwrap();
        let view = v.open_sheet(0).unwrap();
        assert_eq!(view.rows, 2);
        assert_eq!(view.columns.len(), 2);
    }

    /// 본문에 삐져 나온 칸이 있어도 이름 없는 열이 생기지 않는다.
    #[test]
    fn a_stray_cell_does_not_add_a_nameless_column() {
        let csv = "이름,점수\n가람,90,여기 왜 있지\n".as_bytes();
        let mut v = Viewer::open(csv, "삐죽.csv").unwrap();
        let view = v.open_sheet(0).unwrap();
        assert_eq!(view.columns.len(), 2);
        assert!(view.columns.iter().all(|c| !c.label.is_empty()));
    }
}
