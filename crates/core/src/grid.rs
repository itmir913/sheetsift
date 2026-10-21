//! 표 하나를 정렬·필터해 한 쪽씩 읽는다.
//!
//! 무엇이 어느 표의 어느 열인지는 [`Table`] 이 말해 준다. 그 정의를 어디서
//! 읽어 오는지(빌드 때 넣은 자료, 사용자가 연 파일)는 이 모듈이 모른다.
//!
//! SQL 조각(열 참조·기준 테이블 이름)은 [`Table`] 의 정의에서만 온다.
//! 사용자가 넣은 값(검색어·선택 값)은 **반드시 바인딩 파라미터**로만 들어가고,
//! 정렬·필터 키는 정의에 있는 열인지 검증한 뒤에만 SQL 에 쓴다.
//!
//! ## 두 종류의 필터
//! - `contains` : 열 검색. 부분일치(LIKE).
//! - `values`   : 엑셀식 값 선택. 그 열의 고유값 중 체크한 것만 남긴다.
//!   `None` 이면 전체 허용, `Some(빈 목록)` 이면 아무것도 통과하지 않는다.
//!
//! ## 사전 인코딩(Dict) 열
//! 큰 표는 반복되는 문자열을 `dict` 테이블의 정수 id 로 바꿔 저장한다. 이때
//! **id 를 가나다순으로 부여**하므로 `ORDER BY t.col`(정수) 가 곧 가나다순
//! 정렬이다. 덕분에 정렬·집계에는 조인이 전혀 필요 없고, 화면에 내보낼 행에만
//! 조인한다.

use rusqlite::{types::ValueRef, Connection};
use serde::{Deserialize, Serialize};
use serde_json::Value as Json;

use crate::{Error, ErrorCode, Result};

/// 한 번에 가져갈 수 있는 행 수의 상한.
///
/// **부르는 쪽이 알아야 하는 값이다.** 이보다 큰 수를 넣으면 말없이 여기까지만
/// 온다 — 모르고 1000 을 달라고 한 화면은 500 줄을 받고도 다 받은 줄 안다.
pub const MAX_LIMIT: u32 = 500;
/// 값 선택 목록의 상한. 넘으면 잘라서 보내고 검색으로 좁히게 한다.
pub const MAX_VALUES: u32 = 1000;
/// 화면에서 비어 있는 값을 나타내는 표식. 실제 데이터의 빈칸/NULL 에 대응한다.
pub const BLANK_TOKEN: &str = "(비어 있음)";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ColumnKind {
    /// 문자열. 좌측 정렬, 부분일치 필터.
    Text,
    /// 숫자. 우측 정렬, 자릿수 맞춤.
    Number,
}

/// 열 하나. 화면에 보이는 것과 SQL 에 쓰는 것을 함께 안다.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Column {
    /// 결과 컬럼 별칭이자 화면에서 쓰는 키.
    pub key: String,
    /// 화면에 보이는 이름.
    pub label: String,
    /// 기준 테이블의 열 이름. 계산으로 만드는 열은 비어 있다.
    #[serde(skip_serializing)]
    pub column: String,
    pub kind: ColumnKind,
    pub sortable: bool,
    pub filterable: bool,
    /// 양수에 `+` 를 붙일 숫자인가. 격차처럼 부호에 뜻이 있는 열에 쓴다.
    #[serde(default)]
    pub signed: bool,
    /// 사전 인코딩 열인가. 그렇다면 값은 정수 id 이고 dict 를 조인해야 한다.
    #[serde(skip_serializing)]
    pub dict: bool,
    /// dict 조인에 쓸 별칭. 정의를 읽어 들인 뒤 [`assign_aliases`] 가 매긴다.
    #[serde(skip)]
    pub alias: String,
}

impl Column {
    /// 기준 테이블 안에서의 열 참조. 정렬·필터에 쓴다.
    pub fn base_ref(&self) -> String {
        format!("t.\"{}\"", self.column)
    }
    /// 화면에 내보낼 식. 사전 열은 조인한 문자열을 쓴다.
    pub fn display_ref(&self) -> String {
        if self.dict {
            format!("{}.s", self.alias)
        } else {
            self.base_ref()
        }
    }
}

/// 조회할 수 있는 표. 정의를 들고 있는 쪽이 구현한다.
pub trait Table {
    /// 오류에 실어 보낼 이름. 어느 표에서 났는지 가리는 데만 쓴다.
    fn id(&self) -> &str;
    /// 기준 테이블 이름.
    fn base(&self) -> &str;
    /// 열을 화면 차례대로.
    fn columns(&self) -> Box<dyn Iterator<Item = &Column> + '_>;

    fn column(&self, key: &str) -> Option<&Column> {
        self.columns().find(|c| c.key == key)
    }
}

/// 같은 열을 두 번 조인하지 않도록 사전 열마다 별칭(d1, d2 …)을 매긴다.
/// 같은 기준 열을 쓰는 사전 열은 별칭을 함께 쓴다.
pub fn assign_aliases<'a>(columns: impl Iterator<Item = &'a mut Column>) {
    let mut alias_of: Vec<(String, String)> = Vec::new();
    for c in columns {
        if !c.dict {
            continue;
        }
        let found = alias_of.iter().find(|(col, _)| *col == c.column).map(|(_, a)| a.clone());
        c.alias = match found {
            Some(a) => a,
            None => {
                let a = format!("d{}", alias_of.len() + 1);
                alias_of.push((c.column.clone(), a.clone()));
                a
            }
        };
    }
}

/// 화면에 필요한 dict 조인 절을 만든다. (중복 별칭 제거)
fn dict_joins(table: &dyn Table) -> String {
    let mut seen: Vec<&str> = Vec::new();
    let mut out = String::new();
    for c in table.columns() {
        if !c.dict || seen.contains(&c.alias.as_str()) {
            continue;
        }
        seen.push(&c.alias);
        out.push_str(&format!(
            "\n     LEFT JOIN dict {} ON {}.id = t.\"{}\"",
            c.alias, c.alias, c.column
        ));
    }
    out
}

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
    /// 검색창 하나로 두 열을 한꺼번에 찾는 화면이 쓴다. 열끼리는 AND 가 기본이라,
    /// 그대로 두면 "이 열에도 있고 저 열에도 있다" 가 된다.
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
    /// 줄까지 세어 쪽을 나누면 통째로 빈 쪽이 생긴다.
    #[serde(default)]
    pub not_blank: bool,
}

impl Filter {
    pub fn is_empty(&self) -> bool {
        self.contains.trim().is_empty() && self.values.is_none() && !self.not_blank
    }
}

/// 한 쪽을 읽는 요청.
#[derive(Debug, Clone, Default)]
pub struct Query {
    pub sorts: Vec<Sort>,
    pub filters: Vec<Filter>,
    pub offset: u32,
    pub limit: u32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Page {
    /// 필터를 적용한 전체 행 수.
    pub total: u32,
    pub offset: u32,
    /// 열 차례대로 담은 행들.
    pub rows: Vec<Vec<Json>>,
    pub elapsed_ms: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnValue {
    pub value: String,
    pub count: u32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnValues {
    pub values: Vec<ColumnValue>,
    /// [`MAX_VALUES`] 를 넘어 잘렸는가.
    pub truncated: bool,
    pub elapsed_ms: f64,
}

/// `LIKE` 에서 뜻을 갖는 글자를 막는다.
pub fn like_escape(s: &str) -> String {
    s.replace('\\', r"\\").replace('%', r"\%").replace('_', r"\_")
}

/// 값이 없는 칸을 가려내는 SQL 조각.
///
/// **무엇을 "없음" 으로 볼지는 자료가 정한다**(`blank`). 여기에 글자를 박아 두면,
/// 자료가 `–` 나 `N/A` 로 바뀌었을 때 값 선택 목록과 필터가 같은 파일 안에서
/// 갈라진다 — 후보에는 `(비어 있음)` 이 뜨는데 그걸 고르면 0건이 되고, 아무 오류도
/// 나지 않는다.
fn is_blank_sql(base: &str, blank: &str) -> String {
    // 자료가 정하는 글자라 따옴표가 들어올 일은 드물지만, SQL 에 그대로 넣으므로 막는다.
    let mark = blank.replace('\'', "''");
    format!("({base} IS NULL OR {base} = '' OR {base} = '{mark}')")
}

fn unknown_column(table: &dyn Table, key: &str, what: &str) -> Error {
    Error::new(ErrorCode::InvalidSortKey)
        .with("key", key)
        .with("screen", table.id())
        .detail(what)
}

/// 필터 하나를 WHERE 조각으로 바꾼다.
fn filter_clause(col: &Column, f: &Filter, blank: &str, binds: &mut Vec<String>) -> Option<String> {
    let base = col.base_ref();
    let mut parts: Vec<String> = Vec::new();

    // ① 열 검색 (부분일치)
    let needle = f.contains.trim();
    if !needle.is_empty() {
        binds.push(format!("%{}%", like_escape(needle)));
        parts.push(if col.dict {
            // 사전에서 먼저 id 집합을 구한 뒤 정수 비교한다.
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
            alt.push(if col.dict { format!("{base} IS NULL") } else { is_blank_sql(&base, blank) });
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
    table: &dyn Table,
    filters: &[Filter],
    blank: &str,
    skip_key: Option<&str>,
) -> Result<(String, Vec<String>)> {
    let mut clauses = Vec::new();
    let mut binds = Vec::new();

    for f in filters {
        let names_this = skip_key.is_some_and(|k| k == f.key || f.keys.iter().any(|x| x == k));
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
            let col = table
                .column(key)
                .ok_or_else(|| unknown_column(table, key, "unknown filter column"))?;
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
fn build_order(table: &dyn Table, sorts: &[Sort], blank: &str) -> Result<String> {
    if sorts.is_empty() {
        return Ok(String::new());
    }
    let mut parts = Vec::with_capacity(sorts.len());
    for s in sorts {
        let col = table
            .column(&s.key)
            .ok_or_else(|| unknown_column(table, &s.key, "unknown sort column"))?;
        if !col.sortable {
            continue;
        }
        let dir = match s.dir {
            SortDir::Asc => "ASC",
            SortDir::Desc => "DESC",
        };
        let base = col.base_ref();
        // 값 없음은 방향과 무관하게 늘 뒤로 보낸다. 무엇이 없음인지는 자료가 정한다.
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
        ValueRef::Real(f) => serde_json::Number::from_f64(f).map(Json::Number).unwrap_or(Json::Null),
        ValueRef::Text(t) => Json::String(String::from_utf8_lossy(t).into_owned()),
        ValueRef::Blob(_) => Json::Null,
    }
}

/// 한 쪽을 읽는다.
pub fn run(conn: &Connection, table: &dyn Table, blank: &str, q: &Query) -> Result<Page> {
    // `std::time::Instant` 가 아닌 까닭은 상자 파일에 적어 두었다. wasm 에는
    // 운영체제의 시계가 없어 **부르는 순간 패닉한다.**
    let started = web_time::Instant::now();

    let (where_sql, binds) = build_where(table, &q.filters, blank, None)?;
    let order_sql = build_order(table, &q.sorts, blank)?;
    let limit = q.limit.clamp(1, MAX_LIMIT);

    let params: Vec<&dyn rusqlite::ToSql> =
        binds.iter().map(|s| s as &dyn rusqlite::ToSql).collect();

    // 전체 행 수. 기준 테이블만 보므로 조인이 전혀 없다.
    let count_sql = format!("SELECT COUNT(*) FROM {base} t{where_sql}", base = table.base());
    let total: i64 = conn.query_row(&count_sql, params.as_slice(), |r| r.get(0))?;

    // 본문: 안쪽에서 정렬·LIMIT 으로 행을 좁힌 뒤, 그 몇십 행에만 dict 를 조인한다.
    let select_list = table
        .columns()
        .map(|c| format!("{} AS \"{}\"", c.display_ref(), c.key))
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "SELECT {select_list} FROM (\
           SELECT * FROM {base} t{where_sql}{order_sql} LIMIT {limit} OFFSET {offset}\
         ) t{joins}{order_sql}",
        base = table.base(),
        joins = dict_joins(table),
        offset = q.offset,
    );

    let ncol = table.columns().count();
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

    Ok(Page {
        total: total.max(0) as u32,
        offset: q.offset,
        rows,
        elapsed_ms: started.elapsed().as_secs_f64() * 1000.0,
    })
}

/// 한 열의 고유값 목록과 각 값의 행 수.
///
/// 엑셀 필터 드롭다운처럼, **그 열 자신의 필터는 빼고** 나머지 필터를 반영한다.
/// 그래야 체크를 풀었다 걸었다 해도 후보 목록이 사라지지 않는다.
///
/// `needle` 은 **후보 자체를 좁히는 글자**다. 비면 좁히지 않는다.
///
/// 이것이 필터와 따로 있는 까닭이 있다. 값이 [`MAX_VALUES`] 를 넘으면 잘라서
/// 보내는데, 그 뒤를 볼 길이 없으면 **목록에 없는 값을 없는 값으로 읽는다.**
/// 받은 쪽에서 걸러 보여 주는 것으로는 안 된다 — 거기 온 것이 이미 앞에서
/// 잘린 것이라, 찾는 값이 그 뒤에 있으면 "없습니다" 가 뜬다. 조용히 틀리는
/// 쪽이다.
///
/// 필터로 넘길 수도 없다. 제 열의 필터는 위에서 말한 까닭으로 **일부러**
/// 빠지고, 전체 검색처럼 그 열을 걸치는 필터도 함께 빠진다.
pub fn column_values(
    conn: &Connection,
    table: &dyn Table,
    blank: &str,
    key: &str,
    filters: &[Filter],
    needle: &str,
) -> Result<ColumnValues> {
    let started = web_time::Instant::now();

    let col = table
        .column(key)
        .ok_or_else(|| unknown_column(table, key, "unknown column for value list"))?;

    let (mut where_sql, mut binds) = build_where(table, filters, blank, Some(key))?;

    /* 좁히는 글자는 **이 열에 거는 부분일치**다. 사전 열이면 사전에서 찾는다 —
       그 처리를 여기 한 벌 더 적지 않고 필터 한 조각을 빌려 쓴다. */
    if !needle.trim().is_empty() {
        let only = Filter { contains: needle.to_string(), ..Filter::default() };
        if let Some(clause) = filter_clause(col, &only, blank, &mut binds) {
            where_sql = if where_sql.is_empty() {
                format!(" WHERE {clause}")
            } else {
                format!("{where_sql} AND {clause}")
            };
        }
    }

    let params: Vec<&dyn rusqlite::ToSql> =
        binds.iter().map(|s| s as &dyn rusqlite::ToSql).collect();

    let base = col.base_ref();
    let over = MAX_VALUES + 1; // 하나 더 받아 "잘렸는지" 판단한다.

    // 기준 테이블에서만 GROUP BY 한 뒤, 좁혀진 목록에만 dict 를 조인한다.
    let inner = format!(
        "SELECT {base} AS v, COUNT(*) AS n FROM {table} t{where_sql} \
         GROUP BY v ORDER BY v LIMIT {over}",
        table = table.base(),
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
            Json::String(s) if s.is_empty() || s == blank => BLANK_TOKEN.to_string(),
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

    Ok(ColumnValues { values, truncated, elapsed_ms: started.elapsed().as_secs_f64() * 1000.0 })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 시험용 표. 정의는 코드에 적지만 **조회는 정의만 보고 한다.**
    struct T {
        columns: Vec<Column>,
    }

    impl Table for T {
        fn id(&self) -> &str {
            "people"
        }
        fn base(&self) -> &str {
            "people"
        }
        fn columns(&self) -> Box<dyn Iterator<Item = &Column> + '_> {
            Box::new(self.columns.iter())
        }
    }

    fn col(key: &str, kind: ColumnKind, dict: bool) -> Column {
        Column {
            key: key.into(),
            label: key.into(),
            column: key.into(),
            kind,
            sortable: true,
            filterable: true,
            signed: false,
            dict,
            alias: String::new(),
        }
    }

    const BLANK: &str = "-";

    /// 사전 열(`city`)과 글자 열(`name`)과 숫자 열(`score`). 사전 id 는 가나다순이다.
    fn fixture() -> (Connection, T) {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE dict(id INTEGER PRIMARY KEY, s TEXT);
             INSERT INTO dict VALUES(1, '대구'), (2, '부산'), (3, '서울');
             CREATE TABLE people(name TEXT, city INTEGER, score TEXT);
             INSERT INTO people VALUES
               ('가', 3, '90'), ('나', 2, '100'), ('다', 3, '-'),
               ('라', 1, '85.5'), ('마', NULL, ''), ('100%', 2, '70');",
        )
        .unwrap();
        let mut t = T {
            columns: vec![
                col("name", ColumnKind::Text, false),
                col("city", ColumnKind::Text, true),
                col("score", ColumnKind::Number, false),
            ],
        };
        assign_aliases(t.columns.iter_mut());
        (conn, t)
    }

    fn query(sorts: Vec<Sort>, filters: Vec<Filter>) -> Query {
        Query { sorts, filters, offset: 0, limit: 100 }
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
    fn names(page: &Page) -> Vec<String> {
        page.rows.iter().map(|r| r[0].as_str().unwrap().to_string()).collect()
    }

    /// 값이 없는 칸을 가려내는 글자가 **자료에서 온다**는 것을 못 박는다.
    #[test]
    fn the_blank_mark_comes_from_the_data() {
        let sql = is_blank_sql("t.x", "N/A");
        assert!(sql.contains("'N/A'"), "{sql}");
        assert!(!sql.contains("'-'"), "{sql}");
        // 따옴표가 든 표기도 SQL 을 깨뜨리지 않는다.
        assert!(is_blank_sql("t.x", "it's").contains("'it''s'"));
    }

    /// 사전 열은 **조인한 글자로** 나간다. 정수 id 가 화면에 새면 안 된다.
    #[test]
    fn dictionary_columns_come_out_as_text() {
        let (conn, t) = fixture();
        let page = run(&conn, &t, BLANK, &query(vec![], vec![])).unwrap();
        assert_eq!(page.total, 6);
        assert_eq!(page.rows[0][1], Json::from("서울"));
    }

    /// 사전 열의 정렬은 id 정렬이고, id 가 가나다순이라 곧 가나다순이다.
    /// 값 없음은 방향과 무관하게 뒤로 간다.
    #[test]
    fn sorting_a_dictionary_column_is_alphabetical_and_blanks_go_last() {
        let (conn, t) = fixture();
        let page = run(&conn, &t, BLANK, &query(vec![s("city", SortDir::Asc)], vec![])).unwrap();
        let cities: Vec<&Json> = page.rows.iter().map(|r| &r[1]).collect();
        assert_eq!(cities.first().unwrap().as_str(), Some("대구"));
        assert_eq!(cities.last().unwrap(), &&Json::Null);
    }

    /// 숫자 열은 글자로 저장돼 있어도 **수로** 정렬한다. 글자로 견주면 "100" 이
    /// "85.5" 보다 앞선다. 빈칸 표식과 빈 글자는 뒤로 간다.
    #[test]
    fn numbers_sort_as_numbers() {
        let (conn, t) = fixture();
        let page = run(&conn, &t, BLANK, &query(vec![s("score", SortDir::Desc)], vec![])).unwrap();
        assert_eq!(names(&page)[..4], ["나", "가", "라", "100%"]);
    }

    /// 배열 차례가 우선순위다. 같은 도시 안에서 이름으로 갈린다.
    #[test]
    fn sorts_apply_in_the_order_given() {
        let (conn, t) = fixture();
        let page = run(
            &conn,
            &t,
            BLANK,
            &query(vec![s("city", SortDir::Desc), s("name", SortDir::Desc)], vec![]),
        )
        .unwrap();
        assert_eq!(names(&page)[..2], ["다", "가"]);
    }

    #[test]
    fn picked_values_narrow_the_result() {
        let (conn, t) = fixture();
        let page = run(&conn, &t, BLANK, &query(vec![], vec![picked("city", &["서울"])])).unwrap();
        assert_eq!(page.total, 2);
    }

    #[test]
    fn unchecking_everything_yields_no_rows() {
        let (conn, t) = fixture();
        let page = run(&conn, &t, BLANK, &query(vec![], vec![picked("name", &[])])).unwrap();
        assert_eq!(page.total, 0);
    }

    /// 빈칸 표식을 고르면 NULL·빈 글자·자료의 빈칸 글자가 모두 걸린다.
    #[test]
    fn picking_the_blank_token_finds_every_kind_of_blank() {
        let (conn, t) = fixture();
        let page =
            run(&conn, &t, BLANK, &query(vec![], vec![picked("score", &[BLANK_TOKEN])])).unwrap();
        assert_eq!(page.total, 2, "'-' 와 '' 둘 다 빈칸이다");
        let page =
            run(&conn, &t, BLANK, &query(vec![], vec![picked("city", &[BLANK_TOKEN])])).unwrap();
        assert_eq!(page.total, 1);
    }

    #[test]
    fn not_blank_keeps_only_filled_rows() {
        let (conn, t) = fixture();
        let f = Filter { key: "score".into(), not_blank: true, ..Filter::default() };
        assert_eq!(run(&conn, &t, BLANK, &query(vec![], vec![f])).unwrap().total, 4);
    }

    /// `keys` 로 묶은 열은 OR 이다.
    #[test]
    fn grouped_keys_are_ored() {
        let (conn, t) = fixture();
        let f = Filter { keys: vec!["name".into(), "city".into()], contains: "가".into(), ..Default::default() };
        assert_eq!(run(&conn, &t, BLANK, &query(vec![], vec![f])).unwrap().total, 1);
        let f = Filter { keys: vec!["name".into(), "city".into()], contains: "서".into(), ..Default::default() };
        assert_eq!(run(&conn, &t, BLANK, &query(vec![], vec![f])).unwrap().total, 2);
    }

    /// 값 목록은 자기 열 필터를 빼고 센다. 빈칸은 한 줄로 합친다.
    #[test]
    fn value_list_ignores_its_own_column_filter_and_merges_blanks() {
        let (conn, t) = fixture();
        let res = column_values(&conn, &t, BLANK, "score", &[picked("score", &["90"])], "").unwrap();
        let blank = res.values.iter().find(|v| v.value == BLANK_TOKEN).unwrap();
        assert_eq!(blank.count, 2);
        assert_eq!(res.values.len(), 5, "자기 열 필터를 제외하지 않았다");
        // 다른 열의 필터는 반영한다.
        let res = column_values(&conn, &t, BLANK, "name", &[picked("city", &["서울"])], "").unwrap();
        assert_eq!(res.values.len(), 2);
    }

    #[test]
    fn unknown_sort_key_is_rejected() {
        let (conn, t) = fixture();
        let e = run(&conn, &t, BLANK, &query(vec![s("no_such", SortDir::Asc)], vec![])).unwrap_err();
        assert_eq!(e.code, ErrorCode::InvalidSortKey);
        let e = column_values(&conn, &t, BLANK, "no_such", &[], "").unwrap_err();
        assert_eq!(e.code, ErrorCode::InvalidSortKey);
    }

    /// `%` 는 와일드카드가 아니라 글자로 찾는다.
    #[test]
    fn filter_wildcards_are_escaped() {
        let (conn, t) = fixture();
        let page = run(&conn, &t, BLANK, &query(vec![], vec![contains("name", "%")])).unwrap();
        assert_eq!(names(&page), ["100%"]);
        let page = run(&conn, &t, BLANK, &query(vec![], vec![contains("name", "_")])).unwrap();
        assert_eq!(page.total, 0);
    }

    /// 같은 기준 열을 쓰는 사전 열은 별칭을 함께 쓴다. 두 번 조인하지 않는다.
    #[test]
    fn the_same_dictionary_column_is_joined_once() {
        let mut cols = vec![col("a", ColumnKind::Text, true), col("b", ColumnKind::Text, true)];
        cols[1].column = "a".into();
        cols.push(col("c", ColumnKind::Text, true));
        assign_aliases(cols.iter_mut());
        assert_eq!(
            cols.iter().map(|c| c.alias.as_str()).collect::<Vec<_>>(),
            ["d1", "d1", "d2"]
        );
        assert_eq!(dict_joins(&T { columns: cols }).matches("LEFT JOIN").count(), 2);
    }
}
