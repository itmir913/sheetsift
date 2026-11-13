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

/// **행 정렬의 순서를 조회 문과 동일하게 산출할 수 있다.**
///
/// 값 선택 목록의 순서(`compare_values`)와 상이하다 — 그쪽은 공백을 선두에
/// 배치한다. 행은 공백을 후위에 배치하고, 수치 열에서 수로 판독되지 않는 값을
/// 수 뒤에 둔다.
#[test]
fn an_embedding_app_can_order_rows_the_way_the_query_does() {
    use std::cmp::Ordering;
    let asc = grid::SortDir::Asc;
    // 공백은 방향과 무관하게 후위다.
    assert_eq!(grid::compare_sorted("", "90", "-", true, asc), Ordering::Greater);
    assert_eq!(
        grid::compare_sorted("", "90", "-", true, grid::SortDir::Desc),
        Ordering::Greater
    );
    // 수가 아닌 값은 수 뒤다(방향과 무관하다).
    assert_eq!(grid::compare_sorted("총 44850점", "90", "-", true, asc), Ordering::Greater);
    // 수치 열은 수의 크기순이다.
    assert_eq!(grid::compare_sorted("90", "1,200", "-", true, asc), Ordering::Less);
    // 문자 열은 문자순이다.
    assert_eq!(grid::compare_sorted("10", "9", "-", false, asc), Ordering::Less);

    let mut rows = vec!["90", "", "1,200", "총 44850점", "85.5"];
    rows.sort_by(|a, b| grid::compare_sorted(a, b, "-", true, asc));
    assert_eq!(rows, ["85.5", "90", "1,200", "총 44850점", ""]);
}

/// **동일한 조건을 받는 앱의 표에도 적용할 수 있다.** 열 식을 부르는 곳이
/// 제시한다.
///
/// 조건을 조립하는 벌이 받는 앱에 한 벌 더 기재되어 있었고(자기 학생 표에
/// 적용하는 조회 문), 그 벌은 `values` 와 `excluded` 를 동시에 기재한 요청을
/// 거절하지 않았다. 조립이 한 곳이면 그 어긋남이 발생할 여지가 부재하다.
#[test]
fn an_embedding_app_can_put_the_same_conditions_on_its_own_table() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE students(name TEXT, score TEXT);
         INSERT INTO students VALUES('가', '712.5'), ('나', '690'), ('다', NULL),
                                    ('라', '총 712점'), ('가나', '800');",
    )
    .unwrap();

    let columns = [("이름", false), ("환산점", true)];
    let filters = vec![
        grid::Filter { key: "이름".into(), contains: "가".into(), ..Default::default() },
        grid::Filter { key: "환산점".into(), min: Some(700.0), ..Default::default() },
    ];
    let m = grid::Match::compile(&filters, &columns, "-").unwrap();

    // 그 표의 열 식은 그 표가 제시한다.
    let expr = ["s.name", "s.score"];
    let (where_sql, binds) = m.where_sql(&|i| grid::Place::of(expr[i]));
    let sql = format!("SELECT name FROM students s{where_sql} ORDER BY s.rowid");
    let params: Vec<&dyn rusqlite::ToSql> =
        binds.iter().map(|b| b as &dyn rusqlite::ToSql).collect();
    let mut q = conn.prepare(&sql).unwrap();
    let mut by_sql: Vec<String> = Vec::new();
    let mut rows = q.query(params.as_slice()).unwrap();
    while let Some(r) = rows.next().unwrap() {
        by_sql.push(r.get(0).unwrap());
    }
    assert_eq!(by_sql, ["가".to_string(), "가나".to_string()]);

    // 행 판정도 동일한 분해다 — 양측이 동일한 행을 선택한다.
    let cells = [("가", "712.5"), ("나", "690"), ("다", ""), ("라", "총 712점"), ("가나", "800")];
    let by_row: Vec<String> = cells
        .iter()
        .filter(|(n, v)| m.row(&[n, v]))
        .map(|(n, _)| n.to_string())
        .collect();
    assert_eq!(by_sql, by_row);
}

/// **열이 자기 바인딩을 보유해도 조건을 적용할 수 있다.**
///
/// 받는 앱의 미리보기 열은 표의 열이 아니라 상관 부질의이고, 교과 묶음·학기·월을
/// 바인딩한다. 값을 식에 글자로 합성하는 경로는 사람이 기재한 값을 SQL 에 직접
/// 기재하는 셈이라 수용하지 않는다 — `Place::params` 로 전달하고, 반환하는
/// 바인딩은 **산출한 SQL 의 `?` 출현 순서**다.
#[test]
fn an_embedding_app_can_filter_a_column_that_is_a_correlated_subquery() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE s(id INTEGER PRIMARY KEY, name TEXT);
         INSERT INTO s VALUES(1, '가'), (2, '나'), (3, '다');
         CREATE TABLE n(student_id INTEGER, subject_set TEXT, grade_avg REAL);
         INSERT INTO n VALUES(1, '전교과', 2.41), (1, '국영수', 1.8),
                             (2, '전교과', 3.5), (3, '국영수', 2.41);",
    )
    .unwrap();

    let columns = [("내신", true)];
    let filters = vec![grid::Filter {
        key: "내신".into(),
        max: Some(3.0),
        ..Default::default()
    }];
    let m = grid::Match::compile(&filters, &columns, "-").unwrap();
    let at = grid::Place {
        expr: "(SELECT n.grade_avg FROM n WHERE n.student_id = s.id AND n.subject_set = ?)"
            .to_string(),
        params: vec!["전교과".to_string()],
        dict: false,
    };
    let (where_sql, binds) = m.where_sql(&|_| at.clone());
    /* **글자의 순서다.** 범위 조건은 식을 여러 번 기재하므로(수치 판정과 변환)
       교과 묶음이 그만큼 출현하고, 상한이 마지막이다. 글자 안의 `?` 를 계수하지
       않는다 — 수치 판정의 GLOB 패턴이 상수로 `?` 를 보유한다. */
    assert_eq!(binds.first().map(String::as_str), Some("전교과"));
    assert_eq!(binds.last().map(String::as_str), Some("3"));
    assert!(binds.iter().filter(|b| *b == "전교과").count() >= 2, "{binds:?}");

    let sql = format!("SELECT s.name FROM s{where_sql} ORDER BY s.id");
    let params: Vec<&dyn rusqlite::ToSql> =
        binds.iter().map(|b| b as &dyn rusqlite::ToSql).collect();
    let mut q = conn.prepare(&sql).unwrap();
    let mut got: Vec<String> = Vec::new();
    let mut rows = q.query(params.as_slice()).unwrap();
    while let Some(r) = rows.next().unwrap() {
        got.push(r.get(0).unwrap());
    }
    // 국영수만 보유한 학생은 그 열이 공백이라 범위에서 탈락한다.
    assert_eq!(got, ["가".to_string()]);
    // 행 판정도 동일하다.
    let cells = [("가", "2.41"), ("나", "3.5"), ("다", "")];
    let by_row: Vec<String> =
        cells.iter().filter(|(_, v)| m.row(&[v])).map(|(n, _)| n.to_string()).collect();
    assert_eq!(got, by_row);
}

/// 정렬 열쇠의 정본을 밖에서 호출할 수 있다. 반출을 되돌리면 이 파일이 컴파일되지 않는다.
#[test]
fn an_embedding_app_can_ask_for_the_engines_order_key() {
    let (sql, binds) = grid::order_sql(&grid::Place::of("x"), "-", true, grid::SortDir::Asc);
    assert!(!sql.is_empty());
    assert!(binds.is_empty());
}

/// 값 선택 목록의 정렬 열쇠도 밖에서 호출할 수 있다.
#[test]
fn an_embedding_app_can_ask_for_the_value_list_order_key() {
    let (sql, binds) = grid::values_order_sql(&grid::Place::of("v"), "-", true);
    assert!(!sql.is_empty());
    assert!(binds.is_empty());
}
