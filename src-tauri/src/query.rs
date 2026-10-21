//! 조회 — 화면 id 로 표를 찾아 `sheetsift-core` 의 조회 엔진에 넘긴다.
//!
//! 정렬·필터·값 목록을 SQL 로 짜는 일은 [`sheetsift_core::grid`] 가 한다. 여기서
//! 더하는 것은 **화면 id 를 화면 정의로 바꾸는 것**과, 값이 없는 칸의 글자를
//! 자료(`manifest.json` 의 `blank`)에서 가져오는 것뿐이다.

use rusqlite::Connection;
use serde::Deserialize;
use sheetsift_core::grid;

use crate::error::{AppError, AppResult, ErrorCode};
use crate::screens::{ScreenDef, Screens};

pub use grid::{like_escape, ColumnValue, Filter, Sort, SortDir, BLANK_TOKEN, MAX_VALUES};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryRequest {
    pub screen_id: String,
    #[serde(default)]
    pub sorts: Vec<Sort>,
    #[serde(default)]
    pub filters: Vec<Filter>,
    pub offset: u32,
    pub limit: u32,
}

pub type QueryResponse = grid::Page;

/// 값 선택 목록 요청. 지금 보고 있는 열을 **제외한** 나머지 필터를 반영한다
/// (엑셀 필터 드롭다운과 같은 동작).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnValuesRequest {
    pub screen_id: String,
    pub column_key: String,
    #[serde(default)]
    pub filters: Vec<Filter>,
}

pub type ColumnValuesResponse = grid::ColumnValues;

fn screen<'a>(screens: &'a Screens, id: &str) -> AppResult<&'a ScreenDef> {
    screens
        .find(id)
        .ok_or_else(|| AppError::new(ErrorCode::ScreenNotFound).with("screen", id))
}

pub fn run(conn: &Connection, screens: &Screens, req: &QueryRequest) -> AppResult<QueryResponse> {
    let q = grid::Query {
        sorts: req.sorts.clone(),
        filters: req.filters.clone(),
        offset: req.offset,
        limit: req.limit,
    };
    Ok(grid::run(conn, screen(screens, &req.screen_id)?, &screens.blank, &q)?)
}

/// 한 열의 고유값 목록과 각 값의 행 수. 그 열 자신의 필터는 빼고 센다.
pub fn column_values(
    conn: &Connection,
    screens: &Screens,
    req: &ColumnValuesRequest,
) -> AppResult<ColumnValuesResponse> {
    let table = screen(screens, &req.screen_id)?;
    Ok(grid::column_values(conn, table, &screens.blank, &req.column_key, &req.filters)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    /// 테스트도 화면 정의를 **DB 에서** 읽는다. 코드에 화면이 없다는 뜻이다.
    fn open() -> (Connection, Screens) {
        let conn = crate::db::open_embedded().unwrap();
        let screens = crate::screens::load(&conn).unwrap();
        (conn, screens)
    }

    /// 성능 시험에 쓸 화면을 **데이터에서 고른다.**
    ///
    /// 예전에는 표 이름을 그대로 적어 두었는데, 그러면 양식에서 시트 하나를
    /// 갈아 끼울 때 시험이 먼저 깨진다. 여기서 보고 싶은 것은 특정 표가 아니라
    /// **가장 큰 표에서도 조회가 빠른가** 이므로, 행이 제일 많은 화면을 고른다.
    fn biggest(conn: &Connection, screens: &Screens) -> String {
        let mut best = (0i64, String::new());
        for sc in &screens.screens {
            if sc.base.is_empty() {
                continue;
            }
            let n: i64 = conn
                .query_row(&format!("SELECT COUNT(*) FROM \"{}\"", sc.base), [], |r| r.get(0))
                .unwrap_or(0);
            if n > best.0 {
                best = (n, sc.id.clone());
            }
        }
        assert!(!best.1.is_empty(), "조회할 화면이 하나도 없다");
        best.1
    }

    /// 그 화면에서 실제로 쓸 수 있는 열 key 를 구실별로 고른다.
    /// (글자 열 / 숫자 열 / 값 선택이 되는 열)
    fn a_text_key(screens: &Screens, id: &str) -> String {
        let sc = screens.find(id).unwrap();
        sc.columns
            .iter()
            .find(|c| c.sortable && c.filterable)
            .expect("정렬·필터가 되는 열이 없다")
            .key
            .clone()
    }

    /// 그 열에 실제로 들어 있는 값 몇 개. 값 선택 필터에 쓴다.
    fn some_values(conn: &Connection, screens: &Screens, id: &str, key: &str, n: usize) -> Vec<String> {
        let r = ColumnValuesRequest {
            screen_id: id.into(),
            column_key: key.into(),
            filters: vec![],
        };
        column_values(conn, screens, &r)
            .unwrap()
            .values
            .into_iter()
            .take(n)
            .map(|v| v.value)
            .collect()
    }

    fn req(screen: &str, sorts: Vec<Sort>, filters: Vec<Filter>) -> QueryRequest {
        QueryRequest { screen_id: screen.into(), sorts, filters, offset: 0, limit: 100 }
    }
    /// 뒤쪽 페이지. 정렬이 걸리면 SQLite 가 offset+limit 만큼을 정렬해야 해서
    /// 페이지가 깊어질수록 느려진다. 그 비용을 눈에 보이게 둔다.
    fn at(mut r: QueryRequest, offset: u32) -> QueryRequest {
        r.offset = offset;
        r.limit = 500;
        r
    }
    fn s(key: &str, dir: SortDir) -> Sort {
        Sort { key: key.into(), dir }
    }
    fn contains(key: &str, text: &str) -> Filter {
        Filter { key: key.into(), contains: text.into(), ..Filter::default() }
    }
    fn picked(key: &str, vals: &[&str]) -> Filter {
        Filter {
            key: key.into(),
            values: Some(vals.iter().map(|v| v.to_string()).collect()),
            ..Filter::default()
        }
    }

    #[test]
    fn grid_queries_are_fast_enough() {
        let (conn, screens) = open();
        let id = biggest(&conn, &screens);
        let sc = screens.find(&id).unwrap();
        // 정렬·필터에 쓸 열을 화면 정의에서 고른다. 이름을 적어 두지 않는다.
        let text: Vec<String> = sc
            .columns
            .iter()
            .filter(|c| c.sortable && c.kind == crate::screens::ColumnKind::Text)
            .map(|c| c.key.clone())
            .collect();
        let num: Vec<String> = sc
            .columns
            .iter()
            .filter(|c| c.sortable && c.kind == crate::screens::ColumnKind::Number)
            .map(|c| c.key.clone())
            .collect();
        assert!(text.len() >= 2 && !num.is_empty(), "시험할 만한 열이 모자란다");

        let pick_col = &text[0];
        let vals = some_values(&conn, &screens, &id, pick_col, 2);
        let two: Vec<&str> = vals.iter().map(String::as_str).collect();
        let total = run(&conn, &screens, &req(&id, vec![], vec![])).unwrap().total;
        let last = total.saturating_sub(500);

        let cases: Vec<(&str, QueryRequest)> = vec![
            ("정렬 없음", req(&id, vec![], vec![])),
            ("1순위 정렬(텍스트)", req(&id, vec![s(&text[0], SortDir::Asc)], vec![])),
            (
                "3순위 정렬",
                req(
                    &id,
                    vec![
                        s(&text[0], SortDir::Asc),
                        s(&text[1], SortDir::Asc),
                        s(&num[0], SortDir::Desc),
                    ],
                    vec![],
                ),
            ),
            (
                "값 선택 필터 + 정렬",
                req(&id, vec![s(&num[0], SortDir::Desc)], vec![picked(pick_col, &two)]),
            ),
            (
                "값 선택 + 열 검색 동시",
                req(
                    &id,
                    vec![],
                    vec![picked(pick_col, &two[..1]), contains(&text[1], "학")],
                ),
            ),
            // 뒤쪽 페이지로 넘어갈 때의 비용.
            ("정렬 없음 · 30쪽", at(req(&id, vec![], vec![]), 15_000)),
            ("정렬 없음 · 마지막 쪽", at(req(&id, vec![], vec![]), last)),
            (
                "1순위 정렬 · 30쪽",
                at(req(&id, vec![s(&text[0], SortDir::Asc)], vec![]), 15_000),
            ),
            (
                "1순위 정렬 · 마지막 쪽",
                at(req(&id, vec![s(&text[0], SortDir::Asc)], vec![]), last),
            ),
        ];
        eprintln!("  가장 큰 화면: {id} ({total} 행)");
        for (name, r) in cases {
            let _ = run(&conn, &screens, &r).unwrap();
            let t = Instant::now();
            let res = run(&conn, &screens, &r).unwrap();
            eprintln!(
                "  {name:<24} {:>7.1} ms   전체 {:>7} 행",
                t.elapsed().as_secs_f64() * 1000.0,
                res.total
            );
        }
    }

    #[test]
    fn column_value_lists_are_fast_enough() {
        let (conn, screens) = open();
        let id = biggest(&conn, &screens);
        let sc = screens.find(&id).unwrap();
        let keys: Vec<String> =
            sc.columns.iter().filter(|c| c.filterable).take(4).map(|c| c.key.clone()).collect();
        assert!(!keys.is_empty(), "값 선택이 되는 열이 없다");

        let first = &keys[0];
        let vals = some_values(&conn, &screens, &id, first, 1);
        let one: Vec<&str> = vals.iter().map(String::as_str).collect();

        let mut cases: Vec<(String, Vec<Filter>)> =
            keys.iter().map(|k| (k.clone(), vec![])).collect();
        cases.push((keys[keys.len() - 1].clone(), vec![picked(first, &one)]));

        for (key, filters) in cases {
            let r = ColumnValuesRequest {
                screen_id: id.clone(),
                column_key: key.clone(),
                filters,
            };
            let _ = column_values(&conn, &screens, &r).unwrap();
            let t = Instant::now();
            let res = column_values(&conn, &screens, &r).unwrap();
            eprintln!(
                "  {id}.{key:<10} {:>7.1} ms   고유값 {:>5}개{}",
                t.elapsed().as_secs_f64() * 1000.0,
                res.values.len(),
                if res.truncated { " (잘림)" } else { "" }
            );
            assert!(!res.values.is_empty());
        }
    }

    #[test]
    fn picked_values_narrow_the_result() {
        let (conn, screens) = open();
        let id = biggest(&conn, &screens);
        let key = a_text_key(&screens, &id);
        let vals = some_values(&conn, &screens, &id, &key, 1);
        let one: Vec<&str> = vals.iter().map(String::as_str).collect();

        let all = run(&conn, &screens, &req(&id, vec![], vec![])).unwrap().total;
        let some = run(&conn, &screens, &req(&id, vec![], vec![picked(&key, &one)])).unwrap().total;
        assert!(some > 0 && some < all, "값 선택이 결과를 좁히지 못했다 ({some} / {all})");
    }

    #[test]
    fn unchecking_everything_yields_no_rows() {
        let (conn, screens) = open();
        let id = biggest(&conn, &screens);
        let key = a_text_key(&screens, &id);
        let res = run(&conn, &screens, &req(&id, vec![], vec![picked(&key, &[])])).unwrap();
        assert_eq!(res.total, 0);
    }

    #[test]
    fn value_list_ignores_its_own_column_filter() {
        let (conn, screens) = open();
        let id = biggest(&conn, &screens);
        let key = a_text_key(&screens, &id);
        let vals = some_values(&conn, &screens, &id, &key, 1);
        let one: Vec<&str> = vals.iter().map(String::as_str).collect();

        // 한 값만 체크한 상태여도, 그 열의 후보 목록에는 전체 값이 남아야 한다.
        let r = ColumnValuesRequest {
            screen_id: id.clone(),
            column_key: key.clone(),
            filters: vec![picked(&key, &one)],
        };
        let res = column_values(&conn, &screens, &r).unwrap();
        assert!(res.values.len() > 1, "자기 열 필터를 제외하지 않았다");
    }

    #[test]
    fn unknown_sort_key_is_rejected() {
        let (conn, screens) = open();
        let id = biggest(&conn, &screens);
        let e = run(&conn, &screens, &req(&id, vec![s("no_such", SortDir::Asc)], vec![]))
            .unwrap_err();
        assert_eq!(e.code, ErrorCode::Core(sheetsift_core::ErrorCode::InvalidSortKey));
    }

    #[test]
    fn filter_wildcards_are_escaped() {
        let (conn, screens) = open();
        let id = biggest(&conn, &screens);
        let key = a_text_key(&screens, &id);
        let res = run(&conn, &screens, &req(&id, vec![], vec![contains(&key, "%")])).unwrap();
        assert_eq!(res.total, 0, "LIKE 와일드카드가 이스케이프되지 않았다");
    }
}
