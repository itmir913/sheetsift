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

use crate::{
    value::{is_blank_sql, is_number_sql, not_a_number_sql, number_sql},
    Error, ErrorCode, Result,
};

/// 1회에 수령 가능한 행 수의 상한.
///
/// **부르는 곳이 인지해야 하는 값이다.** 이보다 큰 수를 주입하면 묵시적으로
/// 여기까지만 도달한다 — 인지하지 못한 채 1000 을 요청한 화면은 500행을 수령하고도
/// 전량 수령한 것으로 판단한다.
pub const MAX_LIMIT: u32 = 500;
/// 값 선택 목록의 상한. 초과하면 절단해 전송하고 검색으로 축소하게 한다.
///
/// **이 수는 묘사하는 쪽이 결정한다.** 목록을 산출하는 비용은 고유값 수가
/// 아니라 자료 전 행의 집약이 지배하므로 상한을 상향해도 바탕은 변동하지 않는다.
/// 반면 화면은 항목마다 확인란을 묘사하므로, 그 수가 상한을 결정한다 —
/// 묘사하는 범위만 생성하게 한 이후(`ColumnFilter.vue`) 1만이 성립한다.
/// 엑셀의 필터 목록 상한도 1만이다.
///
/// **32,766 을 초과할 수 없다.** 선택한 것을 열거하는 조건이 그 수만큼의 값을
/// 조건에 기재하는데, 그것이 SQLite 의 바인딩 상한이다. 전체에서 해제하는 쪽은
/// 제외 목록으로 기재하므로 그 상한과 무관하다.
pub const MAX_VALUES: u32 = 10_000;
/// 값이 부재하는 셀을 지시하는 표식. 실제 자료의 공백 셀·NULL 에 대응한다.
///
/// **자료에서 유래할 수 없는 문자다.** 사람이 판독할 문구를 기재했더니 동일한
/// 문구가 셀에 기재된 양식에서 양자가 병합되어, 목록에 한 항목으로 출현하고
/// 선택하면 상이한 행들이 함께 통과했다. 유니코드가 문자로 배정하지 않는
/// 위치(noncharacter)는 양식이 산출하지 않는다.
///
/// 이것은 **수수하는 값이지 묘사하는 문구가 아니다.** 화면은 자체 사전의
/// 문구로 치환해 묘사한다(`i18n` 의 `filter.blank`).
pub const BLANK_TOKEN: &str = "\u{FFFF}";

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
    /// 별칭이 부재하는 열 참조. 색인 정의에 사용한다 — 거기에는 테이블 별칭을
    /// 기재할 수 없다.
    pub fn plain_ref(&self) -> String {
        format!("\"{}\"", self.column)
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

/// 그 조건을 **어느 조작이 기재했는가.**
///
/// 값 선택 목록은 「그 열 자신의 선택」을 제외해 산출한다 — 그러지 않으면 하나를
/// 고른 순간 후보가 그 하나로 축소되어 선택을 변경할 수단이 소멸한다. 그런데
/// **무엇이 「자신의 것」인지를 열 명칭으로 추정했다.** 한 열을 조작부 둘이
/// 좁히는 화면에서 그 추정이 파손된다 — 상위의 검색 조건 카드가 `대학` 을
/// 좁혀 표는 축소되었는데, 그 열의 깔대기는 전체 대학을 전체 건수와 함께
/// 열거한다. 엔진은 카드와 깔대기를 판별할 수단을 보유하지 않았다.
///
/// 그래서 **추정을 중단하고 조건 자체가 진술한다.** 이 저장소의 화면도 한 열을
/// 조작부 둘이 좁히는 날 동일한 상황에 도달하므로, 받는 앱의 특례가 아니라
/// 엔진의 개념으로 배치한다.
///
/// 기재하지 않으면 [`Origin::OwnPicker`] 다 — **종전과 동일하게 동작한다.**
/// 반입하는 앱이 아무것도 변경하지 않았을 때 동작이 변동하는 쪽은, 그 변동이
/// 자기 선택에 갇히는 형태로 발현한다(고른 값 하나만 남은 목록). 진술을
/// 추가하는 쪽이 그것을 인지하고 기재하는 작업이다.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Origin {
    /// 그 열의 값 선택 팝오버 자신이 기재한 조건. **그 열의 값 목록에서
    /// 제외한다**(범위와 `not_blank` 은 적용한다).
    #[default]
    OwnPicker,
    /// 그 열 외부의 조작이 기재한 조건(상위의 검색 조건 카드 등). **그 열의 값
    /// 목록에도 적용한다** — 표가 축소되었는데 목록만 전체를 열거하면, 사람은
    /// 거기 열거된 값을 선택해 놓고 0건을 수령한다.
    Elsewhere,
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
    /// **제외할 값들.** 전체에서 몇 개를 해제한 상태를 그대로 기재한다.
    ///
    /// 선택한 것을 열거하는 쪽([`values`](Self::values))만 존재하면, 목록이
    /// 상한에 걸려 절단된 열에서 "하나만 제외" 를 기재할 수 없다 — 노출되지
    /// 않은 값까지 함께 제외된다. 제외를 그대로 기재하면 목록의 분량과 무관
    /// 하게 정확하고, 조건에 기재하는 값도 해제한 수만큼이다.
    ///
    /// 양자를 동시에 기재하면 **중단한다.** 우선순위를 묵시적으로 부여하면
    /// 어느 쪽이 적용되었는지 사람도 코드도 판별할 수 없다.
    #[serde(default)]
    pub excluded: Option<Vec<String>>,
    /// 이 열이 **공백이 아닌 행만** 선택한다.
    ///
    /// 도형을 묘사하는 화면이 사용한다. 값이 부재하는 행은 막대를 묘사할 수 없는데,
    /// 그런 행까지 산입해 페이지를 분할하면 전체가 공백인 페이지가 발생한다.
    #[serde(default)]
    pub not_blank: bool,
    /// **숫자 열의 하한.** 이 값 이상인 행만 통과시킨다(경계를 포함한다).
    ///
    /// 열 전체가 숫자로 판독되는 값만 통과한다 — 공백과 `총 44850점` 같은 문자는
    /// 하한이나 상한이 하나라도 기재되면 탈락한다. 공백을 고르려면 범위가 아니라 값
    /// 선택(`values` 에 `BLANK_TOKEN`)으로 기재한다. 범위가 걸린 열의 값 목록
    /// ([`column_values`])에는 공백이 없다.
    ///
    /// 숫자 열이 아닌 열에 기재하면 **중단한다.** 문자 열에 숫자 비교를 적용하면
    /// 모든 행이 탈락하는데, 그것이 오류로 출현하지 않는다.
    #[serde(default)]
    pub min: Option<f64>,
    /// **숫자 열의 상한.** 이 값 이하인 행만 통과시킨다(경계를 포함한다). 하나만
    /// 기재해도 유효하나, 하한이 상한을 초과하면 중단한다.
    #[serde(default)]
    pub max: Option<f64>,
    /// **이 조건을 기재한 조작.** 값 목록이 자기 선택을 제외할 때 판별한다.
    /// 기재하지 않으면 그 열 자신의 팝오버로 간주한다([`Origin`]).
    #[serde(default)]
    pub origin: Origin,
}

impl Filter {
    pub fn is_empty(&self) -> bool {
        self.contains.trim().is_empty()
            && self.values.is_none()
            && self.excluded.is_none()
            && !self.not_blank
            && self.min.is_none()
            && self.max.is_none()
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

/// 숫자 열의 최소와 최대.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct NumberSpan {
    pub min: f64,
    pub max: f64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnValues {
    pub values: Vec<ColumnValue>,
    /// [`MAX_VALUES`] 를 초과해 절단되었는가.
    pub truncated: bool,
    /// 숫자 열의 **자료가 존재하는 범위.** 숫자 열이 아니거나 숫자로 판독되는 값이
    /// 하나도 부재하거나, **축소 문자가 있는 요청이면** `None` 이다. 그 열 자신의 필터와 축소 문자를 제외하고 산출한다
    /// — 값 목록과 같은 규칙이다.
    pub range: Option<NumberSpan>,
    pub elapsed_ms: f64,
}

/// `LIKE` 에서 의미를 보유하는 문자를 차단한다.
pub fn like_escape(s: &str) -> String {
    s.replace('\\', r"\\").replace('%', r"\%").replace('_', r"\_")
}

/// **값 선택 목록의 순서.** 이 상자 밖에서도 그 목록을 구성하므로 노출한다.
///
/// 목록을 Rust 로 구성하는 곳이 `a.cmp(b)` 로 정렬하면, 동일한 열이 한 화면에서
/// `1 · 10 · 100` 이고 타 화면에서 `1 · 2 · 10 · 100` 이 된다. 사람은 목록에서
/// 판독한 순서로 표를 탐색할 수 없고, 고유값이 다수인 열에서는 [`MAX_VALUES`]
/// 의 절단이 **상이한 항목을 폐기한다.**
///
/// 순서는 넷이다.
/// 1. **빈칸이 선두다.** 표기가 둘 이상이어도(`NULL`·공백 문자·표식) 인접하므로
///    하나로 병합되고, 절단이 발생해도 목록에서 소멸하지 않는다.
/// 2. 수치 열이면 **수로 판독되는 값이 그 다음**이고, 수의 크기순이다.
/// 3. 수로 판독되지 않는 값이 그 후위다 — 문자순이다.
/// 4. 동일한 수치인 표기(`90` 과 `90.0`)의 순서는 문자로 고정한다.
///
/// 문자 열이면 1 과 4 만 작용한다.
///
/// `numeric` 은 **그 열이 수치 열인가**이고(열의 종류는 정의가 결정한다),
/// `blank` 는 양식이 "없음" 을 기재하는 문자다. 무엇을 수로 보고 무엇을 빈칸으로
/// 보는지는 [`crate::value`] 하나가 판정한다.
///
/// SQL 로 동일한 순서를 산출하는 것은 [`values_order_sql`] 이고, 양자가 동일한
/// 순서를 산출하는지는 이 모듈의 시험이 대조한다.
pub fn compare_values(a: &str, b: &str, blank: &str, numeric: bool) -> std::cmp::Ordering {
    use crate::value::{as_real, is_blank};
    use std::cmp::Ordering;

    // ① 빈칸이 선두다.
    let order = is_blank(b, blank).cmp(&is_blank(a, blank));
    if order != Ordering::Equal {
        return order;
    }
    if numeric {
        let (x, y) = (as_real(a), as_real(b));
        // ② 수가 선두이고, ③ 수의 크기순이다.
        let order = y.is_some().cmp(&x.is_some());
        if order != Ordering::Equal {
            return order;
        }
        if let (Some(x), Some(y)) = (x, y) {
            // 양자 모두 유한하다 — `as_real` 이 유한하지 않은 값을 판독하지 않는다.
            let order = x.partial_cmp(&y).unwrap_or(Ordering::Equal);
            if order != Ordering::Equal {
                return order;
            }
        }
    }
    // ④ 나머지는 문자순이다.
    a.cmp(b)
}

/// **행 정렬의 순서.** `ORDER BY` 가 기재하는 것과 동일한 순서를 Rust 로
/// 산출한다([`order_keys`] 의 거울).
///
/// 행을 Rust 로 보유한 화면(인쇄·내보내기)이 자체로 정렬하면, 동일한 자료가
/// 화면마다 상이한 순서로 묘사된다 — 그것은 오류로 출현하지 않는다.
///
/// [`compare_values`] 로 대체할 수 없다. 그쪽은 **값 선택 목록**의 순서라 공백을
/// 선두에 배치하고, 동일한 수치의 표기를 문자로 고정한다. 행의 순서는 공백을
/// **후위**에 배치하고(방향과 무관하다), 수로 판독되지 않는 값을 수 뒤에 둔다.
///
/// 규칙 — ① 공백은 방향과 무관하게 후위. ② 수치 열이면 수로 판독되지 않는 값이
/// 방향과 무관하게 수 뒤. ③ 그 다음이 방향을 적용하는 비교다.
///
/// `numeric` 은 **수치로 정렬하는 열인가**다. 사전 인코딩 열은 조회 문이 id 로
/// 정렬하므로(id 가 가나다순이다) 수치 열이어도 `false` 를 전달한다.
pub fn compare_sorted(
    a: &str,
    b: &str,
    blank: &str,
    numeric: bool,
    dir: SortDir,
) -> std::cmp::Ordering {
    use crate::value::{is_blank, is_number};
    // ① 값 부재는 방향과 무관하게 후위다. 무엇이 부재인지는 자료가 결정한다.
    let (x, y) = (is_blank(a, blank), is_blank(b, blank));
    if x != y {
        return x.cmp(&y);
    }
    let ord = if numeric {
        /* ② 수로 판독되지 않는 값은 방향과 무관하게 수 뒤다. 그러지 않으면
           `총 44850점` 같은 합계 행이 변환값 0 으로 0 과 1 사이에 삽입되는데,
           표를 판독하는 사람에게는 그것이 0점으로 보인다. */
        let (nx, ny) = (!is_number(a), !is_number(b));
        if nx != ny {
            return nx.cmp(&ny);
        }
        /* ③ 수치 비교. **수가 아닌 값들 상호간도 이 변환이 순서를 결정한다** —
           조회 문의 열쇠가 `CAST(…)` 이고, 거기에는 문자 비교가 후행하지 않는다. */
        crate::value::cast_real(a)
            .partial_cmp(&crate::value::cast_real(b))
            .unwrap_or(std::cmp::Ordering::Equal)
    } else {
        // 문자 비교. SQLite 의 기본 조합(BINARY)이 바이트 비교다.
        a.cmp(b)
    };
    match dir {
        SortDir::Asc => ord,
        SortDir::Desc => ord.reverse(),
    }
}

/// 값 선택 목록의 정렬 열쇠. [`compare_values`] 를 SQL 로 기재한 것이다.
fn values_order_sql(expr: &str, blank: &str, numeric: bool) -> String {
    let first = format!("{} DESC", is_blank_sql(expr, blank));
    if numeric {
        /* **수로 판독되지 않는 값은 변환하지 않는다.** SQLite 의 변환은 판독하지
           못한 문자에서 접두사의 수를 산출하므로(`7점` → 7), 그대로 정렬 열쇠에
           기재하면 그 집단이 **허위의 수로** 정렬된다. 수를 판독하는 문은 하나이고
           (`crate::value::as_real`), 그 문이 판독하지 않는 값에는 수가 부재한다.
           전 행이 NULL 인 열쇠는 순서에 작용하지 않고, 그 집단은 문자순이 된다. */
        let num = format!("CASE WHEN {} THEN {} END", is_number_sql(expr), number_sql(expr));
        format!("{first}, {}, {num}, {expr}", not_a_number_sql(expr))
    } else {
        format!("{first}, {expr}")
    }
}

/// 요청이 지시한 열을 사용할 수 없다. **사유마다 코드가 상이하다** — 코드가
/// 하나이면 화면이 그 사유를 진술할 수 없고(`detail` 은 영문이고 노출되지
/// 않는다), 수령하는 앱이 사유마다 상이한 문구를 부여할 수단이 부재하다.
fn bad_column(code: ErrorCode, table: &dyn Table, key: &str, what: &str) -> Error {
    Error::new(code)
        .with("key", key)
        .with("screen", table.id())
        .detail(what)
}

/// 분해해 둔 검사 하나를 WHERE 조각으로 변환한다. [`Match::row`] 와 **동일한
/// 분해의 타 번역**이다 — 조건을 판독하는 코드는 [`Match::compile`] 하나다.
fn filter_clause(col: &Column, f: &Test, blank: &str, binds: &mut Vec<String>) -> Option<String> {
    let base = col.base_ref();
    let mut parts: Vec<String> = Vec::new();

    // ① 열 검색 (부분일치)
    let needle = f.contains.as_str();
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
        if values.nothing() {
            // 전부 해제 → 어떠한 행도 통과하지 않는다.
            return Some("0 = 1".to_string());
        }
        let has_blank = values.blank;
        let concrete: Vec<&String> = values.concrete.iter().collect();

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

    // ③ 제외 목록 (전체에서 해제한 것들)
    if let Some(excluded) = &f.excluded {
        if !excluded.nothing() {
            let has_blank = excluded.blank;
            let concrete: Vec<&String> = excluded.concrete.iter().collect();

            let mut all: Vec<String> = Vec::new();
            if !concrete.is_empty() {
                let holes = vec!["?"; concrete.len()].join(", ");
                for v in &concrete {
                    binds.push((*v).clone());
                }
                /* **값이 부재하는 셀이 함께 탈락하지 않게 한다.** `NOT IN` 은
                   NULL 과의 비교가 NULL 이라 그 행이 통과하지 못한다 — 공백을
                   제외하지 않았는데 공백 행이 소멸한다. */
                all.push(if col.dict {
                    format!("({base} IS NULL OR {base} NOT IN (SELECT id FROM dict WHERE s IN ({holes})))")
                } else {
                    format!("({base} IS NULL OR {base} NOT IN ({holes}))")
                });
            }
            if has_blank {
                all.push(format!(
                    "NOT {}",
                    if col.dict { format!("({base} IS NULL)") } else { is_blank_sql(&base, blank) }
                ));
            }
            parts.push(format!("({})", all.join(" AND ")));
        }
    }

    // ④ 공백이 아닌 행만
    if f.not_blank {
        parts.push(if col.dict {
            format!("{base} IS NOT NULL")
        } else {
            format!("NOT {}", is_blank_sql(&base, blank))
        });
    }

    // ⑤ 숫자 범위. 열 전체가 수치로 판독되는 값만 통과한다.
    if f.min.is_some() || f.max.is_some() {
        let inside = |expr: &str, binds: &mut Vec<String>| -> String {
            let num = number_sql(expr);
            let mut conds = vec![is_number_sql(expr)];
            /* 값을 SQL 에 기재하지 않고 바인딩한다. `CAST` 로 받는 쪽이 셀 값을 변환하는
               것과 **같은 변환**을 거치므로, 경계의 값이 부동소수 표기 때문에 어긋나지 않는다. */
            if let Some(lo) = f.min {
                binds.push(format!("{lo}"));
                conds.push(format!("{num} >= CAST(? AS REAL)"));
            }
            if let Some(hi) = f.max {
                binds.push(format!("{hi}"));
                conds.push(format!("{num} <= CAST(? AS REAL)"));
            }
            conds.join(" AND ")
        };
        parts.push(if col.dict {
            format!("{base} IN (SELECT id FROM dict WHERE {})", inside("s", binds))
        } else {
            format!("({})", inside(&base, binds))
        });
    }

    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" AND "))
    }
}

/// 숫자 열의 자료가 존재하는 범위. `where_sql` 은 그 열 자신의 필터를 제외한 조건이다.
///
/// 사전 열은 행마다가 아니라 **고유한 id 마다** 변환한다. 고유값 수가 행 수보다 훨씬
/// 적고 사전은 기본 열쇠 탐색이다.
fn number_span(
    conn: &Connection,
    table: &dyn Table,
    col: &Column,
    where_sql: &str,
    params: &[&dyn rusqlite::ToSql],
) -> Result<Option<NumberSpan>> {
    let base = col.base_ref();
    let glue = if where_sql.is_empty() { " WHERE " } else { " AND " };
    let sql = if col.dict {
        let (num, is_num) = (number_sql("d.s"), is_number_sql("d.s"));
        format!(
            "SELECT MIN({num}), MAX({num}) FROM \
             (SELECT DISTINCT {base} AS v FROM {table} t{where_sql}) c \
             JOIN dict d ON d.id = c.v WHERE {is_num}",
            table = table.base(),
        )
    } else {
        let (num, is_num) = (number_sql(&base), is_number_sql(&base));
        format!(
            "SELECT MIN({num}), MAX({num}) FROM {table} t{where_sql}{glue}{is_num}",
            table = table.base(),
        )
    };
    let (lo, hi): (Option<f64>, Option<f64>) = conn
        .prepare_cached(&sql)?
        .query_row(params, |r| Ok((r.get(0)?, r.get(1)?)))?;
    Ok(match (lo, hi) {
        (Some(min), Some(max)) if min.is_finite() && max.is_finite() => {
            Some(NumberSpan { min, max })
        }
        _ => None,
    })
}

/// 조건이 지시한 열이 잘못되었다. **표를 인지하지 않는다** — 어느 표에서
/// 발생했는지는 조회를 수행하는 곳이 부착한다(`bad_column` 과 동일한 항목을
/// 부착해 문구가 변동하지 않는다).
fn bad_filter(key: &str, what: &str) -> Error {
    Error::new(ErrorCode::InvalidFilterKey).with("key", key).detail(what)
}

/// 범위가 잘못되었다. 코드가 범위 전용인 사유는 `bad_column` 의 호출부에 있다.
fn bad_range(key: &str, what: &str) -> Error {
    Error::new(ErrorCode::InvalidRange).with("key", key).detail(what)
}

/* ── 조건의 분해 ─────────────────────────────────────────────────
   **조회 문과 행 하나의 판정이 같은 분해에서 유래한다.**

   SQL 은 SQL 로 존치된다 — 닫힘(closure)은 WHERE 절이 될 수 없어 「함수 하나를
   양측이 호출한다」는 성립하지 않는다. 성립하는 것은 **분해 하나, 번역 둘**
   이다. 요청을 1회 분해해([`Match::compile`]) 그것을 WHERE 절로 번역하거나
   ([`Match::where_sql`]) 셀 묶음에 적용한다([`Match::row`]).

   조건 하나를 판정하는 작은 함수를 반출하는 쪽은 **실제로 어긋나는 층을
   존치한다** — 조건을 어떻게 결합하는지(필터 상호간은 AND, `keys` 상호간은 OR)
   와 무엇을 오류로 간주하는지가 양측에 두 벌로 남는다. 어긋남은 거기 소재한다.
   그래서 거절도 전부 `compile` 에 집약한다. */

/// 선택하거나 제외한 값들. 공백 표식을 별도로 보유한다 — 그것은 자료의 값이
/// 아니라 「값이 부재하는 셀」을 지시하는 표식이다([`BLANK_TOKEN`]).
#[derive(Debug, Clone)]
struct Picks {
    blank: bool,
    concrete: Vec<String>,
}

impl Picks {
    fn of(values: &[String]) -> Picks {
        Picks {
            blank: values.iter().any(|v| v == BLANK_TOKEN),
            concrete: values.iter().filter(|v| *v != BLANK_TOKEN).cloned().collect(),
        }
    }
    /// 아무 값도 기재되지 않았는가.
    fn nothing(&self) -> bool {
        !self.blank && self.concrete.is_empty()
    }
    fn has(&self, cell: &str, blank: &str) -> bool {
        (self.blank && crate::value::is_blank(cell, blank))
            || self.concrete.iter().any(|v| v == cell)
    }
}

/// 셀 하나에 적용하는 검사. **열이 무엇인지와 무관한 부분**이다.
#[derive(Debug, Clone)]
struct Test {
    /// 축소 문자. 선후의 공백을 제거해 둔다. 공백이면 적용하지 않는다.
    contains: String,
    values: Option<Picks>,
    excluded: Option<Picks>,
    not_blank: bool,
    min: Option<f64>,
    max: Option<f64>,
}

impl Test {
    /// 그 셀이 이 검사를 통과하는가. [`filter_clause`] 가 SQL 로 기재하는 것과
    /// **동일한 분해의 타 번역**이다.
    fn hits(&self, cell: &str, blank: &str) -> bool {
        // ① 열 검색. SQLite 의 LIKE 는 ASCII 범위만 대소를 동일시한다.
        if !self.contains.is_empty() {
            let hay = cell.to_ascii_lowercase();
            if !hay.contains(&self.contains.to_ascii_lowercase()) {
                return false;
            }
        }
        // ② 값 선택. 전부 해제한 것은 어떠한 셀도 통과하지 않는다.
        if let Some(v) = &self.values {
            if v.nothing() || !v.has(cell, blank) {
                return false;
            }
        }
        // ③ 제외 목록.
        if let Some(e) = &self.excluded {
            if e.has(cell, blank) {
                return false;
            }
        }
        // ④ 공백이 아닌 셀만.
        if self.not_blank && crate::value::is_blank(cell, blank) {
            return false;
        }
        // ⑤ 숫자 범위. 수로 판독되지 않는 셀은 범위가 하나라도 기재되면 탈락한다.
        if self.min.is_some() || self.max.is_some() {
            let Some(v) = crate::value::as_real(cell) else { return false };
            if self.min.is_some_and(|lo| v < lo) || self.max.is_some_and(|hi| v > hi) {
                return false;
            }
        }
        true
    }
}

/// 조건 하나. `keys` 로 결합한 열들은 **OR** 로 묶인다.
#[derive(Debug, Clone)]
struct Cond {
    /// 적용할 열들. `compile` 에 전달한 열 목록의 번호다.
    on: Vec<usize>,
    test: Test,
}

/// 분해해 둔 조회 조건. **조회 문과 행 하나의 판정이 여기서 갈라진다.**
///
/// 받는 앱이 행을 Rust 로 보유한 화면(인쇄·내보내기·집약)에서 조건을 자체로
/// 판정하면, 어긋남은 조건 하나의 판정이 아니라 **결합과 거절**에서 발생한다 —
/// 실제로 `values` 와 `excluded` 를 동시에 기재한 요청을 엔진은 거절하는데
/// 받는 쪽은 양자를 조용히 함께 적용했다.
///
/// 거절은 전부 [`Match::compile`] 이 수행한다. [`Match::row`] 는 정지하지
/// 않는다 — 행마다 거절할 것이 있으면 그것은 요청의 결함이고, 요청은 1회
/// 분해할 때 판정하는 것이 맞다.
#[derive(Debug, Clone)]
pub struct Match {
    blank: String,
    conds: Vec<Cond>,
}

impl Match {
    /// 요청을 1회 분해한다. **여기가 거절하는 유일한 곳이다.**
    ///
    /// `columns` 는 부르는 곳의 열 순서대로 `(열쇠, 수치 열인가)` 다.
    /// [`Match::row`] 에 전달하는 셀 묶음의 순서가 그 순서다.
    ///
    /// 거절하는 것 — 정의에 부재하는 열, 하한이 상한을 초과하는 범위, 수치 열이
    /// 아닌 열의 범위, `values` 와 `excluded` 의 동시 기재. 오류 코드는 각각
    /// [`ErrorCode::InvalidFilterKey`] 와 [`ErrorCode::InvalidRange`] 다.
    ///
    /// **아무것도 기재하지 않은 조건은 조건이 아니다**(`Filter::is_empty`).
    /// 그것까지 보유하면 WHERE 에 항상 참인 조각이 기재되고, 행 판정에서도
    /// 열 번호를 요구한다.
    pub fn compile(filters: &[Filter], columns: &[(&str, bool)], blank: &str) -> Result<Match> {
        let mut conds = Vec::new();
        for f in filters {
            if f.is_empty() {
                continue;
            }
            /* **선택과 제외를 동시에 기재하면 중단한다.** 우선순위를 묵시적으로
               부여하면 어느 쪽이 적용되었는지 사람도 코드도 판별할 수 없다. */
            if f.values.is_some() && f.excluded.is_some() {
                return Err(bad_filter(&f.key, "both values and excluded were given"));
            }
            let wanted: Vec<&str> = if f.keys.is_empty() {
                vec![f.key.as_str()]
            } else {
                f.keys.iter().map(String::as_str).collect()
            };
            let ranged = f.min.is_some() || f.max.is_some();
            if let (Some(lo), Some(hi)) = (f.min, f.max) {
                if lo > hi {
                    return Err(bad_range(&f.key, "the range minimum exceeds its maximum"));
                }
            }
            let mut on = Vec::with_capacity(wanted.len());
            for key in wanted {
                let at = columns
                    .iter()
                    .position(|(k, _)| *k == key)
                    .ok_or_else(|| bad_filter(key, "unknown filter column"))?;
                /* **숫자 열이 아닌 열의 범위는 중단한다.** 문자 열에 숫자 비교를
                   적용하면 전 행이 탈락하는데, 그것이 오류로 출현하지 않는다. */
                if ranged && !columns[at].1 {
                    return Err(bad_range(key, "a range was given for a column that is not a number"));
                }
                on.push(at);
            }
            conds.push(Cond {
                on,
                test: Test {
                    contains: f.contains.trim().to_string(),
                    values: f.values.as_deref().map(Picks::of),
                    excluded: f.excluded.as_deref().map(Picks::of),
                    not_blank: f.not_blank,
                    min: f.min,
                    max: f.max,
                },
            });
        }
        Ok(Match { blank: blank.to_string(), conds })
    }

    /// 행 하나가 조건 전부를 통과하는가. **정지하지 않는다.**
    ///
    /// `cells[i]` 는 `compile` 에 전달한 `columns[i]` 의 **기재된 글자**다. 셀이
    /// 부재하면 공백 문자열과 동일하게 취급한다 — 짧은 묶음을 전달해도 정지하지
    /// 않고, 그 열이 공백인 행으로 판정한다.
    ///
    /// 사전 인코딩 열은 **사전에 보관된 형태**(선후의 공백을 제거한 글자, 공백은
    /// 공백 문자열)로 전달한다. 조회 문이 비교하는 것이 그 형태다.
    pub fn row(&self, cells: &[&str]) -> bool {
        self.conds.iter().all(|c| {
            c.on.iter().any(|i| c.test.hits(cells.get(*i).copied().unwrap_or(""), &self.blank))
        })
    }

    /// 동일한 분해를 WHERE 절로 번역한다.
    ///
    /// **`compile` 에 전달한 열 목록을 제시한 표여야 한다** — 보유하는 것이 그
    /// 목록의 번호다. 이 상자 내부에서만 호출하므로 번호로 둔다. 반출하는 쪽
    /// ([`Match::row`])은 번호를 부르는 곳의 열 순서로 수령한다.
    pub(crate) fn where_sql(&self, table: &dyn Table) -> (String, Vec<String>) {
        let cols: Vec<&Column> = table.columns().collect();
        let mut clauses = Vec::new();
        let mut binds = Vec::new();
        for c in &self.conds {
            let mut alts = Vec::new();
            for i in &c.on {
                if let Some(x) = filter_clause(cols[*i], &c.test, &self.blank, &mut binds) {
                    alts.push(x);
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
        (sql, binds)
    }
}

/// 한 열의 필터를 WHERE 에서 어떻게 다루는가.
#[derive(Clone, Copy)]
enum Own<'a> {
    /// 모든 필터를 적용한다.
    Apply,
    /// 그 열의 필터를 **통째로** 제외한다. 열의 자료가 존재하는 범위를 산출할 때 쓴다 —
    /// 자기 범위가 자기 범위를 좁히면 손잡이의 양 끝이 걸어 둔 범위로 줄어든다.
    SkipAll(&'a str),
    /// 그 열의 **값 선택·제외·검색만** 제외하고, 범위(`min`·`max`)와 `not_blank`(화면이 거는
    /// 구조 조건이다)는 적용한다. 값 목록을 만들 때 쓴다.
    ///
    /// 자기 값 선택은 빼되 자기 범위는 적용한다. 그러지 않으면 목록이 범위 밖의 값을 보여
    /// 주고, 그것을 선택하면 둘이 AND 로 겹쳐 조용히 0건이 된다. 범위 밖의 값은 목록에 없으니
    /// 선택할 수도 없다. 공백은 어떤 범위에도 들지 않으므로 범위가 걸려 있으면 목록에서도 빠진다.
    SkipPick(&'a str),
}

impl Own<'_> {
    /// 그 지시에 따라 조건 목록을 손질한다. **조건을 판독하지 않는다** — 어느
    /// 조건을 제외하고 어느 칸만 존치할지만 결정한다.
    fn apply(self, filters: &[Filter]) -> Vec<Filter> {
        let mut out = Vec::with_capacity(filters.len());
        for f in filters {
            let names = |k: &str| k == f.key || f.keys.iter().any(|x| x == k);
            /* **「자신의 것」은 조건이 진술한다.** 열 명칭으로 추정하면, 한 열을
               조작부 둘이 좁히는 화면에서 외부의 조건까지 제외되어 목록만 전체를
               열거한다(`Origin`). */
            let own_picker = f.origin == Origin::OwnPicker;
            match self {
                Own::SkipAll(k) if names(k) && own_picker => continue,
                Own::SkipPick(k) if names(k) && own_picker => {
                    if f.min.is_none() && f.max.is_none() && !f.not_blank {
                        continue;
                    }
                    /* **칸을 하나씩 열거한다.** `..Filter::default()` 로 채우면
                       앞으로 더해지는 칸이 조용히 떨어진다 — 열거하면 칸이 늘 때
                       여기서 컴파일이 멈춘다. */
                    out.push(Filter {
                        key: f.key.clone(),
                        keys: f.keys.clone(),
                        contains: String::new(),
                        values: None,
                        excluded: None,
                        not_blank: f.not_blank,
                        min: f.min,
                        max: f.max,
                        origin: f.origin,
                    });
                }
                _ => out.push(f.clone()),
            }
        }
        out
    }
}

/// WHERE 절과 바인딩 값을 생성한다. 기준 테이블 기준으로만 생성한다(조인 없음).
/// `own` 이 지정되면 그 열의 필터를 그에 따라 다룬다(값 선택 목록 생성 시 사용한다).
fn build_where(
    table: &dyn Table,
    filters: &[Filter],
    blank: &str,
    own: Own<'_>,
) -> Result<(String, Vec<String>)> {
    /* **조건을 판독하는 코드는 `Match::compile` 하나다.** 여기서 한 벌 더
       판독하면, 어긋나는 것은 조건 하나의 판정이 아니라 결합과 거절이 된다. */
    let trimmed = own.apply(filters);
    let cols: Vec<(&str, bool)> = table
        .columns()
        .map(|c| (c.key.as_str(), c.kind == ColumnKind::Number))
        .collect();

    /* **조회할 수 없는 열은 이 상자의 개념이다.** 받는 앱의 열은 전부 조회
       가능하므로 `compile` 의 열 목록이 보유하지 않는다. 폐기하면 조건을 걸지
       않은 것이 되어 더 많은 행이 산출되는데, 화면은 조건이 걸렸다고 묘사한다. */
    for f in &trimmed {
        if f.is_empty() {
            continue;
        }
        let keys: Vec<&str> =
            if f.keys.is_empty() { vec![f.key.as_str()] } else { f.keys.iter().map(String::as_str).collect() };
        for key in keys {
            if table.column(key).is_some_and(|c| !c.filterable) {
                return Err(bad_column(ErrorCode::InvalidFilterKey, table, key, "a filter was given for a column that cannot be filtered"));
            }
        }
    }

    /* 어느 표에서 발생했는지는 여기서 부착한다 — `Match` 는 표를 인지하지
       않는다. 부착하는 항목은 종전과 동일하다. */
    let m = Match::compile(&trimmed, &cols, blank)
        .map_err(|e| e.with("screen", table.id()))?;
    Ok(m.where_sql(table))
}

/// 정렬 열쇠를 열 순서대로. 배열 순서가 그대로 정렬 우선순위(1순위, 2순위, …)다.
/// 사전 인코딩 열은 id 자체가 가나다순이라 조인 없이 정수 정렬로 완결된다.
///
/// `qualified` 는 열을 `t."열"` 로 참조할 것인지를 결정한다. **동일한 목록이
/// ORDER BY 와 색인 정의 양측에 사용되어야 한다** — 한 문자라도 상이하면
/// SQLite 가 그 색인을 정렬에 사용하지 않고, 그 사실은 어디에서도 오류로
/// 출현하지 않는다(완결되는 데 수 초를 소요하는 것으로만 발현한다). 색인
/// 정의는 테이블 별칭을 수용하지 않아 참조만 분기한다.
fn order_keys(
    table: &dyn Table,
    sorts: &[Sort],
    blank: &str,
    qualified: bool,
) -> Result<Vec<String>> {
    let mut parts = Vec::with_capacity(sorts.len());
    for s in sorts {
        let col = table
            .column(&s.key)
            .ok_or_else(|| bad_column(ErrorCode::InvalidSortKey, table, &s.key, "unknown sort column"))?;
        // 폐기하면 정렬하지 않은 순서가 산출되는데 화면은 그 열로 정렬했다고 묘사한다.
        if !col.sortable {
            return Err(bad_column(ErrorCode::InvalidSortKey, table, &s.key, "a sort was given for a column that cannot be sorted"));
        }
        let dir = match s.dir {
            SortDir::Asc => "ASC",
            SortDir::Desc => "DESC",
        };
        let base = if qualified { col.base_ref() } else { col.plain_ref() };
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
    Ok(parts)
}

/// 쪽을 나눠 판독할 때 행 하나를 유일하게 지시하는 열의 별칭.
///
/// 기준 테이블의 `rowid` 다. 안쪽 질의가 그 이름으로 산출하고 바깥 질의가 그것
/// 으로 정렬한다 — 바깥에서는 `rowid` 가 더 이상 참조되지 않기 때문이다.
pub(crate) const RID: &str = "_sheetsift_rid";

/// ORDER BY 절. **늘 유일 열 꼬리로 종결한다.**
///
/// 꼬리가 부재하면 동점 행들의 순서가 정의되지 않는다. 그 상태로 offset 을
/// 상승시키며 쪽을 이어 붙이면 **동일한 행이 두 번 실리고 다른 행이 누락되는데,
/// 건수는 정합해 눈에 출현하지 않는다** — 인쇄하는 쪽이 그렇게 판독한다.
/// 정렬을 아예 걸지 않은 요청도 동일하다. 그래서 정렬 열쇠가 공백이어도 꼬리
/// 하나로 전순서가 성립한다.
///
/// 색인 정의에는 이 꼬리를 기재하지 않는다 — **SQLite 의 모든 색인은 마지막
/// 열쇠가 `rowid` 다.** 기재하면 동일한 열쇠를 두 번 기재하는 것이 되고, 그러면
/// 색인 명칭의 지문과 ORDER BY 의 글자가 어긋나 색인이 정렬에 사용되지 않는다.
fn build_order(table: &dyn Table, sorts: &[Sort], blank: &str, rid: &str) -> Result<String> {
    /* **꼬리가 유일한 열을 지시하는지 확인한다.** SQLite 의 `rowid` 는 동일한
       명칭의 열이 존재하면 그 열을 지시한다 — 그러면 꼬리가 유일하지 않고
       전순서가 조용히 소멸한다. 표를 생성하는 층이 그런 명칭을 산출하지 않으나
       (`sheet` 의 `shadows_rowid`), 정의를 기재하는 곳은 이 상자 밖이다.

       **사람이 지시한 것의 잘못이 아니라 정의의 결함이라 코드가 상이하다**
       (`TableDefinitionInvalid`). 정렬 열쇠 오류를 차용하면 「정렬 기준으로 지정한
       열을 찾지 못했습니다」가 묘사되는데, 사람이 변경할 수 있는 것이 부재하다. */
    if let Some(col) = table.columns().find(|c| crate::sheet::shadows_rowid(&c.column)) {
        return Err(bad_column(ErrorCode::TableDefinitionInvalid, table, &col.key, "a column name shadows the rowid used to order pages"));
    }
    let mut parts = order_keys(table, sorts, blank, true)?;
    parts.push(format!("{rid} ASC"));
    Ok(format!(" ORDER BY {}", parts.join(", ")))
}

/// 정렬 색인의 명칭 접두. 이 접두를 보유한 색인은 여기서 생성한 것이다.
const SORT_INDEX_PREFIX: &str = "sheetsift_sort_";

/// 정렬 색인을 생성할 행 수의 하한.
///
/// 이보다 소수의 행은 전량 정렬이 이미 즉시 완결되어, 색인은 구축 비용만
/// 추가한다.
const SORT_INDEX_LEAST: i64 = 20_000;

/// 열쇠 목록에서 색인 명칭을 산출한다(FNV-1a).
///
/// 명칭에 열 명칭을 기재하지 않는다 — 열쇠는 식이라 SQLite 의 식별자에
/// 수용되지 않는 문자를 포함한다.
fn fingerprint(s: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.as_bytes() {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x100_0000_01b3);
    }
    h
}

/// 현 정렬에 대응하는 식 색인을 배치한다.
///
/// 정렬 열쇠는 식(`CAST(REPLACE(…))`·`GLOB`)이라 어떠한 열 색인도 사용되지
/// 않는다. 그 결과 **마지막 페이지를 요청하면 SQLite 가 전 행을 정렬한다** —
/// 30만 행에서 수 초다. 동일한 식으로 색인을 1회 구축하면 그 이후의 페이지
/// 이동은 즉시 완결된다.
///
/// **표마다 하나만 존치한다.** 열을 변경하며 정렬할 때마다 적재하면 자료와
/// 동급의 메모리가 정렬 횟수만큼 누적된다. 선행하는 것은 소거하므로, 한
/// 정렬을 열람하는 동안만 구축 비용을 1회 부담한다.
///
/// **실패를 오류로 반환하지 않는다.** 봉인한 `.sift` 는 읽기 전용으로 적재되어
/// 생성이 거부되는데, 색인이 부재해도 산출되는 행과 그 순서는 동일하다 —
/// 소요 시간만 상이하다.
fn ensure_sort_index(conn: &Connection, table: &dyn Table, sorts: &[Sort], blank: &str, total: i64) {
    if total < SORT_INDEX_LEAST {
        return;
    }
    let Ok(parts) = order_keys(table, sorts, blank, false) else {
        return;
    };
    if parts.is_empty() {
        return;
    }
    let keys = parts.join(", ");
    let base = table.base();
    let name = format!("{SORT_INDEX_PREFIX}{:016x}", fingerprint(&format!("{base}\u{0}{keys}")));

    // 이미 존재하면 아무것도 수행하지 않는다. 페이지를 이동할 때마다 도달한다.
    let exists: std::result::Result<i64, _> = conn.query_row(
        "SELECT 1 FROM sqlite_schema WHERE type = 'index' AND name = ?1",
        [&name],
        |r| r.get(0),
    );
    if exists.is_ok() {
        return;
    }

    // 동일 표의 선행 정렬 색인을 소거한다.
    let stale: Vec<String> = conn
        .prepare(
            "SELECT name FROM sqlite_schema \
             WHERE type = 'index' AND tbl_name = ?1 AND name LIKE ?2",
        )
        .and_then(|mut st| {
            let rows = st.query_map(rusqlite::params![base, format!("{SORT_INDEX_PREFIX}%")], |r| {
                r.get::<_, String>(0)
            })?;
            rows.collect()
        })
        .unwrap_or_default();
    for old in stale {
        let _ = conn.execute_batch(&format!("DROP INDEX IF EXISTS \"{old}\""));
    }

    let _ = conn.execute_batch(&format!("CREATE INDEX \"{name}\" ON {base}({keys})"));
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

    let (where_sql, binds) = build_where(table, &q.filters, blank, Own::Apply)?;
    /* 안쪽은 기준 테이블의 `rowid` 로, 바깥은 안쪽이 산출한 별칭으로 정렬한다.
       양쪽이 동일한 꼬리를 보아야 쪽을 이어 붙인 결과에 중복도 누락도 없다. */
    let inner_order = build_order(table, &q.sorts, blank, "t.rowid")?;
    let outer_order = build_order(table, &q.sorts, blank, &format!("t.\"{RID}\""))?;
    let limit = q.limit.clamp(1, MAX_LIMIT);

    let params: Vec<&dyn rusqlite::ToSql> =
        binds.iter().map(|s| s as &dyn rusqlite::ToSql).collect();

    // 전체 행 수. 기준 테이블만 참조하므로 조인이 전혀 없다.
    let count_sql = format!("SELECT COUNT(*) FROM {base} t{where_sql}", base = table.base());
    let total: i64 = conn.query_row(&count_sql, params.as_slice(), |r| r.get(0))?;

    /* 정렬 열쇠가 식이라 색인이 부재하면 전 행을 정렬한다. 행 수는 상기
       집약으로 이미 판별했으므로 그것으로 필요를 판정한다. */
    ensure_sort_index(conn, table, &q.sorts, blank, total);

    // 본문: 내부에서 정렬·LIMIT 으로 행을 축소한 뒤, 그 수십 행에만 dict 를 조인한다.
    let select_list = table
        .columns()
        .map(|c| format!("{} AS \"{}\"", c.display_ref(), c.key))
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "SELECT {select_list} FROM (\
           SELECT t.*, t.rowid AS \"{RID}\" FROM {base} t{where_sql}{inner_order} \
           LIMIT {limit} OFFSET {offset}\
         ) t{joins}{outer_order}",
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
/// 엑셀 필터 드롭다운과 동일하게, **그 열 자신의 값 선택은 제외하고** 나머지
/// 필터를 반영한다. 그래야 선택을 해제하고 재설정해도 후보 목록이 소멸하지
/// 않는다.
///
/// **그 열 자신의 범위는 적용한다**([`Own::SkipPick`]). 목록이 범위 밖의 값을 담으면 그것을
/// 선택하는 순간 둘이 AND 로 겹쳐 조용히 0건이 된다.
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
        .ok_or_else(|| bad_column(ErrorCode::InvalidFilterKey, table, key, "unknown column for value list"))?;

    // 목록은 자기 값 선택만 빼고 자기 범위는 적용한다. 범위 밖의 값이 목록에 없어야 한다(`Own::SkipPick`).
    let (mut where_sql, mut binds) = build_where(table, filters, blank, Own::SkipPick(key))?;

    /* 자료의 범위는 **자기 필터를 통째로 빼고, 축소 문자도 적용하지 않고** 산출한다. 자기
       범위가 자기 범위를 좁히거나 문자로 후보를 좁힐 때마다 범위가 따라 좁아지면, 손잡이의
       양 끝이 이동한다. */
    /* 축소 문자가 있는 요청에서는 **산출하지 않는다.** 범위는 한 세션 안에서 변하지 않고 화면도
       축소한 응답의 범위는 버리는데, 산출은 전 행 집약이다. */
    let range = if col.kind == ColumnKind::Number && needle.trim().is_empty() {
        let (whole_sql, whole_binds) = build_where(table, filters, blank, Own::SkipAll(key))?;
        let params: Vec<&dyn rusqlite::ToSql> =
            whole_binds.iter().map(|s| s as &dyn rusqlite::ToSql).collect();
        number_span(conn, table, col, &whole_sql, params.as_slice())?
    } else {
        None
    };

    /* 축소하는 문자는 **이 열에 적용하는 부분일치**다. 사전 열이면 사전에서
       탐색한다 — 그 처리를 여기 한 벌 더 기재하지 않고 필터 한 조각을 차용한다. */
    if !needle.trim().is_empty() {
        let only = Test {
            contains: needle.trim().to_string(),
            values: None,
            excluded: None,
            not_blank: false,
            min: None,
            max: None,
        };
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
    /* 공백 표기(`NULL`·공백 문자·표식)는 셋을 초과하지 않고 **선두에 집약된다.**
       그만큼 더 수령해야 비공백 값이 상한+1 개 도달하고, 그래야 절단 여부를
       판정할 수 있다. */
    let over = MAX_VALUES + 1 + 3;
    let numeric = col.kind == ColumnKind::Number;

    let sql = if col.dict && numeric {
        /* **사전 열의 수치 순서는 사전의 문자를 참조해야 한다.** id 는 문자
           순서로 부여되어 있어 `1 · 10 · 100 · 11` 이 된다. 그래서 조인 이후에
           정렬하고 절단한다 — 정렬 열쇠를 내부에 두면 자료 전 행마다 변환이
           발생한다. 조인은 고유값 수만큼이고 사전은 기본 열쇠 탐색이다. */
        let inner = format!(
            "SELECT {base} AS v, COUNT(*) AS n FROM {table} t{where_sql} GROUP BY v",
            table = table.base(),
        );
        let order = values_order_sql("d.s", blank, true);
        format!(
            "SELECT d.s, c.n FROM ({inner}) c LEFT JOIN dict d ON d.id = c.v \
             ORDER BY {order} LIMIT {over}"
        )
    } else {
        // 기준 테이블에서만 GROUP BY 한 뒤, 축소된 목록에만 dict 를 조인한다.
        let order = values_order_sql("v", blank, numeric);
        let inner = format!(
            "SELECT {base} AS v, COUNT(*) AS n FROM {table} t{where_sql} \
             GROUP BY v ORDER BY {order} LIMIT {over}",
            table = table.base(),
        );
        if col.dict {
            format!("SELECT d.s, c.n FROM ({inner}) c LEFT JOIN dict d ON d.id = c.v")
        } else {
            format!("SELECT c.v, c.n FROM ({inner}) c")
        }
    };

    let mut stmt = conn.prepare_cached(&sql)?;
    let mut rows = stmt.query(params.as_slice())?;
    let mut values = Vec::new();
    while let Some(r) = rows.next()? {
        let raw = value_to_json(r.get_ref(0)?);
        let count: i64 = r.get(1)?;
        let text = match raw {
            Json::Null => BLANK_TOKEN.to_string(),
            // 공백의 정의는 한 곳이다 — 여기 기재하면 SQL 의 판정과 어긋난다.
            Json::String(s) if crate::value::is_blank(&s, blank) => BLANK_TOKEN.to_string(),
            Json::String(s) => s,
            other => other.to_string(),
        };
        /* 공백 값 표식이 복수 회 출현할 수 있으므로 병합한다. **선두에
           집약되어 있다** — 정렬 열쇠의 첫 조각이 공백 여부라, 표기가 상이해도
           인접한다. */
        if let Some(last) = values.last_mut() {
            let last: &mut ColumnValue = last;
            if last.value == text {
                last.count += count.max(0) as u32;
                continue;
            }
        }
        values.push(ColumnValue { value: text, count: count.max(0) as u32 });
    }

    /* **묘사할 수 있는 항목 수로 판정한다.** 공백 표기가 몇 벌이든 하나로
       병합되므로, 그만큼 더 수령해 두면(`over`) 병합 이후의 길이가 상한을
       초과하는 것과 "목록에 담기지 못한 값이 존재한다" 가 동치가 된다. */
    let truncated = values.len() > MAX_VALUES as usize;
    values.truncate(MAX_VALUES as usize);

    Ok(ColumnValues {
        values,
        truncated,
        range,
        elapsed_ms: started.elapsed().as_secs_f64() * 1000.0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 시험용 표. 정의는 코드에 기재하나 **조회는 정의만 참조해 수행한다.**
    struct T {
        columns: Vec<Column>,
        /// 기준 테이블 명칭. 시험마다 상이한 표를 수용한다.
        base: String,
    }

    impl Table for T {
        fn id(&self) -> &str {
            &self.base
        }
        fn base(&self) -> &str {
            &self.base
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
            base: "people".into(),
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
        let mut t = T { base: "people".into(), columns: vec![col("name", ColumnKind::Text, false), col("score", ColumnKind::Number, false)] };
        assign_aliases(t.columns.iter_mut());

        let asc = query(vec![s("score", SortDir::Asc)], vec![]);
        let page = run(&conn, &t, BLANK, &asc).unwrap();
        assert_eq!(names(&page), ["영", "하나", "합계", "공백"], "문자가 0 과 1 사이에 삽입되었습니다");

        // 방향을 역전시켜도 수치가 아닌 것과 부재는 후위를 유지한다.
        let desc = query(vec![s("score", SortDir::Desc)], vec![]);
        let page = run(&conn, &t, BLANK, &desc).unwrap();
        assert_eq!(names(&page), ["하나", "영", "합계", "공백"]);
    }

    /// **수치가 접두사로 기재된 문자가 그 수치의 위치로 정렬되지 않는다.**
    ///
    /// SQLite 의 변환은 접두사를 수용하므로 `3 apples` 가 3 의 자리에, `12abc`
    /// 가 12 의 자리에 삽입된다. 표를 판독하는 사람에게 그것은 수치다.
    #[test]
    fn text_that_merely_starts_with_a_number_sorts_after_the_numbers() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE people(name TEXT, score TEXT);
             INSERT INTO people VALUES
               ('셋', '3'), ('사과', '3 apples'), ('열둘', '12'), ('뒤섞', '12abc'),
               ('지수', '1e'), ('점', '7점');",
        )
        .unwrap();
        let mut t = T {
            base: "people".into(),
            columns: vec![col("name", ColumnKind::Text, false), col("score", ColumnKind::Number, false)],
        };
        assign_aliases(t.columns.iter_mut());

        let page = run(&conn, &t, BLANK, &query(vec![s("score", SortDir::Asc)], vec![])).unwrap();
        let got = names(&page);
        assert_eq!(&got[..2], ["셋", "열둘"], "수치가 아닌 것이 수치 사이에 삽입되었습니다: {got:?}");
        assert_eq!(got.len(), 6);
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
        let mut t = T { base: "people".into(), columns: vec![col("name", ColumnKind::Text, false), col("score", ColumnKind::Number, false)] };
        assign_aliases(t.columns.iter_mut());

        let page = run(&conn, &t, BLANK, &query(vec![s("score", SortDir::Asc)], vec![])).unwrap();
        assert_eq!(names(&page), ["오십", "구백팔십", "천이백"]);
    }

    /// **목록에 담기지 못한 값이 존재하면 그렇다고 보고한다.**
    ///
    /// 공백 표기만큼 더 수령해 두므로(`over`), 병합 이후의 길이가 상한을
    /// 초과하는 것과 "담기지 못한 값이 존재한다" 가 동치다. 그 여유가 부재하면
    /// 병합이 발생한 열에서 **값 하나가 누락된 목록이 "전부" 로 보고된다.**
    #[test]
    fn a_merged_blank_does_not_hide_that_the_list_was_cut() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE people(v TEXT);").unwrap();
        // 공백 표기 둘(`''` 과 `-`)과 고유값 MAX_VALUES 개.
        let mut sql = String::from("INSERT INTO people VALUES (''), ('-')");
        for i in 0..MAX_VALUES {
            sql.push_str(&format!(", ('가{i:05}')"));
        }
        conn.execute_batch(&sql).unwrap();
        let mut t = T { base: "people".into(), columns: vec![col("v", ColumnKind::Text, false)] };
        assign_aliases(t.columns.iter_mut());

        let got = column_values(&conn, &t, BLANK, "v", &[], "").unwrap();
        assert!(got.truncated, "값이 누락되었는데 전부라고 보고했습니다");
    }

    /// **자료에 기재된 문자가 공백 표식과 병합되지 않는다.**
    ///
    /// 표식이 사람이 판독할 문구였을 때는 동일한 문구가 셀에 기재된 양식에서
    /// 양자가 한 항목으로 병합되고, 선택하면 상이한 행들이 함께 통과했다.
    #[test]
    fn a_cell_can_never_impersonate_the_blank_mark() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE people(name TEXT, v TEXT);
             INSERT INTO people VALUES ('가', ''), ('나', '(비어 있음)'), ('다', 'x');",
        )
        .unwrap();
        let mut t = T {
            base: "people".into(),
            columns: vec![col("name", ColumnKind::Text, false), col("v", ColumnKind::Text, false)],
        };
        assign_aliases(t.columns.iter_mut());

        let got = column_values(&conn, &t, BLANK, "v", &[], "").unwrap();
        let shown: Vec<&str> = got.values.iter().map(|v| v.value.as_str()).collect();
        assert_eq!(shown, [BLANK_TOKEN, "(비어 있음)", "x"], "자료의 문자가 표식과 병합되었습니다");

        // 표식을 선택하면 공백 행만 통과한다.
        let f = Filter {
            key: "v".into(),
            values: Some(vec![BLANK_TOKEN.into()]),
            ..Filter::default()
        };
        let page = run(&conn, &t, BLANK, &query(vec![], vec![f])).unwrap();
        assert_eq!(names(&page), ["가"]);
    }

    /// **공백 표기가 몇 벌이든 목록에는 하나로 출현한다.**
    ///
    /// 종전에는 인접한 것만 병합했는데, 공백 문자와 표식 사이에 타 값이 끼면
    /// (`''` · `(가)` · `+5` · `-`) `(비어 있음)` 이 두 번 출현했다. 사람은
    /// 어느 쪽을 선택해야 하는지 판별할 수 없다.
    #[test]
    fn every_spelling_of_blank_becomes_one_entry() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE people(v TEXT);
             INSERT INTO people VALUES (''), ('(가)'), ('+5'), ('-'), ('100'), (NULL);",
        )
        .unwrap();
        let mut t = T { base: "people".into(), columns: vec![col("v", ColumnKind::Text, false)] };
        assign_aliases(t.columns.iter_mut());

        let got = column_values(&conn, &t, BLANK, "v", &[], "").unwrap();
        let shown: Vec<&str> = got.values.iter().map(|v| v.value.as_str()).collect();
        assert_eq!(shown.iter().filter(|v| **v == BLANK_TOKEN).count(), 1, "{shown:?}");
        // 선두에 집약된다. 절단이 발생해도 목록에서 소멸하지 않는다.
        assert_eq!(shown[0], BLANK_TOKEN);
        assert_eq!(got.values[0].count, 3, "공백 셋의 수가 합산되지 않았습니다");
    }

    /// **숫자 열의 목록은 표와 동일한 순서다.**
    ///
    /// 문자 순서로 산출하면 `1 · 10 · 100 · 11` 이 된다. 사람이 목록에서 판독한
    /// 순서로 표를 탐색할 수 없고, 상한에 걸린 열에서는 중간 수치가 목록에서
    /// 누락된다.
    #[test]
    fn a_number_columns_value_list_counts_like_numbers() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE people(v TEXT);
             INSERT INTO people VALUES ('100'), ('20'), ('9'), ('-5'), ('1,200'), ('');",
        )
        .unwrap();
        let mut t = T { base: "people".into(), columns: vec![col("v", ColumnKind::Number, false)] };
        assign_aliases(t.columns.iter_mut());

        let got = column_values(&conn, &t, BLANK, "v", &[], "").unwrap();
        let shown: Vec<&str> = got.values.iter().map(|v| v.value.as_str()).collect();
        assert_eq!(shown, [BLANK_TOKEN, "-5", "9", "20", "100", "1,200"]);
    }

    /// **제외 목록은 해제한 값만 탈락시킨다.**
    ///
    /// 목록이 상한에 걸려 절단된 열에서 "하나만 제외" 를 기재할 수 있어야 한다.
    /// 선택한 것을 열거하는 방식으로는 노출되지 않은 값까지 함께 제외된다.
    #[test]
    fn an_exclusion_drops_only_what_it_names() {
        let (conn, t) = fixture();
        let f = Filter {
            key: "city".into(),
            excluded: Some(vec!["서울".into()]),
            ..Filter::default()
        };
        let page = run(&conn, &t, BLANK, &query(vec![], vec![f])).unwrap();
        // 서울(가·다)만 탈락하고, **값이 부재하는 행(마)은 탈락하지 않는다.**
        assert_eq!(names(&page), ["나", "라", "마", "100%"]);
    }

    /// 공백을 제외하면 공백 행만 탈락한다.
    #[test]
    fn excluding_the_blank_drops_only_the_blank_rows() {
        let (conn, t) = fixture();
        let f = Filter {
            key: "city".into(),
            excluded: Some(vec![BLANK_TOKEN.into()]),
            ..Filter::default()
        };
        let page = run(&conn, &t, BLANK, &query(vec![], vec![f])).unwrap();
        assert_eq!(names(&page), ["가", "나", "다", "라", "100%"]);
    }

    /// **선택과 제외를 동시에 기재하면 중단한다.** 우선순위를 묵시적으로
    /// 부여하면 어느 쪽이 적용되었는지 판별할 수 없다.
    #[test]
    fn naming_both_a_pick_and_an_exclusion_stops() {
        let (conn, t) = fixture();
        let f = Filter {
            key: "city".into(),
            values: Some(vec!["서울".into()]),
            excluded: Some(vec!["부산".into()]),
            ..Filter::default()
        };
        assert!(run(&conn, &t, BLANK, &query(vec![], vec![f])).is_err());
    }

    /// **열 검색은 ASCII 의 대소문자를 구별하지 않는다.**
    ///
    /// SQLite 의 `LIKE` 가 그렇고, 엑셀의 "포함" 필터도 그렇다. 그러나 그 외의
    /// 문자(악센트가 부착된 라틴·한글)는 구별한다 — 전부 무시하려면 별도의
    /// 조합 규칙이 필요하다. **우연에 의존하는 거동이라 여기 고정해 둔다.**
    #[test]
    fn a_column_search_ignores_ascii_case_and_nothing_else() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE people(name TEXT);
             INSERT INTO people VALUES ('abc'), ('ABC'), ('Àbc'), ('àbc');",
        )
        .unwrap();
        let mut t = T { base: "people".into(), columns: vec![col("name", ColumnKind::Text, false)] };
        assign_aliases(t.columns.iter_mut());

        let page = run(&conn, &t, BLANK, &query(vec![], vec![contains("name", "ABC")])).unwrap();
        assert_eq!(names(&page), ["abc", "ABC"]);

        let page = run(&conn, &t, BLANK, &query(vec![], vec![contains("name", "Àbc")])).unwrap();
        assert_eq!(names(&page), ["Àbc"], "비ASCII 는 구별한다");
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

    /// **한 열을 조작부 둘이 좁힐 때, 값 목록이 외부의 조건을 반영한다.**
    ///
    /// 「자신의 것」을 열 명칭으로 추정했을 때, 상위의 검색 조건 카드가 그 열을
    /// 좁혀 표는 축소되었는데 그 열의 깔대기는 전체 값을 전체 건수와 함께
    /// 열거했다 — 거기 열거된 값을 선택하면 둘이 AND 로 겹쳐 조용히 0건이 된다.
    /// 엔진은 카드와 깔대기를 판별할 수단을 보유하지 않았으므로, **조건이 자기
    /// 유래를 진술한다**(`Origin`).
    #[test]
    fn a_value_list_applies_the_conditions_that_came_from_elsewhere() {
        let (conn, t) = fixture();
        // 외부의 조작(카드)이 도시를 좁힌다. 그 열의 목록도 축소되어야 한다.
        let card = Filter { origin: Origin::Elsewhere, ..picked("city", &["서울"]) };
        let res = column_values(&conn, &t, BLANK, "city", std::slice::from_ref(&card), "").unwrap();
        assert_eq!(
            res.values.iter().map(|v| v.value.as_str()).collect::<Vec<_>>(),
            ["서울"],
            "외부의 조건을 반영하지 않았다"
        );

        // 그 열 자신의 팝오버가 기재한 것은 종전대로 제외한다 — 그러지 않으면
        // 고른 값 하나만 남아 선택을 변경할 수단이 소멸한다.
        let own = picked("city", &["서울"]);
        assert_eq!(own.origin, Origin::OwnPicker, "기재하지 않으면 자기 팝오버다");
        let res = column_values(&conn, &t, BLANK, "city", std::slice::from_ref(&own), "").unwrap();
        // 셋과 공백 한 행이다(도시가 부재하는 행이 존재한다).
        assert_eq!(res.values.len(), 4, "자기 선택을 제외하지 않았다");

        /* 한 열에 조건이 둘이고 한쪽만 자기 것인 경우. 목록은 **기재하지 않은
           쪽만** 반영한다 — 자기 선택(대구)은 제외하고 카드(부산·서울)는 적용해
           둘이 남는다. */
        let two = vec![
            Filter { origin: Origin::Elsewhere, ..picked("city", &["부산", "서울"]) },
            picked("city", &["대구"]),
        ];
        let res = column_values(&conn, &t, BLANK, "city", &two, "").unwrap();
        assert_eq!(
            res.values.iter().map(|v| v.value.as_str()).collect::<Vec<_>>(),
            ["부산", "서울"]
        );

        // 조회 자체는 양측을 전부 적용한다 — 목록의 규칙이 행을 증가시키지 않는다.
        assert_eq!(run(&conn, &t, BLANK, &query(vec![], two)).unwrap().total, 0);
    }

    /// **자료가 존재하는 범위도 외부의 조건을 반영한다.** 손잡이의 양 끝이 표에
    /// 부재하는 값까지 덮으면, 끝까지 민 손잡이가 조용히 0건을 산출한다.
    ///
    /// 그 열 자신의 범위는 종전대로 제외한다 — 자기 범위가 자기 범위를 좁히면
    /// 손잡이의 양 끝이 걸어 둔 범위로 축소된다.
    #[test]
    fn the_span_of_a_number_column_follows_the_conditions_from_elsewhere() {
        let (conn, t) = fixture();
        // 외부의 조작이 그 수치 열 자체를 좁힌 경우.
        let card = Filter { origin: Origin::Elsewhere, ..picked("score", &["90", "85.5"]) };
        let res = column_values(&conn, &t, BLANK, "score", &[card], "").unwrap();
        let span = res.range.expect("수치 열의 범위");
        assert_eq!((span.min, span.max), (85.5, 90.0), "외부의 조건을 반영하지 않았다");

        // 자기 팝오버가 기재한 범위는 제외한다. 자료 전체의 폭이 그대로 나온다.
        let mine = ranged("score", Some(88.0), Some(92.0));
        let res = column_values(&conn, &t, BLANK, "score", &[mine], "").unwrap();
        let span = res.range.expect("수치 열의 범위");
        assert_eq!((span.min, span.max), (70.0, 100.0));
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

    fn ranged(key: &str, min: Option<f64>, max: Option<f64>) -> Filter {
        Filter { key: key.into(), min, max, ..Filter::default() }
    }

    #[test]
    fn a_range_may_be_negative() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE n(v TEXT); INSERT INTO n VALUES('-10'),('-5'),('-0.5'),('3');")
            .unwrap();
        let mut t = T { base: "n".into(), columns: vec![col("v", ColumnKind::Number, false)] };
        assign_aliases(t.columns.iter_mut());
        let page =
            run(&conn, &t, BLANK, &query(vec![], vec![ranged("v", Some(-5.0), Some(0.0))])).unwrap();
        assert_eq!(page.total, 2, "-5 와 -0.5 만 통과해야 한다");
        let got = column_values(&conn, &t, BLANK, "v", &[], "").unwrap();
        assert_eq!(got.range, Some(NumberSpan { min: -10.0, max: 3.0 }));
    }

    /// 숫자 열에 수치가 하나도 없으면 범위는 `None` 이다(0 ~ 0 이 아니다).
    #[test]
    fn a_number_column_without_a_number_has_no_span() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE e(v TEXT); INSERT INTO e VALUES('-'),(''),('해당 없음');").unwrap();
        let mut t = T { base: "e".into(), columns: vec![col("v", ColumnKind::Number, false)] };
        assign_aliases(t.columns.iter_mut());
        let got = column_values(&conn, &t, BLANK, "v", &[], "").unwrap();
        assert_eq!(got.range, None);
    }

    /// 사전 **문자** 열에 범위를 주면 중단한다.
    #[test]
    fn a_range_on_a_dictionary_text_column_stops() {
        let (conn, t) = fixture();
        let f = ranged("city", Some(1.0), None);
        assert!(run(&conn, &t, BLANK, &query(vec![], vec![f])).is_err());
    }

    /// 범위는 경계를 포함하고, **수치로 판독되지 않는 값(공백·`-`·문자)은 통과하지 못한다.**
    #[test]
    fn a_range_keeps_the_numbers_inside_it_and_drops_everything_else() {
        let (conn, t) = fixture();
        let f = ranged("score", Some(85.5), Some(90.0));
        let page = run(&conn, &t, BLANK, &query(vec![], vec![f])).unwrap();
        assert_eq!(names(&page), ["가", "라"], "경계를 포함하지 않았거나 공백이 통과했다");
    }

    #[test]
    fn a_range_may_name_only_one_side() {
        let (conn, t) = fixture();
        let low = ranged("score", Some(90.0), None);
        assert_eq!(run(&conn, &t, BLANK, &query(vec![], vec![low])).unwrap().total, 2);
        let high = ranged("score", None, Some(85.5));
        assert_eq!(run(&conn, &t, BLANK, &query(vec![], vec![high])).unwrap().total, 2);
    }

    #[test]
    fn a_range_reads_a_number_with_a_thousands_separator_as_that_number() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE r(v TEXT);
             INSERT INTO r VALUES('1,200'), ('50'), ('총 44850점'), ('7점');",
        )
        .unwrap();
        let mut t = T { base: "r".into(), columns: vec![col("v", ColumnKind::Number, false)] };
        assign_aliases(t.columns.iter_mut());
        let page =
            run(&conn, &t, BLANK, &query(vec![], vec![ranged("v", Some(100.0), Some(5000.0))])).unwrap();
        assert_eq!(page.total, 1, "1,200 은 1 이 아니라 1200 으로 판독해야 한다");
        let page =
            run(&conn, &t, BLANK, &query(vec![], vec![ranged("v", Some(0.0), Some(99999.0))])).unwrap();
        assert_eq!(page.total, 2, "접두사가 수치인 문자가 통과했다");
    }

    #[test]
    fn a_range_works_on_a_dictionary_number_column() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE dict(id INTEGER PRIMARY KEY, s TEXT);
             INSERT INTO dict VALUES(1, '10'), (2, '9'), (3, '해당 없음');
             CREATE TABLE d(v INTEGER);
             INSERT INTO d VALUES(1), (2), (2), (3), (NULL);",
        )
        .unwrap();
        let mut t = T { base: "d".into(), columns: vec![col("v", ColumnKind::Number, true)] };
        assign_aliases(t.columns.iter_mut());
        let page = run(&conn, &t, BLANK, &query(vec![], vec![ranged("v", Some(9.0), Some(9.5))])).unwrap();
        assert_eq!(page.total, 2);
        let got = column_values(&conn, &t, BLANK, "v", &[], "").unwrap();
        assert_eq!(got.range, Some(NumberSpan { min: 9.0, max: 10.0 }));
    }

    /// 범위가 숫자 열이 아닌 열에 기재되거나 하한이 상한을 초과하면 **중단한다.**
    #[test]
    fn a_range_that_cannot_mean_anything_stops() {
        let (conn, t) = fixture();
        let on_text = ranged("name", Some(1.0), None);
        assert!(run(&conn, &t, BLANK, &query(vec![], vec![on_text])).is_err());
        let reversed = ranged("score", Some(90.0), Some(80.0));
        assert!(run(&conn, &t, BLANK, &query(vec![], vec![reversed])).is_err());
    }

    /// 범위와 값 선택은 AND 로 겹친다.
    #[test]
    fn a_range_and_a_value_pick_both_apply() {
        let (conn, t) = fixture();
        let mut f = ranged("score", Some(80.0), None);
        f.values = Some(vec!["90".into()]);
        let page = run(&conn, &t, BLANK, &query(vec![], vec![f])).unwrap();
        assert_eq!(names(&page), ["가"]);
    }

    fn listed(res: &ColumnValues) -> Vec<(String, u32)> {
        res.values
            .iter()
            .map(|v| (if v.value == BLANK_TOKEN { "<blank>".into() } else { v.value.clone() }, v.count))
            .collect()
    }

    /// **값 목록은 지금 걸린 범위 안의 값만 담는다**(건수도 그렇다). 범위 밖의 값이 목록에 있으면
    /// 그것을 선택하는 순간 범위와 AND 로 겹쳐 조용히 0건이 된다.
    #[test]
    fn the_value_list_holds_only_what_is_inside_the_range() {
        let (conn, t) = fixture();
        let r = ranged("score", Some(80.0), Some(95.0));
        let got = column_values(&conn, &t, BLANK, "score", &[r], "").unwrap();
        assert_eq!(listed(&got), [("85.5".into(), 1), ("90".into(), 1)]);
    }

    /// 범위를 풀면 목록은 전부로 돌아오고, 공백은 어떤 범위에도 들지 않으므로 범위가 걸려 있는
    /// 동안에는 목록에 없다.
    #[test]
    fn the_blank_is_listed_only_while_no_range_is_set() {
        let (conn, t) = fixture();
        let all = column_values(&conn, &t, BLANK, "score", &[], "").unwrap();
        let names: Vec<String> = listed(&all).into_iter().map(|(v, _)| v).collect();
        assert_eq!(names, ["<blank>", "70", "85.5", "90", "100"]);
        let r = ranged("score", Some(0.0), Some(1000.0));
        let got = column_values(&conn, &t, BLANK, "score", &[r], "").unwrap();
        assert!(!listed(&got).iter().any(|(v, _)| v == "<blank>"), "범위가 걸렸는데 공백이 목록에 있다");
    }

    /// 범위가 모든 값을 걸러 내면 목록은 빈다.
    #[test]
    fn a_range_that_removes_every_value_leaves_an_empty_list() {
        let (conn, t) = fixture();
        let r = ranged("score", Some(1000.0), Some(2000.0));
        let got = column_values(&conn, &t, BLANK, "score", &[r], "").unwrap();
        assert!(got.values.is_empty());
        assert!(!got.truncated);
    }

    /// 화면이 거는 `not_blank` 는 자기 열의 것이어도 목록에 적용한다(구조 조건이다).
    #[test]
    fn the_list_keeps_its_own_not_blank() {
        let (conn, t) = fixture();
        let f = Filter { key: "score".into(), not_blank: true, ..Filter::default() };
        let got = column_values(&conn, &t, BLANK, "score", &[f], "").unwrap();
        assert!(!listed(&got).iter().any(|(v, _)| v == "<blank>"));
    }

    /// 같은 열의 값 선택은 목록을 좁히지 않고(그래야 선택을 해제·재설정할 수 있다), 범위는 좁힌다.
    #[test]
    fn the_list_drops_its_own_pick_but_keeps_its_own_range() {
        let (conn, t) = fixture();
        let mut f = ranged("score", Some(80.0), Some(95.0));
        f.values = Some(vec!["70".into()]);
        let got = column_values(&conn, &t, BLANK, "score", &[f], "").unwrap();
        let names: Vec<String> = listed(&got).into_iter().map(|(v, _)| v).collect();
        assert_eq!(names, ["85.5", "90"]);
    }

    /// 자료의 범위는 **그 열 자신의 필터를 제외하고 다른 열의 필터는 반영한다.**
    #[test]
    fn the_span_ignores_its_own_filter_but_not_the_others() {
        let (conn, t) = fixture();
        let got = column_values(&conn, &t, BLANK, "score", &[], "").unwrap();
        assert_eq!(got.range, Some(NumberSpan { min: 70.0, max: 100.0 }));

        let own = ranged("score", Some(95.0), Some(96.0));
        let got = column_values(&conn, &t, BLANK, "score", &[own], "").unwrap();
        assert_eq!(got.range, Some(NumberSpan { min: 70.0, max: 100.0 }), "자기 필터가 범위를 좁혔다");

        let got = column_values(&conn, &t, BLANK, "score", &[picked("city", &["서울"])], "").unwrap();
        assert_eq!(got.range, Some(NumberSpan { min: 90.0, max: 90.0 }));
    }

    /// 축소 문자가 있으면 범위를 산출하지 않는다(화면이 쓰지 않는 전 행 집약이다). 문자 열에는
    /// 범위가 없다.
    #[test]
    fn the_span_is_not_computed_for_a_needle_and_text_has_none() {
        let (conn, t) = fixture();
        let got = column_values(&conn, &t, BLANK, "score", &[], "100").unwrap();
        assert_eq!(got.range, None);
        assert!(!got.values.is_empty(), "축소한 목록은 그대로 와야 한다");
        let got = column_values(&conn, &t, BLANK, "name", &[], "").unwrap();
        assert_eq!(got.range, None);
    }

    #[test]
    fn unknown_sort_key_is_rejected() {
        let (conn, t) = fixture();
        let e = run(&conn, &t, BLANK, &query(vec![s("no_such", SortDir::Asc)], vec![])).unwrap_err();
        assert_eq!(e.code, ErrorCode::InvalidSortKey);
        // 값 목록이 지시한 열은 필터 쪽이다.
        let e = column_values(&conn, &t, BLANK, "no_such", &[], "").unwrap_err();
        assert_eq!(e.code, ErrorCode::InvalidFilterKey);
    }

    /// **사유마다 코드가 상이하다.**
    ///
    /// 종전에는 일곱 가지가 `InvalidSortKey` 하나였고, 사유는 화면에 노출되지
    /// 않는 영문 `detail` 만 진술했다 — 범위를 잘못 기재한 사람에게 「정렬 기준으로
    /// 지정한 열을 찾지 못했습니다」가 묘사되었고, 수령하는 앱은 사유마다 상이한
    /// 문구를 부여할 수단이 부재했다.
    #[test]
    fn each_kind_of_bad_request_has_its_own_code() {
        let (conn, mut t) = fixture();
        let code = |q: &Query| run(&conn, &t, BLANK, q).unwrap_err().code;

        // 정렬 — 부재하는 열, 정렬할 수 없는 열.
        assert_eq!(
            code(&query(vec![s("no_such", SortDir::Asc)], vec![])),
            ErrorCode::InvalidSortKey
        );

        // 필터 — 부재하는 열, 선택과 제외를 동시에 기재.
        assert_eq!(
            code(&query(vec![], vec![contains("no_such", "가")])),
            ErrorCode::InvalidFilterKey
        );
        let both = Filter {
            key: "name".into(),
            values: Some(vec!["가".into()]),
            excluded: Some(vec!["나".into()]),
            ..Filter::default()
        };
        assert_eq!(code(&query(vec![], vec![both])), ErrorCode::InvalidFilterKey);

        // 범위 — 숫자 열이 아닌 열에 기재, 최소가 최대를 초과.
        let on_text = Filter { key: "name".into(), min: Some(1.0), ..Filter::default() };
        assert_eq!(code(&query(vec![], vec![on_text])), ErrorCode::InvalidRange);
        let upside_down =
            Filter { key: "score".into(), min: Some(9.0), max: Some(1.0), ..Filter::default() };
        assert_eq!(code(&query(vec![], vec![upside_down])), ErrorCode::InvalidRange);

        // 쓸 수 없는 열의 조건도 각자의 코드다.
        t.columns[0].filterable = false;
        t.columns[0].sortable = false;
        let err = run(&conn, &t, BLANK, &query(vec![], vec![contains("name", "가")])).unwrap_err();
        assert_eq!(err.code, ErrorCode::InvalidFilterKey);
        let err = run(&conn, &t, BLANK, &query(vec![s("name", SortDir::Asc)], vec![])).unwrap_err();
        assert_eq!(err.code, ErrorCode::InvalidSortKey);
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
        assert_eq!(dict_joins(&T { base: "people".into(), columns: cols }).matches("LEFT JOIN").count(), 2);
    }

    /// 정렬 색인 시험용. 하한을 초과하는 수의 행을 수용한다.
    fn big() -> (Connection, T) {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE big(name TEXT, score TEXT);").unwrap();
        {
            let tx = conn.unchecked_transaction().unwrap();
            let mut st = conn.prepare("INSERT INTO big VALUES(?1, ?2)").unwrap();
            for i in 0..(SORT_INDEX_LEAST + 1) {
                let v = (i * 7919) % (SORT_INDEX_LEAST + 1);
                st.execute(rusqlite::params![format!("n{i}"), format!("{v}")]).unwrap();
            }
            drop(st);
            tx.commit().unwrap();
        }
        let mut t = T {
            base: "big".into(),
            columns: vec![col("name", ColumnKind::Text, false), col("score", ColumnKind::Number, false)],
        };
        assign_aliases(t.columns.iter_mut());
        (conn, t)
    }

    /// 동점이 다수인 표. `rows` 행이고 점수는 `세 가지`뿐이라 전 행이 동점군에
    /// 소속된다 — 꼬리가 부재하면 쪽마다 순서가 상이할 수 있는 형태다.
    fn ties(rows: i64) -> (Connection, T) {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE ties(name TEXT, score TEXT);").unwrap();
        {
            let tx = conn.unchecked_transaction().unwrap();
            let mut st = conn.prepare("INSERT INTO ties VALUES(?1, ?2)").unwrap();
            for i in 0..rows {
                st.execute(rusqlite::params![format!("n{i}"), format!("{}", i % 3)]).unwrap();
            }
            drop(st);
            tx.commit().unwrap();
        }
        let mut t = T {
            base: "ties".into(),
            columns: vec![col("name", ColumnKind::Text, false), col("score", ColumnKind::Number, false)],
        };
        assign_aliases(t.columns.iter_mut());
        (conn, t)
    }

    /// **값 목록의 순서가 수의 크기순이다.** `a.cmp(b)` 로 정렬하면 동일한 열이
    /// 한 화면에서 `1 · 10 · 100` 이고 타 화면에서 `1 · 2 · 10 · 100` 이 된다.
    #[test]
    fn the_value_list_order_is_numeric_for_a_number_column() {
        let mut got = vec!["100", "2", "10", "1"];
        got.sort_by(|a, b| compare_values(a, b, BLANK, true));
        assert_eq!(got, ["1", "2", "10", "100"]);

        // 문자 열은 문자순이다 — 그 열의 값은 수가 아니다.
        let mut text = vec!["100", "2", "10", "1"];
        text.sort_by(|a, b| compare_values(a, b, BLANK, false));
        assert_eq!(text, ["1", "10", "100", "2"]);

        // 빈칸이 선두, 수가 그 다음, 수로 판독되지 않는 값이 후위다.
        let mut mixed = vec!["총 44850점", "90", "", "1,200", "-", "7점", " ", "85.5"];
        mixed.sort_by(|a, b| compare_values(a, b, BLANK, true));
        assert_eq!(mixed, ["", " ", "-", "85.5", "90", "1,200", "7점", "총 44850점"]);

        // 동일한 수치인 표기의 순서는 문자로 고정한다 — 목록에서 소멸하지 않는다.
        assert_eq!(compare_values("90", "90.0", BLANK, true), std::cmp::Ordering::Less);
        assert_eq!(compare_values("90", "90", BLANK, true), std::cmp::Ordering::Equal);
    }

    /// **Rust 와 SQL 이 동일한 순서를 산출한다.**
    ///
    /// 목록을 구성하는 경로가 둘이다 — 이 상자의 질의와, 목록을 Rust 로 구성하는
    /// 곳. 순서가 어긋나면 동일한 열이 화면마다 상이한 순서로 묘사되고, 고유값이
    /// 다수인 열에서는 절단이 상이한 항목을 폐기한다.
    #[test]
    fn rust_and_sql_put_the_value_list_in_the_same_order() {
        let samples = [
            "90", "90.0", "85.5", "1,200", "100", "2", "10", "1", "", " ", "-", "7점",
            "총 44850점", "NaN", "inf", "1e5", "1,2,3", "0", "-5", "+5", "가", "나", "100%",
        ];
        for numeric in [true, false] {
            let conn = Connection::open_in_memory().unwrap();
            conn.execute_batch("CREATE TABLE t(v TEXT);").unwrap();
            {
                let mut ins = conn.prepare("INSERT INTO t VALUES (?)").unwrap();
                for v in samples {
                    ins.execute([v]).unwrap();
                }
            }
            let sql = format!(
                "SELECT v FROM t t ORDER BY {}",
                values_order_sql("v", BLANK, numeric)
            );
            let from_sql: Vec<String> = conn
                .prepare(&sql)
                .unwrap()
                .query_map([], |r| r.get(0))
                .unwrap()
                .map(|r| r.unwrap())
                .collect();

            let mut from_rust: Vec<String> = samples.iter().map(|s| (*s).to_string()).collect();
            from_rust.sort_by(|a, b| compare_values(a, b, BLANK, numeric));
            assert_eq!(from_sql, from_rust, "numeric={numeric}");
        }
    }

    /// **판정이 갈라졌던 입력들을 조회 엔진도 동일하게 다룬다.**
    ///
    /// `1,200` 은 수이고, `1,2,3`·`1e5`·`NaN`·`inf` 는 문자이고, `" "` 와
    /// `" - "` 는 빈칸이다. 수치 열의 정렬에서 문자는 방향과 무관하게 후위이고
    /// 빈칸은 그 후위이며, 범위 필터는 수만 통과시키고, 값 목록은 빈칸 표기를
    /// 하나로 병합한다. 판정을 한 곳에 모은 것이 여기까지 도달하는지를 판정한다.
    #[test]
    fn the_inputs_that_used_to_split_are_handled_the_same_way_here() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE people(name TEXT, score TEXT);
             INSERT INTO people VALUES
               ('쉼표', '1,200'), ('열거', '1,2,3'), ('지수', '1e5'), ('엔', 'NaN'),
               ('무한', 'inf'), ('한칸', ' '), ('없음', ' - '), ('칠십', '70');",
        )
        .unwrap();
        let mut t = T {
            base: "people".into(),
            columns: vec![
                col("name", ColumnKind::Text, false),
                col("score", ColumnKind::Number, false),
            ],
        };
        assign_aliases(t.columns.iter_mut());

        // 정렬. 수 → 문자 → 빈칸 순이고, 수는 `1,200` 이 `70` 보다 크다.
        let page = run(&conn, &t, BLANK, &query(vec![s("score", SortDir::Desc)], vec![])).unwrap();
        let got = names(&page);
        assert_eq!(&got[..2], ["쉼표", "칠십"], "{got:?}");
        assert_eq!(&got[6..], ["한칸", "없음"], "빈칸은 후위다: {got:?}");

        // 범위. 수만 통과한다 — 문자가 0 의 위치로 유입되지 않는다.
        let range = Filter { key: "score".into(), min: Some(0.0), ..Filter::default() };
        let page = run(&conn, &t, BLANK, &query(vec![], vec![range])).unwrap();
        let mut inside = names(&page);
        inside.sort();
        assert_eq!(inside, ["쉼표", "칠십"]);

        // 값 목록. 빈칸 표기 둘이 하나로 병합되고 문자는 그대로 열거된다.
        let listed = column_values(&conn, &t, BLANK, "score", &[], "").unwrap();
        let listed: Vec<(&str, u32)> =
            listed.values.iter().map(|v| (v.value.as_str(), v.count)).collect();
        assert_eq!(listed[0], (BLANK_TOKEN, 2), "{listed:?}");
        assert_eq!(&listed[1..3], [("70", 1), ("1,200", 1)], "수가 선두다: {listed:?}");
        let rest: std::collections::BTreeSet<&str> =
            listed[3..].iter().map(|(v, _)| *v).collect();
        assert_eq!(rest, ["1,2,3", "1e5", "NaN", "inf"].into_iter().collect());
    }

    /// **못 쓰는 열의 조건을 폐기하지 않는다.**
    ///
    /// 폐기하면 조건을 걸지 않은 것이 되어 **더 많은 행이 산출되는데**, 화면은
    /// 조건이 걸렸다고 묘사한다. 바로 옆 「숫자 아닌 열에 범위」는 중단하므로,
    /// 동급의 잘못을 상이하게 다루고 있었다.
    #[test]
    fn a_condition_on_a_column_that_cannot_take_it_stops() {
        let (conn, mut t) = fixture();
        t.columns[0].filterable = false;
        t.columns[0].sortable = false;

        let err = run(&conn, &t, BLANK, &query(vec![], vec![contains("name", "가")])).unwrap_err();
        assert_eq!(err.code, ErrorCode::InvalidFilterKey);
        let err = run(&conn, &t, BLANK, &query(vec![s("name", SortDir::Asc)], vec![])).unwrap_err();
        assert_eq!(err.code, ErrorCode::InvalidSortKey);
        // 쓸 수 있는 열은 그대로 작동한다.
        let page = run(&conn, &t, BLANK, &query(vec![s("score", SortDir::Desc)], vec![])).unwrap();
        assert_eq!(page.total, 6);
    }

    /// **공백 한 칸이 기재된 셀도 공백이다.**
    ///
    /// Rust 는 절단해 판정하고 SQL 은 절단하지 않아, `" "` 한 칸이 일방에는
    /// 공백이고 타방에는 값이 존재하는 칸이었다 — 정렬의 「공백은 후위」에
    /// 포착되지 않고, `not_blank` 를 통과하고, 범위 필터에서는 탈락했다.
    #[test]
    fn a_cell_that_holds_only_a_space_is_blank_everywhere() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE people(name TEXT, score TEXT);
             INSERT INTO people VALUES ('가', '90'), ('나', ' '), ('다', '70'), ('라', '');",
        )
        .unwrap();
        let mut t = T {
            base: "people".into(),
            columns: vec![
                col("name", ColumnKind::Text, false),
                col("score", ColumnKind::Number, false),
            ],
        };
        assign_aliases(t.columns.iter_mut());

        // 값이 존재하는 행만 — 공백 한 칸은 통과하지 않는다.
        let only = Filter { key: "score".into(), not_blank: true, ..Filter::default() };
        let page = run(&conn, &t, BLANK, &query(vec![], vec![only])).unwrap();
        assert_eq!(names(&page), ["가", "다"]);

        // 정렬. 공백은 방향과 무관하게 후위다.
        let page = run(&conn, &t, BLANK, &query(vec![s("score", SortDir::Asc)], vec![])).unwrap();
        assert_eq!(names(&page), ["다", "가", "나", "라"]);

        // 값 목록. 표기가 상이한 공백은 하나로 병합된다.
        let got = column_values(&conn, &t, BLANK, "score", &[], "").unwrap();
        let listed: Vec<(&str, u32)> =
            got.values.iter().map(|v| (v.value.as_str(), v.count)).collect();
        assert_eq!(listed, [(BLANK_TOKEN, 2), ("70", 1), ("90", 1)]);
    }

    /// **정렬 열쇠가 유일 열 꼬리로 종결한다.**
    ///
    /// 쪽을 이어 붙여 판독하는 쪽(인쇄)이 전순서를 요구한다. 꼬리가 부재하면
    /// 동점 행들의 순서는 **질의 계획이 결정한다** — 색인이 생성되는 경계,
    /// 필터의 유무, SQLite 의 판에 따라 상이해지고, 그 결과로 중복과 누락이
    /// 발생하는데 건수는 정합한다. 산출되는 행으로는 그것을 포착할 수 없으므로
    /// 절을 글자로 판정한다.
    ///
    /// 정렬을 걸지 않은 요청도 동일하다. 종전에는 그 요청의 ORDER BY 가 공백
    /// 문자였다 — 순서가 전혀 지정되지 않은 질의다.
    #[test]
    fn every_order_by_ends_with_a_unique_column() {
        let (_conn, t) = fixture();
        for sorts in [
            vec![],
            vec![s("score", SortDir::Desc)],
            vec![s("city", SortDir::Asc), s("name", SortDir::Desc)],
        ] {
            let order = build_order(&t, &sorts, BLANK, "t.rowid").unwrap();
            assert!(order.starts_with(" ORDER BY "), "{order}");
            assert!(order.ends_with("t.rowid ASC"), "{order}");
        }
        // 안쪽 질의와 바깥 질의가 동일한 꼬리를 보아야 한다. 바깥에서는 `rowid`
        // 가 더 이상 참조되지 않으므로 안쪽이 산출한 별칭을 사용한다.
        let outer = build_order(&t, &[], BLANK, &format!("t.\"{RID}\"")).unwrap();
        assert!(outer.ends_with(&format!("t.\"{RID}\" ASC")), "{outer}");
    }

    /// 쪽을 끝까지 이어 붙여 판독한다. 인쇄하는 쪽이 수행하는 것과 동일하다.
    fn every_page(conn: &Connection, t: &T, sorts: Vec<Sort>) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let mut offset = 0u32;
        loop {
            let q = Query { sorts: sorts.clone(), filters: vec![], offset, limit: MAX_LIMIT };
            let page = run(conn, t, BLANK, &q).unwrap();
            if page.rows.is_empty() {
                break;
            }
            out.extend(names(&page));
            offset += MAX_LIMIT;
            if offset >= page.total {
                break;
            }
        }
        out
    }

    /// **쪽을 나눠 전부 판독하면 중복도 누락도 부재한다.**
    ///
    /// 정렬 열쇠에 유일 열 꼬리가 부재하면 동점 행들의 순서가 정의되지 않는다.
    /// 그 상태로 offset 을 상승시키며 쪽을 이어 붙이면 **동일한 행이 두 번
    /// 실리고 다른 행이 누락되는데, 건수는 정합해 눈에 출현하지 않는다** — 그
    /// 표를 인쇄한 사람은 자신의 줄이 소멸한 것을 인지하지 못한다.
    #[test]
    fn reading_every_page_of_a_table_full_of_ties_loses_no_row_and_repeats_none() {
        let rows = 2_100i64;
        let (conn, t) = ties(rows);
        let expected: std::collections::BTreeSet<String> =
            (0..rows).map(|i| format!("n{i}")).collect();

        for sorts in [
            vec![],
            vec![s("score", SortDir::Asc)],
            vec![s("score", SortDir::Desc)],
            vec![s("name", SortDir::Asc)],
        ] {
            let got = every_page(&conn, &t, sorts.clone());
            let unique: std::collections::BTreeSet<String> = got.iter().cloned().collect();
            assert_eq!(got.len() as i64, rows, "건수가 상이합니다: {sorts:?}");
            assert_eq!(unique.len(), got.len(), "동일한 행이 두 번 실렸습니다: {sorts:?}");
            assert_eq!(unique, expected, "누락된 행이 존재합니다: {sorts:?}");
        }
    }

    /// **`rowid` 라는 열이 기재된 양식도 쪽을 나눠 전부 판독한다.**
    ///
    /// SQLite 의 `rowid` 는 동일한 명칭의 열이 존재하면 그 열을 지시한다. 그
    /// 양식을 그대로 생성하면 정렬 꼬리가 유일하지 않은 열(전 행이 동일한 값인
    /// 열이어도 무방하다)을 지시하게 되고, 전순서가 조용히 소멸한다. 표를
    /// 생성하는 층이 명칭을 회피하므로 꼬리는 실제 `rowid` 를 유지한다.
    #[test]
    fn a_workbook_with_a_column_named_rowid_still_pages_without_loss() {
        let rows = 1_200usize;
        let conn = Connection::open_in_memory().unwrap();
        let mut sheet_rows: Vec<Vec<String>> = vec![vec!["rowid".into(), "점수".into()]];
        for i in 0..rows {
            // `rowid` 열의 값은 전 행이 동일하다 — 그 열을 꼬리로 삼으면 전순서가 부재한다.
            sheet_rows.push(vec!["같음".into(), format!("{}", i % 3)]);
        }
        let book = crate::sheet::Sheet { rows: sheet_rows, ..Default::default() };
        let plan = crate::sheet::TablePlan {
            table: "t".into(),
            map: Vec::new(),
            indexes: Vec::new(),
        };
        let made = crate::sheet::build(&conn, &[(plan, book)], BLANK).unwrap();
        let keys: Vec<String> = made["t"].columns.iter().map(|(_, db)| db.clone()).collect();
        assert_eq!(keys[0], "_rowid", "그 명칭을 회피해야 한다");

        let mut t = T {
            base: "t".into(),
            columns: vec![
                col(&keys[0], ColumnKind::Text, false),
                col(&keys[1], ColumnKind::Number, false),
            ],
        };
        assign_aliases(t.columns.iter_mut());

        // 전 행이 동점인 열로 정렬해 쪽을 끝까지 이어 붙인다.
        let mut seen: Vec<i64> = Vec::new();
        let mut offset = 0u32;
        loop {
            let q = Query {
                sorts: vec![s(&keys[1], SortDir::Asc)],
                filters: vec![],
                offset,
                limit: MAX_LIMIT,
            };
            let page = run(&conn, &t, BLANK, &q).unwrap();
            if page.rows.is_empty() {
                break;
            }
            // 실제 `rowid` 로 행을 식별한다. 산출된 열에는 그것이 부재하므로 질의로 대조한다.
            for r in &page.rows {
                let score = r[1].as_str().unwrap().to_string();
                seen.push(score.parse::<i64>().unwrap());
            }
            offset += MAX_LIMIT;
            if offset >= page.total {
                break;
            }
        }
        assert_eq!(seen.len(), rows, "건수가 상이합니다");
        // 동점군의 크기가 보전되었는가 — 중복이 발생하면 어느 군이 비대해진다.
        let mut counts = [0usize; 3];
        for v in &seen {
            counts[*v as usize] += 1;
        }
        assert_eq!(counts, [400, 400, 400], "동점군의 크기가 상이합니다: {counts:?}");

        // 열 정의가 그 명칭을 기재하면 중단한다 — 정의는 이 상자 밖에서 작성된다.
        let mut shadowed = T {
            base: "t".into(),
            columns: vec![Column { column: "rowid".into(), ..col("x", ColumnKind::Text, false) }],
        };
        assign_aliases(shadowed.columns.iter_mut());
        /* 사람이 지시한 것의 잘못이 아니라 정의의 결함이다 — 정렬 열쇠 오류를
           차용하면 사람이 변경할 수 없는 것을 변경하라는 문구가 묘사된다. */
        let err = run(&conn, &shadowed, BLANK, &query(vec![], vec![])).unwrap_err();
        assert_eq!(err.code, ErrorCode::TableDefinitionInvalid);
    }

    /// **색인을 생성하는 경계의 양측이 동일한 순서를 산출한다.**
    ///
    /// 색인은 행 수가 [`SORT_INDEX_LEAST`] 이상일 때만 생성되므로, 그 경계에서는
    /// 쪽마다 상이한 질의 계획으로 작동할 수 있다. 꼬리가 양측에서 동일해야
    /// 색인 순회와 임시 정렬이 동일한 답을 산출한다.
    #[test]
    fn both_sides_of_the_index_threshold_answer_the_same() {
        let sorts = vec![s("score", SortDir::Asc)];
        let (small, ts) = ties(SORT_INDEX_LEAST - 1);
        let (large, tl) = ties(SORT_INDEX_LEAST);

        let below = every_page(&small, &ts, sorts.clone());
        let above = every_page(&large, &tl, sorts.clone());
        // 색인이 경계의 상측에서만 생성된다 — 그런데도 순서는 동일해야 한다.
        assert!(sort_indexes(&small).is_empty(), "경계 하측에는 색인이 부재한다");
        assert_eq!(sort_indexes(&large).len(), 1, "경계 상측에는 색인이 존재한다");

        assert_eq!(below.len() as i64, SORT_INDEX_LEAST - 1);
        assert_eq!(above.len() as i64, SORT_INDEX_LEAST);
        // 상측은 하측보다 행이 하나 많다. 그 행을 제외하면 순서가 일치해야 한다.
        let last = format!("n{}", SORT_INDEX_LEAST - 1);
        let trimmed: Vec<String> = above.into_iter().filter(|n| *n != last).collect();
        assert_eq!(trimmed, below, "색인의 유무로 순서가 상이합니다");
    }

    fn sort_indexes(conn: &Connection) -> Vec<String> {
        let mut st = conn
            .prepare("SELECT name FROM sqlite_schema WHERE type = 'index' AND name LIKE 'sheetsift_sort_%' ORDER BY name")
            .unwrap();
        let rows = st.query_map([], |r| r.get::<_, String>(0)).unwrap();
        rows.map(|r| r.unwrap()).collect()
    }

    /// **색인이 실제로 정렬에 사용되는가.** 식이 한 문자라도 상이하면 SQLite 는
    /// 그것을 사용하지 않고 전 행을 정렬하는데, 산출되는 행은 동일해 어떠한
    /// 시험도 그 사실을 발견하지 못한다. 계획을 판독해 확인한다.
    #[test]
    fn sorting_a_large_table_does_not_sort_every_row() {
        let (conn, t) = big();
        let q = query(vec![s("score", SortDir::Asc)], vec![]);
        run(&conn, &t, BLANK, &q).unwrap();

        let names = sort_indexes(&conn);
        assert_eq!(names.len(), 1, "정렬마다 색인 하나");

        let order = build_order(&t, &q.sorts, BLANK, "t.rowid").unwrap();
        let sql = format!("SELECT * FROM big t{order} LIMIT 100 OFFSET {}", SORT_INDEX_LEAST - 50);
        let plan: Vec<String> = {
            let mut st = conn.prepare(&format!("EXPLAIN QUERY PLAN {sql}")).unwrap();
            let rows = st.query_map([], |r| r.get::<_, String>(3)).unwrap();
            rows.map(|r| r.unwrap()).collect()
        };
        let plan = plan.join(" | ");
        assert!(plan.contains(&names[0]), "색인을 경유하지 않는다: {plan}");
        assert!(!plan.contains("TEMP B-TREE FOR ORDER BY"), "전 행을 정렬한다: {plan}");
    }

    /// 열을 변경해 정렬하면 선행 색인을 소거한다. 존치하면 자료와 동급의
    /// 메모리가 정렬 횟수만큼 누적된다.
    #[test]
    fn only_the_current_sort_keeps_an_index() {
        let (conn, t) = big();
        run(&conn, &t, BLANK, &query(vec![s("score", SortDir::Asc)], vec![])).unwrap();
        let first = sort_indexes(&conn);
        run(&conn, &t, BLANK, &query(vec![s("name", SortDir::Desc)], vec![])).unwrap();
        let second = sort_indexes(&conn);
        assert_eq!(second.len(), 1);
        assert_ne!(first, second, "방향과 열이 상이하면 상이한 색인이다");
    }

    /// 하한 미만의 표에는 생성하지 않는다. 구축 비용만 추가된다.
    #[test]
    fn a_small_table_gets_no_index() {
        let (conn, t) = fixture();
        run(&conn, &t, BLANK, &query(vec![s("score", SortDir::Asc)], vec![])).unwrap();
        assert!(sort_indexes(&conn).is_empty());
    }

    /// **생성하지 못해도 조회는 완결된다.** 봉인한 `.sift` 는 읽기 전용으로
    /// 적재되어 생성이 거부되는데, 색인이 부재해도 산출되는 순서는 동일하다.
    #[test]
    fn a_read_only_database_still_answers() {
        let (conn, t) = big();
        conn.execute_batch("PRAGMA query_only = ON;").unwrap();
        let page = run(&conn, &t, BLANK, &query(vec![s("score", SortDir::Asc)], vec![])).unwrap();
        assert!(sort_indexes(&conn).is_empty());
        assert_eq!(page.rows.len(), 100);
        assert_eq!(page.rows[0][1].as_str(), Some("0"));
    }

    /* ── 분해 하나, 번역 둘 ──────────────────────────────────────
       `Match::row` 와 `Match::where_sql` 이 동일한 분해의 두 번역이라는 것은
       **대조해야 성립한다.** 손으로 고른 사례는 결합(필터 상호간 AND, `keys`
       상호간 OR)과 경계(공백 표식·제외와 NULL·수가 아닌 셀의 범위)에서
       어긋나는 위치를 발견하지 못한다. 그래서 조건을 생성해 전수 대조한다. */

    /// 양측을 대조할 표. 문자 열·사전 열·수치 열을 함께 보유한다.
    ///
    /// 사전 열의 셀은 **사전에 보관되는 형태**로 전달한다 — 선후의 공백을
    /// 제거한 글자이고 공백은 NULL 이다(`sheet` 가 그렇게 보관한다).
    fn agree_fixture() -> (Connection, T, Vec<Vec<String>>) {
        let cells: Vec<Vec<String>> = vec![
            vec!["가".into(), "서울".into(), "90".into()],
            vec!["나".into(), "부산".into(), "1,200".into()],
            vec!["다".into(), "".into(), "-".into()],
            vec!["라".into(), "대구".into(), "85.5".into()],
            vec!["".into(), "서울".into(), "".into()],
            vec!["100%".into(), "부산".into(), "70".into()],
            vec!["-".into(), "서울".into(), "총 44850점".into()],
            vec!["가나".into(), "".into(), "7점".into()],
            vec![" ".into(), "대구".into(), "0".into()],
            vec!["A".into(), "서울".into(), "-5".into()],
            vec!["a".into(), "부산".into(), "1200".into()],
        ];
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE dict(id INTEGER PRIMARY KEY, s TEXT);
             INSERT INTO dict VALUES(1, '대구'), (2, '부산'), (3, '서울');
             CREATE TABLE rows(name TEXT, city INTEGER, score TEXT);",
        )
        .unwrap();
        for r in &cells {
            let city: Option<i64> = match r[1].as_str() {
                "대구" => Some(1),
                "부산" => Some(2),
                "서울" => Some(3),
                _ => None,
            };
            conn.execute(
                "INSERT INTO rows(name, city, score) VALUES(?, ?, ?)",
                rusqlite::params![r[0], city, r[2]],
            )
            .unwrap();
        }
        let mut t = T {
            base: "rows".into(),
            columns: vec![
                col("name", ColumnKind::Text, false),
                col("city", ColumnKind::Text, true),
                col("score", ColumnKind::Number, false),
            ],
        };
        assign_aliases(t.columns.iter_mut());
        (conn, t, cells)
    }

    /// 조회 문이 통과시키는 행의 번호. `rowid` 는 삽입 순서다.
    fn sql_hits(conn: &Connection, t: &T, m: &Match) -> Vec<usize> {
        let (where_sql, binds) = m.where_sql(t);
        let sql = format!("SELECT rowid FROM rows t{where_sql} ORDER BY rowid");
        let params: Vec<&dyn rusqlite::ToSql> =
            binds.iter().map(|s| s as &dyn rusqlite::ToSql).collect();
        let mut q = conn.prepare(&sql).unwrap();
        let mut out = Vec::new();
        let mut rows = q.query(params.as_slice()).unwrap();
        while let Some(r) = rows.next().unwrap() {
            let id: i64 = r.get(0).unwrap();
            out.push(id as usize - 1);
        }
        out
    }

    /// 생성할 조건들. 결합과 경계를 포괄한다.
    fn agree_filters() -> Vec<Vec<Filter>> {
        let vals: [&[&str]; 6] = [
            &[],
            &["서울"],
            &["서울", "부산"],
            &[BLANK_TOKEN],
            &[BLANK_TOKEN, "서울"],
            &["없는 값"],
        ];
        let needles = ["", "가", "a", "A", "%", "_", "서", " "];
        let mut out: Vec<Vec<Filter>> = Vec::new();

        // ① 한 열에 하나씩. 열 셋 × 값 선택 · 제외 · 축소 문자.
        for key in ["name", "city", "score"] {
            for v in vals {
                let pick: Vec<String> = v.iter().map(|s| s.to_string()).collect();
                out.push(vec![Filter {
                    key: key.into(),
                    values: Some(pick.clone()),
                    ..Filter::default()
                }]);
                out.push(vec![Filter {
                    key: key.into(),
                    excluded: Some(pick),
                    ..Filter::default()
                }]);
            }
            for n in needles {
                out.push(vec![contains(key, n)]);
            }
            out.push(vec![Filter { key: key.into(), not_blank: true, ..Filter::default() }]);
        }

        // ② 수치 범위. 수가 아닌 셀과 경계를 포괄한다.
        for (lo, hi) in [
            (Some(0.0), None),
            (None, Some(90.0)),
            (Some(85.5), Some(90.0)),
            (Some(90.0), Some(90.0)),
            (Some(-5.0), Some(1200.0)),
            (Some(1000.0), None),
        ] {
            out.push(vec![ranged("score", lo, hi)]);
            // 범위와 값 선택이 함께 걸린 경우(AND 로 겹친다).
            out.push(vec![
                ranged("score", lo, hi),
                Filter { key: "name".into(), not_blank: true, ..Filter::default() },
            ]);
        }

        // ③ `keys` 로 결합한 열(OR). 사전 열과 문자 열을 함께 묶는다.
        for n in needles {
            out.push(vec![Filter {
                keys: vec!["name".into(), "city".into()],
                contains: n.into(),
                ..Filter::default()
            }]);
        }
        out.push(vec![Filter {
            keys: vec!["name".into(), "city".into()],
            values: Some(vec!["서울".into(), "가".into()]),
            ..Filter::default()
        }]);
        out.push(vec![Filter {
            keys: vec!["name".into(), "city".into()],
            excluded: Some(vec![BLANK_TOKEN.into(), "서울".into()]),
            ..Filter::default()
        }]);

        // ④ 필터 상호간(AND). 공백 조건까지 섞는다.
        out.push(vec![contains("name", "가"), picked("city", &["서울"])]);
        out.push(vec![picked("city", &[BLANK_TOKEN]), ranged("score", Some(0.0), None)]);
        out.push(vec![Filter::default()]);
        out.push(vec![]);
        out
    }

    /// **조회 문과 행 판정이 동일한 행 집합을 산출한다.**
    ///
    /// 어긋나면 인쇄하는 쪽과 표가 상이한 행을 묘사하는데, 건수조차 정합할 수
    /// 있어 눈에 출현하지 않는다.
    #[test]
    fn the_two_renderings_of_one_request_select_the_same_rows() {
        let (conn, t, cells) = agree_fixture();
        let cols: Vec<(&str, bool)> =
            t.columns().map(|c| (c.key.as_str(), c.kind == ColumnKind::Number)).collect();
        for filters in agree_filters() {
            let m = Match::compile(&filters, &cols, BLANK).unwrap();
            let by_sql = sql_hits(&conn, &t, &m);
            let by_row: Vec<usize> = cells
                .iter()
                .enumerate()
                .filter(|(_, r)| {
                    let refs: Vec<&str> = r.iter().map(String::as_str).collect();
                    m.row(&refs)
                })
                .map(|(i, _)| i)
                .collect();
            assert_eq!(by_sql, by_row, "{filters:?} 에서 두 번역이 어긋난다");
        }
    }

    /// **짧은 셀 묶음을 전달해도 정지하지 않는다.** 부재하는 셀은 공백이다.
    #[test]
    fn a_short_row_is_judged_as_if_the_missing_cells_were_blank() {
        let (_, t, _) = agree_fixture();
        let cols: Vec<(&str, bool)> =
            t.columns().map(|c| (c.key.as_str(), c.kind == ColumnKind::Number)).collect();
        let m = Match::compile(
            &[Filter { key: "score".into(), not_blank: true, ..Filter::default() }],
            &cols,
            BLANK,
        )
        .unwrap();
        assert!(!m.row(&["가"]));
        assert!(m.row(&["가", "서울", "90"]));
    }

    /// **거절은 `compile` 하나가 수행한다.** 행마다 거절할 것이 있으면 그것은
    /// 요청의 결함이고, 요청은 1회 분해할 때 판정하는 것이 맞다.
    #[test]
    fn compile_is_the_only_place_that_refuses() {
        let cols = [("name", false), ("score", true)];
        let bad = [
            (
                vec![Filter { key: "없는 열".into(), contains: "가".into(), ..Filter::default() }],
                ErrorCode::InvalidFilterKey,
            ),
            (
                vec![Filter {
                    key: "name".into(),
                    values: Some(vec!["가".into()]),
                    excluded: Some(vec!["나".into()]),
                    ..Filter::default()
                }],
                ErrorCode::InvalidFilterKey,
            ),
            (vec![ranged("name", Some(0.0), None)], ErrorCode::InvalidRange),
            (vec![ranged("score", Some(10.0), Some(1.0))], ErrorCode::InvalidRange),
        ];
        for (filters, code) in bad {
            let err = Match::compile(&filters, &cols, BLANK).unwrap_err();
            assert_eq!(err.code, code, "{filters:?}");
        }
        // 아무것도 기재하지 않은 조건은 조건이 아니다 — 거절하지 않는다.
        assert!(Match::compile(&[Filter::default()], &cols, BLANK).is_ok());
    }

    /// **행 정렬의 순서가 조회 문과 동일하다.**
    ///
    /// 열쇠를 글자로 대조하지 않고 **SQLite 에 정렬을 요구해** 결과의 순서를
    /// 대조한다 — 동점은 양측 모두 삽입 순서로 종결된다(조회 문은 `rowid` 꼬리,
    /// Rust 는 안정 정렬).
    #[test]
    fn the_two_renderings_of_one_sort_put_the_rows_in_the_same_order() {
        let (conn, t, cells) = agree_fixture();
        for (at, key) in ["name", "city", "score"].iter().enumerate() {
            let col = t.column(key).unwrap();
            // 사전 열은 조회 문이 id 로 정렬한다(id 가 가나다순이다).
            let numeric = col.kind == ColumnKind::Number && !col.dict;
            for dir in [SortDir::Asc, SortDir::Desc] {
                let order = order_keys(&t, &[s(key, dir)], BLANK, true).unwrap();
                let sql =
                    format!("SELECT rowid FROM rows t ORDER BY {}, t.rowid ASC", order.join(", "));
                let mut q = conn.prepare(&sql).unwrap();
                let mut by_sql = Vec::new();
                let mut rows = q.query([]).unwrap();
                while let Some(r) = rows.next().unwrap() {
                    let id: i64 = r.get(0).unwrap();
                    by_sql.push(id as usize - 1);
                }

                let mut by_rust: Vec<usize> = (0..cells.len()).collect();
                by_rust.sort_by(|i, j| {
                    compare_sorted(&cells[*i][at], &cells[*j][at], BLANK, numeric, dir)
                });
                assert_eq!(by_sql, by_rust, "{key} {dir:?} 에서 두 순서가 어긋난다");
            }
        }
    }
}
