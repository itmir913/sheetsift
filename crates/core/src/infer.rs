//! **임의의 표에서 구조를 판별한다.** 머리글이 어느 행인지, 2행인지, 어느 열이
//! 숫자인지.
//!
//! 정의 파일을 수동 작성하는 작업의 착수 단계다. 사람이 기재하면 오류가 발생할
//! 여지가 없으나, 임의의 xlsx 를 개방하려면 수령한 위치에서 판별해야 한다.
//!
//! ## 확신이 부재하면 "미상" 이다
//!
//! 이 모듈이 준수하는 것은 하나다. **추정한 것을 확정한 것처럼 반환하지
//! 않는다.** 머리글 행을 오선정하면 자료 1행이 열 명칭이 되고, 숫자 열을
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

/// 머리글 하단으로 몇 행까지 조사하는가. 자료의 구조를 판정하는 데 사용한다.
const SAMPLE: usize = 200;

/// 머리글 탐색을 위해 상단에서 점검하는 행 수. 안내 문장 수 행이 전단에 부착된 양식이 빈번하다.
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

/// 머리글이 어디인가.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Header {
    /// `at` 행부터 `rows` 행이 머리글이다.
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
    /// 화면에 노출할 명칭. 2행 머리글이면 하단 행이다.
    pub label: String,
    /// 묶음 명칭. 2행 머리글의 상단 행이고, 1행이면 부재한다.
    pub group: Option<String>,
    pub kind: Kind,
}

/// 표 하나의 구조.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    pub header: Header,
    /// 머리글을 탐색하지 못했으면 공백이다. **첫 행을 열 명칭으로 사용하지 않는다.**
    pub columns: Vec<Column>,
}

/* ── 셀 하나를 어떻게 판정하는가 ────────────────────────────────── */

/// 숫자로 판독되는가. **`inf` 와 `NaN` 은 숫자로 판정하지 않는다** — Rust 의
/// 파서가 수용하지만, 표에 그 문자가 기재되어 있으면 그것은 문자다.
fn is_number(s: &str) -> bool {
    let t = s.trim().replace(',', "");
    if t.is_empty() {
        return false;
    }
    t.parse::<f64>().map(|v| v.is_finite()).unwrap_or(false)
}

/// 값이 부재하는 셀인가. 양식이 "없음" 을 기재하는 문자(`blank`)도 공백으로 판정한다.
fn empty(s: &str, blank: &str) -> bool {
    crate::sheet::is_blank(s, blank)
}

/// 머리글 행으로 적합한가. **공백이 아닌 셀이 존재하고, 그 중에 숫자가 부재해야**
/// 한다. 머리글에 숫자만 기재된 양식은 희소하고, 자료 첫 행을 머리글로 선정하는
/// 것이 이 모듈이 산출할 수 있는 최악의 결과다.
fn looks_like_header(row: &[String], blank: &str) -> bool {
    let filled: Vec<&String> = row.iter().filter(|c| !empty(c, blank)).collect();
    !filled.is_empty() && filled.iter().all(|c| !is_number(c))
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

/// **자료의 절반에도 미달하는 협소한 행은 머리글이 아니다.**
///
/// 안내 문장 1행이 그렇다 — 최상단에 "이 표는 …" 1셀만 기재된 양식이 빈번하고,
/// 그 행도 문자뿐이라 머리글로 인식된다. 그것을 채택하면 **열 명칭이 안내
/// 문장이 되고 나머지 열은 명칭이 부재하게 된다.**
///
/// 완전 일치를 요구하지 않는 사유는 반대 방향의 사고다. 본문에 머리글보다
/// 우측으로 돌출한 셀이 존재하는 양식도 빈번한데, 완전 일치를 요구하면 그러한
/// 정상 표에서 머리글을 전혀 탐색하지 못한다.
fn wide_enough(row: &[String], under: usize, blank: &str) -> bool {
    let n = filled(row, blank);
    n > 0 && n * 2 > under
}

/// 명칭이 부재하는 열이 중간에 개재하는가.
///
/// 1행 머리글에 공백이 존재하면 그 열은 명칭이 **공백 문자**가 된다. 그것은
/// 대개 2행 머리글의 상단 행(묶음 명칭)만 단독 포착한 것인데, 하단 행을 함께
/// 판정하지 못했다면 **선정하지 않는 것이 타당하다.** 후단의 공백 셀은 무방하다 —
/// 거기까지 산정하지 않는다.
fn has_a_nameless_column(row: &[String]) -> bool {
    let Some(last) = row.iter().rposition(|c| !c.trim().is_empty()) else { return false };
    row[..last].iter().any(|c| c.trim().is_empty())
}

/* ── 판별 ───────────────────────────────────────────────────────── */

/// 머리글이 어느 행인지, 2행인지 판별한다.
///
/// 상단부터 점검해 **최초로 머리글로 인식되고 하단에 자료가 존재하는 행**을
/// 선정한다. 최유력 행을 점수로 선정하지 않는 사유는, 그 점수가 오류일 때 왜 그
/// 행이 채택되었는지 누구도 설명할 수 없기 때문이다. 상단부터라는 규칙은 오류
/// 시에도 오류 사유가 노출된다.
fn find_header(sheet: &Sheet, blank: &str) -> Header {
    let rows = &sheet.rows;
    let look = rows.len().min(LOOK);
    for r in 0..look {
        if !looks_like_header(&rows[r], blank) {
            continue;
        }
        // 하단에 자료가 존재해야 머리글이다. 최종 행은 머리글이 아니다.
        let Some(below) = rows.get(r + 1..) else { continue };
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

/// `top` 과 그 하단 행이 함께 머리글인가.
///
/// **확산 기재된 형태로 식별한다.** 교육청 원본처럼 2행 머리글은 상단 행이
/// 묶음 명칭이라 **희소**하고(`전형개요` 하나가 복수의 열에 걸친다), 하단 행이
/// **조밀**하다. 양자 모두 조밀하면 2번째 행은 머리글이 아니라 자료다.
///
/// 하단 행에 숫자가 혼재하면 그것도 자료다. 그리고 **그 하단에 자료가 추가로
/// 존재해야** 한다 — 2행짜리 표에서 2번째 행을 머리글로 채택하면 자료가 소멸한다.
fn is_two_row(rows: &[Vec<String>], top: usize, blank: &str) -> bool {
    let Some(sub) = rows.get(top + 1) else { return false };
    if !looks_like_header(sub, blank) {
        return false;
    }
    let below_has_body = rows
        .get(top + 2..)
        .is_some_and(|rest| rest.iter().any(|row| looks_like_body(row, blank)));
    if !below_has_body {
        return false;
    }
    let up = filled(&rows[top], blank);
    let down = filled(sub, blank);
    // 상단 행이 하단 행보다 **희소하게** 충전되어 있어야 묶음 명칭이다. 동일하거나
    // 과다하면 2행 머리글이 아니다.
    if up == 0 || down <= up {
        return false;
    }
    // 하단 행이 그 하단 자료만큼 광범위해야 한다. 협소하면 그것도 묶음 행이거나
    // 안내 행이다.
    wide_enough(sub, body_width(&rows[top + 2..], blank), blank)
}

/// 머리글 `rows` 행을 `(묶음, 명칭)` 쌍으로 전개한다.
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
/// **머리글이 실제로 미치는 범위까지만** 산정한다. 시트 폭은 본문의 이탈한 셀
/// 하나로 확장되는데, 그 셀까지 열로 산입하면 확산 충전된 묶음이 부착되어
/// 무의미한 열이 발생한다.
fn spread(rows: &[Vec<String>], at: usize, count: usize) -> Vec<(Option<String>, String)> {
    let head: Vec<&Vec<String>> = (at..at + count).filter_map(|r| rows.get(r)).collect();
    if head.is_empty() {
        return Vec::new();
    }
    let width = head.iter().map(|r| r.len()).max().unwrap_or(0);
    let last = (0..width)
        .rposition(|c| head.iter().any(|r| !Sheet::at(r, c).trim().is_empty()))
        .map(|i| i + 1)
        .unwrap_or(0);

    // 상단 행마다 직전 공백이 아닌 값을 보유한다. 최하단 행은 확산하지 않는다.
    let mut carried: Vec<Option<String>> = vec![None; head.len().saturating_sub(1)];
    let mut out = Vec::with_capacity(last);
    for c in 0..last {
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

/// 머리글의 위치와 행 수를 **지정받아** 구조를 산출한다.
///
/// 추정하지 않는다. 사용자가 조작부에서 지정한 값이 여기로 전달되고, 그대로
/// 수용한다 — 이 그릇은 임의의 양식을 담당하므로 열마다 깊이가 상이하거나
/// 상단에 안내 행이 존재하는 양식을 추정만으로 전부 적중시킬 수 없다. 적중시킬
/// 수 없는 것을 적중한 것처럼 반환하는 편이 더 유해하다.
///
/// 범위가 시트를 이탈하면 **열이 부재하는 구조**를 반환한다. 임의로 보정하면
/// 사용자가 지정한 것과 상이한 표가 묵시적으로 표시된다.
pub fn layout_at(sheet: &Sheet, blank: &str, at: usize, rows: usize) -> Layout {
    if rows == 0 || at + rows > sheet.rows.len() {
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
    spread(rows, at, count)
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

    /// 안내 문장 수 행이 전단에 부착된 양식이 빈번하다. 그 행을 머리글로 채택하면
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

    /// 2행 머리글. 상단 행이 희소하고 하단 행이 조밀하다.
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

    /// 2행 모두 조밀하면 2번째 행은 자료다. 머리글로 채택하면 자료 1행이
    /// 소멸한다.
    #[test]
    fn a_dense_second_row_is_data_not_a_header() {
        let s = sheet(&[&["이름", "지역"], &["가람", "서울"], &["나리", "부산"]]);
        assert_eq!(layout(&s, "-").header, Header::Found { at: 0, rows: 1 });
    }

    /// 머리글 하단에 자료가 부재하면 머리글이 아니다.
    #[test]
    fn a_header_with_nothing_under_it_is_not_a_header() {
        let s = sheet(&[&["이름", "점수"]]);
        let l = layout(&s, "-");
        assert_eq!(l.header, Header::Unknown);
        assert!(l.columns.is_empty(), "머리글을 탐색하지 못했는데 열을 반환했다");
    }

    /// 2행짜리 표에서 2번째 행을 머리글로 채택하면 자료가 소멸한다.
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

    /// `inf` 와 `NaN` 은 Rust 의 파서가 수용하지만, 표에 기재된 그 문자는
    /// 문자다. 숫자로 판정하면 정렬이 무의미한 위치로 이동한다.
    #[test]
    fn inf_and_nan_are_words_not_numbers() {
        let s = sheet(&[&["값"], &["inf"], &["NaN"]]);
        assert_eq!(kinds(&layout(&s, "-")), [Kind::Text]);
    }

    /// 머리글이 숫자뿐인 행은 머리글이 아니다 — 자료 첫 행이다.
    #[test]
    fn a_row_of_numbers_is_not_a_header() {
        let s = sheet(&[&["1", "2"], &["3", "4"], &["5", "6"]]);
        assert_eq!(layout(&s, "-").header, Header::Unknown);
    }

    /// **명칭 없는 열을 생성하지 않는다.** 공백이 존재하는 1행은 대개 2행
    /// 머리글의 상단 행인데, 하단 행을 함께 판정하지 못했다면 선정하지 않는 것이
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

    /* ── 사용자가 지정하는 머리글 ──────────────────────────────── */

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
        // 자료는 머리글 2행 하단부터다. 그래야 종류가 정확히 산출된다.
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

    /// 3행 머리글. **둘로 한정하지 않는다.**
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
