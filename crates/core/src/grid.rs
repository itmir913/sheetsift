//! 표 하나를 정렬·필터해 한 페이지씩 판독한다.
//!
//! 무엇이 어느 표의 어느 열인지는 [`Table`] 이 제시한다. 그 정의를 어디서
//! 판독하는지(빌드 시 주입한 자료, 사용자가 개방한 파일)는 이 모듈이 인지하지
//! 않는다.
//!
//! SQL 조각(열 참조·기준 테이블 명칭)은 [`Table`] 의 정의에서만 유래한다.
//! 사용자가 주입한 값(검색어·선택 값)은 **반드시 바인딩 파라미터**로만
//! 진입하고, 정렬·필터 열쇠는 정의에 존재하는 열인지 검증한 뒤에만 SQL 에
//! 사용한다.
//!
//! ## 필터 2종
//! - `contains` : 열 검색. 부분일치(LIKE).
//! - `values`   : 엑셀식 값 선택. 그 열의 고유값 중 선택한 것만 존치한다.
//!   `None` 이면 전체 허용, `Some(공백 목록)` 이면 아무것도 통과하지 않는다.
//!
//! ## 사전 인코딩(Dict) 열
//! 대형 표는 반복되는 문자열을 `dict` 테이블의 정수 id 로 변환해 저장한다. 이때
//! **id 를 가나다순으로 부여**하므로 `ORDER BY t.col`(정수) 가 곧 가나다순
//! 정렬이다. 그 결과 정렬·집계에는 조인이 전혀 불필요하고, 화면에 노출할 행에만
//! 조인한다.

use rusqlite::{types::ValueRef, Connection};
use serde::{Deserialize, Serialize};
use serde_json::Value as Json;

use crate::{Error, ErrorCode, Result};

/// 1회에 수령 가능한 행 수의 상한.
///
/// **부르는 곳이 인지해야 하는 값이다.** 이보다 큰 수를 주입하면 묵시적으로
/// 여기까지만 도달한다 — 인지하지 못한 채 1000 을 요청한 화면은 500행을 수령하고도
/// 전량 수령한 것으로 판단한다.
pub const MAX_LIMIT: u32 = 500;
/// 값 선택 목록의 상한. 초과하면 절단해 전송하고 검색으로 축소하게 한다.
pub const MAX_VALUES: u32 = 1000;
/// 화면에서 공백 값을 표현하는 표식. 실제 자료의 공백 셀/NULL 에 대응한다.
pub const BLANK_TOKEN: &str = "(비어 있음)";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ColumnKind {
    /// 문자열. 좌측 정렬, 부분일치 필터.
    Text,
    /// 숫자. 우측 정렬, 자릿수 정합.
    Number,
}

/// 열 하나. 화면에 노출되는 것과 SQL 에 사용하는 것을 함께 보유한다.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Column {
    /// 결과 컬럼 별칭이자 화면에서 사용하는 열쇠.
    pub key: String,
    /// 화면에 노출되는 명칭.
    pub label: String,
    /// 기준 테이블의 열 명칭. 연산으로 생성하는 열은 공백이다.
    #[serde(skip_serializing)]
    pub column: String,
    pub kind: ColumnKind,
    pub sortable: bool,
    pub filterable: bool,
    /// 양수에 `+` 를 부착할 숫자인가. 격차처럼 부호에 의미가 존재하는 열에 사용한다.
    #[serde(default)]
    pub signed: bool,
    /// 사전 인코딩 열인가. 해당하면 값은 정수 id 이고 dict 를 조인해야 한다.
    #[serde(skip_serializing)]
    pub dict: bool,
    /// dict 조인에 사용할 별칭. 정의를 판독한 뒤 [`assign_aliases`] 가 부여한다.
    #[serde(skip)]
    pub alias: String,
}

impl Column {
    /// 기준 테이블 내부의 열 참조. 정렬·필터에 사용한다.
    pub fn base_ref(&self) -> String {
        format!("t.\"{}\"", self.column)
    }
    /// 화면에 노출할 식. 사전 열은 조인한 문자열을 사용한다.
    pub fn display_ref(&self) -> String {
        if self.dict {
            format!("{}.s", self.alias)
        } else {
            self.base_ref()
        }
    }
}

/// 조회 가능한 표. 정의를 보유한 곳이 구현한다.
pub trait Table {
    /// 오류에 첨부할 명칭. 어느 표에서 발생했는지 식별하는 용도로만 사용한다.
    fn id(&self) -> &str;
    /// 기준 테이블 명칭.
    fn base(&self) -> &str;
    /// 열을 화면 순서대로.
    fn columns(&self) -> Box<dyn Iterator<Item = &Column> + '_>;

    fn column(&self, key: &str) -> Option<&Column> {
        self.columns().find(|c| c.key == key)
    }
}

/// 동일한 열을 2회 조인하지 않도록 사전 열마다 별칭(d1, d2 …)을 부여한다.
/// 동일한 기준 열을 사용하는 사전 열은 별칭을 공유한다.
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

/// 화면에 필요한 dict 조인 절을 생성한다. (중복 별칭 제거)
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
    /// 복수의 열을 **OR 로 결합**해 조회할 때 사용한다. 공백이면 `key` 하나만 참조한다.
    ///
    /// 검색창 하나로 두 열을 동시에 탐색하는 화면이 사용한다. 열 상호간은 AND 가
    /// 기본이라, 그대로 유지하면 "이 열에도 존재하고 저 열에도 존재한다" 가 된다.
    #[serde(default)]
    pub keys: Vec<String>,
    /// 열 검색(부분일치). 공백 문자열이면 적용하지 않는다.
    #[serde(default)]
    pub contains: String,
    /// 값 선택. None = 전체 허용.
    #[serde(default)]
    pub values: Option<Vec<String>>,
    /// 이 열이 **공백이 아닌 행만** 선택한다.
    ///
    /// 도형을 묘사하는 화면이 사용한다. 값이 부재하는 행은 막대를 묘사할 수 없는데,
    /// 그런 행까지 산입해 페이지를 분할하면 전체가 공백인 페이지가 발생한다.
    #[serde(default)]
    pub not_blank: bool,
}

impl Filter {
    pub fn is_empty(&self) -> bool {
        self.contains.trim().is_empty() && self.values.is_none() && !self.not_blank
    }
}

/// 한 페이지를 판독하는 요청.
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
    /// 열 순서대로 수용한 행들.
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
    /// [`MAX_VALUES`] 를 초과해 절단되었는가.
    pub truncated: bool,
    pub elapsed_ms: f64,
}

/// `LIKE` 에서 의미를 보유하는 문자를 차단한다.
pub fn like_escape(s: &str) -> String {
    s.replace('\\', r"\\").replace('%', r"\%").replace('_', r"\_")
}

/// 값이 부재하는 셀을 식별하는 SQL 조각.
///
/// **무엇을 "부재" 로 간주할지는 자료가 결정한다**(`blank`). 여기에 문자를 고정하면,
/// 자료가 `–` 나 `N/A` 로 변경되었을 때 값 선택 목록과 필터가 동일한 파일 내부에서
/// 분기한다 — 후보에는 `(비어 있음)` 이 표시되는데 그것을 선택하면 0건이 되고, 어떠한
/// 오류도 발생하지 않는다.
fn is_blank_sql(base: &str, blank: &str) -> String {
    // 자료가 결정하는 문자라 따옴표가 진입할 여지는 희소하나, SQL 에 그대로 주입하므로 차단한다.
    let mark = blank.replace('\'', "''");
    format!("({base} IS NULL OR {base} = '' OR {base} = '{mark}')")
}

/// 숫자 열의 정렬 열쇠.
///
/// 숫자 열도 TEXT 로 수용되어 있다(원자료에 "없음" 표기가 혼입한다). 그래서
/// 변환해 정렬하는데, **구조를 식별할 때는 제거한 천 단위 구분을 여기서도
/// 제거해야 한다** — `1,200` 을 숫자로 판정해 두고 변환만 그대로 수행하면
/// SQLite 가 `1` 로 산출해 `50` 보다 앞에 배치한다.
fn number_sql(base: &str) -> String {
    format!("CAST(REPLACE({base}, ',', '') AS REAL)")
}

/// 수치로 판독되지 않는 값인가. **그런 값은 방향과 무관하게 후위로 배치한다.**
///
/// SQLite 의 변환은 판독하지 못한 문자를 `0` 으로 산출한다. 그대로 정렬하면
/// `총 44850점` 같은 합계 행이 `0` 과 `1` 사이에 삽입되는데, 표를 판독하는
/// 사람에게는 그것이 0점으로 보인다. 구조 식별이 표본(선두 일부)만 조사하므로
/// 그런 행은 후단에 상존한다.
///
/// 변환이 0을 산출한 값 중 **실제로 0인 것**만 수치로 간주한다. 부호와 0과
/// 소수점과 구분 기호로만 구성된 문자가 그것이다.
fn not_a_number_sql(base: &str) -> String {
    let n = number_sql(base);
    /* 변환이 0 을 산출한 값 중 **실제로 0 인 것**을 판별한다. 부호·0·소수점·
       구분 기호를 제거하고 공백 문자가 잔류하면 그것이다. GLOB 에는 반복을
       지정하는 수단이 부재해 그 제거를 문자 치환으로 수행한다. */
    let zeroish = ["','", "'0'", "'.'", "'-'", "'+'", "' '"]
        .iter()
        .fold(base.to_string(), |acc, ch| format!("REPLACE({acc}, {ch}, '')"));
    format!("(CASE WHEN {n} <> 0 OR {zeroish} = '' THEN 0 ELSE 1 END) ASC")
}

fn unknown_column(table: &dyn Table, key: &str, what: &str) -> Error {
    Error::new(ErrorCode::InvalidSortKey)
        .with("key", key)
        .with("screen", table.id())
        .detail(what)
}

/// 필터 하나를 WHERE 조각으로 변환한다.
fn filter_clause(col: &Column, f: &Filter, blank: &str, binds: &mut Vec<String>) -> Option<String> {
    let base = col.base_ref();
    let mut parts: Vec<String> = Vec::new();

    // ① 열 검색 (부분일치)
    let needle = f.contains.trim();
    if !needle.is_empty() {
        binds.push(format!("%{}%", like_escape(needle)));
        parts.push(if col.dict {
            // 사전에서 우선 id 집합을 산출한 뒤 정수 비교한다.
            format!(r"{base} IN (SELECT id FROM dict WHERE s LIKE ? ESCAPE '\')")
        } else {
            format!(r"{base} LIKE ? ESCAPE '\'")
        });
    }

    // ② 값 선택 (엑셀식 체크박스)
    if let Some(values) = &f.values {
        if values.is_empty() {
            // 전부 해제 → 어떠한 행도 통과하지 않는다.
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

    // ③ 공백이 아닌 행만
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

/// WHERE 절과 바인딩 값을 생성한다. 기준 테이블 기준으로만 생성한다(조인 없음).
/// `skip_key` 가 지정되면 그 열의 필터는 제외한다(값 선택 목록 생성 시 사용한다).
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
/// 사전 인코딩 열은 id 자체가 가나다순이라 조인 없이 정수 정렬로 완결된다.
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
        // 값 부재는 방향과 무관하게 항상 후위로 배치한다. 무엇이 부재인지는 자료가 결정한다.
        let missing = format!("{} ASC", is_blank_sql(&base, blank));
        let expr = if col.dict {
            // 사전 열은 정수 id 그대로 (가나다순).
            base.clone()
        } else if col.kind == ColumnKind::Number {
            format!("{}, {}", not_a_number_sql(&base), number_sql(&base))
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

/// 한 페이지를 판독한다.
pub fn run(conn: &Connection, table: &dyn Table, blank: &str, q: &Query) -> Result<Page> {
    // `std::time::Instant` 가 아닌 사유는 상자 파일에 기재했다. wasm 에는
    // 운영체제의 시계가 부재해 **호출하는 즉시 패닉한다.**
    let started = web_time::Instant::now();

    let (where_sql, binds) = build_where(table, &q.filters, blank, None)?;
    let order_sql = build_order(table, &q.sorts, blank)?;
    let limit = q.limit.clamp(1, MAX_LIMIT);

    let params: Vec<&dyn rusqlite::ToSql> =
        binds.iter().map(|s| s as &dyn rusqlite::ToSql).collect();

    // 전체 행 수. 기준 테이블만 참조하므로 조인이 전혀 없다.
    let count_sql = format!("SELECT COUNT(*) FROM {base} t{where_sql}", base = table.base());
    let total: i64 = conn.query_row(&count_sql, params.as_slice(), |r| r.get(0))?;

    // 본문: 내부에서 정렬·LIMIT 으로 행을 축소한 뒤, 그 수십 행에만 dict 를 조인한다.
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
/// 엑셀 필터 드롭다운과 동일하게, **그 열 자신의 필터는 제외하고** 나머지
/// 필터를 반영한다. 그래야 선택을 해제하고 재설정해도 후보 목록이 소멸하지
/// 않는다.
///
/// `needle` 은 **후보 자체를 축소하는 문자**다. 공백이면 축소하지 않는다.
///
/// 이것이 필터와 별도로 존재하는 사유가 존재한다. 값이 [`MAX_VALUES`] 를
/// 초과하면 절단해 전송하는데, 그 이후를 확인할 경로가 부재하면 **목록에 없는
/// 값을 부재하는 값으로 판독한다.** 받은 곳에서 여과해 표시하는 방식으로는
/// 불가하다 — 거기 도달한 것이 이미 전단에서 절단된 것이라, 탐색하는 값이 그
/// 이후에 소재하면 "없습니다" 가 표시된다. 묵시적으로 오류를 범하는 쪽이다.
///
/// 필터로 전달할 수도 없다. 자체 열의 필터는 상기한 사유로 **의도적으로**
/// 누락되고, 전체 검색처럼 그 열을 포괄하는 필터도 함께 누락된다.
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

    /* 축소하는 문자는 **이 열에 적용하는 부분일치**다. 사전 열이면 사전에서
       탐색한다 — 그 처리를 여기 한 벌 더 기재하지 않고 필터 한 조각을 차용한다. */
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
    let over = MAX_VALUES + 1; // 하나 더 수령해 "절단 여부" 를 판정한다.

    // 기준 테이블에서만 GROUP BY 한 뒤, 축소된 목록에만 dict 를 조인한다.
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
        // 공백 값 표식이 복수 회 출현할 수 있으므로 병합한다.
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

    /// 시험용 표. 정의는 코드에 기재하나 **조회는 정의만 참조해 수행한다.**
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

    /// 사전 열(`city`)과 문자 열(`name`)과 숫자 열(`score`). 사전 id 는 가나다순이다.
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

    /// **수치로 판독되지 않는 값이 0 의 자리에 삽입되지 않는다.**
    ///
    /// SQLite 의 변환은 판독하지 못한 문자를 0 으로 산출한다. 구조 식별은 선두
    /// 표본만 조사하므로, 합계 행처럼 후단에 소재하는 문자는 숫자 열에 그대로
    /// 잔류한다 — 그것이 0 점으로 묘사되면 표를 판독하는 사람이 오판한다.
    #[test]
    fn text_in_a_number_column_does_not_sit_where_zero_would() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE people(name TEXT, score TEXT);
             INSERT INTO people VALUES
               ('영', '0'), ('합계', '총 44850점'), ('하나', '1'), ('공백', '');",
        )
        .unwrap();
        let mut t = T { columns: vec![col("name", ColumnKind::Text, false), col("score", ColumnKind::Number, false)] };
        assign_aliases(t.columns.iter_mut());

        let asc = query(vec![s("score", SortDir::Asc)], vec![]);
        let page = run(&conn, &t, BLANK, &asc).unwrap();
        assert_eq!(names(&page), ["영", "하나", "합계", "공백"], "문자가 0 과 1 사이에 삽입되었습니다");

        // 방향을 역전시켜도 수치가 아닌 것과 부재는 후위를 유지한다.
        let desc = query(vec![s("score", SortDir::Desc)], vec![]);
        let page = run(&conn, &t, BLANK, &desc).unwrap();
        assert_eq!(names(&page), ["하나", "영", "합계", "공백"]);
    }

    /// **천 단위 구분이 부착된 수치가 구분 앞자리로 정렬되지 않는다.**
    ///
    /// 구조를 식별할 때는 구분을 제거해 숫자로 판정하면서 변환만 그대로
    /// 수행했다. 그러면 `1,200` 이 `1` 로 산출되어 `50` 보다 앞에 배치된다.
    #[test]
    fn a_thousands_separator_does_not_cut_the_number_short() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE people(name TEXT, score TEXT);
             INSERT INTO people VALUES ('천이백', '1,200'), ('구백팔십', '980'), ('오십', '50');",
        )
        .unwrap();
        let mut t = T { columns: vec![col("name", ColumnKind::Text, false), col("score", ColumnKind::Number, false)] };
        assign_aliases(t.columns.iter_mut());

        let page = run(&conn, &t, BLANK, &query(vec![s("score", SortDir::Asc)], vec![])).unwrap();
        assert_eq!(names(&page), ["오십", "구백팔십", "천이백"]);
    }

    /// 값이 부재하는 셀을 식별하는 문자가 **자료에서 유래한다**는 것을 고정한다.
    #[test]
    fn the_blank_mark_comes_from_the_data() {
        let sql = is_blank_sql("t.x", "N/A");
        assert!(sql.contains("'N/A'"), "{sql}");
        assert!(!sql.contains("'-'"), "{sql}");
        // 따옴표가 포함된 표기도 SQL 을 훼손하지 않는다.
        assert!(is_blank_sql("t.x", "it's").contains("'it''s'"));
    }

    /// 사전 열은 **조인한 문자로** 노출된다. 정수 id 가 화면에 누출되면 안 된다.
    #[test]
    fn dictionary_columns_come_out_as_text() {
        let (conn, t) = fixture();
        let page = run(&conn, &t, BLANK, &query(vec![], vec![])).unwrap();
        assert_eq!(page.total, 6);
        assert_eq!(page.rows[0][1], Json::from("서울"));
    }

    /// 사전 열의 정렬은 id 정렬이고, id 가 가나다순이라 곧 가나다순이다.
    /// 값 부재는 방향과 무관하게 후위로 이동한다.
    #[test]
    fn sorting_a_dictionary_column_is_alphabetical_and_blanks_go_last() {
        let (conn, t) = fixture();
        let page = run(&conn, &t, BLANK, &query(vec![s("city", SortDir::Asc)], vec![])).unwrap();
        let cities: Vec<&Json> = page.rows.iter().map(|r| &r[1]).collect();
        assert_eq!(cities.first().unwrap().as_str(), Some("대구"));
        assert_eq!(cities.last().unwrap(), &&Json::Null);
    }

    /// 숫자 열은 문자로 저장되어 있어도 **수치로** 정렬한다. 문자로 비교하면 "100" 이
    /// "85.5" 보다 선행한다. 공백 표식과 공백 문자는 후위로 이동한다.
    #[test]
    fn numbers_sort_as_numbers() {
        let (conn, t) = fixture();
        let page = run(&conn, &t, BLANK, &query(vec![s("score", SortDir::Desc)], vec![])).unwrap();
        assert_eq!(names(&page)[..4], ["나", "가", "라", "100%"]);
    }

    /// 배열 순서가 우선순위다. 동일 도시 내부에서 명칭으로 분기한다.
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

    /// 공백 표식을 선택하면 NULL·공백 문자·자료의 공백 문자가 전부 포착된다.
    #[test]
    fn picking_the_blank_token_finds_every_kind_of_blank() {
        let (conn, t) = fixture();
        let page =
            run(&conn, &t, BLANK, &query(vec![], vec![picked("score", &[BLANK_TOKEN])])).unwrap();
        assert_eq!(page.total, 2, "'-' 와 '' 양자 모두 공백이다");
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

    /// `keys` 로 결합한 열은 OR 이다.
    #[test]
    fn grouped_keys_are_ored() {
        let (conn, t) = fixture();
        let f = Filter { keys: vec!["name".into(), "city".into()], contains: "가".into(), ..Default::default() };
        assert_eq!(run(&conn, &t, BLANK, &query(vec![], vec![f])).unwrap().total, 1);
        let f = Filter { keys: vec!["name".into(), "city".into()], contains: "서".into(), ..Default::default() };
        assert_eq!(run(&conn, &t, BLANK, &query(vec![], vec![f])).unwrap().total, 2);
    }

    /// 값 목록은 자체 열 필터를 제외하고 산정한다. 공백은 한 행으로 병합한다.
    #[test]
    fn value_list_ignores_its_own_column_filter_and_merges_blanks() {
        let (conn, t) = fixture();
        let res = column_values(&conn, &t, BLANK, "score", &[picked("score", &["90"])], "").unwrap();
        let blank = res.values.iter().find(|v| v.value == BLANK_TOKEN).unwrap();
        assert_eq!(blank.count, 2);
        assert_eq!(res.values.len(), 5, "자체 열 필터를 제외하지 않았다");
        // 타 열의 필터는 반영한다.
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

    /// `%` 는 와일드카드가 아니라 문자로 탐색한다.
    #[test]
    fn filter_wildcards_are_escaped() {
        let (conn, t) = fixture();
        let page = run(&conn, &t, BLANK, &query(vec![], vec![contains("name", "%")])).unwrap();
        assert_eq!(names(&page), ["100%"]);
        let page = run(&conn, &t, BLANK, &query(vec![], vec![contains("name", "_")])).unwrap();
        assert_eq!(page.total, 0);
    }

    /// 동일한 기준 열을 사용하는 사전 열은 별칭을 공유한다. 2회 조인하지 않는다.
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
