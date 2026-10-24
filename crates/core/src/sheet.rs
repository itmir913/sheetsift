//! **표를 수용해 SQLite 로 변환한다.**
//!
//! xlsx 는 zip 내부의 XML 이라 그대로는 고속 조회가 불가하다. 1회 SQLite 로
//! 변환해 두면 정렬·필터가 색인을 경유하고, 반복되는 문자를 사전으로 압축하면
//! 비교가 정수 비교로 전환된다.
//!
//! **바이트를 수용하고 바이트를 반환한다.** 파일을 개폐하는 작업은 부르는 곳이
//! 수행한다 — 데스크톱과 웹이 동일한 코드를 사용하려면 그래야 한다.
//!
//! ## 무엇을 생성할지는 자료가 결정한다
//!
//! 이 모듈은 시트 명칭도 표 명칭도 열 명칭도 인지하지 않는다. [`TablePlan`] 으로
//! 수용한다. 부르는 곳의 정의 파일이 그것을 구성한다.
//!
//! ## 묵시적으로 오류를 범하느니 중단한다
//!
//! 판독하지 못한 시트를 생략하지 않는다. 생략하면 표 하나가 부재하는 DB 가
//! 묵시적으로 생성되고, 그것은 화면에서야 발현된다. 문자 표기를 판별하지 못한
//! csv 도 훼손된 문자로 판독하지 않는다 — 그러면 사전이 분기한다.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use rusqlite::Connection;

use crate::{Error, ErrorCode, Result};

fn failed(code: ErrorCode, e: impl ToString) -> Error {
    Error::new(code).detail(e)
}

/* ── 시트 ───────────────────────────────────────────────────────── */

/// 시트 하나. 셀은 **전부 문자로 통일한 상태**다.
///
/// 숫자로 수용하지 않는 사유가 존재한다. 원자료는 "없음" 을 `-` 로 기재해 숫자
/// 열에도 문자가 혼입되고, 엑셀은 셀 속성이 문자인데 내용은 숫자인 열도 생성한다.
/// 값의 구조로 추정하면 그 위치에서 묵시적으로 분기한다. 어느 열이 숫자인지는
/// [`TablePlan`] 이 기재해 전달한다.
#[derive(Debug, Clone, Default)]
pub struct Sheet {
    /// 첫 행이 머리글이고 나머지가 알맹이다.
    pub rows: Vec<Vec<String>>,
    /// 이 시트가 양식에서 **개시되는 위치**(0부터). 행과 열 양측이다.
    ///
    /// calamine 은 선두의 공백 행·열을 소거한 범위를 반환한다. 1~5행이 공백인
    /// 양식은 6행이 `rows[0]` 이 되므로, 그 소거량을 보유하지 않으면 조작부가
    /// 머리글을 `1행` 으로 표시하고 **사람이 원본의 행 번호를 참조해 지정할 수
    /// 없다.** 공백 열도 동일해, 명칭이 부재하는 열에 부여하는 열 기호가
    /// 어긋난다. csv 에는 그러한 소거가 부재해 양자 모두 0이다.
    pub origin: (usize, usize),
}

impl Sheet {
    /// 머리글 행. 공백이면 공백 조각이다.
    pub fn header(&self) -> &[String] {
        self.rows.first().map(Vec::as_slice).unwrap_or_default()
    }

    /// 머리글을 제외한 알맹이 행들.
    pub fn body(&self) -> &[Vec<String>] {
        self.rows.get(1..).unwrap_or_default()
    }

    /// 행의 `col` 번째 셀. **부재하는 셀은 공백 문자다** — 엑셀은 우측 종단이
    /// 불균일한 시트를 생성한다.
    pub fn at(row: &[String], col: usize) -> &str {
        row.get(col).map(String::as_str).unwrap_or_default()
    }
}

/// 엑셀 셀 하나를 문자로. **엑셀 화면에 노출되는 형태를 준수한다.**
fn cell(v: &calamine::Data) -> String {
    use calamine::Data;
    match v {
        Data::Empty => String::new(),
        Data::String(s) => s.trim().to_string(),
        Data::Float(f) => {
            // 12.0 을 "12" 로 — 엑셀 화면과 동일하게 노출되도록.
            if f.fract() == 0.0 && f.abs() < 1e15 {
                format!("{}", *f as i64)
            } else {
                format!("{f}")
            }
        }
        Data::Int(i) => i.to_string(),
        Data::Bool(b) => b.to_string(),
        Data::DateTime(d) => datetime(d),
        other => other.to_string(),
    }
}

/// 엑셀의 날짜 셀을 문자로.
///
/// **일련번호를 그대로 기재하지 않는다.** 엑셀은 날짜를 1899-12-30 기점의 수로
/// 수용하므로, 그 수를 문자화하면 화면에 `45,659` 가 묘사된다 — 숫자로 판정되어
/// 천 단위 구분까지 부착된다. 열람하는 사람이 그것을 날짜로 판독할 수 없다.
///
/// **엑셀의 표시 서식을 추종하지 않고 ISO 로 고정한다.** 서식은 양식마다
/// 상이하고 셀마다도 상이해, 추종하려면 서식 문자열을 해석하는 층이 증가한다.
/// ISO 는 사전순이 시간순이라 정렬이 자료의 순서와 일치한다는 이점도 존재한다.
///
/// 경과 시간(`[hh]:mm:ss`)은 날짜가 아니다. 24시간을 초과하는 값이 존재하므로
/// 시를 절단하지 않고 그대로 누적해 기재한다.
fn datetime(d: &calamine::ExcelDateTime) -> String {
    if d.is_duration() {
        let total = (d.as_f64() * 86_400.0).round() as i64;
        let (h, m, s) = (total / 3_600, (total / 60) % 60, total % 60);
        return format!("{h}:{m:02}:{s:02}");
    }
    let (y, mon, day, h, min, sec, _) = d.to_ymd_hms_milli();
    if (h, min, sec) == (0, 0, 0) {
        format!("{y:04}-{mon:02}-{day:02}")
    } else {
        format!("{y:04}-{mon:02}-{day:02} {h:02}:{min:02}:{sec:02}")
    }
}

/// 양식 바이트에서 시트를 **탭 순서대로** 추출한다.
///
/// 순서를 준수하는 사유는 부르는 곳이 그것을 참조한다는 점이다 — 양식을 열람하는
/// 사람의 판독 순서가 정의와 일치하는지 검사하는 위치가 존재한다.
///
/// **판독하지 못한 시트에서 중단한다.** 생략하면 표 하나가 부재하는 DB 가 생성된다.
pub fn from_xlsx(bytes: &[u8]) -> Result<Vec<(String, Sheet)>> {
    use calamine::Reader;
    let mut wb = calamine::open_workbook_auto_from_rs(std::io::Cursor::new(bytes))
        .map_err(|e| failed(ErrorCode::SheetWorkbookUnreadable, e))?;
    let names = wb.sheet_names().to_vec();
    let mut out = Vec::with_capacity(names.len());
    for name in names {
        let range = wb
            .worksheet_range(&name)
            .map_err(|e| failed(ErrorCode::SheetUnreadable, e).with("sheet", &name))?;
        let rows = range.rows().map(|r| r.iter().map(cell).collect()).collect();
        // 소거된 선두의 공백 행·열. 부재하면 범위가 공백이라는 의미다.
        let (row, col) = range.start().unwrap_or((0, 0));
        out.push((name, Sheet { rows, origin: (row as usize, col as usize) }));
    }
    Ok(out)
}

/// csv 바이트에서 시트 하나를 추출한다.
///
/// **문자 표기를 추정하지 않는다.** UTF-8 로 판독을 시도하고, 아니면 CP949 로
/// 시도한다. 양자 모두 아니면 **중단한다** — 국내의 csv 는 둘 중 하나이고,
/// 제3의 것을 훼손된 문자로 판독하면 사전이 묵시적으로 분기한다. 동일한 어휘가
/// 둘로 산정되고, 정렬이 교란되고, 값 선택 목록에 훼손된 문자가 표시된다. 어느
/// 쪽도 화면에서야 발현된다.
pub fn from_csv(bytes: &[u8]) -> Result<Sheet> {
    let text = decode(bytes)?;
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .from_reader(text.as_bytes());
    let mut rows = Vec::new();
    for record in reader.records() {
        let record = record.map_err(|e| failed(ErrorCode::SheetUnreadable, e))?;
        rows.push(record.iter().map(|c| c.trim().to_string()).collect());
    }
    Ok(Sheet { rows, origin: (0, 0) })
}

/// UTF-8 이면 그대로, 아니면 CP949. 양자 모두 아니면 중단한다.
///
/// BOM 은 제거한다. 엑셀이 저장한 csv 는 BOM 을 부착하는데, 그것을 존치하면 첫
/// 열의 머리글 명칭 앞에 비가시 문자가 부착되어 **그 열만 탐색하지 못한다.**
fn decode(bytes: &[u8]) -> Result<String> {
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    if let Ok(text) = std::str::from_utf8(bytes) {
        return Ok(text.to_string());
    }
    let (text, _, broken) = encoding_rs::EUC_KR.decode(bytes);
    if broken {
        return Err(Error::new(ErrorCode::SheetEncodingUnknown));
    }
    Ok(text.into_owned())
}

/* ── 무엇을 만들지 ──────────────────────────────────────────────── */

/// 열 하나가 DB 에 수용되는 형태.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueType {
    /// 문자를 사전에 집약하고 본문에는 정수 id 만 배치한다.
    Dict,
    Int,
    Real,
}

impl ValueType {
    fn sql(self) -> &'static str {
        match self {
            ValueType::Real => "REAL",
            _ => "INT",
        }
    }
}

/// 시트의 열 하나를 DB 의 열 하나로.
#[derive(Debug, Clone)]
pub struct ColumnMap {
    /// 시트 머리글에 기재된 명칭. 이것으로 열을 탐색한다.
    pub sheet: String,
    /// DB 열 명칭.
    pub column: String,
    pub ty: ValueType,
}

/// 시트 하나를 표 하나로 변환하는 방법.
#[derive(Debug, Clone)]
pub struct TablePlan {
    pub table: String,
    /// 사전으로 압축할 열들. 공백이면 머리글을 그대로 열 명칭으로 사용하고
    /// 값은 전부 문자로 수용한다.
    pub map: Vec<ColumnMap>,
    /// 생성할 색인. 한 행이 한 색인이고, 내부의 문자가 DB 열 명칭이다.
    pub indexes: Vec<Vec<String>>,
}

impl TablePlan {
    fn dict_encoded(&self) -> bool {
        !self.map.is_empty()
    }
}

/* ── 바꾸기 ─────────────────────────────────────────────────────── */

/// 값이 부재하는 셀인가. `blank` 는 양식이 "없음" 을 기재하는 문자다.
pub fn is_blank(s: &str, blank: &str) -> bool {
    let t = s.trim();
    t.is_empty() || t == blank
}

/// 머리글을 SQLite 열 명칭으로. 문자·숫자·밑줄만 존치하고, 숫자로 개시되면 앞에
/// 밑줄을 부착한다. 동일한 명칭이 재차 출현하면 `_2`, `_3` 을 부착한다.
pub fn sanitize(header: &str, seen: &mut HashMap<String, usize>) -> String {
    let mut out = String::with_capacity(header.len());
    for ch in header.trim().chars() {
        if ch.is_alphanumeric() || ch == '_' {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    if out.is_empty() {
        out.push('_');
    }
    if out.starts_with(|c: char| c.is_ascii_digit()) {
        out.insert(0, '_');
    }
    let n = seen.entry(out.clone()).or_insert(0);
    *n += 1;
    if *n > 1 {
        out = format!("{out}_{n}");
    }
    out
}

/// 머리글 행을 (원본 문자, DB 열 명칭) 쌍으로.
pub fn column_names(sheet: &Sheet) -> Vec<(String, String)> {
    let mut seen = HashMap::new();
    sheet
        .header()
        .iter()
        .map(|raw| {
            let name = sanitize(raw, &mut seen);
            (raw.clone(), name)
        })
        .collect()
}

/// 머리글에서 열의 위치를 탐색한다. **하나라도 부재하면 중단한다.**
pub fn locate(sheet: &Sheet, wanted: &[&str]) -> Result<Vec<usize>> {
    let header = sheet.header();
    if header.is_empty() {
        return Err(Error::new(ErrorCode::SheetEmpty));
    }
    wanted
        .iter()
        .map(|w| {
            header
                .iter()
                .position(|h| h == w)
                .ok_or_else(|| Error::new(ErrorCode::SheetColumnMissing).with("column", w))
        })
        .collect()
}

fn make_indexes(conn: &Connection, table: &str, indexes: &[Vec<String>]) -> Result<()> {
    for (i, cols) in indexes.iter().enumerate() {
        let cols = cols.iter().map(|c| format!("\"{c}\"")).collect::<Vec<_>>().join(",");
        let sql = format!("CREATE INDEX \"ix_{table}_{i}\" ON \"{table}\"({cols})");
        conn.execute_batch(&sql)
            .map_err(|e| failed(ErrorCode::SheetTableFailed, e).with("sql", &sql))?;
    }
    Ok(())
}

/// 사전에 수록할 문자를 전부 집약해 **가나다순으로** 번호를 부여한다.
///
/// 이 순서가 곧 정렬 순서다. 정수 비교 하나가 가나다 비교로 전환되는 사유가
/// 그것이고, 그래서 번호를 출현 순서가 아니라 **문자 순서로** 부여한다. 출현
/// 순서로 부여하면 정렬이 양식에 기재된 순서를 추종하고, 누구도 그것을 인지하지
/// 못한다.
///
/// 사전을 사용하는 표가 복수라 **선행해 일괄** 생성한다. 표마다 개별 생성하면
/// 동일한 어휘가 표마다 상이한 번호를 수령해 조인이 불일치한다.
pub fn build_dict(
    conn: &Connection,
    plans: &[(&TablePlan, &Sheet)],
    blank: &str,
) -> Result<HashMap<String, i64>> {
    let mut words: BTreeSet<String> = BTreeSet::new();
    for (plan, sheet) in plans {
        let wanted: Vec<&str> = plan.map.iter().map(|m| m.sheet.as_str()).collect();
        let at = locate(sheet, &wanted)?;
        for row in sheet.body() {
            for (i, m) in plan.map.iter().enumerate() {
                if m.ty != ValueType::Dict {
                    continue;
                }
                let s = Sheet::at(row, at[i]);
                if !is_blank(s, blank) {
                    words.insert(s.to_string());
                }
            }
        }
    }

    conn.execute_batch("CREATE TABLE dict(id INTEGER PRIMARY KEY, s TEXT NOT NULL)")
        .map_err(|e| failed(ErrorCode::SheetTableFailed, e))?;
    let mut ids = HashMap::with_capacity(words.len());
    {
        let mut stmt = conn
            .prepare("INSERT INTO dict(id, s) VALUES (?, ?)")
            .map_err(|e| failed(ErrorCode::SheetTableFailed, e))?;
        for (i, w) in words.iter().enumerate() {
            let id = i as i64 + 1;
            stmt.execute(rusqlite::params![id, w])
                .map_err(|e| failed(ErrorCode::SheetTableFailed, e))?;
            ids.insert(w.clone(), id);
        }
    }
    conn.execute_batch("CREATE INDEX ix_dict_s ON dict(s)")
        .map_err(|e| failed(ErrorCode::SheetTableFailed, e))?;
    Ok(ids)
}

/// 머리글을 그대로 열 명칭으로 사용하는 표. 값은 전부 문자로 수용한다.
pub fn build_plain(
    conn: &Connection,
    sheet: &Sheet,
    table: &str,
    indexes: &[Vec<String>],
) -> Result<(usize, Vec<(String, String)>)> {
    let header = column_names(sheet);
    let decl = header.iter().map(|(_, n)| format!("\"{n}\"")).collect::<Vec<_>>().join(",");
    conn.execute_batch(&format!("CREATE TABLE \"{table}\"({decl})"))
        .map_err(|e| failed(ErrorCode::SheetTableFailed, e).with("table", table))?;

    let holes = vec!["?"; header.len()].join(",");
    let mut stmt = conn
        .prepare(&format!("INSERT INTO \"{table}\" VALUES ({holes})"))
        .map_err(|e| failed(ErrorCode::SheetTableFailed, e).with("table", table))?;
    let mut n = 0usize;
    for row in sheet.body() {
        let vals: Vec<&str> = (0..header.len()).map(|i| Sheet::at(row, i)).collect();
        stmt.execute(rusqlite::params_from_iter(vals))
            .map_err(|e| failed(ErrorCode::SheetTableFailed, e).with("table", table))?;
        n += 1;
    }
    drop(stmt);
    make_indexes(conn, table, indexes)?;
    Ok((n, header))
}

/// 사전으로 압축하는 표. 문자는 `dict` 에 집약하고 본문에는 정수 id 만 배치한다.
///
/// `ids` 는 [`build_dict`] 가 반환한 것이다.
pub fn build_dict_table(
    conn: &Connection,
    sheet: &Sheet,
    plan: &TablePlan,
    ids: &HashMap<String, i64>,
    blank: &str,
) -> Result<(usize, Vec<(String, String)>)> {
    use rusqlite::types::Value;

    let wanted: Vec<&str> = plan.map.iter().map(|m| m.sheet.as_str()).collect();
    let at = locate(sheet, &wanted)?;
    let table = &plan.table;

    let decl = plan
        .map
        .iter()
        .map(|m| format!("\"{}\" {}", m.column, m.ty.sql()))
        .collect::<Vec<_>>()
        .join(",");
    conn.execute_batch(&format!("CREATE TABLE \"{table}\"({decl})"))
        .map_err(|e| failed(ErrorCode::SheetTableFailed, e).with("table", table))?;

    let names = plan.map.iter().map(|m| format!("\"{}\"", m.column)).collect::<Vec<_>>().join(",");
    let holes = vec!["?"; plan.map.len()].join(",");
    let mut stmt = conn
        .prepare(&format!("INSERT INTO \"{table}\"({names}) VALUES ({holes})"))
        .map_err(|e| failed(ErrorCode::SheetTableFailed, e).with("table", table))?;

    let mut n = 0usize;
    for row in sheet.body() {
        let mut vals: Vec<Value> = Vec::with_capacity(plan.map.len());
        for (i, m) in plan.map.iter().enumerate() {
            let s = Sheet::at(row, at[i]);
            vals.push(if is_blank(s, blank) {
                Value::Null
            } else {
                match m.ty {
                    ValueType::Dict => ids.get(s).map(|id| Value::Integer(*id)).unwrap_or(Value::Null),
                    ValueType::Real => s.parse::<f64>().map(Value::Real).unwrap_or(Value::Null),
                    ValueType::Int => s
                        .parse::<f64>()
                        .map(|v| Value::Integer(v.round() as i64))
                        .unwrap_or(Value::Null),
                }
            });
        }
        stmt.execute(rusqlite::params_from_iter(vals))
            .map_err(|e| failed(ErrorCode::SheetTableFailed, e).with("table", table))?;
        n += 1;
    }
    drop(stmt);
    make_indexes(conn, table, &plan.indexes)?;

    // 사전 표는 DB 열 명칭이 머리글과 상이하다. 화면이 사용하는 것은 DB 열 명칭이다.
    let header = plan.map.iter().map(|m| (m.sheet.clone(), m.column.clone())).collect();
    Ok((n, header))
}

/// 표 하나를 생성한 뒤 판명되는 것.
#[derive(Debug, Clone)]
pub struct Made {
    pub rows: usize,
    /// (시트 머리글, DB 열 명칭) 쌍. 부르는 곳이 화면 정의를 작성할 때 사용한다.
    pub columns: Vec<(String, String)>,
}

/// 복수의 시트를 복수의 표로. 사전을 선행 생성하고 표를 순차 생성한다.
///
/// 반환하는 것은 표별 [`Made`] 다.
pub fn build(
    conn: &Connection,
    plans: &[(TablePlan, Sheet)],
    blank: &str,
) -> Result<BTreeMap<String, Made>> {
    let dict: Vec<(&TablePlan, &Sheet)> =
        plans.iter().filter(|(p, _)| p.dict_encoded()).map(|(p, s)| (p, s)).collect();
    let ids = if dict.is_empty() { HashMap::new() } else { build_dict(conn, &dict, blank)? };

    let mut out = BTreeMap::new();
    for (plan, sheet) in plans {
        let (rows, columns) = if plan.dict_encoded() {
            build_dict_table(conn, sheet, plan, &ids, blank)?
        } else {
            build_plain(conn, sheet, &plan.table, &plan.indexes)?
        };
        out.insert(plan.table.clone(), Made { rows, columns });
    }
    Ok(out)
}

/// 생성한 DB 를 **바이트로 추출한다.** 파일에 기록하는 작업은 부르는 곳이
/// 수행한다.
///
/// 이 함수가 자료를 외부로 반출하는 문이다. 그러한 기능을 배치하지 않는 앱이
/// 이것을 호출하지 않는지는 그 앱의 검사가 판정한다 — 여기서는 차단할 수 없다.
pub fn serialize(conn: &Connection) -> Result<Vec<u8>> {
    conn.serialize("main")
        .map(|data| data.to_vec())
        .map_err(|e| failed(ErrorCode::SheetSerializeFailed, e))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sheet(rows: &[&[&str]]) -> Sheet {
        Sheet {
            rows: rows.iter().map(|r| r.iter().map(|c| c.to_string()).collect()).collect(),
            ..Sheet::default()
        }
    }

    fn mem() -> Connection {
        Connection::open_in_memory().unwrap()
    }

    /// **날짜는 일련번호가 아니다.** 이 시험이 정지하면 화면에 `45,659` 가
    /// 묘사된다 — 묵시적으로, 숫자 열로 판정된 채로.
    #[test]
    fn a_date_cell_is_not_its_serial_number() {
        use calamine::{Data, ExcelDateTime, ExcelDateTimeType::*};
        // 45292 는 2024-01-01 이다. 45943.541 은 calamine 이 문서에 기재한
        // 2025-10-13 12:59:02.400 이고, 밀리초는 절단한다.
        let day = Data::DateTime(ExcelDateTime::new(45292.0, DateTime, false));
        assert_eq!(cell(&day), "2024-01-01");
        let moment = Data::DateTime(ExcelDateTime::new(45943.541, DateTime, false));
        assert_eq!(cell(&moment), "2025-10-13 12:59:02");
    }

    /// 경과 시간은 날짜가 아니다. 24시간을 초과해도 절단하지 않는다.
    #[test]
    fn an_elapsed_time_keeps_hours_past_a_day() {
        use calamine::{Data, ExcelDateTime, ExcelDateTimeType::*};
        let d = Data::DateTime(ExcelDateTime::new(1.5, TimeDelta, false));
        assert_eq!(cell(&d), "36:00:00");
    }

    #[test]
    fn headers_become_usable_column_names() {
        let s = sheet(&[&["전형개요·지역", "1순위", "", "1순위"]]);
        let names: Vec<String> = column_names(&s).into_iter().map(|(_, n)| n).collect();
        // 문자가 아닌 것은 밑줄로, 숫자로 개시되면 앞에 밑줄, 중복되면 번호.
        assert_eq!(names, ["전형개요_지역", "_1순위", "_", "_1순위_2"]);
    }

    /// **사전 번호는 문자 순서다.** 출현 순서로 부여하면 정수 정렬이 양식에
    /// 기재된 순서를 추종하고, 누구도 그것을 인지하지 못한다.
    #[test]
    fn dict_ids_follow_the_alphabet_not_the_sheet() {
        let plan = TablePlan {
            table: "t".into(),
            map: vec![ColumnMap { sheet: "이름".into(), column: "name".into(), ty: ValueType::Dict }],
            indexes: vec![],
        };
        let s = sheet(&[&["이름"], &["하"], &["가"], &["나"], &["가"]]);
        let conn = mem();
        let ids = build_dict(&conn, &[(&plan, &s)], "-").unwrap();
        let mut by_id: Vec<(i64, String)> = ids.into_iter().map(|(w, i)| (i, w)).collect();
        by_id.sort();
        assert_eq!(by_id, [(1, "가".to_string()), (2, "나".to_string()), (3, "하".to_string())]);
    }

    #[test]
    fn a_dict_table_holds_ids_and_blanks_become_null() {
        let plan = TablePlan {
            table: "t".into(),
            map: vec![
                ColumnMap { sheet: "이름".into(), column: "name".into(), ty: ValueType::Dict },
                ColumnMap { sheet: "점수".into(), column: "score".into(), ty: ValueType::Int },
            ],
            indexes: vec![vec!["name".into()]],
        };
        let s = sheet(&[&["이름", "점수"], &["가", "90"], &["나", "-"]]);
        let conn = mem();
        let ids = build_dict(&conn, &[(&plan, &s)], "-").unwrap();
        let (n, columns) = build_dict_table(&conn, &s, &plan, &ids, "-").unwrap();
        assert_eq!(n, 2);
        assert_eq!(columns, [("이름".to_string(), "name".to_string()), ("점수".into(), "score".into())]);

        let joined: String = conn
            .query_row("SELECT d.s FROM t JOIN dict d ON d.id = t.name WHERE t.score = 90", [], |r| r.get(0))
            .unwrap();
        assert_eq!(joined, "가");
        // `-` 는 값이 부재한다는 표시다. 0 으로 수용하면 묵시적으로 상이한 값이 된다.
        let null: Option<i64> =
            conn.query_row("SELECT score FROM t WHERE name = 2", [], |r| r.get(0)).unwrap();
        assert_eq!(null, None);
    }

    #[test]
    fn a_plain_table_keeps_every_cell_as_text() {
        let s = sheet(&[&["가", "나"], &["1", "-"], &["2"]]);
        let conn = mem();
        let (n, _) = build_plain(&conn, &s, "t", &[]).unwrap();
        assert_eq!(n, 2);
        // 단축된 행의 부재하는 셀은 공백 문자다. 행을 폐기하지 않는다.
        let got: Vec<(String, String)> = conn
            .prepare("SELECT \"가\", \"나\" FROM t ORDER BY \"가\"")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        assert_eq!(got, [("1".to_string(), "-".to_string()), ("2".into(), "".into())]);
    }

    #[test]
    fn a_missing_column_stops_instead_of_being_skipped() {
        let s = sheet(&[&["이름"]]);
        let err = locate(&s, &["이름", "없는열"]).unwrap_err();
        assert_eq!(err.code, ErrorCode::SheetColumnMissing);
    }

    #[test]
    fn an_empty_sheet_stops() {
        assert_eq!(locate(&Sheet::default(), &["이름"]).unwrap_err().code, ErrorCode::SheetEmpty);
    }

    /// 국내의 csv 는 UTF-8 이 아니면 대개 CP949 다. 양자 모두 수용한다.
    #[test]
    fn csv_is_read_as_utf8_or_cp949() {
        let utf8 = from_csv("이름,점수\n가,90\n".as_bytes()).unwrap();
        assert_eq!(utf8.header(), ["이름", "점수"]);
        assert_eq!(utf8.body().len(), 1);

        let (cp949, _, broken) = encoding_rs::EUC_KR.encode("이름,점수\n가,90\n");
        assert!(!broken, "시험 자료가 CP949 로 기재되지 않았다");
        let read = from_csv(&cp949).unwrap();
        assert_eq!(read.header(), ["이름", "점수"]);
        assert_eq!(Sheet::at(&read.body()[0], 0), "가");
    }

    /// 엑셀이 저장한 csv 는 BOM 을 부착한다. 존치하면 첫 열만 탐색하지 못한다.
    #[test]
    fn a_bom_does_not_stick_to_the_first_header() {
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend_from_slice("이름,점수\n".as_bytes());
        assert_eq!(from_csv(&bytes).unwrap().header(), ["이름", "점수"]);
    }

    /// **훼손된 문자를 표시하지 않는다.** 그러면 사전이 묵시적으로 분기한다.
    #[test]
    fn an_unknown_encoding_stops() {
        // UTF-8 도 CP949 도 아닌 바이트. 양자 모두에서 의미가 부재한다.
        let err = from_csv(&[0x81, 0xFF, 0xFE, 0x80]).unwrap_err();
        assert_eq!(err.code, ErrorCode::SheetEncodingUnknown);
    }

    /// 생성한 것을 바이트로 추출해 **재개방할 수 있어야** 한다. 추출만 가능하고
    /// 개방되지 않으면 반출한 파일이 무용하다.
    #[test]
    fn a_built_db_comes_back_out_as_bytes() {
        let s = sheet(&[&["이름"], &["가"]]);
        let conn = mem();
        build_plain(&conn, &s, "t", &[]).unwrap();
        let bytes = serialize(&conn).unwrap();
        assert!(bytes.starts_with(b"SQLite format 3\0"), "SQLite 머리가 아니다");

        let mut back = Connection::open_in_memory().unwrap();
        back.deserialize_read_exact("main", std::io::Cursor::new(&bytes), bytes.len(), true)
            .unwrap();
        let got: String = back.query_row("SELECT \"이름\" FROM t", [], |r| r.get(0)).unwrap();
        assert_eq!(got, "가");
    }

    #[test]
    fn build_makes_the_dict_once_for_every_table() {
        let plan = |table: &str, sheet_col: &str| TablePlan {
            table: table.into(),
            map: vec![ColumnMap {
                sheet: sheet_col.into(),
                column: "name".into(),
                ty: ValueType::Dict,
            }],
            indexes: vec![],
        };
        let plans = vec![
            (plan("a", "이름"), sheet(&[&["이름"], &["나"]])),
            (plan("b", "이름"), sheet(&[&["이름"], &["가"]])),
        ];
        let conn = mem();
        let made = build(&conn, &plans, "-").unwrap();
        assert_eq!(made["a"].rows, 1);
        assert_eq!(made["b"].rows, 1);
        // 두 표가 동일한 사전을 사용한다. 개별 생성하면 동일한 어휘가 상이한 번호를 수령한다.
        let words: u32 = conn.query_row("SELECT count(*) FROM dict", [], |r| r.get(0)).unwrap();
        assert_eq!(words, 2);
        let a: i64 = conn.query_row("SELECT name FROM a", [], |r| r.get(0)).unwrap();
        let b: i64 = conn.query_row("SELECT name FROM b", [], |r| r.get(0)).unwrap();
        assert!(b < a, "가나다순이면 '가' 가 '나' 보다 소수여야 한다");
    }
}
