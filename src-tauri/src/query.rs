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
use crate::screens::{ColumnDef, ColumnKind, ScreenDef, Screens};

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

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Filter {
    pub key: String,
    /// 여러 열을 **OR 로 묶어** 보고 싶을 때. 비면 `key` 하나만 본다.
    ///
    /// 검색창 하나로 대학과 계열을 한꺼번에 찾는 화면이 쓴다. 열끼리는 AND 가
    /// 기본이라, 그대로 두면 "대학이 서울대이면서 계열도 서울대" 가 된다.
    #[serde(default)]
    pub keys: Vec<String>,
    /// 열 검색(부분일치). 빈 문자열이면 적용하지 않는다.
    #[serde(default)]
    pub contains: String,
    /// 값 선택. None = 전체 허용.
    #[serde(default)]
    pub values: Option<Vec<String>>,
    /// 이 열이 **빈칸이 아닌 줄만** 고른다.
    ///
    /// 그림을 그리는 화면이 쓴다. 값이 없는 줄은 막대를 그릴 수 없는데, 그런
    /// 줄까지 세어 쪽을 나누면 통째로 빈 쪽이 생긴다 — 합격사례의 국영수 기준은
    /// 절반이 넘는 줄에 값이 없다.
    #[serde(default)]
    pub not_blank: bool,
}

impl Filter {
    fn is_empty(&self) -> bool {
        self.contains.trim().is_empty() && self.values.is_none() && !self.not_blank
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
pub const MAX_VALUES: u32 = 1000;
/// 화면에서 비어 있는 값을 나타내는 표식. 실제 데이터의 빈칸/NULL 에 대응한다.
pub const BLANK_TOKEN: &str = "(비어 있음)";

fn like_escape(s: &str) -> String {
    s.replace('\\', r"\\").replace('%', r"\%").replace('_', r"\_")
}

/// 값이 없는 칸을 가려내는 SQL 조각.
///
/// **무엇을 "없음" 으로 볼지는 양식이 정한다**(`screens.json` 의 `blank`).
/// 여기에 글자를 박아 두면, 양식이 `–` 나 `N/A` 로 바뀌었을 때 값 선택 목록과
/// 필터가 같은 파일 안에서 갈라진다 — 후보에는 `(비어 있음)` 이 뜨는데 그걸
/// 고르면 0건이 되고, 아무 오류도 나지 않는다.
fn is_blank_sql(base: &str, blank: &str) -> String {
    // 양식이 정하는 글자라 따옴표가 들어올 일은 없지만, SQL 에 그대로 넣으므로 막는다.
    let mark = blank.replace('\'', "''");
    format!("({base} IS NULL OR {base} = '' OR {base} = '{mark}')")
}

/// 필터 하나를 WHERE 조각으로 바꾼다.
fn filter_clause(
    col: &ColumnDef,
    f: &Filter,
    blank: &str,
    binds: &mut Vec<String>,
) -> Option<String> {
    let base = col.base_ref();
    let mut parts: Vec<String> = Vec::new();

    // ① 열 검색 (부분일치)
    let needle = f.contains.trim();
    if !needle.is_empty() {
        binds.push(format!("%{}%", like_escape(needle)));
        parts.push(if col.dict {
            // 사전(3천여 행)에서 먼저 id 집합을 구한 뒤 정수 비교한다.
            format!(r"{base} IN (SELECT id FROM dict WHERE s LIKE ? ESCAPE '\')")
        } else {
            format!(r"{base} LIKE ? ESCAPE '\'")
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
            alt.push(if col.dict {
                format!("{base} IN (SELECT id FROM dict WHERE s IN ({holes}))")
            } else {
                format!("{base} IN ({holes})")
            });
        }
        if has_blank {
            alt.push(if col.dict {
                format!("{base} IS NULL")
            } else {
                is_blank_sql(&base, blank)
            });
        }
        parts.push(format!("({})", alt.join(" OR ")));
    }

    // ③ 빈칸이 아닌 줄만
    if f.not_blank {
        parts.push(if col.dict {
            format!("{base} IS NOT NULL")
        } else {
            format!("NOT {}", is_blank_sql(&base, blank))
        });
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
    blank: &str,
    skip_key: Option<&str>,
) -> AppResult<(String, Vec<String>)> {
    let mut clauses = Vec::new();
    let mut binds = Vec::new();

    for f in filters {
        let names_this = skip_key
            .is_some_and(|k| k == f.key || f.keys.iter().any(|x| x == k));
        if f.is_empty() || names_this {
            continue;
        }
        let wanted: Vec<&str> = if f.keys.is_empty() {
            vec![f.key.as_str()]
        } else {
            f.keys.iter().map(String::as_str).collect()
        };

        let mut alts = Vec::new();
        for key in wanted {
            let col = screen.column(key).ok_or_else(|| {
                AppError::new(ErrorCode::InvalidSortKey)
                    .with("key", key)
                    .with("screen", &screen.id)
                    .detail("unknown filter column")
            })?;
            if !col.filterable {
                continue;
            }
            if let Some(c) = filter_clause(col, f, blank, &mut binds) {
                alts.push(c);
            }
        }
        match alts.len() {
            0 => {}
            1 => clauses.push(alts.pop().unwrap()),
            _ => clauses.push(format!("({})", alts.join(" OR "))),
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
fn build_order(screen: &ScreenDef, sorts: &[Sort], blank: &str) -> AppResult<String> {
    if sorts.is_empty() {
        return Ok(String::new());
    }
    let mut parts = Vec::with_capacity(sorts.len());
    for s in sorts {
        let col = screen.column(&s.key).ok_or_else(|| {
            AppError::new(ErrorCode::InvalidSortKey)
                .with("key", &s.key)
                .with("screen", &screen.id)
                .detail("unknown sort column")
        })?;
        if !col.sortable {
            continue;
        }
        let dir = match s.dir {
            SortDir::Asc => "ASC",
            SortDir::Desc => "DESC",
        };
        let base = col.base_ref();
        // 값 없음은 방향과 무관하게 늘 뒤로 보낸다. 무엇이 없음인지는 양식이 정한다.
        let missing = format!("{} ASC", is_blank_sql(&base, blank));
        let expr = if col.dict {
            // 사전 열은 정수 id 그대로 (가나다순).
            base.clone()
        } else if col.kind == ColumnKind::Number {
            // 숫자 열은 TEXT 로 저장돼 있을 수 있어 CAST 후 정렬.
            format!("CAST({base} AS REAL)")
        } else {
            base.clone()
        };
        parts.push(format!("{missing}, {expr} {dir}"));
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

pub fn run(
    conn: &Connection,
    screens: &Screens,
    req: &QueryRequest,
) -> AppResult<QueryResponse> {
    let started = std::time::Instant::now();

    let screen = screens
        .find(&req.screen_id)
        .ok_or_else(|| AppError::new(ErrorCode::ScreenNotFound).with("screen", &req.screen_id))?;

    let (where_sql, binds) = build_where(screen, &req.filters, &screens.blank, None)?;
    let order_sql = build_order(screen, &req.sorts, &screens.blank)?;
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
        .map(|c| format!("{} AS \"{}\"", c.display_ref(), c.key))
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "SELECT {select_list} FROM (\
           SELECT * FROM {base} t{where_sql}{order_sql} LIMIT {limit} OFFSET {offset}\
         ) t{joins}{order_sql}",
        base = &screen.base,
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
    screens: &Screens,
    req: &ColumnValuesRequest,
) -> AppResult<ColumnValuesResponse> {
    let started = std::time::Instant::now();

    let screen = screens
        .find(&req.screen_id)
        .ok_or_else(|| AppError::new(ErrorCode::ScreenNotFound).with("screen", &req.screen_id))?;
    let col = screen.column(&req.column_key).ok_or_else(|| {
        AppError::new(ErrorCode::InvalidSortKey)
            .with("key", &req.column_key)
            .with("screen", &screen.id)
            .detail("unknown column for value list")
    })?;

    let (where_sql, binds) =
        build_where(screen, &req.filters, &screens.blank, Some(&req.column_key))?;
    let params: Vec<&dyn rusqlite::ToSql> =
        binds.iter().map(|s| s as &dyn rusqlite::ToSql).collect();

    let base = col.base_ref();
    let over = MAX_VALUES + 1; // 하나 더 받아 "잘렸는지" 판단한다.

    // 기준 테이블에서만 GROUP BY 한 뒤, 좁혀진 목록에만 dict 를 조인한다.
    let inner = format!(
        "SELECT {base} AS v, COUNT(*) AS n FROM {table} t{where_sql} \
         GROUP BY v ORDER BY v LIMIT {over}",
        table = &screen.base,
    );
    let sql = if col.dict {
        format!("SELECT d.s, c.n FROM ({inner}) c LEFT JOIN dict d ON d.id = c.v")
    } else {
        format!("SELECT c.v, c.n FROM ({inner}) c")
    };

    let mut stmt = conn.prepare_cached(&sql)?;
    let mut rows = stmt.query(params.as_slice())?;
    let mut values = Vec::new();
    while let Some(r) = rows.next()? {
        let raw = value_to_json(r.get_ref(0)?);
        let count: i64 = r.get(1)?;
        let text = match raw {
            Json::Null => BLANK_TOKEN.to_string(),
            Json::String(s) if s.is_empty() || s == screens.blank => BLANK_TOKEN.to_string(),
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

    /// 값이 없는 칸을 가려내는 글자가 **양식에서 온다**는 것을 못 박는다.
    ///
    /// 예전에는 `'-'` 가 SQL 세 곳에 박혀 있었다. 양식이 `N/A` 로 바뀌면 값
    /// 선택 목록과 필터가 갈라지는데, 그 어긋남은 오류를 내지 않는다.
    #[test]
    fn the_blank_mark_comes_from_the_form() {
        let sql = is_blank_sql("t.x", "N/A");
        assert!(sql.contains("'N/A'"), "{sql}");
        assert!(!sql.contains("'-'"), "{sql}");
        // 따옴표가 든 표기도 SQL 을 깨뜨리지 않는다.
        assert!(is_blank_sql("t.x", "it's").contains("'it''s'"));
    }

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
        assert_eq!(e.code, ErrorCode::InvalidSortKey);
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
