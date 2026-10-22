//! **경계는 요청 단일 덩어리로 통과한다.**
//!
//! 브라우저와 수수하는 겹([`sheetsift-wasm`](../src/lib.rs))의 메서드는 self
//! 이외의 인자를 둘 이상 수용하지 않는다. 수용하는 것은 JSON 문자 하나다.
//!
//! 사유는 부르는 곳이다. 동일한 요청을 위치마다 상이한 수의 인자로 전달하면,
//! 무엇이 어떤 순서로 전달되는지가 호출부마다 분기한다 — 그리고 문자 3개를
//! 위치만 교환해 전달해도 **오류가 발생하지 않는다.** 열 명칭 위치에 필터가
//! 삽입된 채로 "그런 값 없습니다" 가 표시된다.
//!
//! **이 검사는 소스를 글자로 판독한다.** 그 파일은 최상단이
//! `#![cfg(target_arch = "wasm32")]` 라 데스크톱에서는 컴파일되지 않아,
//! 실행하는 시험이 구조를 판정할 수 없다.

/// 껍질의 메서드 하나. 명칭과 self 를 제외한 인자 수.
#[derive(Debug)]
struct Method {
    name: String,
    args: usize,
}

/// `pub fn` 들을 탐색해 self 를 제외한 인자 수를 산정한다.
///
/// `#[wasm_bindgen(constructor)]` 이 부착된 것은 제외한다 — 그것은 요청이 아니라
/// 양식 바이트와 명칭을 수용하는 위치다.
fn methods(src: &str) -> Vec<Method> {
    let mut found = Vec::new();
    let mut rest = src;
    let mut constructor_next = false;
    for line in src.lines() {
        if line.contains("#[wasm_bindgen(constructor)]") {
            constructor_next = true;
            continue;
        }
        let Some(at) = line.find("pub fn ") else { continue };
        if constructor_next {
            constructor_next = false;
            continue;
        }
        // 서명이 복수 행으로 분할되어 있어도 괄호가 폐쇄되는 지점까지 판독한다.
        let from = rest.find(&line[at..]).expect("행을 소스에서 탐색하지 못했습니다");
        let sig = &rest[from..];
        let close = sig.find(')').expect("폐쇄 괄호가 부재합니다");
        let (head, args) = sig[..close].split_once('(').expect("개시 괄호가 부재합니다");
        let name = head.trim_start_matches("pub fn ").trim().to_string();
        let args = args
            .split(',')
            .map(str::trim)
            .filter(|a| !a.is_empty() && !a.ends_with("self"))
            .count();
        rest = &rest[from + close..];
        found.push(Method { name, args });
    }
    found
}

const SRC: &str = include_str!("../src/lib.rs");

/// **보전 대상을 탐색하지 못하면 통과가 아니라 실패다.** 파일 명칭이 변경되거나
/// 껍질이 타 위치로 이전되면, 아무것도 판정하지 못하는 검사가 묵시적으로
/// 통과한다.
#[test]
fn the_shell_methods_are_there_to_look_at() {
    let found = methods(SRC);
    assert!(
        found.len() > 2,
        "껍질에서 판정할 메서드를 탐색하지 못했습니다 — 검사가 실명한 것입니다: {found:?}"
    );
}

#[test]
fn a_request_crosses_as_one_piece() {
    let many: Vec<Method> = methods(SRC).into_iter().filter(|m| m.args > 1).collect();
    assert!(
        many.is_empty(),
        "self 이외의 인자를 둘 이상 수용하는 위치가 존재합니다 — JSON 요청 하나로 수용하십시오: {:?}",
        many.iter().map(|m| (&m.name, m.args)).collect::<Vec<_>>()
    );
}

/* ── 비가시 파일의 호출부 ───────────────────────────────────────── */

const WASM_TESTS: &str = include_str!("wasm.rs");

/// 껍질에서 호출하는 명칭들을 탐색한다 (`book.<명칭>(`).
fn called(src: &str) -> Vec<String> {
    let mut names = Vec::new();
    for (_, rest) in src.match_indices("book.").map(|(i, _)| (i, &src[i + "book.".len()..])) {
        let Some(open) = rest.find('(') else { continue };
        let name = &rest[..open];
        if !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            names.push(name.to_string());
        }
    }
    names.sort();
    names.dedup();
    names
}

/// **wasm 시험이 호출하는 명칭이 껍질에 존재하는가.**
///
/// `tests/wasm.rs` 는 최상단이 `#![cfg(target_arch = "wasm32")]` 라 데스크톱에서
/// `cargo test --workspace` 를 실행하면 전체가 공백 파일이다. 껍질의 함수 명칭을
/// 변경하고 그 파일을 수정하지 않아도 **`ci:gate` 는 통과**한다. 1회 그렇게
/// 발생했다.
///
/// 인자의 수까지는 판정하지 않는다 — 요청 하나로만 통과한다는 것은 상기에서
/// 판정한다.
#[test]
fn the_wasm_tests_call_names_that_exist() {
    let have: Vec<String> = methods(SRC).into_iter().map(|m| m.name).collect();
    let calls = called(WASM_TESTS);
    assert!(!calls.is_empty(), "wasm 시험에서 호출부를 탐색하지 못했습니다 — 검사가 실명한 것입니다");
    let missing: Vec<&String> = calls.iter().filter(|c| !have.contains(c)).collect();
    assert!(missing.is_empty(), "껍질에 부재하는 명칭을 호출합니다: {missing:?} (존재하는 것: {have:?})");
}

/// 그 검사가 실제로 차단하는지 판정한다.
#[test]
fn the_check_catches_a_wasm_test_calling_a_gone_name() {
    let have: Vec<String> = methods(SRC).into_iter().map(|m| m.name).collect();
    let doctored = r#"let v = book.pageWithThreeStrings("x");"#;
    let calls = called(doctored);
    assert_eq!(calls, ["pageWithThreeStrings"]);
    assert!(!have.contains(&calls[0]), "껍질에 그러한 명칭이 존재합니다");
}

/* 관문이 거기 존재하는가 — 액션이 조각 전부를 실행하는가, 배포가 그것을
   대기하는가 — 는 `gate.rs` 가 판정한다. 여기서도 판정하면 두 벌이 되고, 두 벌은
   한쪽만 수정된다. */

/// **이 검사가 실제로 차단하는지**를 의도적으로 훼손한 소스로 판정한다. 글자를
/// 판독하는 검사는 자체가 실명해도 묵시적이다.
#[test]
fn the_check_catches_a_shell_that_takes_three_strings() {
    let doctored = r#"
#[wasm_bindgen]
impl Book {
    pub fn values(&self, key: &str, filters: &str, needle: &str) -> Result<String, JsValue> {
        todo!()
    }
}
"#;
    let found = methods(doctored);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].args, 3, "{found:?}");
}

/// 복수 행으로 분할된 서명도 판정한다. 행만 판정하면 인자가 하나씩 분리되어
/// 있어 전부 통과한다.
#[test]
fn the_check_reads_a_signature_split_over_lines() {
    let doctored = r#"
#[wasm_bindgen]
impl Book {
    pub fn values(
        &self,
        key: &str,
        filters: &str,
    ) -> Result<String, JsValue> {
        todo!()
    }
}
"#;
    assert_eq!(methods(doctored)[0].args, 2, "{:?}", methods(doctored));
}

/// 양식 바이트를 수용하는 위치는 요청이 아니다. 그것까지 차단하면 개방 경로가 부재한다.
#[test]
fn the_constructor_is_not_a_request() {
    let doctored = r#"
#[wasm_bindgen]
impl Book {
    #[wasm_bindgen(constructor)]
    pub fn new(bytes: &[u8], name: &str) -> Result<Book, JsValue> {
        todo!()
    }
}
"#;
    assert!(methods(doctored).is_empty(), "{:?}", methods(doctored));
}
