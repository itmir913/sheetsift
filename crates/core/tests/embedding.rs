//! **조회 문을 자체로 구성하는 앱이 동일한 판정을 호출할 수 있는가.**
//!
//! 이 바탕을 반입하는 앱(navi-core)은 자기 표에 자기 조회 문을 구성한다. 그
//! 문이 "값이 부재하는 셀" 을 자체로 기재하면, **동일한 개념이 경로에 따라 두
//! 가지로 판독된다** — 실제로 `x IS NULL OR CAST(x AS TEXT) = ''` 를 기재해
//! 두었는데, 그것은 공백을 제거하지도 않고 자료의 공백 표식을 인지하지도
//! 않는다. 반입하는 값이 절단되어 도달하는 동안에만 양측이 일치한다.
//!
//! 그래서 정본을 반출한다. **이 파일은 상자 외부라, 반출을 비공개로 되돌리면
//! 컴파일되지 않는다** — 받는 앱이 자기 벌을 재차 기재하기 이전에 여기서
//! 정지한다. 동작이 아니라 **호출 가능성**을 수호하는 시험이다.

use sheetsift_core::{grid, value};

/// 공백 판정을 Rust 와 SQL 양측으로 호출할 수 있다.
#[test]
fn an_embedding_app_can_ask_what_this_crate_calls_blank() {
    let sql = value::is_blank_sql("x", "-");
    // 공백을 제거하고, 자료의 표식을 인지한다 — 양자가 자체 기재와 상이한 지점이다.
    assert!(sql.contains("TRIM("), "{sql}");
    assert!(sql.contains("'-'"), "{sql}");

    // 그 문을 실제로 질의해 Rust 쪽과 대조한다. 공백 집합이 양측에서 동일해야 한다.
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute("CREATE TABLE t(x TEXT)", []).unwrap();
    for s in [" ", "\t", "\u{a0}", " - ", "가", " 가 ", "0", ""] {
        conn.execute("DELETE FROM t", []).unwrap();
        conn.execute("INSERT INTO t(x) VALUES (?)", [s]).unwrap();
        let sql_says: bool = conn
            .query_row(&format!("SELECT {} FROM t", value::is_blank_sql("x", "-")), [], |r| r.get(0))
            .unwrap();
        assert_eq!(sql_says, value::is_blank(s, "-"), "{s:?}");
    }
}

/// 절단과 수치 판독도 호출할 수 있다. 조건을 자체로 조립하는 앱이 요구한다.
#[test]
fn an_embedding_app_can_ask_for_the_same_trim_and_the_same_number() {
    assert_eq!(value::trim("\u{a0} 90 \t"), "90");
    assert!(value::trim_sql("x").starts_with("TRIM(x, char("));
    assert!(value::is_number("1,200") && !value::is_number("1e5"));
    assert_eq!(value::as_real("1,200"), Some(1200.0));
    // 보관해도 환원되는 규모인지의 판정(`REAL` 열에 보관하는 앱이 요구한다).
    assert!(value::fits_real(1234.5) && !value::fits_real(1e15));
}

/// **조건을 1회 분해해 행 하나에 적용할 수 있다.** 받는 앱이 행을 Rust 로
/// 보유한 화면(인쇄·내보내기·집약)에서 조건을 자체로 판정하면, 어긋남은 조건
/// 하나의 판정이 아니라 **결합과 거절**에서 발생한다 — `values` 와 `excluded`
/// 를 동시에 기재한 요청을 엔진은 거절하는데 받는 쪽은 양자를 조용히 함께
/// 적용했다.
///
/// 이 파일은 상자 외부라, 반출을 되돌리면 받는 앱이 자기 벌을 재차 기재하기
/// 이전에 여기서 정지한다.
#[test]
fn an_embedding_app_can_compile_a_request_once_and_judge_its_own_rows() {
    let columns = [("대학", false), ("환산점", true)];
    let filters = vec![
        grid::Filter { key: "대학".into(), contains: "서울".into(), ..Default::default() },
        grid::Filter { key: "환산점".into(), min: Some(700.0), ..Default::default() },
    ];
    let m = grid::Match::compile(&filters, &columns, "-").unwrap();
    assert!(m.row(&["서울대학교", "712.5"]));
    assert!(!m.row(&["부산대학교", "712.5"]));
    assert!(!m.row(&["서울대학교", "690"]));
    // 수로 판독되지 않는 셀은 범위가 기재되면 탈락한다.
    assert!(!m.row(&["서울대학교", "총 712점"]));
    // 셀이 부재하면 공백과 동일하다 — 정지하지 않는다.
    assert!(!m.row(&["서울대학교"]));

    // 거절은 분해할 때 1회 수행한다.
    let both = vec![grid::Filter {
        key: "대학".into(),
        values: Some(vec!["서울대학교".into()]),
        excluded: Some(vec!["부산대학교".into()]),
        ..Default::default()
    }];
    let err = grid::Match::compile(&both, &columns, "-").unwrap_err();
    assert_eq!(err.code, sheetsift_core::ErrorCode::InvalidFilterKey);
}
