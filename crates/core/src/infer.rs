//! **임의의 표에서 생김새를 알아본다.** 머리글이 어느 줄인지, 두 줄인지,
//! 어느 열이 숫자인지.
//!
//! 정의 파일을 손으로 쓰는 일의 첫 삽이다. 사람이 적어 주면 틀릴 일이 없지만,
//! 아무 xlsx 나 열어 보려면 받아 든 자리에서 알아내야 한다.
//!
//! ## 확신이 없으면 "모름" 이다
//!
//! 이 모듈이 지키는 것은 하나다. **짐작한 것을 아는 것처럼 돌려주지
//! 않는다.** 머리글 줄을 잘못 고르면 자료 한 줄이 열 이름이 되고, 숫자 열을
//! 글자로 보면 정렬이 1 · 10 · 2 가 된다. 둘 다 조용하다 — 화면이 열리고,
//! 값이 보이고, 차례만 틀린다.
//!
//! 그래서 [`Kind::Unknown`] 과 [`Header::Unknown`] 이 있다. 받은 쪽이 사람에게
//! 묻든 기본값을 쓰든 그것은 그쪽 몫이고, **여기서 고르지 않는다.**
//!
//! ## 여기서 알 수 없는 것
//!
//! 뜻은 모른다. 어느 열이 이름이고 어느 열이 점수인지, 어느 열을 사전으로
//! 눌러 담아야 이득인지는 글자만 보고 알 수 없다. 이 모듈은 **생김새**까지다.

use crate::sheet::Sheet;

/// 머리글 아래로 몇 줄까지 들여다보는가. 자료의 생김새를 보는 데 쓴다.
const SAMPLE: usize = 200;

/// 머리글을 찾느라 위에서 훑는 줄 수. 안내 글 몇 줄이 앞에 붙은 양식이 흔하다.
const LOOK: usize = 20;

/// 열 하나가 무엇을 담는가.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// 비지 않은 값이 **전부** 숫자다.
    Number,
    /// 비지 않은 값이 **하나도** 숫자가 아니다.
    Text,
    /// 섞여 있거나, 볼 값이 없다. **고르지 않는다.**
    Unknown,
}

/// 머리글이 어디인가.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Header {
    /// `at` 줄부터 `rows` 줄이 머리글이다.
    ///
    /// 두 줄 이상이면 위쪽 줄은 묶음 명칭이고, 병합된 칸이라 우측으로 흘려
    /// 기재되어 있다. **줄 수를 둘로 한정하지 않는다** — 이 그릇은 임의의
    /// 양식을 수용해야 하고, 몇 줄인지는 최종적으로 사용자가 지정한다.
    Found { at: usize, rows: usize },
    /// 판별 불가. **첫 줄로 추정하지 않는다.**
    Unknown,
}

impl Header {
    /// 자료가 개시되는 줄. 판별 불가이면 없다.
    pub fn body_at(self) -> Option<usize> {
        match self {
            Header::Found { at, rows } => Some(at + rows),
            Header::Unknown => None,
        }
    }
}

/// 열 하나에 대해 알아본 것.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Column {
    /// 화면에 보일 이름. 두 줄 머리글이면 아래 줄이다.
    pub label: String,
    /// 묶음 이름. 두 줄 머리글의 위 줄이고, 한 줄이면 없다.
    pub group: Option<String>,
    pub kind: Kind,
}

/// 표 하나의 생김새.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    pub header: Header,
    /// 머리글을 찾지 못했으면 비어 있다. **첫 줄을 열 이름으로 쓰지 않는다.**
    pub columns: Vec<Column>,
}

/* ── 칸 하나를 어떻게 보는가 ────────────────────────────────────── */

/// 숫자로 읽히는가. **`inf` 와 `NaN` 은 숫자로 보지 않는다** — Rust 의 파서가
/// 받아들이지만, 표에 그 글자가 적혀 있으면 그것은 글자다.
fn is_number(s: &str) -> bool {
    let t = s.trim().replace(',', "");
    if t.is_empty() {
        return false;
    }
    t.parse::<f64>().map(|v| v.is_finite()).unwrap_or(false)
}

/// 값이 없는 칸인가. 양식이 "없음" 을 적는 글자(`blank`)도 빈 것으로 본다.
fn empty(s: &str, blank: &str) -> bool {
    crate::sheet::is_blank(s, blank)
}

/// 머리글 줄로 쓸 만한가. **비지 않은 칸이 있고, 그 가운데 숫자가 없어야**
/// 한다. 머리글에 숫자만 적힌 양식은 드물고, 자료 첫 줄을 머리글로 고르는
/// 것이 이 모듈이 낼 수 있는 가장 나쁜 답이다.
fn looks_like_header(row: &[String], blank: &str) -> bool {
    let filled: Vec<&String> = row.iter().filter(|c| !empty(c, blank)).collect();
    !filled.is_empty() && filled.iter().all(|c| !is_number(c))
}

/// 자료 줄로 쓸 만한가. 비지 않은 칸이 하나라도 있으면 그렇다.
fn looks_like_body(row: &[String], blank: &str) -> bool {
    row.iter().any(|c| !empty(c, blank))
}

fn filled(row: &[String], blank: &str) -> usize {
    row.iter().filter(|c| !empty(c, blank)).count()
}

/// 아래 자료 줄들이 쓰는 칸 수의 가운뎃값.
///
/// 평균이 아니라 가운뎃값인 까닭은, 본문에 엉뚱한 칸 하나가 있는 줄이 섞여도
/// 흔들리지 않기 때문이다. 그런 줄이 실제로 있다.
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

/// **자료보다 절반도 안 되게 좁은 줄은 머리글이 아니다.**
///
/// 안내 글 한 줄이 그렇다 — 맨 위에 "이 표는 …" 한 칸만 적힌 양식이 흔하고,
/// 그 줄도 글자뿐이라 머리글처럼 보인다. 그것을 먹으면 **열 이름이 안내
/// 문장이 되고 나머지 열은 이름이 없어진다.**
///
/// 꼭 같기를 요구하지 않는 까닭은 반대쪽 사고다. 본문에 머리글보다 오른쪽으로
/// 삐져 나온 칸이 있는 양식도 흔한데, 같기를 요구하면 그런 멀쩡한 표에서
/// 머리글을 아예 못 찾는다.
fn wide_enough(row: &[String], under: usize, blank: &str) -> bool {
    let n = filled(row, blank);
    n > 0 && n * 2 > under
}

/// 이름이 없는 열이 가운데 끼어 있는가.
///
/// 한 줄 머리글에 구멍이 있으면 그 열은 이름이 **빈 글자**가 된다. 그것은
/// 대개 두 줄 머리글의 위 줄(묶음 이름)을 혼자 집은 것인데, 아래 줄을 함께
/// 보지 못했다면 **고르지 않는 것이 맞다.** 뒤쪽의 빈 칸은 괜찮다 — 거기까지
/// 세지 않는다.
fn has_a_nameless_column(row: &[String]) -> bool {
    let Some(last) = row.iter().rposition(|c| !c.trim().is_empty()) else { return false };
    row[..last].iter().any(|c| c.trim().is_empty())
}

/* ── 알아보기 ───────────────────────────────────────────────────── */

/// 머리글이 어느 줄인지, 두 줄인지 알아본다.
///
/// 위에서부터 훑어 **처음으로 머리글처럼 보이고 아래에 자료가 있는 줄**을
/// 고른다. 가장 그럴듯한 줄을 점수로 고르지 않는 까닭은, 그 점수가 틀렸을 때
/// 왜 그 줄이 뽑혔는지 아무도 설명할 수 없기 때문이다. 위에서부터라는 규칙은
/// 틀릴 때도 왜 틀렸는지가 보인다.
fn find_header(sheet: &Sheet, blank: &str) -> Header {
    let rows = &sheet.rows;
    let look = rows.len().min(LOOK);
    for r in 0..look {
        if !looks_like_header(&rows[r], blank) {
            continue;
        }
        // 아래에 자료가 있어야 머리글이다. 맨 끝 줄은 머리글이 아니다.
        let Some(below) = rows.get(r + 1..) else { continue };
        if !below.iter().any(|row| looks_like_body(row, blank)) {
            continue;
        }
        if is_two_row(rows, r, blank) {
            return Header::Found { at: r, rows: 2 };
        }
        // 두 줄이 아니라면 이 줄 하나가 모든 열의 이름이어야 한다.
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

/// `top` 과 그 아래 줄이 함께 머리글인가.
///
/// **흘려 채운 모양으로 가린다.** 교육청 원본처럼 두 줄짜리 머리글은 위 줄이
/// 묶음 이름이라 **띄엄띄엄**하고(`전형개요` 하나가 여러 열에 걸친다), 아래
/// 줄이 **촘촘**하다. 둘 다 촘촘하면 둘째 줄은 머리글이 아니라 자료다.
///
/// 아래 줄에 숫자가 섞여 있으면 그것도 자료다. 그리고 **그 아래로 자료가 더
/// 있어야** 한다 — 두 줄짜리 표에서 둘째 줄을 머리글로 먹으면 자료가 없어진다.
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
    // 위 줄이 아래 줄보다 **적게** 채워져 있어야 묶음 이름이다. 같거나 많으면
    // 그냥 두 줄짜리 머리글이 아니다.
    if up == 0 || down <= up {
        return false;
    }
    // 아래 줄이 그 아래 자료만큼 넓어야 한다. 좁으면 그것도 묶음 줄이거나
    // 안내 줄이다.
    wide_enough(sub, body_width(&rows[top + 2..], blank), blank)
}

/// 머리글 `rows` 줄을 `(묶음, 명칭)` 짝으로 전개한다.
///
/// 상단 줄들은 병합된 묶음 명칭이라 우측으로 흘려 채운다. 최하단 줄은 각 열의
/// 고유 명칭이므로 흘리지 않는다 — 흘리면 비어 있는 칸에 좌측 명칭이 복제되어
/// 동일 명칭의 열이 두 개 발생한다.
///
/// **깊이가 열마다 상이한 양식을 수용한다.** 실제 양식은 좌측 다수가 1단이고
/// 우측 일부만 2단인 경우가 흔하다(`학년`·`반` 은 상단에만, `국어 선택과목` 은
/// 2단). 그래서 각 열의 명칭 조각을 상단부터 모아 **공백을 제거한 뒤** 최후
/// 조각을 명칭으로, 나머지를 묶음으로 채택한다. 1단 열은 묶음이 없는 열이 된다.
///
/// **머리글이 실제로 미치는 범위까지만** 산정한다. 시트 폭은 본문의 이탈한 칸
/// 하나로 확장되는데, 그 칸까지 열로 산입하면 흘려 채운 묶음이 부착되어 무의미한
/// 열이 발생한다.
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

    // 상단 줄마다 직전 비어 있지 않은 값을 보유한다. 최하단 줄은 흘리지 않는다.
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
        // 상하 줄에 동일 문구가 반복되는 양식이 있다. 중복은 한 번만 취한다.
        parts.dedup();
        let label = parts.pop().unwrap_or_default();
        let group = (!parts.is_empty()).then(|| parts.join(" "));
        out.push((group, label));
    }
    out
}

/// 열 하나가 숫자인지 글자인지. **섞여 있으면 고르지 않는다.**
///
/// 값의 생김새만 본다. "없음" 을 `-` 로 적는 양식이 흔하므로 그 글자는 빈 것
/// 으로 세고, 그러지 않으면 숫자 열이 죄다 섞인 것으로 보인다.
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
        // 볼 값이 없다. 빈 열을 글자로 두면 나중에 숫자가 들어오는 날 조용히
        // 틀린 차례로 정렬된다.
        (0, 0) => Kind::Unknown,
        (_, 0) => Kind::Number,
        (0, _) => Kind::Text,
        // 섞였다. 받은 쪽이 정한다.
        _ => Kind::Unknown,
    }
}

/// 표 하나의 생김새를 알아본다.
///
/// `blank` 는 양식이 "없음" 을 적는 글자다(`-` 같은 것). 모르면 빈 글자를
/// 넘긴다 — 그때는 비어 있는 칸만 빈 것으로 센다.
pub fn layout(sheet: &Sheet, blank: &str) -> Layout {
    let header = find_header(sheet, blank);
    let columns = match header {
        Header::Unknown => Vec::new(),
        Header::Found { at, rows } => columns_of(sheet, blank, at, rows),
    };
    Layout { header, columns }
}

/// 머리글의 위치와 줄 수를 **지정받아** 생김새를 산출한다.
///
/// 추정하지 않는다. 사용자가 조작부에서 지정한 값이 여기로 전달되고, 그대로
/// 수용한다 — 이 그릇은 임의의 양식을 담당하므로 열마다 깊이가 상이하거나
/// 상단에 안내 줄이 존재하는 양식을 추정만으로 전부 적중시킬 수 없다. 적중시킬
/// 수 없는 것을 적중한 것처럼 반환하는 편이 더 해롭다.
///
/// 범위가 시트를 벗어나면 **열이 없는 생김새**를 반환한다. 적당히 보정하면
/// 사용자가 지정한 것과 상이한 표가 조용히 표시된다.
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
        Sheet { rows: rows.iter().map(|r| r.iter().map(|c| c.to_string()).collect()).collect() }
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

    /// 안내 글 몇 줄이 앞에 붙은 양식이 흔하다. 그 줄을 머리글로 먹으면 열
    /// 이름이 안내 문장이 된다.
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

    /// 두 줄 머리글. 위 줄이 띄엄띄엄하고 아래 줄이 촘촘하다.
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

    /// 두 줄 다 촘촘하면 둘째 줄은 자료다. 머리글로 먹으면 자료 한 줄이
    /// 사라진다.
    #[test]
    fn a_dense_second_row_is_data_not_a_header() {
        let s = sheet(&[&["이름", "지역"], &["가람", "서울"], &["나리", "부산"]]);
        assert_eq!(layout(&s, "-").header, Header::Found { at: 0, rows: 1 });
    }

    /// 머리글 아래로 자료가 없으면 머리글이 아니다.
    #[test]
    fn a_header_with_nothing_under_it_is_not_a_header() {
        let s = sheet(&[&["이름", "점수"]]);
        let l = layout(&s, "-");
        assert_eq!(l.header, Header::Unknown);
        assert!(l.columns.is_empty(), "머리글을 못 찾았는데 열을 돌려주었다");
    }

    /// 두 줄짜리 표에서 둘째 줄을 머리글로 먹으면 자료가 없어진다.
    #[test]
    fn two_rows_alone_do_not_make_a_two_row_header() {
        let s = sheet(&[&["전형개요", "", "", "모집"], &["지역", "대학", "전형명", "인원"]]);
        assert_eq!(layout(&s, "-").header, Header::Unknown);
    }

    /// **섞인 열은 고르지 않는다.** 글자로 두면 정렬이 1 · 10 · 2 가 되고,
    /// 숫자로 두면 글자 값이 사라진다. 둘 다 조용하다.
    #[test]
    fn a_mixed_column_is_unknown() {
        let s = sheet(&[&["값"], &["90"], &["미정"], &["75"]]);
        assert_eq!(kinds(&layout(&s, "-")), [Kind::Unknown]);
    }

    /// "없음" 표기는 빈 것으로 센다. 그러지 않으면 숫자 열이 죄다 섞인 것으로
    /// 보인다 — 이 양식은 숫자 열에도 `-` 가 잔뜩 있다.
    #[test]
    fn the_blank_token_does_not_make_a_number_column_mixed() {
        let s = sheet(&[&["정시백분위"], &["90"], &["-"], &["75"]]);
        assert_eq!(kinds(&layout(&s, "-")), [Kind::Number]);
        // 그 글자를 모른다고 하면 섞인 것으로 보인다. 그때도 고르지 않는다.
        assert_eq!(kinds(&layout(&s, "")), [Kind::Unknown]);
    }

    /// 볼 값이 없는 열도 모름이다. 글자로 두면 나중에 숫자가 들어오는 날
    /// 조용히 틀린 차례로 정렬된다.
    #[test]
    fn an_empty_column_is_unknown() {
        let s = sheet(&[&["이름", "비고"], &["가", ""], &["나", "-"]]);
        assert_eq!(kinds(&layout(&s, "-")), [Kind::Text, Kind::Unknown]);
    }

    /// 천 단위 쉼표가 붙은 수도 숫자다. 사람이 적는 표에 흔하다.
    #[test]
    fn thousands_separators_are_still_numbers() {
        let s = sheet(&[&["인원"], &["1,200"], &["980"]]);
        assert_eq!(kinds(&layout(&s, "-")), [Kind::Number]);
    }

    /// `inf` 와 `NaN` 은 Rust 의 파서가 받아들이지만, 표에 적힌 그 글자는
    /// 글자다. 숫자로 보면 정렬이 뜻 없는 자리로 간다.
    #[test]
    fn inf_and_nan_are_words_not_numbers() {
        let s = sheet(&[&["값"], &["inf"], &["NaN"]]);
        assert_eq!(kinds(&layout(&s, "-")), [Kind::Text]);
    }

    /// 머리글이 숫자뿐인 줄은 머리글이 아니다 — 자료 첫 줄이다.
    #[test]
    fn a_row_of_numbers_is_not_a_header() {
        let s = sheet(&[&["1", "2"], &["3", "4"], &["5", "6"]]);
        assert_eq!(layout(&s, "-").header, Header::Unknown);
    }

    /// **이름 없는 열을 만들지 않는다.** 구멍이 있는 한 줄은 대개 두 줄
    /// 머리글의 위 줄인데, 아래 줄을 함께 보지 못했다면 고르지 않는 것이 맞다.
    #[test]
    fn a_header_with_a_hole_is_not_chosen() {
        let s = sheet(&[&["전형개요", "", "모집"], &["서울", "가대", "30"], &["부산", "나대", "20"]]);
        let l = layout(&s, "-");
        assert_eq!(l.header, Header::Unknown);
        assert!(l.columns.is_empty(), "이름이 빈 열을 돌려주었다");
    }

    /// 빈 시트에서 아무것도 고르지 않는다.
    #[test]
    fn an_empty_sheet_gives_nothing() {
        let l = layout(&Sheet::default(), "-");
        assert_eq!(l.header, Header::Unknown);
        assert!(l.columns.is_empty());
    }

    /* ── 사용자가 지정하는 머리글 ──────────────────────────────── */

    /// **깊이가 열마다 상이한 양식.** 좌측은 1단, 우측만 2단이다. 실재하는
    /// 진학결과 양식의 형태이고, 추정은 여기서 실패한다 — 상단 줄이 하단 줄보다
    /// 조밀해서 "묶음 줄" 로 보이지 않기 때문이다.
    #[test]
    fn a_header_whose_depth_differs_per_column_is_refused_by_the_guess() {
        let s = sheet(&[
            &["학년", "반", "국어", "", "", "영어"],
            &["", "", "선택과목", "표준점수", "등급", "등급"],
            &["3", "1", "언어와매체", "104", "3", "2"],
            &["3", "2", "화법과작문", "99", "4", "1"],
        ]);
        // 추정은 거부한다. 상단 줄에 이름 없는 열이 중간에 존재한다.
        assert_eq!(layout(&s, "-").header, Header::Unknown);
    }

    /// 그 양식도 **사용자가 2줄이라고 지정하면** 제대로 전개된다.
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
        // 1단 열은 묶음이 없고, 2단 열만 묶음을 보유한다.
        assert_eq!(labels(&l), ["학년", "반", "선택과목", "표준점수", "등급", "등급"]);
        let groups: Vec<Option<&str>> =
            l.columns.iter().map(|c| c.group.as_deref()).collect();
        assert_eq!(
            groups,
            [None, None, Some("국어"), Some("국어"), Some("국어"), Some("영어")]
        );
        // 자료는 머리글 두 줄 아래부터다. 그래야 종류가 올바르게 나온다.
        assert_eq!(kinds(&l), [Kind::Number, Kind::Number, Kind::Text, Kind::Number, Kind::Number, Kind::Number]);
    }

    /// 1줄이라고 지정하면 둘째 줄은 **자료가 된다.** 지정을 그대로 수용한다는
    /// 뜻이고, 그래서 사람이 틀리게 지정하면 틀린 표가 나온다 — 다만 그것은
    /// 화면에 보이고 되돌릴 수 있다.
    #[test]
    fn saying_one_row_makes_the_second_row_data() {
        let s = sheet(&[
            &["학년", "반", "국어", "", "", "영어"],
            &["", "", "선택과목", "표준점수", "등급", "등급"],
            &["3", "1", "언어와매체", "104", "3", "2"],
        ]);
        let l = layout_at(&s, "-", 0, 1);
        assert_eq!(l.header, Header::Found { at: 0, rows: 1 });
        // 병합 때문에 비어 있던 칸은 **이름 없는 열**이 된다. 여기서 임의로
        // 메우지 않는다 — 이름이 없다는 사실을 그대로 전달하고, 표시용 명칭은
        // 받는 쪽이 부여한다.
        assert_eq!(labels(&l), ["학년", "반", "국어", "", "", "영어"]);
    }

    /// 상단에 제목 줄이 부착된 양식. 시작 줄을 지정할 수 있어야 한다.
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

    /// **범위를 벗어난 지정은 열을 내지 않는다.** 보정해서 다른 표를 조용히
    /// 표시하지 않는다.
    #[test]
    fn a_span_past_the_end_of_the_sheet_gives_nothing() {
        let s = sheet(&[&["대학", "전형"], &["마루대", "교과"]]);
        assert_eq!(layout_at(&s, "-", 1, 5).header, Header::Unknown);
        assert!(layout_at(&s, "-", 1, 5).columns.is_empty());
        assert_eq!(layout_at(&s, "-", 0, 0).header, Header::Unknown);
    }

    /// 상하 줄에 동일 문구가 반복되는 양식. 명칭을 중복해서 붙이지 않는다.
    #[test]
    fn a_label_repeated_on_both_rows_is_taken_once() {
        let s = sheet(&[&["대학", "점수"], &["대학", "총점"], &["마루대", "91"]]);
        let l = layout_at(&s, "-", 0, 2);
        assert_eq!(labels(&l), ["대학", "총점"]);
        assert_eq!(l.columns[0].group, None);
        assert_eq!(l.columns[1].group.as_deref(), Some("점수"));
    }

    /// 3줄 머리글. **둘로 한정하지 않는다.**
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

    /// 본문의 엉뚱한 칸 하나 때문에 열이 하나 더 생기지 않는다.
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
