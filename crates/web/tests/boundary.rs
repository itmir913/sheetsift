//! **경계는 요청 한 덩이로 넘는다.**
//!
//! 브라우저와 주고받는 겹([`bindings`](../src/bindings.rs))의 메서드는 self 말고
//! 인자를 둘 이상 받지 않는다. 받는 것은 JSON 글자 하나다.
//!
//! 까닭은 부르는 쪽이다. 같은 요청을 자리마다 다른 수의 인자로 넘기면,
//! 무엇이 어떤 차례로 넘어가는지가 부르는 자리마다 갈린다 — 그리고 글자
//! 셋을 자리만 바꿔 넘겨도 **오류가 나지 않는다.** 열 이름 자리에 필터가
//! 들어간 채로 "그런 값 없습니다" 가 뜬다.
//!
//! **이 검사는 소스를 글자로 읽는다.** 그 파일은 맨 위가
//! `#![cfg(target_arch = "wasm32")]` 라 데스크톱에서는 컴파일되지 않아,
//! 돌려 보는 시험이 모양을 볼 수 없다.

/// 껍질의 메서드 하나. 이름과 self 를 뺀 인자 수.
#[derive(Debug)]
struct Method {
    name: String,
    args: usize,
}

/// `pub fn` 들을 찾아 self 를 뺀 인자 수를 센다.
///
/// `#[wasm_bindgen(constructor)]` 이 붙은 것은 뺀다 — 그것은 요청이 아니라
/// 양식 바이트와 이름을 받는 자리다.
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
        // 서명이 여러 줄로 갈려 있어도 괄호가 닫히는 데까지 읽는다.
        let from = rest.find(&line[at..]).expect("줄을 소스에서 찾지 못했습니다");
        let sig = &rest[from..];
        let close = sig.find(')').expect("닫는 괄호가 없습니다");
        let (head, args) = sig[..close].split_once('(').expect("여는 괄호가 없습니다");
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

const SRC: &str = include_str!("../src/bindings.rs");

/// **지키는 대상을 찾지 못하면 통과가 아니라 실패다.** 파일 이름이 바뀌거나
/// 껍질이 다른 자리로 옮겨 가면, 아무것도 못 보는 검사가 조용히 초록불이 된다.
#[test]
fn the_shell_methods_are_there_to_look_at() {
    let found = methods(SRC);
    assert!(
        found.len() > 2,
        "껍질에서 볼 메서드를 찾지 못했습니다 — 검사가 눈먼 것입니다: {found:?}"
    );
}

#[test]
fn a_request_crosses_as_one_piece() {
    let many: Vec<Method> = methods(SRC).into_iter().filter(|m| m.args > 1).collect();
    assert!(
        many.is_empty(),
        "self 말고 인자를 둘 이상 받는 자리가 있습니다 — JSON 요청 하나로 받으세요: {:?}",
        many.iter().map(|m| (&m.name, m.args)).collect::<Vec<_>>()
    );
}

/* ── 보이지 않는 파일의 부르는 자리 ─────────────────────────────── */

const WASM_TESTS: &str = include_str!("wasm.rs");

/// 껍질에서 부르는 이름들을 찾는다 (`book.<이름>(`).
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

/// **wasm 시험이 부르는 이름이 껍질에 있는가.**
///
/// `tests/wasm.rs` 는 맨 위가 `#![cfg(target_arch = "wasm32")]` 라 데스크톱에서
/// `cargo test --workspace` 를 돌리면 통째로 빈 파일이다. 껍질의 함수 이름을
/// 바꾸고 그 파일을 안 고쳐도 **첫째 관문은 초록불**이다. 한 번 그렇게 났다.
///
/// 인자의 수까지는 보지 않는다 — 요청 하나로만 넘는다는 것은 위에서 본다.
#[test]
fn the_wasm_tests_call_names_that_exist() {
    let have: Vec<String> = methods(SRC).into_iter().map(|m| m.name).collect();
    let calls = called(WASM_TESTS);
    assert!(!calls.is_empty(), "wasm 시험에서 부르는 자리를 찾지 못했습니다 — 검사가 눈먼 것입니다");
    let missing: Vec<&String> = calls.iter().filter(|c| !have.contains(c)).collect();
    assert!(missing.is_empty(), "껍질에 없는 이름을 부릅니다: {missing:?} (있는 것: {have:?})");
}

/// 그 검사가 정말 막는지 본다.
#[test]
fn the_check_catches_a_wasm_test_calling_a_gone_name() {
    let have: Vec<String> = methods(SRC).into_iter().map(|m| m.name).collect();
    let doctored = r#"let v = book.pageWithThreeStrings("x");"#;
    let calls = called(doctored);
    assert_eq!(calls, ["pageWithThreeStrings"]);
    assert!(!have.contains(&calls[0]), "껍질에 그런 이름이 있습니다");
}

/// **이 검사가 정말 막는지**를 일부러 어긋낸 소스로 본다. 글자를 읽는 검사는
/// 제 눈이 멀어도 조용하다.
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

/// 여러 줄로 갈린 서명도 본다. 줄만 보면 인자가 하나씩 떨어져 있어 전부
/// 통과한다.
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

/// 양식 바이트를 받는 자리는 요청이 아니다. 그것까지 막으면 열 길이 없다.
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
