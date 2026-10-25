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

/// 조회 엔진이 수용하는 열 수. **연결이 진술하는 값을 판독한다.**
///
/// 컴파일 시점에 결정되는 값이라 기재할 수 없다. 이 저장소는 `.cargo/config.toml`
/// 에서 상향하고 있으나, 저장소 외부에서 빌드한 산출물은 SQLite 의 기본값(2000)
/// 이다 — 수를 기재하면 그 산출물에서 **허위를 진술한다.**
pub fn column_limit(conn: &Connection) -> Result<usize> {
    conn.limit(rusqlite::limits::Limit::SQLITE_LIMIT_COLUMN)
        .map(|n| n.max(0) as usize)
        .map_err(|e| failed(ErrorCode::SheetTooWide, e))
}

/// 열이 상한을 초과하면 **표를 생성하기 전에 중단한다.**
///
/// 그러지 않으면 생성이 `SheetTableFailed` 로 중단되는데, 그 코드는 사유를
/// 진술하지 않는다 — 열은 전부 묘사되는데 자료만 도달하지 않는 상태로 발현한다.
fn check_width(conn: &Connection, table: &str, width: usize) -> Result<()> {
    let limit = column_limit(conn)?;
    if width > limit {
        return Err(Error::new(ErrorCode::SheetTooWide)
            .with("table", table)
            .with("columns", width)
            .with("limit", limit));
    }
    Ok(())
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
    /// 첫 행이 헤더고 나머지가 알맹이다.
    pub rows: Vec<Vec<String>>,
    /// 이 시트가 양식에서 **개시되는 위치**(0부터). 행과 열 양측이다.
    ///
    /// calamine 은 선두의 공백 행·열을 소거한 범위를 반환한다. 1~5행이 공백인
    /// 양식은 6행이 `rows[0]` 이 되므로, 그 소거량을 보유하지 않으면 조작부가
    /// 헤더를 `1행` 으로 표시하고 **사람이 원본의 행 번호를 참조해 지정할 수
    /// 없다.** 공백 열도 동일해, 명칭이 부재하는 열에 부여하는 열 기호가
    /// 어긋난다. csv 도 선두의 공백 행을 동일하게 소거해 여기 기재하고(`from_csv`),
    /// 중간의 공백 행은 자료의 일부라 행으로 복원한다. 열은 csv 에 소거가 부재해
    /// 언제나 0 이다.
    pub origin: (usize, usize),
}

impl Sheet {
    /// 헤더 행. 공백이면 공백 조각이다.
    pub fn header(&self) -> &[String] {
        self.rows.first().map(Vec::as_slice).unwrap_or_default()
    }

    /// 헤더를 제외한 알맹이 행들.
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
        Data::String(s) => cell_text(s),
        Data::Float(f) => number(*f),
        Data::Int(i) => i.to_string(),
        Data::Bool(b) => b.to_string(),
        Data::DateTime(d) => datetime(d),
        other => other.to_string(),
    }
}

/// 셀의 문자를 수용 가능한 형태로.
///
/// **공백 표식과 동일한 문자를 제거한다.** 그 표식은 값이 부재하는 셀을
/// 지시하려고 바탕이 산출하는 것인데, 동일한 문자가 셀에 기재되어 있으면 값
/// 선택 목록에 두 항목이 출현하고 **양자가 동일한 값으로 전송된다** — 선택한
/// 것과 상이한 행이 통과한다. 유니코드가 문자로 배정하지 않는 위치라 그것을
/// 제거해도 사람이 기재한 내용은 소멸하지 않는다.
fn cell_text(s: &str) -> String {
    let t = s.trim();
    /* NUL 도 함께 제거한다. 문자로 수용되기는 하나 **열 검색이 그 위치에서
       정지해**, 그 이후에 기재된 문자를 탐색하지 못한다 — 자료는 존재하는데
       탐색하지 못하는 상태라 사람이 부재한다고 판단한다. */
    if t.contains(crate::grid::BLANK_TOKEN) || t.contains('\0') {
        return t.replace(crate::grid::BLANK_TOKEN, "").replace('\0', "");
    }
    t.to_string()
}

/// 수 하나를 문자로. **엑셀 화면에 노출되는 형태를 준수한다.**
fn number(f: f64) -> String {
    // 12.0 을 "12" 로 — 엑셀 화면과 동일하게 노출되도록.
    if f.fract() == 0.0 && f.abs() < 1e15 {
        format!("{}", f as i64)
    } else {
        format!("{f}")
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
    /* **환산할 수 없는 수는 환산하지 않는다.** 엑셀은 날짜·시간 서식을 임의의
       수에 부여할 수 있고(화면에는 `####` 이 묘사된다), 그 수가 극단이면 환산이
       **중단되거나 허위 값을 산출한다** — 덧셈이 넘쳐 패닉하거나, 포화된 정수가
       `2562047788015215:30:07` 같은 시간이 된다. 수는 수 그대로 기재한다. */
    let serial = d.as_f64();
    if !serial.is_finite() {
        return number(serial);
    }

    if d.is_duration() {
        // 경과 시간은 날짜가 아니라 기간이다. 1e6 일(약 2,700년)을 초과하는
        // 기간은 양식의 오기이지 사람이 기재한 값이 아니다.
        if serial.abs() > 1e6 {
            return number(serial);
        }
        let total = (serial * 86_400.0).round() as i64;
        let (h, m, s) = (total / 3_600, (total / 60) % 60, total % 60);
        return format!("{h}:{m:02}:{s:02}");
    }

    /* **0 이하는 날짜가 아니다.** 엑셀의 일련번호는 1 이 1900-01-01 이고 0 과
       음수에는 대응하는 날짜가 부재하다. 그것을 환산하면 상이한 수들이 전부
       `1899-12-31` 이라는 **동일한 문자로 병합된다** — 자료가 소멸한다. */
    const LAST: f64 = 2_958_465.0; // 9999-12-31
    if serial <= 0.0 || serial > LAST {
        return number(serial);
    }

    let (y, mon, day, h, min, sec, _) = d.to_ymd_hms_milli();
    if serial < 1.0 {
        // 날짜가 부재하고 시각만 기재된 셀. 엑셀도 시각만 묘사한다.
        return format!("{h:02}:{min:02}:{sec:02}");
    }
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
    for (at, name) in names.into_iter().enumerate() {
        /* **명칭이 아니라 위치로 수령한다.** 명칭이 동일한 시트가 둘이면 명칭
           으로 수령하는 쪽은 언제나 첫 시트를 반환한다 — 부르는 곳이 명칭을
           유일하게 만들어도 **자료는 여전히 첫 시트의 것**이고, 사람은 상이한
           탭을 선택하면서 동일한 표를 열람한다. 엑셀은 동일한 명칭을 금하나
           그 파일 형식은 금하지 않는다. */
        let range = wb
            .worksheet_range_at(at)
            .ok_or_else(|| Error::new(ErrorCode::SheetUnreadable).with("sheet", &name))?
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
    /* **소거된 공백 행을 복원한다.** csv 상자는 완전히 공백인 행을 레코드로
       산출하지 않는다. 그대로 수용하면 조작부가 표시하는 행 번호가 원본 파일과
       어긋나고, 그 번호를 참조해 헤더를 지정하는 사람은 전혀 상이한 행을
       판독한다. 선두의 공백 행은 xlsx 와 동일하게 `origin` 으로 기재하고,
       중간의 공백 행은 자료의 일부라 행으로 복원한다.

       레코드가 **개시되는 행 번호**를 상자가 보유하므로, 직전 레코드가 종료된
       위치와의 간격만큼 공백 행을 삽입한다. 따옴표 내부의 개행은 레코드 하나가
       복수 행을 점유하므로 그만큼 가산한다. */
    let raw = text.as_bytes();
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut at = 0usize; // 주사한 위치(바이트)
    let mut line = 1u64; // `at` 이 소재하는 물리 행
    let mut expect = 1u64; // 차기 레코드가 개시되어야 할 행
    for record in reader.records() {
        let record = record.map_err(|e| failed(ErrorCode::SheetUnreadable, e))?;

        /* 상자가 제공하는 위치는 **레코드의 개시 위치가 아니라 주사를 개시한
           위치다** — 공백 행을 건너뛰기 이전의 값이라 행 번호로 사용할 수 없다.
           그래서 그 위치까지 전진한 뒤 공백 행을 직접 건너뛰며 계수한다. */
        let scan = record.position().map(|p| p.byte() as usize).unwrap_or(at);
        while at < scan {
            if is_break(raw, at) {
                line += 1;
            }
            at += 1;
        }
        while at < raw.len() && (raw[at] == b'\n' || raw[at] == b'\r') {
            if is_break(raw, at) {
                line += 1;
            }
            at += 1;
        }

        for _ in expect..line {
            rows.push(Vec::new());
        }
        /* 따옴표 내부의 개행은 레코드 하나가 복수 행을 점유한다. 그 분량만큼
           공백 행을 부가해야 **이후의 행 번호가 원본과 일치한다.** 자료로는
           공백 행이 하나 증가할 뿐이고, 어긋난 행 번호보다 유해하지 않다. */
        let spanned: u64 = record.iter().map(|c| c.matches('\n').count() as u64).sum();
        rows.push(record.iter().map(cell_text).collect::<Vec<String>>());
        for _ in 0..spanned {
            rows.push(Vec::new());
        }
        expect = line + 1 + spanned;
    }
    /* **선두의 공백 행은 소거하고 그 수를 보유한다.** xlsx 는 상자가 그렇게
       반환하므로 양측의 거동이 그래야 일치한다. 복원한 채로 두면 추정에
       실패했을 때 화면이 적용하는 첫 행이 공백 행에 적중해, 열 명칭이 전부
       부재한 표가 묘사되고 실제 헤더가 자료 1행이 된다 — 실제로 그렇게
       발생했다. 중간의 공백 행은 자료의 일부라 존치한다. */
    let lead = rows.iter().take_while(|r| r.iter().all(|c| c.trim().is_empty())).count();
    Ok(Sheet { rows: rows.split_off(lead), origin: (lead, 0) })
}

/// 그 위치에서 행이 종단되는가.
///
/// **`\r` 단독도 개행이다.** 구 맥에서 저장한 csv 가 그렇고, `\n` 만 계수하면
/// 그런 파일의 선두 공백 행이 계수되지 않아 행 번호가 어긋난다. `\r\n` 은
/// 1회만 계수한다.
fn is_break(raw: &[u8], at: usize) -> bool {
    match raw[at] {
        b'\n' => true,
        b'\r' => raw.get(at + 1) != Some(&b'\n'),
        _ => false,
    }
}

/// UTF-8 이면 그대로, 아니면 CP949. 양자 모두 아니면 중단한다.
///
/// BOM 은 제거한다. 엑셀이 저장한 csv 는 BOM 을 부착하는데, 그것을 존치하면 첫
/// 열의 헤더 명칭 앞에 비가시 문자가 부착되어 **그 열만 탐색하지 못한다.**
fn decode(bytes: &[u8]) -> Result<String> {
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    if let Ok(text) = std::str::from_utf8(bytes) {
        return Ok(text.to_string());
    }
    /* **NUL 이 기재된 바이트는 문자가 아니다.** 표식이 부재하는 UTF-16 은
       바이트마다 NUL 이 개재하는데, CP949 는 그 바이트를 유효한 것으로 판독해
       `n\0a\0m\0e\0` 같은 열 명칭을 산출한다 — 훼손되었다고 진술하지 않고
       훼손된 문자를 반환하는 것이 이 모듈이 금한 바로 그 거동이다. */
    if bytes.contains(&0) {
        return Err(Error::new(ErrorCode::SheetEncodingUnknown));
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
    /// 시트 헤더에 기재된 명칭. 이것으로 열을 탐색한다.
    pub sheet: String,
    /// DB 열 명칭.
    pub column: String,
    pub ty: ValueType,
}

/// 시트 하나를 표 하나로 변환하는 방법.
#[derive(Debug, Clone)]
pub struct TablePlan {
    pub table: String,
    /// 사전으로 압축할 열들. 공백이면 헤더를 그대로 열 명칭으로 사용하고
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

/// 헤더를 SQLite 열 명칭으로. 문자·숫자·밑줄만 존치하고, 숫자로 개시되면 앞에
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
    /* **생성한 명칭이 유일할 때까지 번호를 증가시킨다.**
       번호를 1회만 부착하면 `x · x_2 · x` 가 `x · x_2 · x_2` 가 되어 표 생성이
       정지한다. 그리고 **SQLite 의 열 명칭은 대소문자를 구별하지 않으므로**
       `a · A` 도 충돌한다 — 영문 헤더에서 더 빈번하다. 양자 모두 정상적인
       양식인데 개방되지 않았다. */
    /* **직전까지 산출한 번호에서 재개한다.** 매회 1 부터 시도하면 동일한 명칭이
       k 개일 때 비용이 k² 이 된다 — 16,384 열이 전부 동일한 양식에서 개방에
       54초가 소요되었다. 번호를 명칭마다 보유하면 그 비용이 소멸한다. */
    let base = out.clone();
    let key = base.to_lowercase();
    let mut n = seen.get(&key).copied().unwrap_or(1);
    loop {
        out = if n == 1 { base.clone() } else { format!("{base}_{n}") };
        n += 1;
        if !seen.contains_key(&out.to_lowercase()) {
            break;
        }
    }
    seen.insert(key, n);
    // 점유한 명칭을 표시한다. 그 명칭이 차회 헤더로 출현하면 1 부터 시도한다.
    seen.entry(out.to_lowercase()).or_insert(1);
    out
}

/// 헤더 행을 (원본 문자, DB 열 명칭) 쌍으로.
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

/// 헤더에서 열의 위치를 탐색한다. **하나라도 부재하면 중단한다.**
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

/// 헤더를 그대로 열 명칭으로 사용하는 표. 값은 전부 문자로 수용한다.
pub fn build_plain(
    conn: &Connection,
    sheet: &Sheet,
    table: &str,
    indexes: &[Vec<String>],
) -> Result<(usize, Vec<(String, String)>)> {
    let header = column_names(sheet);
    check_width(conn, table, header.len())?;
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

    check_width(conn, table, plan.map.len())?;
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

    // 사전 표는 DB 열 명칭이 헤더와 상이하다. 화면이 사용하는 것은 DB 열 명칭이다.
    let header = plan.map.iter().map(|m| (m.sheet.clone(), m.column.clone())).collect();
    Ok((n, header))
}

/// 표 하나를 생성한 뒤 판명되는 것.
#[derive(Debug, Clone)]
pub struct Made {
    pub rows: usize,
    /// (시트 헤더, DB 열 명칭) 쌍. 부르는 곳이 화면 정의를 작성할 때 사용한다.
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

    /// **상한을 초과하는 열은 표를 생성하기 전에 진술한다.**
    ///
    /// 종전에는 생성이 `SheetTableFailed` 로 중단되었다 — 사유가 부재해, 열은
    /// 전부 묘사되는데 자료만 도달하지 않는 상태로 발현했다. 상한은 연결이
    /// 진술하는 값이라, 시험도 그 값을 변경해 판정한다.
    #[test]
    fn a_sheet_wider_than_the_engine_allows_says_so() {
        let conn = mem();
        conn.set_limit(rusqlite::limits::Limit::SQLITE_LIMIT_COLUMN, 6).unwrap();
        assert_eq!(column_limit(&conn).unwrap(), 6);

        let s = sheet(&[&["가", "나", "다", "라", "마", "바", "사"], &["1", "2", "3", "4", "5", "6", "7"]]);
        let err = build_plain(&conn, &s, "t", &[]).unwrap_err();
        assert_eq!(err.code, ErrorCode::SheetTooWide);
        assert_eq!(err.params.get("columns").map(String::as_str), Some("7"));
        assert_eq!(err.params.get("limit").map(String::as_str), Some("6"));

        // 상한 이내는 그대로 생성된다.
        let s = sheet(&[&["가", "나", "다", "라", "마", "바"], &["1", "2", "3", "4", "5", "6"]]);
        assert_eq!(build_plain(&conn, &s, "t", &[]).unwrap().0, 1);
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

    /// **동일한 명칭이 다수여도 비용이 선형이다.**
    ///
    /// 매회 1 부터 번호를 시도하면 k 개의 동일 명칭에서 비용이 k² 이 된다 —
    /// 16,384 열이 전부 동일한 양식에서 개방에 54초가 소요되었다. 이 시험은
    /// 그 비용이 재유입되면 정지한다.
    #[test]
    fn a_thousand_identical_headers_cost_no_more_than_a_thousand() {
        let header: Vec<&str> = vec!["같음"; 4000];
        let s = sheet(&[&header]);
        let started = std::time::Instant::now();
        let names: Vec<String> = column_names(&s).into_iter().map(|(_, n)| n).collect();
        let took = started.elapsed();
        assert_eq!(names.len(), 4000);
        assert_eq!(names[3999], "같음_4000");
        assert!(took.as_secs() < 2, "{took:?} 가 소요되었습니다 — 비용이 선형이 아닙니다");
    }

    /// **날짜로 환산할 수 없는 수에서 중단하지 않는다.**
    ///
    /// 엑셀은 날짜 서식을 임의의 수에 부여할 수 있다. 그 수가 극단이면 환산이
    /// 패닉하고, 그것은 창과 브라우저 양측에서 포착되지 않는다.
    #[test]
    fn a_date_cell_that_is_not_a_date_does_not_stop_everything() {
        use calamine::{Data, ExcelDateTime, ExcelDateTimeType::*};
        // 환산하지 않고 수 그대로 기재한다. 중단하지 않는 것이 요점이다.
        let huge = Data::DateTime(ExcelDateTime::new(1e300, DateTime, false));
        assert!(cell(&huge).starts_with('1'), "{}", cell(&huge));
        assert!(!cell(&huge).contains('-'), "날짜로 환산했습니다");
        let infinite = Data::DateTime(ExcelDateTime::new(f64::INFINITY, DateTime, false));
        assert_eq!(cell(&infinite), "inf");

        /* **0 이하는 날짜가 아니다.** 환산하면 상이한 수들이 전부
           `1899-12-31` 이라는 동일한 문자로 병합되어 자료가 소멸한다. */
        for v in [0.0, -1.0, -693_593.0, -1e9] {
            let at = Data::DateTime(ExcelDateTime::new(v, DateTime, false));
            assert_eq!(cell(&at), number(v), "{v} 를 날짜로 환산했습니다");
        }

        // 날짜가 부재하고 시각만 기재된 셀은 시각만 묘사한다.
        let noon = Data::DateTime(ExcelDateTime::new(0.5, DateTime, false));
        assert_eq!(cell(&noon), "12:00:00");

        // 경과 시간이 포화해 허위 시간을 산출하지 않는다.
        let forever = Data::DateTime(ExcelDateTime::new(1e300, TimeDelta, false));
        assert!(!cell(&forever).contains(':'), "{}", cell(&forever));
    }

    /// **열쇠가 충돌하면 표가 생성되지 않는다.**
    ///
    /// 번호를 1회만 부착하면 `x · x_2 · x` 가 `x_2` 를 두 번 산출하고, SQLite 의
    /// 열 명칭은 대소문자를 구별하지 않으므로 `a · A` 도 충돌한다. 양자 모두
    /// 정상적인 양식인데 개방되지 않았다.
    #[test]
    fn column_names_stay_unique_even_when_they_collide() {
        let s = sheet(&[&["x", "x_2", "x"]]);
        let names: Vec<String> = column_names(&s).into_iter().map(|(_, n)| n).collect();
        assert_eq!(names, ["x", "x_2", "x_3"]);

        let s = sheet(&[&["a", "A", "a"]]);
        let names: Vec<String> = column_names(&s).into_iter().map(|(_, n)| n).collect();
        assert_eq!(names, ["a", "A_2", "a_3"]);

        // 그 명칭으로 표가 실제로 생성되어야 한다.
        let conn = mem();
        let s = sheet(&[&["x", "x_2", "x"], &["1", "2", "3"]]);
        let plan = TablePlan { table: "t".into(), map: Vec::new(), indexes: Vec::new() };
        build(&conn, &[(plan, s)], "-").expect("열쇠가 충돌해 표가 생성되지 않았습니다");
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
    /// **csv 의 공백 행이 소거되지 않는다.**
    ///
    /// 소거하면 조작부가 표시하는 행 번호가 원본 파일과 어긋나고, 그 번호를
    /// 참조해 헤더를 지정하는 사람은 전혀 상이한 행을 판독한다. xlsx 는
    /// `origin` 으로 보정한다. 중간의 공백 행은 자료의 일부라 행으로 복원한다.
    #[test]
    fn a_blank_line_in_a_csv_keeps_its_place() {
        let csv = "제목\n\n\n이름,점수\n가,90\n\n나,80\n";
        let s = from_csv(csv.as_bytes()).unwrap();
        assert_eq!(s.rows.len(), 7, "{:?}", s.rows);
        assert!(s.rows[1].is_empty() && s.rows[2].is_empty() && s.rows[5].is_empty());
        assert_eq!(s.rows[3], ["이름", "점수"]);
        assert_eq!(s.rows[6], ["나", "80"]);
    }

    /// **공백 표식과 동일한 문자가 셀에 기재되어도 분리된다.**
    ///
    /// 그 문자가 자료에 잔존하면 값 선택 목록에 두 항목이 출현하고 양자가
    /// 동일한 값으로 전송된다 — 선택한 것과 상이한 행이 통과한다.
    #[test]
    fn a_cell_cannot_carry_the_mark_that_means_blank() {
        let mark = crate::grid::BLANK_TOKEN;
        let csv = format!("값
{mark}
x
");
        let s = from_csv(csv.as_bytes()).unwrap();
        assert_eq!(s.rows[1], [""], "표식이 자료에 잔존합니다");
        assert_eq!(s.rows[2], ["x"]);

        // NUL 도 제거한다. 열 검색이 그 위치에서 정지한다.
        let s = from_csv("값
a\0b
".as_bytes()).unwrap();
        assert_eq!(s.rows[1], ["ab"]);
    }

    /// **선두의 공백 행은 소거하고 그 수를 보유한다.**
    ///
    /// xlsx 는 상자가 그렇게 반환하므로 양측의 거동이 그래야 일치한다. 복원한
    /// 채로 두면 추정에 실패했을 때 화면이 적용하는 첫 행이 공백 행에 적중해,
    /// 열 명칭이 전부 부재한 표가 묘사되고 실제 헤더가 자료 1행이 된다.
    #[test]
    fn leading_blank_lines_become_an_origin_not_rows() {
        let s = from_csv("

이름,점수
가,90
".as_bytes()).unwrap();
        assert_eq!(s.origin.0, 2);
        assert_eq!(s.rows.len(), 2);
        assert_eq!(s.rows[0], ["이름", "점수"]);
    }

    /// 따옴표 내부의 개행은 레코드 하나가 복수 행을 점유한다. 그만큼 가산하지
    /// 않으면 그 이후의 행이 전부 밀린다.
    #[test]
    fn a_newline_inside_a_quoted_cell_does_not_shift_the_rows() {
        let csv = "이름,비고\n가,\"한 행\n두 행\"\n\n나,끝\n";
        let s = from_csv(csv.as_bytes()).unwrap();
        assert_eq!(s.rows.len(), 5, "{:?}", s.rows);
        assert!(s.rows[3].is_empty(), "{:?}", s.rows);
        assert_eq!(s.rows[4], ["나", "끝"]);
    }


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
