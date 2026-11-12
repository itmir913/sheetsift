//! **셀 하나를 무엇으로 판독하는가.** 이 저장소의 정본이다.
//!
//! 동일한 판정이 세 곳에서 수행된다 — 구조를 식별하는 층(Rust), 표를 생성하는
//! 층(Rust), 조회 엔진(SQL). **판정이 분기하면 묵시적으로 상이한 수가
//! 산출된다.** 한쪽이 수치로 판정한 값이 타방에서 문자로 취급되면 정렬이
//! 자료와 상이한 순서를 산출하고, 한쪽이 판독한 수를 타방이 판독하지 못하면
//! 행은 존치되고 수만 소멸한다. 양자 모두 오류로 출현하지 않는다.
//!
//! 그래서 문을 둘로만 배치한다.
//!
//! - **무엇을 수로 보는가** — [`is_number`]. SQL 거울은 [`is_number_sql`] 이고,
//!   양자가 동일한 답을 산출하는지는 이 파일의 시험이 문자를 생성해 전수
//!   대조한다.
//! - **그 문자를 수로 어떻게 판독하는가** — [`as_real`]·[`as_int`]. 수로 보지
//!   않는 값은 판독하지 않는다(`None`). 판독하는 쪽이 자체 파서를 보유하면
//!   `is_number` 가 기각한 형태가 거기서 수용되어 분기한다.
//! - **무엇을 공백으로 보는가** — [`is_blank`]. SQL 거울은 [`is_blank_sql`] 이고,
//!   이것도 전수 대조한다.
//!
//! **활성화해야 포함되는 층에 배치할 수 없다.** 항상 포함되는 `sheet`·`grid`
//! 가 사용하므로 여기는 항상 포함된다(`tests/features.rs`).

/// 선후에서 제거하는 문자. **Rust 와 SQL 이 동일한 집합을 제거해야 한다.**
///
/// `str::trim` 은 유니코드의 공백 속성을 사용하고 SQLite 의 `TRIM` 은 제거할
/// 문자를 열거받는다. 그래서 집합을 **글자로 열거하고 양측이 그것만 참조한다**
/// — 일방이 상이한 집합을 제거하면 `" "` 한 칸이 한쪽에는 공백이고 타방에는
/// 값이 존재하는 칸이 된다. 그 칸은 정렬의 「공백은 후위」에 포착되지 않고,
/// `not_blank` 를 통과하고, 범위 필터에서는 탈락한다.
///
/// 열거한 것은 유니코드가 공백으로 배정한 문자 전부다(`White_Space`).
const SPACE: [u32; 25] = [
    0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x20, 0x85, 0xA0, 0x1680, 0x2000, 0x2001, 0x2002, 0x2003,
    0x2004, 0x2005, 0x2006, 0x2007, 0x2008, 0x2009, 0x200A, 0x2028, 0x2029, 0x202F, 0x205F,
    0x3000,
];

/// 선후의 공백을 제거한다. [`SPACE`] 가 열거한 문자만 제거한다.
pub fn trim(s: &str) -> &str {
    s.trim_matches(|c: char| SPACE.contains(&(c as u32)))
}

/// 선후의 공백을 제거하는 SQL 식. [`trim`] 의 거울이다.
///
/// **반출한다**(`is_blank_sql` 와 동일한 사유). 조회 문을 자체로 구성하는 앱이
/// 이 상자와 동일한 공백 집합을 제거해야 한다.
///
/// 제거할 문자를 `char()` 로 구성한다 — 제어 문자를 SQL 문자열에 그대로
/// 기재하면 로그와 편집기에서 판독되지 않는다.
pub fn trim_sql(expr: &str) -> String {
    let codes = SPACE.iter().map(u32::to_string).collect::<Vec<_>>().join(",");
    format!("TRIM({expr}, char({codes}))")
}

/* ── 공백 ───────────────────────────────────────────────────────── */

/// 값이 부재하는 셀인가. `blank` 는 양식이 "없음" 을 기재하는 문자다(`-` 등).
pub fn is_blank(s: &str, blank: &str) -> bool {
    let t = trim(s);
    t.is_empty() || t == blank
}

/// 값이 부재하는 셀을 식별하는 SQL 조건. [`is_blank`] 의 거울이다.
///
/// **무엇을 "부재" 로 간주할지는 자료가 결정한다**(`blank`). 여기에 문자를
/// 고정하면, 자료가 `–` 나 `N/A` 로 변경되었을 때 값 선택 목록과 필터가 동일한
/// 파일 내부에서 분기한다 — 후보에는 `(비어 있음)` 이 표시되는데 그것을 선택하면
/// 0건이 되고, 어떠한 오류도 발생하지 않는다.
///
/// **반출한다 — 이 상자 외부에서 조회 문을 구성하는 앱이 존재한다.**
/// 비공개로 두었을 때 받는 앱이 `x IS NULL OR CAST(x AS TEXT) = ''` 를 자체로
/// 기재했는데, 그것은 공백을 제거하지도 않고 자료의 공백 표식을 인지하지도
/// 않는다. 반입한 값이 절단되어 도달하는 동안에만 양측의 판정이 일치한다.
pub fn is_blank_sql(base: &str, blank: &str) -> String {
    // 자료가 결정하는 문자라 따옴표가 진입할 여지는 희소하나, SQL 에 그대로 주입하므로 차단한다.
    let mark = blank.replace('\'', "''");
    let t = trim_sql(base);
    format!("({base} IS NULL OR {t} = '' OR {t} = '{mark}')")
}

/* ── 무엇을 수로 보는가 ─────────────────────────────────────────── */

/// 숫자로 판독되는가.
///
/// **규칙을 문자로 기재한다.** 파서에 위임하면 그 파서가 수용하는 모든 형태가
/// 규칙이 되는데(`inf`·`NaN`·`1e5`), 조회 엔진은 SQL 로 동일한 판정을
/// 수행하므로 양측이 어긋난다.
///
/// **`NaN` 과 `inf` 는 문자다. 그 글자는 자료가 아니라 이 상자가 산출한다** —
/// 엑셀의 셀에 유한하지 않은 `f64` 가 도달하면 `sheet` 의 `number` 가 그것을
/// `format!` 로 문자화해 `"NaN"`·`"inf"` 라는 글자를 기재하고, 날짜 서식이
/// 부여된 극단의 수도 거기로 경유한다. 그 글자를 수로 판정하면 **환산할 수
/// 없었던 수가 수치 열의 어느 위치에 삽입된다.** 수가 아니라고 판정하면 수치
/// 열에서 공백이 되고 판독하지 못한 칸으로 계수된다([`crate::sheet::Made`] 의
/// `unread`) — 그쪽이 진술이 존재하는 경로다.
///
/// 지수 표기(`1e5`)도 문자다. 그 글자도 자료에서만 유래한다 — Rust 의 `Display`
/// 는 `f64` 를 지수로 기재하지 않으므로(`1e300` 은 301 자리로 기재된다) 수치
/// 셀에서 그 글자가 산출되는 경로는 부재하다. 엑셀이 수로 수용한 셀은 수 그대로
/// 도달하므로, 그 글자가 기재된 셀은 수를 의도한 것이 아니다.
///
/// 규칙: 선후의 공백과 천 단위 구분을 제거한 뒤 — 부호는 선두에 1회까지,
/// 소수점은 1회까지, 숫자가 하나 이상, 그 외의 문자는 부재.
///
/// **쉼표가 기재되어 있으면 천 단위 구분으로 유효해야 한다.** 제거만 수행하면
/// `1,2,3` 이 123 으로 판정되어 그 위치로 정렬된다 — 사람이 열거를 기재한 셀을
/// 수치로 간주하는 셈이다. 쉼표가 부재하는 값은 그 검증과 무관하므로, 자릿수가
/// 다수인 식별자(`20231234`)는 그대로 수치다.
pub fn is_number(s: &str) -> bool {
    let raw = trim(s);
    if raw.contains(',') && !grouped(raw) {
        return false;
    }
    let t = raw.replace(',', "");
    let body = t.strip_prefix(['+', '-']).unwrap_or(&t);
    body.chars().any(|c| c.is_ascii_digit())
        && body.chars().all(|c| c.is_ascii_digit() || c == '.')
        && body.matches('.').count() <= 1
}

/// 쉼표가 천 단위 구분으로 유효하게 기재되었는가.
///
/// 정수부의 선두 묶음은 1~3 자리, 그 이후의 묶음은 정확히 3 자리다. 소수부에는
/// 기재되지 않는다. **소수점 이후가 공백인 것(`1,000.`)은 수용한다** — 쉼표가
/// 부재하는 `5.` 를 수용하고 있어, 여기서 기각하면 쉼표 유무로 판정이 분기한다.
fn grouped(raw: &str) -> bool {
    let body = raw.strip_prefix(['+', '-']).unwrap_or(raw);
    let (int, frac) = match body.split_once('.') {
        Some((i, f)) => (i, f),
        None => (body, ""),
    };
    if frac.contains(',') || !frac.chars().all(|c| c.is_ascii_digit()) {
        return false;
    }
    let mut groups = int.split(',');
    let first = groups.next().unwrap_or_default();
    if first.is_empty() || first.len() > 3 || !first.chars().all(|c| c.is_ascii_digit()) {
        return false;
    }
    groups.all(|g| g.len() == 3 && g.chars().all(|c| c.is_ascii_digit()))
}

/// 전체가 수치의 형태인지 판정하는 SQL 조건. [`is_number`] 의 거울이다.
///
/// 변환 결과가 0 이 아닌지로 판정하면 `3 apples`·`12abc`·`7점` 처럼 수치가
/// **접두사로** 기재된 문자가 그 수치의 위치로 정렬된다 — SQLite 의 변환이
/// 접두사를 수용하기 때문이다. GLOB 에는 반복을 지정하는 수단이 부재하므로
/// "포함하지 않는가" 로 판정하고, 횟수는 문자를 제거한 길이로 판정한다.
pub(crate) fn is_number_sql(base: &str) -> String {
    let bare = format!("REPLACE({}, ',', '')", trim_sql(base));
    let count = |ch: &str| format!("LENGTH({bare}) - LENGTH(REPLACE({bare}, '{ch}', ''))");
    format!(
        "{bare} GLOB '*[0-9]*' \
         AND NOT {bare} GLOB '*[^0-9.+-]*' \
         AND NOT {bare} GLOB '?*[+-]*' \
         AND {} <= 1 \
         AND NOT ({})",
        count("."),
        misgrouped(base)
    )
}

/// 쉼표가 천 단위 구분으로 유효하지 않게 기재되었는가. [`grouped`] 의 거울이다.
///
/// GLOB 에는 반복을 지정하는 수단이 부재하므로 **유효한 형태를 기재하지 않고,
/// 유효하지 않은 국소 형태를 열거해 기각한다** — 쉼표 하나하나가 숫자 3 자리를
/// 선행하고 후행하는지만 판정하면 전체의 유효성이 따라온다.
fn misgrouped(base: &str) -> String {
    let t = trim_sql(base);
    [
        // 쉼표 이후가 숫자 3 자리가 아니다.
        "GLOB '*,[0-9][0-9][0-9][0-9]*'",
        "GLOB '*,[0-9][^0-9]*'",
        "GLOB '*,[0-9][0-9][^0-9]*'",
        "GLOB '*,[^0-9]*'",
        "GLOB '*,'",
        "GLOB '*,[0-9]'",
        "GLOB '*,[0-9][0-9]'",
        // 쉼표 이전이 숫자 1~3 자리가 아니다.
        "GLOB '*[0-9][0-9][0-9][0-9],*'",
        "GLOB ',*'",
        "GLOB '*[^0-9],*'",
        // 소수부에 기재되었다.
        "GLOB '*.*,*'",
    ]
    .iter()
    .map(|p| format!("{t} {p}"))
    .collect::<Vec<_>>()
    .join(" OR ")
}

/// 수치로 판독되지 않는 값을 후위로 배치하는 정렬 열쇠.
///
/// **방향과 무관하게 후위다.** SQLite 의 변환은 판독하지 못한 문자를 `0` 으로
/// 산출한다. 그대로 정렬하면 `총 44850점` 같은 합계 행이 `0` 과 `1` 사이에
/// 삽입되는데, 표를 판독하는 사람에게는 그것이 0점으로 보인다.
pub(crate) fn not_a_number_sql(base: &str) -> String {
    format!("(CASE WHEN {} THEN 0 ELSE 1 END) ASC", is_number_sql(base))
}

/// 수치로 변환하는 SQL 식. [`as_real`] 의 거울이다.
///
/// **판정할 때 제거한 것을 변환할 때도 제거해야 한다** — `1,200` 을 숫자로
/// 판정해 두고 변환만 그대로 수행하면 SQLite 가 `1` 로 산출해 `50` 보다 앞에
/// 배치한다.
pub(crate) fn number_sql(base: &str) -> String {
    format!("CAST(REPLACE({}, ',', '') AS REAL)", trim_sql(base))
}

/* ── 그 문자를 수로 어떻게 판독하는가 ───────────────────────────── */

/// 셀의 문자를 실수로. **[`is_number`] 가 수로 보는 것만 판독한다.**
///
/// 자체 파서를 사용하면 `1,200` 을 판독하지 못하고(쉼표), `1e5`·`inf`·`NaN` 을
/// 판독한다 — 양측이 판정과 어긋난다.
pub fn as_real(s: &str) -> Option<f64> {
    if !is_number(s) {
        return None;
    }
    let t = trim(s).replace(',', "");
    // `.5` 와 `5.` 를 수용한다 — 판정이 수용하고 Rust 의 파서도 수용한다.
    t.parse::<f64>().ok().filter(|v| v.is_finite())
}

/// `REAL` 열에 보관해도 **문자로 환원할 때 지수 표기가 되지 않는가.**
///
/// SQLite 는 `REAL` 을 문자로 환원할 때 규모가 극단이면 `1.0e+17` 처럼
/// 기재한다. 그 글자는 [`is_number`] 가 **문자로 판정한다**(소유자의 결정이다
/// — 지수 표기는 수가 아니다). 그대로 보관하면 구축하는 쪽은 그 셀을 수로
/// 간주했는데 조회하는 쪽은 수가 아니라고 판정해, 동일한 셀이 경로에 따라 두
/// 가지로 판독된다.
///
/// **그래서 판정을 완화하지 않고 보관을 거부한다.** 완화하면 지수 표기를 수로
/// 수용하는 셈이고, 그 결정은 이미 반대로 내려져 있다. 거부된 칸은 공백이
/// 되고 판독하지 못한 칸으로 계수되어([`crate::sheet::Made`] 의 `unread`)
/// 사람에게 진술된다 — 조용히 어긋나는 쪽보다 양호하다.
///
/// 경계는 **현재의 SQLite 보다 협소하게 기재한다**(1e-4 이상, 1e15 미만).
/// 환원 방법은 판마다 상이하다 — 3.53 은 1e16 을 자리 그대로 기재하나 종전의
/// 판은 지수로 기재했고, 받는 앱이 반입하는 판을 이 상자가 고정하지 못한다.
/// 협소한 쪽으로 기재하면 판이 변동해도 거부가 과다해질 뿐 어긋나지 않는다.
/// 수호하는 것은 경계의 수가 아니라 **보관한 것이 수로 환원된다**는 성질이고,
/// 시험은 그 성질을 SQLite 에 질의해 확인한다.
///
/// 요강의 자료는 그 규모에 도달하지 않는다. 도달하지 않는 것을 사유로 미결로
/// 존치하지 않을 뿐이다.
pub fn fits_real(v: f64) -> bool {
    if !v.is_finite() {
        return false;
    }
    if v == 0.0 {
        return true;
    }
    let a = v.abs();
    a < 1e15 && a >= 1e-4
}

/// 셀의 문자를 정수로. **반올림도 포화도 수행하지 않는다.**
///
/// 소수부가 존재하는 값(`2.5`)을 정수 열에 수용하면 조용히 `3` 이 되고, `f64`
/// 의 범위를 초과하는 값은 `as i64` 가 조용히 포화시킨다 — 양자 모두 화면에
/// 출현하는 수가 자료와 상이해지는데 오류로 출현하지 않는다. 판독하지 못하는
/// 것으로 취급해 부르는 곳에 통지하게 한다.
pub fn as_int(s: &str) -> Option<i64> {
    let v = as_real(s)?;
    if v.fract() != 0.0 {
        return None;
    }
    // `as i64` 는 범위를 초과하면 포화시킨다. 수용 가능한 범위인지 선행 판정한다.
    if v < -(2f64.powi(63)) || v >= 2f64.powi(63) {
        return None;
    }
    Some(v as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 양측을 전수 대조할 표본. **명칭을 부여한 표본만으로는 어긋나는 위치를
    /// 발견하지 못한다** — 쉼표의 유효성은 국소 형태의 열거로 기재되어 있어,
    /// 그 열거가 한 항목 누락된 것은 특정 길이의 특정 조합에서만 발현한다.
    /// 공백도 생성한다 — 양측이 동일한 집합을 제거해야 한다.
    fn samples() -> Vec<String> {
        let named = [
            "0", "-0", "007", "1,200", "+5", "3.14", ".5", "5.", "-42", "9007199254740993",
            "1e5", "1E5", "1E+05", "2.5E-05", "1e-5", "2024-01-01", "010-1234-5678", "1 200", "1.2.3", "3 apples", "12abc",
            "7점", "총 44850점", "--5", "+", ".", "inf", "-inf", "NaN", "0x10", "１２",
            // 천 단위 구분. 열거를 기재한 셀이 수치로 판정되어서는 안 된다.
            "1,2,3", "1,23", "12,34,567", "1234,567", "1,000,000", "-1,234.50", "1,000.",
            "1,000,00", ",123", "1,,000", "12,3456", "1.000,50",
            // 선후의 공백. 양측이 동일하게 제거해야 한다.
            " 90", "90 ", "\t90", "90\n", " ", "\t", "\u{a0}90", "\u{3000}", "\u{a0}", "  -  ",
            " 가 ", "가",
        ];
        let alphabet = b"01,.+- ";
        let mut out: Vec<String> = named.iter().map(|s| (*s).to_string()).collect();
        for len in 1..=5u32 {
            for n in 0..alphabet.len().pow(len) {
                let mut at = n;
                let bytes: Vec<u8> = (0..len)
                    .map(|_| {
                        let c = alphabet[at % alphabet.len()];
                        at /= alphabet.len();
                        c
                    })
                    .collect();
                out.push(String::from_utf8(bytes).unwrap());
            }
        }
        out
    }

    fn with_samples(expr_of: impl Fn(&str) -> String) -> Vec<(String, bool)> {
        let samples = samples();
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE t(v TEXT);").unwrap();
        {
            let mut ins = conn.prepare("INSERT INTO t VALUES (?)").unwrap();
            for v in &samples {
                ins.execute([v]).unwrap();
            }
        }
        let sql = format!("SELECT v, ({}) FROM t t", expr_of("t.v"));
        let mut stmt = conn.prepare(&sql).unwrap();
        let mut rows = stmt.query([]).unwrap();
        let mut out = Vec::new();
        while let Some(r) = rows.next().unwrap() {
            out.push((r.get(0).unwrap(), r.get::<_, i64>(1).unwrap() == 1));
        }
        out
    }

    /// **무엇이 수치인가를 Rust 와 SQL 이 동일하게 판정한다.**
    ///
    /// 어긋나면 한쪽이 수치로 판정한 값이 타방에서 문자로 취급되어 정렬이
    /// 자료와 상이한 순서를 산출한다.
    #[test]
    fn rust_and_sql_agree_on_what_a_number_is() {
        let differ: Vec<String> = with_samples(is_number_sql)
            .into_iter()
            .filter(|(v, sql)| *sql != is_number(v))
            .map(|(v, sql)| format!("{v:?}: SQL={sql} Rust={}", is_number(&v)))
            .collect();
        assert!(differ.is_empty(), "판정이 어긋납니다: {differ:#?}");
    }

    /// **무엇이 공백인가를 Rust 와 SQL 이 동일하게 판정한다.**
    ///
    /// 어긋나면 `" "` 한 칸이 한쪽에는 공백이고 타방에는 값이 존재하는 칸이
    /// 된다 — 정렬의 「공백은 후위」에 포착되지 않고, `not_blank` 를 통과하고,
    /// 범위 필터에서는 탈락한다.
    #[test]
    fn rust_and_sql_agree_on_what_is_blank() {
        for blank in ["-", "", "N/A"] {
            let differ: Vec<String> = with_samples(|e| is_blank_sql(e, blank))
                .into_iter()
                .filter(|(v, sql)| *sql != is_blank(v, blank))
                .map(|(v, sql)| format!("{v:?}(blank={blank:?}): SQL={sql} Rust={}", is_blank(&v, blank)))
                .collect();
            assert!(differ.is_empty(), "판정이 어긋납니다: {differ:#?}");
        }
    }

    /// **판정한 수를 SQL 과 Rust 가 동일한 값으로 판독한다.**
    ///
    /// 판독하는 쪽이 자체 파서를 보유하면 `1,200` 이 한쪽에서 1200 이고
    /// 타방에서 판독 불가가 된다 — 행은 존치되고 수만 소멸한다.
    #[test]
    fn rust_and_sql_read_the_same_number() {
        let samples = samples();
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE t(v TEXT);").unwrap();
        {
            let mut ins = conn.prepare("INSERT INTO t VALUES (?)").unwrap();
            for v in &samples {
                ins.execute([v]).unwrap();
            }
        }
        let sql = format!(
            "SELECT v, {}, ({}) FROM t t",
            number_sql("t.v"),
            is_number_sql("t.v")
        );
        let mut stmt = conn.prepare(&sql).unwrap();
        let mut rows = stmt.query([]).unwrap();
        let mut differ: Vec<String> = Vec::new();
        while let Some(r) = rows.next().unwrap() {
            let v: String = r.get(0).unwrap();
            let num: f64 = r.get(1).unwrap();
            let is_num: bool = r.get::<_, i64>(2).unwrap() == 1;
            match (is_num.then_some(num), as_real(&v)) {
                (Some(a), Some(b)) if (a - b).abs() < 1e-9 => {}
                (None, None) => {}
                (a, b) => differ.push(format!("{v:?}: SQL={a:?} Rust={b:?}")),
            }
        }
        assert!(differ.is_empty(), "판독이 어긋납니다: {differ:#?}");
    }

    /// **판정이 갈라졌던 입력들을 한 자리에서 판정한다.**
    ///
    /// `NaN`·`inf` 의 글자는 자료가 아니라 이 상자가 산출한다(`sheet` 의
    /// `number`). 수로 판정하면 환산할 수 없었던 수가 수치 열의 어느 위치에
    /// 삽입되므로 문자로 판정하고, 수치 열에서는 판독하지 못한 칸으로 계수한다.
    /// 세 층이 전부 이 판정을 추종하는지는 상단의 전수 대조가 확인한다.
    #[test]
    fn the_inputs_that_used_to_split_now_get_one_answer() {
        // 수다.
        for s in ["1,200", "90", "85.5", "-42", "+5", "1,000.", ".5"] {
            assert!(is_number(s), "{s:?} 는 수다");
            assert!(as_real(s).is_some(), "{s:?} 를 판독한다");
        }
        // 수가 아니다. 판정도 판독도 동일하게 기각한다.
        for s in [
            "1,2,3", "1,23", "12,3456", ",123", "1e5", "1E5", "1E+05", "2.5E-05", "1e-5",
            "NaN", "inf", "-inf", "총 44850점", "7점",
        ] {
            assert!(!is_number(s), "{s:?} 는 수가 아니다");
            assert_eq!(as_real(s), None, "{s:?}");
        }
        // 빈칸이다. 절단한 뒤 판정하므로 `" - "` 도 "없음" 표기다.
        for s in [" ", "", "\t", " - ", "-", "\u{a0}"] {
            assert!(is_blank(s, "-"), "{s:?} 는 빈칸이다");
            assert!(!is_number(s), "{s:?} 는 수가 아니다");
        }
    }

    /// **쉼표가 기재된 수를 판독한다.** 자체 파서(`str::parse`)는 판독하지
    /// 못하고 `None` 을 산출한다 — 그것을 공백으로 수용하면 행은 존치되고 수만
    /// 소멸한다.
    #[test]
    fn a_number_written_with_separators_is_read() {
        assert_eq!(as_real("1,200"), Some(1200.0));
        assert_eq!(as_int("1,200"), Some(1200));
        assert_eq!("1,200".parse::<f64>().ok(), None);
    }

    /// **수로 보지 않는 형태는 판독하지 않는다.** 파서가 수용하는 형태를 그대로
    /// 판독하면 판정과 어긋난다.
    #[test]
    fn what_is_not_a_number_is_not_read() {
        for s in ["1e5", "inf", "NaN", "0x10", "1,2,3", "총 44850점", ""] {
            assert_eq!(as_real(s), None, "{s:?}");
            assert_eq!(as_int(s), None, "{s:?}");
        }
    }

    /// **정수 열은 반올림도 포화도 수행하지 않는다.** 조용히 상이한 수가
    /// 산출되는 쪽보다 판독하지 못한 것으로 통지하는 쪽이 양호하다.
    #[test]
    fn an_integer_is_neither_rounded_nor_saturated() {
        assert_eq!(as_int("2"), Some(2));
        assert_eq!(as_int("2.0"), Some(2));
        // 반올림하면 2.5 가 3 이 된다.
        assert_eq!(as_int("2.5"), None);
        assert_eq!(as_int("-0.5"), None);
        // `as i64` 는 범위를 초과하면 i64::MAX 로 포화시킨다.
        assert_eq!(as_int("99999999999999999999999"), None);
        assert_eq!(as_int("-99999999999999999999999"), None);
    }

    /// **`REAL` 열에 보관한 수는 조회할 때 수로 판독된다.** 보관하는 쪽이
    /// 수로 간주한 셀을 조회하는 쪽이 문자로 판정하면, 동일한 셀이 경로에
    /// 따라 두 가지로 판독된다 — 어떠한 오류도 발생하지 않는다.
    ///
    /// 환원 방법이 판마다 상이하므로 **동치가 아니라 함의를 수호한다.**
    /// [`fits_real`] 이 수용한 것은 SQLite 가 어떤 판으로 환원해도
    /// [`is_number`] 가 수로 판정해야 한다. 거부가 과다한 것은 통지되는
    /// 경로이므로 결함이 아니다.
    #[test]
    fn what_is_stored_as_a_real_is_read_back_as_a_number() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        let mut cases = vec![
            0.0, 2.5, -2.5, 1234.5, 1e-4, -1e-4, 9.999e-5, 1e-5, 1e14, -1e14,
            999_999_999_999_999.0, 1e15, -1e15, 1e16, 1e17, 1e300, 1e-300,
            123_456_789_012_345.6, 0.000_123_456_789,
        ];
        for e in -320..=308 {
            cases.push(10f64.powi(e));
        }
        for v in cases {
            let text: String =
                conn.query_row("SELECT CAST(? AS TEXT)", [v], |r| r.get(0)).unwrap();
            if fits_real(v) {
                assert!(is_number(&text), "{v:e} 를 SQLite 가 {text:?} 로 기재한다");
                assert_eq!(as_real(&text).is_some(), true, "{text:?}");
            }
        }
        // 유한하지 않은 수는 보관하지 않는다. 그 글자(`Inf`·`NaN`)도 수가 아니다.
        assert!(!fits_real(f64::NAN));
        assert!(!fits_real(f64::INFINITY));
        assert!(!fits_real(f64::NEG_INFINITY));
        // 기재해 둔 경계. 협소한 쪽이라 현재의 SQLite 가 자리 그대로 기재하는
        // 규모(1e15·1e16)도 거부한다.
        assert!(fits_real(1e-4) && fits_real(999_999_999_999_999.0));
        assert!(!fits_real(1e-5) && !fits_real(1e15) && !fits_real(1e16));
    }
}
