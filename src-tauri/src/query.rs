//! 조회 쿼리 조립.
//!
//! SQL 조각(열 참조·기준 테이블 이름)은 `screens.rs` 의 상수에서만 온다.
//! 사용자가 넣은 값(검색어·선택 값)은 **반드시 바인딩 파라미터**로만 들어가고,
//! 정렬·필터 키는 화면 정의에 있는 컬럼인지 검증한 뒤에만 SQL 에 쓴다.
//!
//! ## 두 종류의 필터
//! - `contains` : 열 검색. 부분일치(LIKE).
//! - `values`   : 엑셀식 값 선택. 그 열의 고유값 중 체크한 것만 남긴다.
//!   `None` 이면 전체 허용, `Some(빈 목록)` 이면 아무것도 통과하지 않는다.

use rusqlite::{types::ValueRef, Connection};
use serde::{Deserialize, Serialize};
use serde_json::Value as Json;

use crate::error::{AppError, AppResult, ErrorCode};
use crate::screens::{ColumnKind, ColumnDef, ScreenDef, Source};

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SortDir {
    Asc,
    Desc,
}

/// 정렬 지시. 배열의 순서가 그대로 우선순위(1순위, 2순위, …)다.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Sort {
    pub key: String,
    pub dir: SortDir,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Filter {
    pub key: String,
    /// 열 검색(부분일치). 빈 문자열이면 적용하지 않는다.
    #[serde(default)]
    pub contains: String,
    /// 값 선택. None = 전체 허용.
    #[serde(default)]
    pub values: Option<Vec<String>>,
}

impl Filter {
    fn is_empty(&self) -> bool {
        self.contains.trim().is_empty() && self.values.is_none()
    }
}

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

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryResponse {
    /// 필터를 적용한 전체 행 수.
    pub total: u32,
    pub offset: u32,
    /// 열 순서대로 담은 값 배열. 객체보다 가벼워 IPC 비용이 낮다.
    pub rows: Vec<Vec<Json>>,
    /// 서버에서 걸린 시간(ms).
    pub elapsed_ms: f64,
}

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

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnValue {
    pub value: String,
    pub count: u32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnValuesResponse {
    pub values: Vec<ColumnValue>,
    /// 상한을 넘어 잘렸는지. true 면 화면에서 "검색으로 좁히세요" 를 안내한다.
    pub truncated: bool,
    pub elapsed_ms: f64,
}

/// 한 번에 가져갈 수 있는 최대 행 수. IPC 직렬화 폭주를 막는다.
const MAX_LIMIT: u32 = 500;
/// 값 선택 목록의 상한. 넘으면 잘라서 보내고 검색으로 좁히게 한다.
const MAX_VALUES: u32 = 1000;
/// 화면에서 비어 있는 값을 나타내는 표식. 실제 데이터의 빈칸/NULL 에 대응한다.
const BLANK_TOKEN: &str = "(비어 있음)";

fn like_escape(s: &str) -> String {
    s.replace('\\', r"\\").replace('%', r"\%").replace('_', r"\_")
}

/// 필터 하나를 WHERE 조각으로 바꾼다.
fn filter_clause(col: &ColumnDef, f: &Filter, binds: &mut Vec<String>) -> Option<String> {
    let base = col.source.base_ref();
    let mut parts: Vec<String> = Vec::new();

    // ① 열 검색 (부분일치)
    let needle = f.contains.trim();
    if !needle.is_empty() {
        binds.push(format!("%{}%", like_escape(needle)));
        parts.push(match col.source {
            Source::Plain(_) => format!(r"{base} LIKE ? ESCAPE '\'"),
            // 사전(3천여 행)에서 먼저 id 집합을 구한 뒤 정수 비교한다.
            Source::Dict { .. } => {
                format!(r"{base} IN (SELECT id FROM dict WHERE s LIKE ? ESCAPE '\')")
            }
        });
    }

    // ② 값 선택 (엑셀식 체크박스)
    if let Some(values) = &f.values {
        if values.is_empty() {
            // 전부 해제 → 아무 행도 통과하지 않는다.
            return Some("0 = 1".to_string());
        }
        let has_blank = values.iter().any(|v| v == BLANK_TOKEN);
        let concrete: Vec<&String> = values.iter().filter(|v| *v != BLANK_TOKEN).collect();

        let mut alt: Vec<String> = Vec::new();
        if !concrete.is_empty() {
            let holes = vec!["?"; concrete.len()].join(", ");
            for v in &concrete {
                binds.push((*v).clone());
            }
            alt.push(match col.source {
                Source::Plain(_) => format!("{base} IN ({holes})"),
                Source::Dict { .. } => {
                    format!("{base} IN (SELECT id FROM dict WHERE s IN ({holes}))")
                }
            });
        }
        if has_blank {
            alt.push(match col.source {
                Source::Plain(_) => format!("({base} IS NULL OR {base} = '' OR {base} = '-')"),
                Source::Dict { .. } => format!("{base} IS NULL"),
            });
        }
        parts.push(format!("({})", alt.join(" OR ")));
    }

    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" AND "))
    }
}

/// WHERE 절과 바인딩 값을 만든다. 기준 테이블 기준으로만 만든다(조인 없음).
/// `skip_key` 가 주어지면 그 열의 필터는 제외한다(값 선택 목록을 만들 때 쓴다).
fn build_where(
    screen: &ScreenDef,
    filters: &[Filter],
    skip_key: Option<&str>,
) -> AppResult<(String, Vec<String>)> {
    let mut clauses = Vec::new();
    let mut binds = Vec::new();

    for f in filters {
        if f.is_empty() || skip_key == Some(f.key.as_str()) {
            continue;
        }
        let col = screen.column(&f.key).ok_or_else(|| {
            AppError::new(ErrorCode::InvalidSortKey)
                .with("key", &f.key)
                .with("screen", screen.id)
                .detail("unknown filter column")
        })?;
        if !col.filterable {
            continue;
        }
        if let Some(c) = filter_clause(col, f, &mut binds) {
            clauses.push(c);
        }
    }

    let sql = if clauses.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", clauses.join(" AND "))
    };
    Ok((sql, binds))
}

/// ORDER BY 절. 배열 순서가 그대로 정렬 우선순위(1순위, 2순위, …)다.
/// 사전 인코딩 열은 id 자체가 가나다순이라 조인 없이 정수 정렬로 끝난다.
fn build_order(screen: &ScreenDef, sorts: &[Sort]) -> AppResult<String> {
    if sorts.is_empty() {
        return Ok(String::new());
    }
    let mut parts = Vec::with_capacity(sorts.len());
    for s in sorts {
        let col = screen.column(&s.key).ok_or_else(|| {
            AppError::new(ErrorCode::InvalidSortKey)
                .with("key", &s.key)
                .with("screen", screen.id)
                .detail("unknown sort column")
        })?;
        if !col.sortable {
            continue;
        }
        let dir = match s.dir {
            SortDir::Asc => "ASC",
            SortDir::Desc => "DESC",
        };
        let base = col.source.base_ref();
        // 값 없음(NULL·빈칸·"-")은 방향과 무관하게 항상 뒤로 보낸다.
        let blank = format!("({base} IS NULL OR {base} = '' OR {base} = '-') ASC");
        let expr = match (col.kind, col.source) {
            // 사전 열은 정수 id 그대로 (가나다순).
            (_, Source::Dict { .. }) => base.clone(),
            // 숫자 열은 TEXT 로 저장돼 있을 수 있어 CAST 후 정렬.
            (ColumnKind::Number, _) => format!("CAST({base} AS REAL)"),
            (ColumnKind::Text, _) => base.clone(),
        };
        parts.push(format!("{blank}, {expr} {dir}"));
    }
    if parts.is_empty() {
        return Ok(String::new());
    }
    Ok(format!(" ORDER BY {}", parts.join(", ")))
}

fn value_to_json(v: ValueRef<'_>) -> Json {
    match v {
        ValueRef::Null => Json::Null,
        ValueRef::Integer(i) => Json::from(i),
        ValueRef::Real(f) => {
            serde_json::Number::from_f64(f).map(Json::Number).unwrap_or(Json::Null)
        }
        ValueRef::Text(t) => Json::String(String::from_utf8_lossy(t).into_owned()),
        ValueRef::Blob(_) => Json::Null,
    }
}

pub fn run(conn: &Connection, req: &QueryRequest) -> AppResult<QueryResponse> {
    let started = std::time::Instant::now();

    let screen = crate::screens::find(&req.screen_id)
        .ok_or_else(|| AppError::new(ErrorCode::ScreenNotFound).with("screen", &req.screen_id))?;

    let (where_sql, binds) = build_where(screen, &req.filters, None)?;
    let order_sql = build_order(screen, &req.sorts)?;
    let limit = req.limit.clamp(1, MAX_LIMIT);

    let params: Vec<&dyn rusqlite::ToSql> =
        binds.iter().map(|s| s as &dyn rusqlite::ToSql).collect();

    // 전체 행 수. 기준 테이블만 보므로 조인이 전혀 없다.
    let count_sql = format!("SELECT COUNT(*) FROM {base} t{where_sql}", base = screen.base);
    let total: i64 = conn.query_row(&count_sql, params.as_slice(), |r| r.get(0))?;

    // 본문: 안쪽에서 정렬·LIMIT 으로 행을 좁힌 뒤, 그 몇십 행에만 dict 를 조인한다.
    let select_list = screen
        .columns
        .iter()
        .map(|c| format!("{} AS \"{}\"", c.source.display_ref(), c.key))
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "SELECT {select_list} FROM (\
           SELECT * FROM {base} t{where_sql}{order_sql} LIMIT {limit} OFFSET {offset}\
         ) t{joins}{order_sql}",
        base = screen.base,
        joins = screen.dict_joins(),
        offset = req.offset,
    );

    let ncol = screen.columns.len();
    let mut stmt = conn.prepare_cached(&sql)?;
    let mut rows_iter = stmt.query(params.as_slice())?;
    let mut rows = Vec::with_capacity(limit as usize);
    while let Some(r) = rows_iter.next()? {
        let mut out = Vec::with_capacity(ncol);
        for i in 0..ncol {
            out.push(value_to_json(r.get_ref(i)?));
        }
        rows.push(out);
    }

    Ok(QueryResponse {
        total: total.max(0) as u32,
        offset: req.offset,
        rows,
        elapsed_ms: started.elapsed().as_secs_f64() * 1000.0,
    })
}

/// 한 열의 고유값 목록과 각 값의 행 수.
///
/// 엑셀 필터 드롭다운처럼, **그 열 자신의 필터는 빼고** 나머지 필터를 반영한다.
/// 그래야 체크를 풀었다 걸었다 해도 후보 목록이 사라지지 않는다.
pub fn column_values(
    conn: &Connection,
    req: &ColumnValuesRequest,
) -> AppResult<ColumnValuesResponse> {
    let started = std::time::Instant::now();

    let screen = crate::screens::find(&req.screen_id)
        .ok_or_else(|| AppError::new(ErrorCode::ScreenNotFound).with("screen", &req.screen_id))?;
    let col = screen.column(&req.column_key).ok_or_else(|| {
        AppError::new(ErrorCode::InvalidSortKey)
            .with("key", &req.column_key)
            .with("screen", screen.id)
            .detail("unknown column for value list")
    })?;

    let (where_sql, binds) = build_where(screen, &req.filters, Some(&req.column_key))?;
    let params: Vec<&dyn rusqlite::ToSql> =
        binds.iter().map(|s| s as &dyn rusqlite::ToSql).collect();

    let base = col.source.base_ref();
    let over = MAX_VALUES + 1; // 하나 더 받아 "잘렸는지" 판단한다.

    // 기준 테이블에서만 GROUP BY 한 뒤, 좁혀진 목록에만 dict 를 조인한다.
    let inner = format!(
        "SELECT {base} AS v, COUNT(*) AS n FROM {table} t{where_sql} \
         GROUP BY v ORDER BY v LIMIT {over}",
        table = screen.base,
    );
    let sql = match col.source {
        Source::Plain(_) => format!("SELECT c.v, c.n FROM ({inner}) c"),
        Source::Dict { .. } => {
            format!("SELECT d.s, c.n FROM ({inner}) c LEFT JOIN dict d ON d.id = c.v")
        }
    };

    let mut stmt = conn.prepare_cached(&sql)?;
    let mut rows = stmt.query(params.as_slice())?;
    let mut values = Vec::new();
    while let Some(r) = rows.next()? {
        let raw = value_to_json(r.get_ref(0)?);
        let count: i64 = r.get(1)?;
        let text = match raw {
            Json::Null => BLANK_TOKEN.to_string(),
            Json::String(s) if s.is_empty() || s == "-" => BLANK_TOKEN.to_string(),
            Json::String(s) => s,
            other => other.to_string(),
        };
        // 빈 값 표식이 여러 번 나올 수 있으니 합친다.
        if let Some(last) = values.last_mut() {
            let last: &mut ColumnValue = last;
            if last.value == text {
                last.count += count.max(0) as u32;
                continue;
            }
        }
        values.push(ColumnValue { value: text, count: count.max(0) as u32 });
    }

    let truncated = values.len() > MAX_VALUES as usize;
    values.truncate(MAX_VALUES as usize);

    Ok(ColumnValuesResponse {
        values,
        truncated,
        elapsed_ms: started.elapsed().as_secs_f64() * 1000.0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

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
        Filter { key: key.into(), contains: text.into(), values: None }
    }
    fn picked(key: &str, vals: &[&str]) -> Filter {
        Filter {
            key: key.into(),
            contains: String::new(),
            values: Some(vals.iter().map(|v| v.to_string()).collect()),
        }
    }

    #[test]
    fn grid_queries_are_fast_enough() {
        let conn = crate::db::open_embedded().unwrap();
        let cases: Vec<(&str, QueryRequest)> = vec![
            ("정렬 없음", req("mojipdanwibyeol", vec![], vec![])),
            ("1순위 정렬(텍스트)", req("mojipdanwibyeol", vec![s("univ", SortDir::Asc)], vec![])),
            (
                "3순위 정렬",
                req(
                    "mojipdanwibyeol",
                    vec![
                        s("univ", SortDir::Asc),
                        s("mj", SortDir::Asc),
                        s("rate", SortDir::Desc),
                    ],
                    vec![],
                ),
            ),
            (
                "값 선택 필터 + 정렬",
                req(
                    "mojipdanwibyeol",
                    vec![s("rate", SortDir::Desc)],
                    vec![picked("region", &["서울", "경인권"])],
                ),
            ),
            (
                "값 선택 + 열 검색 동시",
                req(
                    "mojipdanwibyeol",
                    vec![],
                    vec![picked("region", &["서울"]), contains("mj", "간호")],
                ),
            ),
            // 뒤쪽 페이지로 넘어갈 때의 비용.
            ("정렬 없음 · 30쪽", at(req("mojipdanwibyeol", vec![], vec![]), 15_000)),
            ("정렬 없음 · 마지막 쪽", at(req("mojipdanwibyeol", vec![], vec![]), 119_500)),
            (
                "1순위 정렬 · 30쪽",
                at(req("mojipdanwibyeol", vec![s("univ", SortDir::Asc)], vec![]), 15_000),
            ),
            (
                "1순위 정렬 · 마지막 쪽",
                at(req("mojipdanwibyeol", vec![s("univ", SortDir::Asc)], vec![]), 119_500),
            ),
        ];
        for (name, r) in cases {
            let _ = run(&conn, &r).unwrap();
            let t = Instant::now();
            let res = run(&conn, &r).unwrap();
            eprintln!(
                "  {name:<24} {:>7.1} ms   전체 {:>7} 행",
                t.elapsed().as_secs_f64() * 1000.0,
                res.total
            );
        }
    }

    #[test]
    fn column_value_lists_are_fast_enough() {
        let conn = crate::db::open_embedded().unwrap();
        let cases = [
            ("mojipdanwibyeol", "region", vec![]),
            ("mojipdanwibyeol", "univ", vec![]),
            ("mojipdanwibyeol", "mj", vec![]),
            ("mojipdanwibyeol", "quota", vec![]),
            ("mojipdanwibyeol", "univ", vec![picked("region", &["서울"])]),
            ("data", "대학명", vec![]),
        ];
        for (screen, key, filters) in cases {
            let key = if screen == "data" && key == "대학명" { "univ" } else { key };
            let r = ColumnValuesRequest {
                screen_id: screen.into(),
                column_key: key.into(),
                filters,
            };
            let _ = column_values(&conn, &r).unwrap();
            let t = Instant::now();
            let res = column_values(&conn, &r).unwrap();
            eprintln!(
                "  {screen}.{key:<10} {:>7.1} ms   고유값 {:>5}개{}",
                t.elapsed().as_secs_f64() * 1000.0,
                res.values.len(),
                if res.truncated { " (잘림)" } else { "" }
            );
            assert!(!res.values.is_empty());
        }
    }

    #[test]
    fn picked_values_narrow_the_result() {
        let conn = crate::db::open_embedded().unwrap();
        let all = run(&conn, &req("mojipdanwibyeol", vec![], vec![])).unwrap().total;
        let one =
            run(&conn, &req("mojipdanwibyeol", vec![], vec![picked("region", &["서울"])]))
                .unwrap()
                .total;
        assert!(one > 0 && one < all, "값 선택이 결과를 좁히지 못했다 ({one} / {all})");
    }

    #[test]
    fn unchecking_everything_yields_no_rows() {
        let conn = crate::db::open_embedded().unwrap();
        let res =
            run(&conn, &req("mojipdanwibyeol", vec![], vec![picked("region", &[])])).unwrap();
        assert_eq!(res.total, 0);
    }

    #[test]
    fn value_list_ignores_its_own_column_filter() {
        let conn = crate::db::open_embedded().unwrap();
        // 서울만 체크한 상태여도, 지역 열의 후보 목록에는 전체 지역이 남아야 한다.
        let r = ColumnValuesRequest {
            screen_id: "mojipdanwibyeol".into(),
            column_key: "region".into(),
            filters: vec![picked("region", &["서울"])],
        };
        let res = column_values(&conn, &r).unwrap();
        assert!(res.values.len() > 1, "자기 열 필터를 제외하지 않았다");
    }

    #[test]
    fn unknown_sort_key_is_rejected() {
        let conn = crate::db::open_embedded().unwrap();
        let e = run(&conn, &req("mojipdanwibyeol", vec![s("no_such", SortDir::Asc)], vec![]))
            .unwrap_err();
        assert_eq!(e.code, ErrorCode::InvalidSortKey);
    }

    #[test]
    fn filter_wildcards_are_escaped() {
        let conn = crate::db::open_embedded().unwrap();
        let res = run(&conn, &req("mojipdanwibyeol", vec![], vec![contains("mj", "%")])).unwrap();
        assert_eq!(res.total, 0, "LIKE 와일드카드가 이스케이프되지 않았다");
    }
}
