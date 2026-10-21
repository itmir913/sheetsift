//! **표를 받아 SQLite 로 바꾼다.**
//!
//! xlsx 는 zip 안의 XML 이라 그대로는 빠르게 조회할 수 없다. 한 번 SQLite 로
//! 바꿔 두면 정렬·필터가 색인을 타고, 반복되는 글자를 사전으로 눌러 담으면
//! 비교가 정수 비교가 된다.
//!
//! **바이트를 받고 바이트를 돌려준다.** 파일을 여닫는 일은 부르는 쪽이 한다 —
//! 데스크톱과 웹이 같은 코드를 쓰려면 그래야 한다.
//!
//! ## 무엇을 만들지는 데이터가 정한다
//!
//! 이 모듈은 시트 이름도 표 이름도 열 이름도 모른다. [`TablePlan`] 으로 받는다.
//! 부르는 쪽의 정의 파일이 그것을 짓는다.
//!
//! ## 조용히 틀리느니 선다
//!
//! 읽지 못한 시트를 건너뛰지 않는다. 건너뛰면 표 하나가 없는 DB 가 조용히
//! 만들어지고, 그것은 화면에서야 드러난다. 글자 표기를 알아내지 못한 csv 도
//! 깨진 글자로 읽지 않는다 — 그러면 사전이 갈린다.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use rusqlite::Connection;

use crate::{Error, ErrorCode, Result};

fn failed(code: ErrorCode, e: impl ToString) -> Error {
    Error::new(code).detail(e)
}

/* ── 시트 ───────────────────────────────────────────────────────── */

/// 시트 하나. 칸은 **전부 글자로 고른 뒤**다.
///
/// 숫자로 담지 않는 까닭이 있다. 원자료는 "없음" 을 `-` 로 적어서 숫자 열에도
/// 글자가 섞여 들어오고, 엑셀은 셀 속성이 글자인데 내용은 숫자인 열도 만든다.
/// 값의 생김새로 짐작하면 그 자리에서 조용히 갈린다. 어느 열이 숫자인지는
/// [`TablePlan`] 이 적어서 알려 준다.
#[derive(Debug, Clone, Default)]
pub struct Sheet {
    /// 첫 줄이 머리글이고 나머지가 알맹이다.
    pub rows: Vec<Vec<String>>,
}

impl Sheet {
    /// 머리글 줄. 비어 있으면 빈 조각이다.
    pub fn header(&self) -> &[String] {
        self.rows.first().map(Vec::as_slice).unwrap_or_default()
    }

    /// 머리글을 뺀 알맹이 줄들.
    pub fn body(&self) -> &[Vec<String>] {
        self.rows.get(1..).unwrap_or_default()
    }

    /// 줄의 `col` 번째 칸. **없는 칸은 빈 글자다** — 엑셀은 오른쪽 끝이
    /// 들쭉날쭉한 시트를 만든다.
    pub fn at(row: &[String], col: usize) -> &str {
        row.get(col).map(String::as_str).unwrap_or_default()
    }
}

/// 엑셀 칸 하나를 글자로. **엑셀 화면에 보이는 모양을 따른다.**
fn cell(v: &calamine::Data) -> String {
    use calamine::Data;
    match v {
        Data::Empty => String::new(),
        Data::String(s) => s.trim().to_string(),
        Data::Float(f) => {
            // 12.0 을 "12" 로 — 엑셀 화면과 같게 보이도록.
            if f.fract() == 0.0 && f.abs() < 1e15 {
                format!("{}", *f as i64)
            } else {
                format!("{f}")
            }
        }
        Data::Int(i) => i.to_string(),
        Data::Bool(b) => b.to_string(),
        Data::DateTime(d) => d.to_string(),
        other => other.to_string(),
    }
}

/// 양식 바이트에서 시트를 **탭 차례대로** 꺼낸다.
///
/// 차례를 지키는 까닭은 부르는 쪽이 그것을 본다는 것이다 — 양식을 보는 사람의
/// 읽는 차례가 정의와 맞는지 검사하는 자리가 있다.
///
/// **읽지 못한 시트에서 선다.** 건너뛰면 표 하나가 없는 DB 가 만들어진다.
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
        out.push((name, Sheet { rows }));
    }
    Ok(out)
}

/// csv 바이트에서 시트 하나를 꺼낸다.
///
/// **글자 표기를 짐작하지 않는다.** UTF-8 로 읽어 보고, 아니면 CP949 로 읽어
/// 본다. 둘 다 아니면 **선다** — 이 땅의 csv 는 둘 중 하나이고, 셋째를 깨진
/// 글자로 읽으면 사전이 조용히 갈린다. 같은 낱말이 두 개로 세어지고, 정렬이
/// 뒤섞이고, 값 선택 목록에 깨진 글자가 뜬다. 어느 쪽도 화면에서야 드러난다.
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
    Ok(Sheet { rows })
}

/// UTF-8 이면 그대로, 아니면 CP949. 둘 다 아니면 선다.
///
/// BOM 은 떼어 낸다. 엑셀이 저장한 csv 는 BOM 을 붙이는데, 그것을 남기면 첫
/// 열의 머리글 이름 앞에 보이지 않는 글자가 붙어 **그 열만 찾지 못한다.**
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

/// 열 하나가 DB 에 담기는 모양.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueType {
    /// 글자를 사전에 모으고 본문에는 정수 id 만 둔다.
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
    /// 시트 머리글에 적힌 이름. 이것으로 열을 찾는다.
    pub sheet: String,
    /// DB 열 이름.
    pub column: String,
    pub ty: ValueType,
}

/// 시트 하나를 표 하나로 바꾸는 법.
#[derive(Debug, Clone)]
pub struct TablePlan {
    pub table: String,
    /// 사전으로 눌러 담을 열들. 비어 있으면 머리글을 그대로 열 이름으로 쓰고
    /// 값은 전부 글자로 담는다.
    pub map: Vec<ColumnMap>,
    /// 만들 색인. 한 줄이 한 색인이고, 안의 글자가 DB 열 이름이다.
    pub indexes: Vec<Vec<String>>,
}

impl TablePlan {
    fn dict_encoded(&self) -> bool {
        !self.map.is_empty()
    }
}

/* ── 바꾸기 ─────────────────────────────────────────────────────── */

/// 값이 없는 칸인가. `blank` 는 양식이 "없음" 을 적는 글자다.
pub fn is_blank(s: &str, blank: &str) -> bool {
    let t = s.trim();
    t.is_empty() || t == blank
}

/// 머리글을 SQLite 열 이름으로. 글자·숫자·밑줄만 남기고, 숫자로 시작하면 앞에
/// 밑줄을 붙인다. 같은 이름이 또 나오면 `_2`, `_3` 을 단다.
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

/// 머리글 줄을 (원래 글자, DB 열 이름) 짝으로.
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

/// 머리글에서 열의 자리를 찾는다. **하나라도 없으면 선다.**
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

/// 사전에 들어갈 글자를 모두 모아 **가나다순으로** 번호를 매긴다.
///
/// 이 차례가 곧 정렬 차례다. 정수 비교 하나가 가나다 비교가 되는 까닭이 그것
/// 이고, 그래서 번호를 나온 차례가 아니라 **글자 차례로** 매긴다. 나온 차례로
/// 매기면 정렬이 양식에 적힌 차례를 따라가고, 아무도 그것을 눈치채지 못한다.
///
/// 사전을 쓰는 표가 여럿이라 **먼저 한 번에** 만든다. 표마다 따로 만들면 같은
/// 낱말이 표마다 다른 번호를 받아 조인이 어긋난다.
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

/// 머리글을 그대로 열 이름으로 쓰는 표. 값은 전부 글자로 담는다.
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

/// 사전으로 눌러 담는 표. 글자는 `dict` 에 모으고 본문에는 정수 id 만 둔다.
///
/// `ids` 는 [`build_dict`] 가 돌려준 것이다.
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

    // 사전 표는 DB 열 이름이 머리글과 다르다. 화면이 쓰는 것은 DB 열 이름이다.
    let header = plan.map.iter().map(|m| (m.sheet.clone(), m.column.clone())).collect();
    Ok((n, header))
}

/// 표 하나를 만들고 나서 알게 되는 것.
#[derive(Debug, Clone)]
pub struct Made {
    pub rows: usize,
    /// (시트 머리글, DB 열 이름) 짝. 부르는 쪽이 화면 정의를 쓸 때 쓴다.
    pub columns: Vec<(String, String)>,
}

/// 시트 여럿을 표 여럿으로. 사전을 먼저 만들고 표를 차례대로 만든다.
///
/// 돌려주는 것은 표마다의 [`Made`] 다.
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

/// 만든 DB 를 **바이트로 꺼낸다.** 파일에 쓰는 일은 부르는 쪽이 한다.
///
/// 이 함수가 자료를 밖으로 내보내는 문이다. 그런 기능을 두지 않는 앱이
/// 이것을 부르지 않는지는 그 앱의 검사가 본다 — 여기서는 막을 수 없다.
pub fn serialize(conn: &Connection) -> Result<Vec<u8>> {
    conn.serialize("main")
        .map(|data| data.to_vec())
        .map_err(|e| failed(ErrorCode::SheetSerializeFailed, e))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sheet(rows: &[&[&str]]) -> Sheet {
        Sheet { rows: rows.iter().map(|r| r.iter().map(|c| c.to_string()).collect()).collect() }
    }

    fn mem() -> Connection {
        Connection::open_in_memory().unwrap()
    }

    #[test]
    fn headers_become_usable_column_names() {
        let s = sheet(&[&["전형개요·지역", "1순위", "", "1순위"]]);
        let names: Vec<String> = column_names(&s).into_iter().map(|(_, n)| n).collect();
        // 글자가 아닌 것은 밑줄로, 숫자로 시작하면 앞에 밑줄, 겹치면 번호.
        assert_eq!(names, ["전형개요_지역", "_1순위", "_", "_1순위_2"]);
    }

    /// **사전 번호는 글자 차례다.** 나온 차례로 매기면 정수 정렬이 양식에 적힌
    /// 차례를 따라가고, 아무도 그것을 눈치채지 못한다.
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
        // `-` 는 값이 없다는 표시다. 0 으로 담으면 조용히 틀린 값이 된다.
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
        // 짧은 줄의 없는 칸은 빈 글자다. 줄을 버리지 않는다.
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

    /// 이 땅의 csv 는 UTF-8 이 아니면 대개 CP949 다. 둘 다 받는다.
    #[test]
    fn csv_is_read_as_utf8_or_cp949() {
        let utf8 = from_csv("이름,점수\n가,90\n".as_bytes()).unwrap();
        assert_eq!(utf8.header(), ["이름", "점수"]);
        assert_eq!(utf8.body().len(), 1);

        let (cp949, _, broken) = encoding_rs::EUC_KR.encode("이름,점수\n가,90\n");
        assert!(!broken, "시험 자료가 CP949 로 적히지 않았다");
        let read = from_csv(&cp949).unwrap();
        assert_eq!(read.header(), ["이름", "점수"]);
        assert_eq!(Sheet::at(&read.body()[0], 0), "가");
    }

    /// 엑셀이 저장한 csv 는 BOM 을 붙인다. 남기면 첫 열만 찾지 못한다.
    #[test]
    fn a_bom_does_not_stick_to_the_first_header() {
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend_from_slice("이름,점수\n".as_bytes());
        assert_eq!(from_csv(&bytes).unwrap().header(), ["이름", "점수"]);
    }

    /// **깨진 글자를 보이지 않는다.** 그러면 사전이 조용히 갈린다.
    #[test]
    fn an_unknown_encoding_stops() {
        // UTF-8 도 CP949 도 아닌 바이트. 둘 다에서 뜻이 없다.
        let err = from_csv(&[0x81, 0xFF, 0xFE, 0x80]).unwrap_err();
        assert_eq!(err.code, ErrorCode::SheetEncodingUnknown);
    }

    /// 만든 것을 바이트로 꺼내 **다시 열 수 있어야** 한다. 꺼내기만 되고
    /// 열리지 않으면 내보낸 파일이 쓸모가 없다.
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
        // 두 표가 같은 사전을 쓴다. 따로 만들면 같은 낱말이 다른 번호를 받는다.
        let words: u32 = conn.query_row("SELECT count(*) FROM dict", [], |r| r.get(0)).unwrap();
        assert_eq!(words, 2);
        let a: i64 = conn.query_row("SELECT name FROM a", [], |r| r.get(0)).unwrap();
        let b: i64 = conn.query_row("SELECT name FROM b", [], |r| r.get(0)).unwrap();
        assert!(b < a, "가나다순이면 '가' 가 '나' 보다 작아야 한다");
    }
}
