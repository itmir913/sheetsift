//! **임의의 표에서 구조를 판별한다.** 헤더가 어느 행인지, 2행인지, 어느 열이
//! 숫자인지.
//!
//! 정의 파일을 수동 작성하는 작업의 착수 단계다. 사람이 기재하면 오류가 발생할
//! 여지가 없으나, 임의의 xlsx 를 개방하려면 수령한 위치에서 판별해야 한다.
//!
//! ## 확신이 부재하면 "미상" 이다
//!
//! 이 모듈이 준수하는 것은 하나다. **추정한 것을 확정한 것처럼 반환하지
//! 않는다.** 헤더 행을 오선정하면 자료 1행이 열 명칭이 되고, 숫자 열을
//! 문자로 판정하면 정렬이 1 · 10 · 2 가 된다. 양자 모두 묵시적이다 — 화면이
//! 개방되고, 값이 노출되고, 순서만 상이하다.
//!
//! 그래서 [`Kind::Unknown`] 과 [`Header::Unknown`] 이 존재한다. 수령한 곳이
//! 사람에게 질의하든 기본값을 사용하든 그것은 그쪽 소관이고, **여기서 선정하지
//! 않는다.**
//!
//! ## 여기서 판별 불가한 것
//!
//! 의미는 인지하지 않는다. 어느 열이 명칭이고 어느 열이 점수인지, 어느 열을
//! 사전으로 압축해야 유리한지는 문자만 참조해 판별할 수 없다. 이 모듈은
//! **구조**까지다.

use crate::sheet::Sheet;

/// 헤더 하단으로 몇 행까지 조사하는가. 자료의 구조를 판정하는 데 사용한다.
const SAMPLE: usize = 200;

/// 헤더 탐색을 위해 상단에서 점검하는 행 수. 안내 문장 수 행이 전단에 부착된 양식이 빈번하다.
const LOOK: usize = 20;

/// 열 하나가 무엇을 수용하는가.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// 공백이 아닌 값이 **전부** 숫자다.
    Number,
    /// 공백이 아닌 값이 **하나도** 숫자가 아니다.
    Text,
    /// 혼재하거나, 판정할 값이 부재한다. **선정하지 않는다.**
    Unknown,
}

/// 헤더가 어디인가.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Header {
    /// `at` 행부터 `rows` 행이 헤더다.
    ///
    /// 2행 이상이면 상단 행은 묶음 명칭이고, 병합된 셀이라 우측으로 확산
    /// 기재되어 있다. **행 수를 둘로 한정하지 않는다** — 이 그릇은 임의의
    /// 양식을 수용해야 하고, 몇 행인지는 최종적으로 사용자가 지정한다.
    Found { at: usize, rows: usize },
    /// 판별 불가. **첫 행으로 추정하지 않는다.**
    Unknown,
}

impl Header {
    /// 자료가 개시되는 행. 판별 불가이면 부재한다.
    pub fn body_at(self) -> Option<usize> {
        match self {
            Header::Found { at, rows } => Some(at + rows),
            Header::Unknown => None,
        }
    }
}

/// 열 하나에 관해 판별한 것.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Column {
    /// 화면에 노출할 명칭. 2행 헤더면 하단 행이다.
    pub label: String,
    /// 묶음 명칭. 2행 헤더의 상단 행이고, 1행이면 부재한다.
    pub group: Option<String>,
    pub kind: Kind,
}

/// 표 하나의 구조.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    pub header: Header,
    /// 헤더를 탐색하지 못했으면 공백이다. **첫 행을 열 명칭으로 사용하지 않는다.**
    pub columns: Vec<Column>,
}

/* ── 셀 하나를 어떻게 판정하는가 ────────────────────────────────── */

/// 숫자로 판독되는가.
///
/// **규칙을 문자로 기재한다.** 파서에 위임하면 그 파서가 수용하는 모든 형태가
/// 규칙이 되는데(`inf`·`NaN`·`1e5`), 조회 엔진은 SQL 로 동일한 판정을
/// 수행하므로 양측이 어긋난다 — 한쪽이 수치로 판정한 값이 타방에서 문자로
/// 취급되면 **정렬이 자료와 상이한 순서를 산출한다.**
///
/// 규칙: 천 단위 구분을 제거한 뒤 — 부호는 선두에 1회까지, 소수점은 1회까지,
/// 숫자가 하나 이상, 그 외의 문자는 부재. 지수 표기(`1e5`)는 **문자다** —
/// 엑셀이 수로 수용한 셀은 수 그대로 도달하므로, 그 문자가 기재된 셀은 수를
/// 의도한 것이 아니다. [`crate::grid`] 의 `not_a_number_sql` 이 동일한 규칙을
/// SQL 로 기재한다.
///
/// **쉼표가 기재되어 있으면 천 단위 구분으로 유효해야 한다.** 제거만 수행하면
/// `1,2,3` 이 123 으로 판정되어 그 위치로 정렬된다 — 사람이 열거를 기재한 셀을
/// 수치로 간주하는 셈이다. 쉼표가 부재하는 값은 그 검증과 무관하므로, 자릿수가
/// 다수인 식별자(`20231234`)는 그대로 수치다.
pub(crate) fn is_number(s: &str) -> bool {
    /* **절단하지 않는다.** SQL 은 절단을 수행할 수 없어(Rust 와 공백의 정의가
       상이하다) 여기서 절단하면 양측의 판정이 선후 공백에서 어긋난다. 셀은
       이미 절단되어 도달한다(`crate::sheet` 의 `cell_text`) — xlsx 도 csv 도
       그 문을 경유하므로, 절단할 대상이 잔존하는 값은 자료에서 유래할 수 없다. */
    let raw = s;
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

/// 값이 부재하는 셀인가. 양식이 "없음" 을 기재하는 문자(`blank`)도 공백으로 판정한다.
fn empty(s: &str, blank: &str) -> bool {
    crate::sheet::is_blank(s, blank)
}

/// 헤더 행으로 적합한가.
///
/// 공백이 아닌 셀이 존재해야 하고, **숫자가 혼재하면 하단 자료와 구조가
/// 상이해야** 한다. 자료 첫 행을 헤더로 선정하는 것이 이 모듈이 산출할 수 있는
/// 최악의 결과이므로 숫자를 경계하되, 숫자를 전부 기각하지는 않는다 — 연도·회차·
/// 학년을 열 명칭으로 기재한 양식(`학교 · 2023 · 2024`)이 실재하고, 그것을
/// 기각하면 **하단의 자료 1행이 열 명칭이 된다.**
fn looks_like_header(row: &[String], body: &[Vec<String>], blank: &str) -> bool {
    if !row.iter().any(|c| !empty(c, blank)) {
        return false;
    }
    !matches_body(row, body, blank)
}

/// 행이 **자료와 동일한 종류인가.**
///
/// 숫자 열의 상단에 숫자가 기재되어 있으면 그 행은 자료다. 역으로 열 명칭은
/// 자료와 종류가 상이하다.
///
/// 종류를 판정하지 못한 열(혼재)은 **근거로 삼지 않는다.** 거기 기재된 숫자를
/// 자료의 증거로 간주하면, `이름 · 2023` 같은 헤더가 하단 열의 혼재 하나로
/// 기각되고 **자료 행이 열 명칭으로 승격된다.**
///
/// 반대급부가 존재한다 — 전 열이 혼재이고 첫 자료 행에 숫자가 기재된 무헤더
/// 표는 그 행을 헤더로 채택한다. 종전에는 판별 실패로 처리되어 화면이 통지했다.
/// 양자 모두 자료 1행이 열 명칭이 되는 것은 동일하고, **실재하는 빈도는 전자가
/// 현저히 높다** — 연도·학년·회차를 열 명칭으로 기재한 양식은 흔하다.
fn matches_body(row: &[String], body: &[Vec<String>], blank: &str) -> bool {
    (0..row.len()).any(|c| {
        let cell = Sheet::at(row, c);
        !empty(cell, blank) && is_number(cell) && kind_of(body, c, blank) == Kind::Number
    })
}

/// 행의 구조가 **하단 자료와 상이한가.**
///
/// 헤더는 열 명칭이라 그 열의 자료와 종류가 상이하다 — 숫자 열의 상단에는
/// 문자가 기재된다. 역으로 자료 행은 하단과 종류가 일치한다. 그 차이를
/// 판정한다.
///
/// 종류를 판정하지 못한 열(혼재)은 **근거로 삼지 않는다.** 상이하다고 간주하면
/// 혼재한 열 하나가 자료 행을 헤더로 승격시킨다.
fn differs_from_body(row: &[String], body: &[Vec<String>], blank: &str) -> bool {
    (0..row.len()).any(|c| {
        let cell = Sheet::at(row, c);
        if empty(cell, blank) {
            return false;
        }
        match kind_of(body, c, blank) {
            Kind::Number => !is_number(cell),
            Kind::Text => is_number(cell),
            Kind::Unknown => false,
        }
    })
}

/// 자료 행으로 적합한가. 공백이 아닌 셀이 하나라도 존재하면 해당한다.
fn looks_like_body(row: &[String], blank: &str) -> bool {
    row.iter().any(|c| !empty(c, blank))
}

fn filled(row: &[String], blank: &str) -> usize {
    row.iter().filter(|c| !empty(c, blank)).count()
}

/// 하단 자료 행들이 사용하는 셀 수의 중앙값.
///
/// 평균이 아니라 중앙값인 사유는, 본문에 이탈한 셀 하나가 존재하는 행이 혼재해도
/// 변동하지 않기 때문이다. 그러한 행이 실재한다.
fn body_width(rows: &[Vec<String>], blank: &str) -> usize {
    let mut counts: Vec<usize> = rows
        .iter()
        .take(SAMPLE)
        .map(|row| filled(row, blank))
        .filter(|n| *n > 0)
        .collect();
    if counts.is_empty() {
        return 0;
    }
    counts.sort_unstable();
    counts[counts.len() / 2]
}

/// 하단 자료 행들이 **실제로 미치는 폭**의 중앙값.
///
/// 충전된 셀 수가 아니라 최종 셀의 위치다 — 중간에 공백이 개재하는 열이 존재해도
/// 그 우측의 열이 소거되지 않아야 한다. 중앙값인 사유는 [`body_width`] 와 동일해,
/// 돌출한 셀 하나가 존재하는 행이 혼재해도 확장되지 않는다.
fn body_extent(rows: &[Vec<String>], blank: &str) -> usize {
    let mut ends: Vec<usize> = rows
        .iter()
        .take(SAMPLE)
        .filter_map(|row| row.iter().rposition(|c| !empty(c, blank)).map(|i| i + 1))
        .collect();
    if ends.is_empty() {
        return 0;
    }
    ends.sort_unstable();
    let middle = ends[ends.len() / 2];
    /* **행 하나만으로는 확장하지 않는다.** 행 하나짜리 본문에서는 돌출한 셀과
       정상적인 열을 구분할 근거가 부재하고, 그 경우 돌출로 간주한다 — 열을
       신설해 전 행이 공백인 열이 출현하는 쪽보다 양호하다. */
    if ends.iter().filter(|e| **e >= middle).count() < 2 {
        return 0;
    }
    middle
}

/// **자료의 절반에도 미달하는 협소한 행은 헤더가 아니다.**
///
/// 안내 문장 1행이 그렇다 — 최상단에 "이 표는 …" 1셀만 기재된 양식이 빈번하고,
/// 그 행도 문자뿐이라 헤더로 인식된다. 그것을 채택하면 **열 명칭이 안내
/// 문장이 되고 나머지 열은 명칭이 부재하게 된다.**
///
/// 완전 일치를 요구하지 않는 사유는 반대 방향의 사고다. 본문에 헤더보다
/// 우측으로 돌출한 셀이 존재하는 양식도 빈번한데, 완전 일치를 요구하면 그러한
/// 정상 표에서 헤더를 전혀 탐색하지 못한다.
fn wide_enough(row: &[String], under: usize, blank: &str) -> bool {
    let n = filled(row, blank);
    n > 0 && n * 2 > under
}

/// 명칭이 부재하는 열이 중간에 개재하는가.
///
/// 1행 헤더에 공백이 존재하면 그 열은 명칭이 **공백 문자**가 된다. 그것은
/// 대개 2행 헤더의 상단 행(묶음 명칭)만 단독 포착한 것인데, 하단 행을 함께
/// 판정하지 못했다면 **선정하지 않는 것이 타당하다.** 후단의 공백 셀은 무방하다 —
/// 거기까지 산정하지 않는다.
fn has_a_nameless_column(row: &[String]) -> bool {
    let Some(last) = row.iter().rposition(|c| !c.trim().is_empty()) else { return false };
    row[..last].iter().any(|c| c.trim().is_empty())
}

/* ── 판별 ───────────────────────────────────────────────────────── */

/// 헤더가 어느 행인지, 2행인지 판별한다.
///
/// 상단부터 점검해 **최초로 헤더로 인식되고 하단에 자료가 존재하는 행**을
/// 선정한다. 최유력 행을 점수로 선정하지 않는 사유는, 그 점수가 오류일 때 왜 그
/// 행이 채택되었는지 누구도 설명할 수 없기 때문이다. 상단부터라는 규칙은 오류
/// 시에도 오류 사유가 노출된다.
fn find_header(sheet: &Sheet, blank: &str) -> Header {
    let rows = &sheet.rows;
    /* **공백 행은 점검한 수로 계수하지 않는다.** csv 는 소거된 공백 행을
       복원해 수용하므로(`sheet::from_csv`), 상단에 여백이 수 행 존재하는 양식
       에서 그것을 계수하면 헤더에 도달하기 전에 점검이 종료된다. xlsx 는
       선두의 공백 행이 이미 소거되어 있어 양측의 거동이 그래야 일치한다. */
    let mut budget = LOOK;
    for r in 0..rows.len() {
        if budget == 0 {
            break;
        }
        if looks_like_body(&rows[r], blank) {
            budget -= 1;
        }
        // 하단에 자료가 존재해야 헤더다. 최종 행은 헤더가 아니다.
        let Some(below) = rows.get(r + 1..) else { continue };
        if !looks_like_header(&rows[r], below, blank) {
            continue;
        }
        if !below.iter().any(|row| looks_like_body(row, blank)) {
            continue;
        }
        if is_two_row(rows, r, blank) {
            return Header::Found { at: r, rows: 2 };
        }
        // 2행이 아니라면 이 행 하나가 전 열의 명칭이어야 한다.
        if !wide_enough(&rows[r], body_width(below, blank), blank) {
            continue;
        }
        if has_a_nameless_column(&rows[r]) {
            return Header::Unknown;
        }
        return Header::Found { at: r, rows: 1 };
    }
    Header::Unknown
}

/// `top` 과 그 하단 행이 함께 헤더인가.
///
/// **확산 기재된 형태로 식별한다.** 교육청 원본처럼 2행 헤더는 상단 행이
/// 묶음 명칭이라 **희소**하고(`전형개요` 하나가 복수의 열에 걸친다), 하단 행이
/// **조밀**하다. 양자 모두 조밀하면 2번째 행은 헤더가 아니라 자료다.
///
/// 하단 행에 숫자가 혼재하면 그것도 자료다. 그리고 **그 하단에 자료가 추가로
/// 존재해야** 한다 — 2행짜리 표에서 2번째 행을 헤더로 채택하면 자료가 소멸한다.
fn is_two_row(rows: &[Vec<String>], top: usize, blank: &str) -> bool {
    let Some(sub) = rows.get(top + 1) else { return false };
    let Some(rest) = rows.get(top + 2..) else { return false };
    if !looks_like_header(sub, rest, blank) {
        return false;
    }
    if !rest.iter().any(|row| looks_like_body(row, blank)) {
        return false;
    }
    /* **하단 행이 자료와 구조가 동일하면 그것은 자료다.** 상단이 희소하고
       하단이 조밀한 것만으로는 제목 1셀 + 자료(`명단` · `가,서울`)와 구분되지
       않는다. 그 경우 자료 1행이 열 명칭으로 흡수된다. */
    if !differs_from_body(sub, rest, blank) {
        return false;
    }
    let up = filled(&rows[top], blank);
    let down = filled(sub, blank);
    // 상단 행이 하단 행보다 **희소하게** 충전되어 있어야 묶음 명칭이다. 동일하거나
    // 과다하면 2행 헤더가 아니다.
    if up == 0 || down <= up {
        return false;
    }
    // 하단 행이 그 하단 자료만큼 광범위해야 한다. 협소하면 그것도 묶음 행이거나
    // 안내 행이다.
    wide_enough(sub, body_width(&rows[top + 2..], blank), blank)
}

/// 헤더 `rows` 행을 `(묶음, 명칭)` 쌍으로 전개한다.
///
/// 상단 행들은 병합된 묶음 명칭이라 우측으로 확산 충전한다. 최하단 행은 각 열의
/// 고유 명칭이므로 확산하지 않는다 — 확산하면 공백 셀에 좌측 명칭이 복제되어
/// 동일 명칭의 열이 2개 발생한다.
///
/// **깊이가 열마다 상이한 양식을 수용한다.** 실제 양식은 좌측 다수가 1단이고
/// 우측 일부만 2단인 경우가 빈번하다(`학년`·`반` 은 상단에만, `국어 선택과목` 은
/// 2단). 그래서 각 열의 명칭 조각을 상단부터 집약해 **공백을 제거한 뒤** 최종
/// 조각을 명칭으로, 나머지를 묶음으로 채택한다. 1단 열은 묶음이 부재하는 열이
/// 된다.
///
/// **헤더가 실제로 미치는 범위까지만** 산정한다. 시트 폭은 본문의 이탈한 셀
/// 하나로 확장되는데, 그 셀까지 열로 산입하면 확산 충전된 묶음이 부착되어
/// 무의미한 열이 발생한다.
fn spread(
    rows: &[Vec<String>],
    at: usize,
    count: usize,
    least: usize,
) -> Vec<(Option<String>, String)> {
    let head: Vec<&Vec<String>> = (at..at + count).filter_map(|r| rows.get(r)).collect();
    if head.is_empty() {
        return Vec::new();
    }
    let width = head.iter().map(|r| r.len()).max().unwrap_or(0);
    let named = (0..width)
        .rposition(|c| head.iter().any(|r| !Sheet::at(r, c).trim().is_empty()))
        .map(|i| i + 1)
        .unwrap_or(0);
    /* **본문이 일관되게 더 넓으면 그 폭까지 산출한다.** 헤더가 미치는 범위
       까지만 산출하면 그 우측의 열이 전 행에서 묵시적으로 소거된다 — 표는
       정상으로 묘사되고 자료만 부재한다. 명칭이 부재하는 열이 되고, 부르는
       곳이 열 기호를 부여한다. `least` 는 중앙값이라 돌출한 셀 하나로는
       확장되지 않는다. */
    let last = named.max(least);

    // 상단 행마다 직전 공백이 아닌 값을 보유한다. 최하단 행은 확산하지 않는다.
    let mut carried: Vec<Option<String>> = vec![None; head.len().saturating_sub(1)];
    let mut out = Vec::with_capacity(last);
    for c in 0..last {
        /* **헤더가 미치지 않는 열에는 상단 묶음을 확산하지 않는다.** 확산하면
           그 열의 명칭이 묶음 명칭(`모집`)이 되어, 명칭이 부재한다는 사실이
           소멸하고 부르는 곳이 열 기호를 부여할 근거를 상실한다. */
        if c >= named {
            out.push((None, String::new()));
            continue;
        }
        let mut parts: Vec<String> = Vec::with_capacity(head.len());
        for (i, carry) in carried.iter_mut().enumerate() {
            let cell = Sheet::at(head[i], c).trim();
            if !cell.is_empty() {
                *carry = Some(cell.to_string());
            }
            if let Some(v) = carry {
                parts.push(v.clone());
            }
        }
        let own = Sheet::at(head[head.len() - 1], c).trim();
        if !own.is_empty() {
            parts.push(own.to_string());
        }
        // 상하 행에 동일 문구가 반복되는 양식이 존재한다. 중복은 1회만 채택한다.
        parts.dedup();
        let label = parts.pop().unwrap_or_default();
        let group = (!parts.is_empty()).then(|| parts.join(" "));
        out.push((group, label));
    }
    out
}

/// 열 하나가 숫자인지 문자인지. **혼재하면 선정하지 않는다.**
///
/// 값의 구조만 판정한다. "없음" 을 `-` 로 기재하는 양식이 빈번하므로 그 문자는
/// 공백으로 산정하고, 그러지 않으면 숫자 열이 전부 혼재로 인식된다.
fn kind_of(rows: &[Vec<String>], col: usize, blank: &str) -> Kind {
    let mut numbers = 0usize;
    let mut words = 0usize;
    for row in rows.iter().take(SAMPLE) {
        let s = Sheet::at(row, col);
        if empty(s, blank) {
            continue;
        }
        if is_number(s) {
            numbers += 1;
        } else {
            words += 1;
        }
    }
    match (numbers, words) {
        // 판정할 값이 부재한다. 공백 열을 문자로 설정하면 추후 숫자가 유입되는
        // 시점에 묵시적으로 상이한 순서로 정렬된다.
        (0, 0) => Kind::Unknown,
        (_, 0) => Kind::Number,
        (0, _) => Kind::Text,
        // 혼재한다. 수령한 곳이 결정한다.
        _ => Kind::Unknown,
    }
}

/// 표 하나의 구조를 판별한다.
///
/// `blank` 는 양식이 "없음" 을 기재하는 문자다(`-` 등). 인지하지 못하면 공백
/// 문자를 전달한다 — 그때는 공백 셀만 공백으로 산정한다.
pub fn layout(sheet: &Sheet, blank: &str) -> Layout {
    let header = find_header(sheet, blank);
    let columns = match header {
        Header::Unknown => Vec::new(),
        Header::Found { at, rows } => columns_of(sheet, blank, at, rows),
    };
    Layout { header, columns }
}

/// 헤더의 위치와 행 수를 **지정받아** 구조를 산출한다.
///
/// 추정하지 않는다. 사용자가 조작부에서 지정한 값이 여기로 전달되고, 그대로
/// 수용한다 — 이 그릇은 임의의 양식을 담당하므로 열마다 깊이가 상이하거나
/// 상단에 안내 행이 존재하는 양식을 추정만으로 전부 적중시킬 수 없다. 적중시킬
/// 수 없는 것을 적중한 것처럼 반환하는 편이 더 유해하다.
///
/// 범위가 시트를 이탈하면 **열이 부재하는 구조**를 반환한다. 임의로 보정하면
/// 사용자가 지정한 것과 상이한 표가 묵시적으로 표시된다.
pub fn layout_at(sheet: &Sheet, blank: &str, at: usize, rows: usize) -> Layout {
    /* **넘치는 덧셈을 수행하지 않는다.** 지정은 화면이 송신하는 값이라 임의의
       수가 도달할 수 있고, 넘치면 그 자리에서 중단되거나(시험·개발 빌드) 0 으로
       순환해 검사를 통과한다(배포 빌드) — 후자는 묵시적이다. */
    let Some(end) = at.checked_add(rows) else {
        return Layout { header: Header::Unknown, columns: Vec::new() };
    };
    if rows == 0 || end > sheet.rows.len() {
        return Layout { header: Header::Unknown, columns: Vec::new() };
    }
    Layout {
        header: Header::Found { at, rows },
        columns: columns_of(sheet, blank, at, rows),
    }
}

fn columns_of(sheet: &Sheet, blank: &str, at: usize, count: usize) -> Vec<Column> {
    let rows = &sheet.rows;
    let body = &rows[(at + count).min(rows.len())..];
    spread(rows, at, count, body_extent(body, blank))
        .into_iter()
        .enumerate()
        .map(|(c, (group, label))| Column { label, group, kind: kind_of(body, c, blank) })
        .collect()
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

    fn kinds(l: &Layout) -> Vec<Kind> {
        l.columns.iter().map(|c| c.kind).collect()
    }

    fn labels(l: &Layout) -> Vec<&str> {
        l.columns.iter().map(|c| c.label.as_str()).collect()
    }

    #[test]
    fn a_plain_sheet_has_its_first_row_as_the_header() {
        let s = sheet(&[&["이름", "점수"], &["가", "90"], &["나", "75"]]);
        let l = layout(&s, "-");
        assert_eq!(l.header, Header::Found { at: 0, rows: 1 });
        assert_eq!(labels(&l), ["이름", "점수"]);
        assert_eq!(kinds(&l), [Kind::Text, Kind::Number]);
    }

    /// 안내 문장 수 행이 전단에 부착된 양식이 빈번하다. 그 행을 헤더로 채택하면
    /// 열 명칭이 안내 문장이 된다.
    #[test]
    fn notes_above_the_header_are_skipped() {
        let s = sheet(&[
            &["이 표는 보기용입니다"],
            &[""],
            &["이름", "점수"],
            &["가", "90"],
        ]);
        let l = layout(&s, "-");
        assert_eq!(l.header, Header::Found { at: 2, rows: 1 });
        assert_eq!(labels(&l), ["이름", "점수"]);
    }

    /// 2행 헤더. 상단 행이 희소하고 하단 행이 조밀하다.
    #[test]
    fn a_two_row_header_is_spread_to_the_right() {
        let s = sheet(&[
            &["전형개요", "", "", "모집"],
            &["지역", "대학", "전형명", "인원"],
            &["서울", "가대", "일반", "30"],
            &["부산", "나대", "일반", "20"],
        ]);
        let l = layout(&s, "-");
        assert_eq!(l.header, Header::Found { at: 0, rows: 2 });
        assert_eq!(labels(&l), ["지역", "대학", "전형명", "인원"]);
        let groups: Vec<Option<&str>> =
            l.columns.iter().map(|c| c.group.as_deref()).collect();
        assert_eq!(
            groups,
            [Some("전형개요"), Some("전형개요"), Some("전형개요"), Some("모집")]
        );
        assert_eq!(kinds(&l), [Kind::Text, Kind::Text, Kind::Text, Kind::Number]);
    }

    /// 2행 모두 조밀하면 2번째 행은 자료다. 헤더로 채택하면 자료 1행이
    /// 소멸한다.
    #[test]
    fn a_dense_second_row_is_data_not_a_header() {
        let s = sheet(&[&["이름", "지역"], &["가람", "서울"], &["나리", "부산"]]);
        assert_eq!(layout(&s, "-").header, Header::Found { at: 0, rows: 1 });
    }

    /// 헤더 하단에 자료가 부재하면 헤더가 아니다.
    #[test]
    fn a_header_with_nothing_under_it_is_not_a_header() {
        let s = sheet(&[&["이름", "점수"]]);
        let l = layout(&s, "-");
        assert_eq!(l.header, Header::Unknown);
        assert!(l.columns.is_empty(), "헤더를 탐색하지 못했는데 열을 반환했다");
    }

    /// 2행짜리 표에서 2번째 행을 헤더로 채택하면 자료가 소멸한다.
    #[test]
    fn two_rows_alone_do_not_make_a_two_row_header() {
        let s = sheet(&[&["전형개요", "", "", "모집"], &["지역", "대학", "전형명", "인원"]]);
        assert_eq!(layout(&s, "-").header, Header::Unknown);
    }

    /// **혼재한 열은 선정하지 않는다.** 문자로 설정하면 정렬이 1 · 10 · 2 가 되고,
    /// 숫자로 설정하면 문자 값이 소멸한다. 양자 모두 묵시적이다.
    #[test]
    fn a_mixed_column_is_unknown() {
        let s = sheet(&[&["값"], &["90"], &["미정"], &["75"]]);
        assert_eq!(kinds(&layout(&s, "-")), [Kind::Unknown]);
    }

    /// "없음" 표기는 공백으로 산정한다. 그러지 않으면 숫자 열이 전부 혼재로
    /// 인식된다 — 이 양식은 숫자 열에도 `-` 가 다수 존재한다.
    #[test]
    fn the_blank_token_does_not_make_a_number_column_mixed() {
        let s = sheet(&[&["정시백분위"], &["90"], &["-"], &["75"]]);
        assert_eq!(kinds(&layout(&s, "-")), [Kind::Number]);
        // 그 문자를 인지하지 못한다고 설정하면 혼재로 인식된다. 그때도 선정하지 않는다.
        assert_eq!(kinds(&layout(&s, "")), [Kind::Unknown]);
    }

    /// 판정할 값이 부재하는 열도 미상이다. 문자로 설정하면 추후 숫자가 유입되는
    /// 시점에 묵시적으로 상이한 순서로 정렬된다.
    #[test]
    fn an_empty_column_is_unknown() {
        let s = sheet(&[&["이름", "비고"], &["가", ""], &["나", "-"]]);
        assert_eq!(kinds(&layout(&s, "-")), [Kind::Text, Kind::Unknown]);
    }

    /// 천 단위 쉼표가 부착된 수치도 숫자다. 사람이 기재하는 표에 빈번하다.
    #[test]
    fn thousands_separators_are_still_numbers() {
        let s = sheet(&[&["인원"], &["1,200"], &["980"]]);
        assert_eq!(kinds(&layout(&s, "-")), [Kind::Number]);
    }

    /// **열거를 기재한 셀은 수치가 아니다.**
    ///
    /// 쉼표를 제거만 수행하면 `1,2,3` 이 123 으로 판정되어 그 열이 숫자 열이
    /// 되고, 정렬이 123 의 위치로 이동한다. 자릿수가 다수인 식별자는 쉼표가
    /// 부재하므로 그 검증과 무관하다 — 수치 그대로다.
    #[test]
    fn a_list_written_with_commas_is_not_a_number() {
        let s = sheet(&[&["항목"], &["1,2,3"], &["4,5"]]);
        assert_eq!(kinds(&layout(&s, "-")), [Kind::Text]);

        let s = sheet(&[&["번호"], &["20231234"], &["20231235"]]);
        assert_eq!(kinds(&layout(&s, "-")), [Kind::Number]);
    }

    /// `inf` 와 `NaN` 은 Rust 의 파서가 수용하지만, 표에 기재된 그 문자는
    /// 문자다. 숫자로 판정하면 정렬이 무의미한 위치로 이동한다.
    #[test]
    fn inf_and_nan_are_words_not_numbers() {
        let s = sheet(&[&["값"], &["inf"], &["NaN"]]);
        assert_eq!(kinds(&layout(&s, "-")), [Kind::Text]);
    }

    /// **무엇이 수치인가를 두 층이 동일하게 판정한다.**
    ///
    /// 구조를 식별하는 층은 Rust 로, 조회 엔진은 SQL 로 동일한 판정을 수행한다.
    /// 어긋나면 한쪽이 수치로 판정한 값이 타방에서 문자로 취급되어 **정렬이
    /// 자료와 상이한 순서를 산출한다.**
    #[test]
    fn both_layers_agree_on_what_a_number_is() {
        let named = [
            "0", "-0", "007", "1,200", "+5", "3.14", ".5", "5.", "-42", "9007199254740993",
            "1e5", "1E5", "2024-01-01", "010-1234-5678", "1 200", "1.2.3", "3 apples", "12abc",
            "7점", "총 44850점", "--5", "+", ".", "inf", "NaN", "0x10", "１２",
            // 천 단위 구분. 열거를 기재한 셀이 수치로 판정되어서는 안 된다.
            "1,2,3", "1,23", "12,34,567", "1234,567", "1,000,000", "-1,234.50", "1,000.",
            "1,000,00", ",123", "1,,000", "12,3456", "1.000,50",
        ];
        /* **문자를 생성해 전수 대조한다.** 명칭을 부여한 표본만으로는 양측이
           어긋나는 위치를 발견하지 못한다 — 쉼표의 유효성은 국소 형태의
           열거로 기재되어 있어(`grid` 의 `misgrouped`), 그 열거가 한 항목
           누락된 것은 특정 길이의 특정 조합에서만 발현한다. 공백도 생성한다 —
           양측 모두 절단을 수행하지 않아야 한다. */
        let alphabet = [b'0', b'1', b',', b'.', b'+', b'-', b' '];
        let mut generated: Vec<String> = Vec::new();
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
                generated.push(String::from_utf8(bytes).unwrap());
            }
        }
        let samples: Vec<&str> =
            named.iter().copied().chain(generated.iter().map(String::as_str)).collect();
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE t(v TEXT);").unwrap();
        {
            let mut ins = conn.prepare("INSERT INTO t VALUES (?)").unwrap();
            for v in &samples {
                ins.execute([v]).unwrap();
            }
        }
        let expr = crate::grid::not_a_number_sql("t.v");
        let expr = expr.trim_end_matches(" ASC");
        let sql = format!("SELECT v, {expr} FROM t t");
        let mut stmt = conn.prepare(&sql).unwrap();
        let mut rows = stmt.query([]).unwrap();
        let mut differ: Vec<String> = Vec::new();
        while let Some(r) = rows.next().unwrap() {
            let v: String = r.get(0).unwrap();
            let sql_says_number = r.get::<_, i64>(1).unwrap() == 0;
            let rust_says_number = is_number(&v);
            if sql_says_number != rust_says_number {
                differ.push(format!("{v:?}: SQL={sql_says_number} Rust={rust_says_number}"));
            }
        }
        assert!(differ.is_empty(), "판정이 어긋납니다: {differ:#?}");
    }

    /// **연도를 열 명칭으로 기재한 헤더를 기각하지 않는다.**
    ///
    /// 숫자를 전부 기각하면 그 행이 자료로 간주되고, 하단의 자료 1행이 열
    /// 명칭으로 승격되며 그 행은 표에서 소멸한다.
    #[test]
    fn a_header_may_hold_years_as_column_names() {
        let s = sheet(&[&["학교", "2023", "2024"], &["가고", "A", "B"], &["나고", "C", "D"]]);
        let l = layout(&s, "-");
        assert_eq!(l.header, Header::Found { at: 0, rows: 1 });
        assert_eq!(labels(&l), ["학교", "2023", "2024"]);
    }

    /// **혼재한 열 하나가 헤더를 기각시키지 않는다.**
    ///
    /// `이름 · 2023` 에서 `2023` 이 숫자이고 그 하단 열이 혼재(`미정`·`90`)라,
    /// 종전에는 그 행이 기각되고 자료 행이 열 명칭으로 승격되었다 — 숫자가
    /// 하단에 소재할수록 더 많은 자료 행이 소멸했다.
    #[test]
    fn a_mixed_column_does_not_reject_the_header_above_it() {
        let s = sheet(&[&["이름", "2023"], &["가", "미정"], &["나", "90"], &["다", "80"]]);
        let l = layout(&s, "-");
        assert_eq!(l.header, Header::Found { at: 0, rows: 1 });
        assert_eq!(labels(&l), ["이름", "2023"]);

        // 숫자가 후단에 소재해도 동일하다.
        let s = sheet(&[&["이름", "2023"], &["가", "90"], &["나", "미정"], &["다", "80"]]);
        assert_eq!(layout(&s, "-").header, Header::Found { at: 0, rows: 1 });
    }

    /// 제목 1셀 하단의 **자료 행이 2단 헤더로 흡수되지 않는다.**
    ///
    /// 상단이 희소하고 하단이 조밀한 것만으로는 제목 + 자료와 2단 헤더를
    /// 구분할 수 없다. 하단 행의 구조가 그 아래 자료와 동일하면 그것은 자료다.
    #[test]
    fn a_title_above_plain_rows_does_not_eat_one_of_them() {
        let s = sheet(&[&["명단"], &["가", "서울"], &["나", "부산"], &["다", "대구"]]);
        let l = layout(&s, "-");
        assert_ne!(
            l.header,
            Header::Found { at: 0, rows: 2 },
            "자료 행이 2단 헤더로 흡수되었습니다: {:?}",
            labels(&l)
        );
        /* 제목 하단의 1행을 헤더로 선정하는 것 자체는 존치한다. `명단 · 가,서울`
           과 `안내 문장 · 이름,지역` 은 **구조가 동일해** 어떤 규칙으로도
           구분되지 않는다. 구분할 수 없는 것을 추정으로 선별하지 않고, 사람이
           조작부에서 이동시킨다. 여기서 차단하는 것은 그 행이 상단 제목과
           합성되어 `명단 · 가` 같은 명칭이 생성되는 쪽이다. */
        assert_eq!(l.header, Header::Found { at: 1, rows: 1 });
        assert_eq!(labels(&l), ["가", "서울"]);
    }

    /// 헤더가 숫자뿐인 행은 헤더가 아니다 — 자료 첫 행이다.
    #[test]
    fn a_row_of_numbers_is_not_a_header() {
        let s = sheet(&[&["1", "2"], &["3", "4"], &["5", "6"]]);
        assert_eq!(layout(&s, "-").header, Header::Unknown);
    }

    /// **명칭 없는 열을 생성하지 않는다.** 공백이 존재하는 1행은 대개 2행
    /// 헤더의 상단 행인데, 하단 행을 함께 판정하지 못했다면 선정하지 않는 것이
    /// 타당하다.
    #[test]
    fn a_header_with_a_hole_is_not_chosen() {
        let s = sheet(&[&["전형개요", "", "모집"], &["서울", "가대", "30"], &["부산", "나대", "20"]]);
        let l = layout(&s, "-");
        assert_eq!(l.header, Header::Unknown);
        assert!(l.columns.is_empty(), "명칭이 공백인 열을 반환했다");
    }

    /// 공백 시트에서 아무것도 선정하지 않는다.
    #[test]
    fn an_empty_sheet_gives_nothing() {
        let l = layout(&Sheet::default(), "-");
        assert_eq!(l.header, Header::Unknown);
        assert!(l.columns.is_empty());
    }

    /* ── 사용자가 지정하는 헤더 ──────────────────────────────── */

    /// **깊이가 열마다 상이한 양식.** 좌측은 1단, 우측만 2단이다. 실재하는
    /// 진학결과 양식의 형태이고, 추정은 여기서 실패한다 — 상단 행이 하단 행보다
    /// 조밀해 "묶음 행" 으로 인식되지 않기 때문이다.
    #[test]
    fn a_header_whose_depth_differs_per_column_is_refused_by_the_guess() {
        let s = sheet(&[
            &["학년", "반", "국어", "", "", "영어"],
            &["", "", "선택과목", "표준점수", "등급", "등급"],
            &["3", "1", "언어와매체", "104", "3", "2"],
            &["3", "2", "화법과작문", "99", "4", "1"],
        ]);
        // 추정은 거부한다. 상단 행에 명칭 없는 열이 중간에 존재한다.
        assert_eq!(layout(&s, "-").header, Header::Unknown);
    }

    /// 그 양식도 **사용자가 2행이라고 지정하면** 정확히 전개된다.
    #[test]
    fn the_same_sheet_opens_once_the_user_says_it_is_two_rows() {
        let s = sheet(&[
            &["학년", "반", "국어", "", "", "영어"],
            &["", "", "선택과목", "표준점수", "등급", "등급"],
            &["3", "1", "언어와매체", "104", "3", "2"],
            &["3", "2", "화법과작문", "99", "4", "1"],
        ]);
        let l = layout_at(&s, "-", 0, 2);
        assert_eq!(l.header, Header::Found { at: 0, rows: 2 });
        // 1단 열은 묶음이 부재하고, 2단 열만 묶음을 보유한다.
        assert_eq!(labels(&l), ["학년", "반", "선택과목", "표준점수", "등급", "등급"]);
        let groups: Vec<Option<&str>> =
            l.columns.iter().map(|c| c.group.as_deref()).collect();
        assert_eq!(
            groups,
            [None, None, Some("국어"), Some("국어"), Some("국어"), Some("영어")]
        );
        // 자료는 헤더 2행 하단부터다. 그래야 종류가 정확히 산출된다.
        assert_eq!(kinds(&l), [Kind::Number, Kind::Number, Kind::Text, Kind::Number, Kind::Number, Kind::Number]);
    }

    /// 1행이라고 지정하면 2번째 행은 **자료가 된다.** 지정을 그대로 수용한다는
    /// 의미이고, 그래서 사람이 오지정하면 상이한 표가 산출된다 — 다만 그것은
    /// 화면에 노출되고 복원할 수 있다.
    #[test]
    fn saying_one_row_makes_the_second_row_data() {
        let s = sheet(&[
            &["학년", "반", "국어", "", "", "영어"],
            &["", "", "선택과목", "표준점수", "등급", "등급"],
            &["3", "1", "언어와매체", "104", "3", "2"],
        ]);
        let l = layout_at(&s, "-", 0, 1);
        assert_eq!(l.header, Header::Found { at: 0, rows: 1 });
        // 병합으로 공백이던 셀은 **명칭 없는 열**이 된다. 여기서 임의로 충전하지
        // 않는다 — 명칭이 부재한다는 사실을 그대로 전달하고, 표시용 명칭은 수령한
        // 곳이 부여한다.
        assert_eq!(labels(&l), ["학년", "반", "국어", "", "", "영어"]);
    }

    /// 상단에 제목 행이 부착된 양식. 개시 행을 지정할 수 있어야 한다.
    #[test]
    fn the_user_can_say_where_the_header_starts() {
        let s = sheet(&[
            &["2026학년도 내부 자료"],
            &["대학", "전형"],
            &["", "세부"],
            &["마루대", "교과"],
        ]);
        let l = layout_at(&s, "-", 1, 2);
        assert_eq!(labels(&l), ["대학", "세부"]);
        assert_eq!(l.columns[1].group.as_deref(), Some("전형"));
    }

    /// **범위를 이탈한 지정은 열을 산출하지 않는다.** 보정해 상이한 표를 묵시적으로
    /// 표시하지 않는다.
    #[test]
    fn a_span_past_the_end_of_the_sheet_gives_nothing() {
        let s = sheet(&[&["대학", "전형"], &["마루대", "교과"]]);
        assert_eq!(layout_at(&s, "-", 1, 5).header, Header::Unknown);
        assert!(layout_at(&s, "-", 1, 5).columns.is_empty());
        assert_eq!(layout_at(&s, "-", 0, 0).header, Header::Unknown);
    }

    /// 상하 행에 동일 문구가 반복되는 양식. 명칭을 중복 부착하지 않는다.
    #[test]
    fn a_label_repeated_on_both_rows_is_taken_once() {
        let s = sheet(&[&["대학", "점수"], &["대학", "총점"], &["마루대", "91"]]);
        let l = layout_at(&s, "-", 0, 2);
        assert_eq!(labels(&l), ["대학", "총점"]);
        assert_eq!(l.columns[0].group, None);
        assert_eq!(l.columns[1].group.as_deref(), Some("점수"));
    }

    /// 3행 헤더. **둘로 한정하지 않는다.**
    #[test]
    fn three_header_rows_work_too() {
        let s = sheet(&[
            &["성적", "", ""],
            &["국어", "", "영어"],
            &["표준점수", "등급", "등급"],
            &["104", "3", "2"],
        ]);
        let l = layout_at(&s, "-", 0, 3);
        assert_eq!(labels(&l), ["표준점수", "등급", "등급"]);
        let groups: Vec<Option<&str>> = l.columns.iter().map(|c| c.group.as_deref()).collect();
        assert_eq!(groups, [Some("성적 국어"), Some("성적 국어"), Some("성적 영어")]);
    }

    /// 본문의 이탈한 셀 하나를 사유로 열이 추가 생성되지 않는다.
    /// **헤더보다 넓은 본문의 열이 소거되지 않는다.**
    ///
    /// 헤더가 미치는 범위까지만 열을 산출하면 그 우측의 열이 전 행에서
    /// 묵시적으로 소멸한다 — 표는 정상으로 묘사되고 자료만 부재해, 열람하는
    /// 사람이 그 소멸을 인지할 경로가 전무하다.
    #[test]
    fn a_body_wider_than_its_header_keeps_the_extra_column() {
        let s = sheet(&[
            &["이름", "점수"],
            &["가", "90", "서울"],
            &["나", "80", "부산"],
            &["다", "70", "대구"],
        ]);
        let l = layout(&s, "-");
        assert_eq!(labels(&l), ["이름", "점수", ""], "본문의 3번째 열이 소거되었습니다");
    }

    /// 다단 헤더에서도 **추가 열이 명칭을 수령하지 않는다.**
    ///
    /// 상단 묶음을 확산하면 그 열의 명칭이 `모집` 이 되어, 명칭이 부재한다는
    /// 사실이 소멸하고 부르는 곳이 열 기호를 부여할 근거를 상실한다. 1단
    /// 헤더에서는 확산할 상단이 부재해 발현되지 않는다.
    #[test]
    fn a_two_row_header_does_not_name_the_columns_it_never_reached() {
        let s = sheet(&[
            &["전형개요", "", "모집"],
            &["지역", "대학", "인원"],
            &["서울", "가대", "30", "비고1"],
            &["부산", "나대", "20", "비고2"],
            &["대구", "다대", "10", "비고3"],
        ]);
        let l = layout(&s, "-");
        assert_eq!(labels(&l), ["지역", "대학", "인원", ""]);
        assert_eq!(l.columns[3].group, None, "묶음이 확산되었습니다");
    }

    /// 중간에 공백이 개재해도 그 우측의 열이 소거되지 않는다. 충전된 셀 수가
    /// 아니라 최종 셀의 위치로 판정하기 때문이다.
    #[test]
    fn a_gap_in_the_body_does_not_cut_the_columns_short() {
        let s = sheet(&[
            &["이름", "점수"],
            &["가", "", "서울"],
            &["나", "", "부산"],
        ]);
        assert_eq!(labels(&layout(&s, "-")), ["이름", "점수", ""]);
    }

    #[test]
    fn a_stray_cell_in_the_body_does_not_add_a_column() {
        let s = sheet(&[
            &["전형개요", "", "모집"],
            &["지역", "대학", "인원"],
            &["서울", "가대", "30", "여기 왜 있지"],
        ]);
        assert_eq!(labels(&layout(&s, "-")), ["지역", "대학", "인원"]);
    }
}
