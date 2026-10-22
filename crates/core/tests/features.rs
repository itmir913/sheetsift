//! **활성화하지 않은 층이 누락되어도 컴파일이 되는가.**
//!
//! 이 바탕에는 활성화해야 포함되는 층이 존재한다(`sift`·`infer`). 활성화하지
//! 않는 앱이 실재하므로, **그 조합이 컴파일되는지**가 보전되어야 한다. 그런데
//! 그것은 `cargo test --workspace` 가 확인할 수 없다 — 시험은 기본 기능으로
//! 동작하고, 기본은 전부 활성화한 상태다.
//!
//! 비활성화해 재차 빌드하는 명령을 외부에 배치하는 경로도 존재하나, 실행 명령을
//! 신규 생성하지 않는다. 외부에 배치하면 일부 환경에서만 동작하고 일부
//! 환경에서는 동작하지 않게 된다.
//!
//! 그래서 **그 조합이 훼손될 수 있는 경로 하나**를 글자로 차단한다. 활성화·
//! 비활성화로 훼손되는 경우는 하나뿐이다 — **항상 포함되는 코드가 활성화해야
//! 포함되는 층을 호출하는 것**이다. 그 경로가 부재하면 비활성화는 코드를
//! 제거하는 작업일 뿐이다.
//!
//! 실제 확인은 그 층을 비활성화하는 앱의 빌드가 수행한다. 여기서 차단하는 것은
//! **그 앱이 수령하기 이전에** 판별 가능한 부분이다.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// 활성화해야 포함되는 층. 명칭이 곧 파일 명칭이고 모듈 명칭이다.
const GATED: [&str; 2] = ["sift", "infer"];

fn src() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src")
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("{} 을 판독하지 못했습니다: {e}", path.display()))
        .replace("\r\n", "\n")
}

/// 행이 그 층을 호출하고 있는가. `sift::Foo` · `crate::infer::bar` 형태를
/// 판정한다.
///
/// 주석과 문장 내부의 명칭은 산정하지 않는다 — 그러면 이 시험을 설명하는 주석에
/// 자체가 포착된다. 후단에 `::` 가 부착된 것만 호출로 판정한다.
fn calls(line: &str, module: &str) -> bool {
    let needle = format!("{module}::");
    let mut from = 0;
    while let Some(at) = line[from..].find(&needle) {
        let at = from + at;
        // 전단 문자가 명칭의 일부면 상이한 명칭이다(`my_sift::` 등).
        let before = line[..at].chars().next_back();
        let joined = before.is_some_and(|c| c.is_alphanumeric() || c == '_');
        // 주석 내부면 산정하지 않는다. 블록 주석까지 판정하려면 파서가 되어야
        // 하므로, 행 주석과 문서 주석만 판정한다 — 이 파일이 차단하려는 것은 코드다.
        let commented = line[..at].contains("//");
        if !joined && !commented {
            return true;
        }
        from = at + needle.len();
    }
    false
}

#[test]
fn nothing_that_is_always_there_reaches_into_a_gated_layer() {
    let dir = src();
    let mut looked = BTreeSet::new();
    let mut fail: Vec<String> = Vec::new();

    for entry in std::fs::read_dir(&dir).expect("src 를 개방하지 못했습니다") {
        let path = entry.expect("src 를 판독하지 못했습니다").path();
        if path.extension().is_none_or(|e| e != "rs") {
            continue;
        }
        let stem = path.file_stem().unwrap().to_string_lossy().to_string();
        // 활성화해야 포함되는 층 상호간은 호출해도 무방하다. 함께 누락되거나 함께
        // 포함된다 ... 고 단정할 수 없다. 개별 활성화가 가능하므로 상호간에도
        // 호출하지 않아야 한다.
        looked.insert(stem.clone());
        let source = read(&path);
        for (i, line) in source.split('\n').enumerate() {
            for module in GATED {
                if module == stem {
                    continue;
                }
                if calls(line, module) {
                    fail.push(format!(
                        "src/{stem}.rs:{} 가 활성화해야 포함되는 `{module}` 을 호출합니다\n\
                         \x20   그 층을 비활성화한 앱에서 컴파일이 중단됩니다. `#[cfg(feature = \"{module}\")]` 를\n\
                         \x20   부착하거나, 호출부를 그 층 내부로 이전하십시오.",
                        i + 1
                    ));
                }
            }
        }
    }

    // **탐색하지 못하면 통과가 아니라 실패다.** 파일이 이전되었는데 통과하면
    // 이 시험은 아무것도 판정하지 않으면서 판정한다고 주장하는 것이 된다.
    for module in GATED {
        assert!(
            looked.contains(module),
            "src/{module}.rs 를 탐색하지 못했습니다 — 명칭이 변경되었다면 이 시험도 함께 수정하십시오"
        );
    }
    assert!(
        looked.len() > GATED.len(),
        "항상 포함되는 파일을 하나도 판정하지 못했습니다 — 위치가 이전되었는지 확인하십시오"
    );

    assert!(fail.is_empty(), "\n✗ {}\n", fail.join("\n✗ "));
    println!("✓ 항상 포함되는 코드가 활성화해야 포함되는 층을 호출하지 않습니다 (파일 {}개)", looked.len());
}

/// 이 시험이 실제로 차단하는지 판정한다. 차단 대상 형태를 식별하지 못하면 묵시적으로 통과한다.
#[test]
fn the_check_sees_a_real_call_and_ignores_prose() {
    assert!(calls("    let x = sift::seal(&conn)?;", "sift"));
    assert!(calls("    crate::infer::layout(&s, \"-\")", "infer"));
    // 주석과 문장 내부의 명칭은 호출이 아니다.
    assert!(!calls("// sift::seal 은 타처에 소재한다", "sift"));
    assert!(!calls("/// 상세는 infer::layout 을 참조하라", "infer"));
    // 명칭의 일부에 불과한 것도 아니다.
    assert!(!calls("    my_sift::seal(&conn)", "sift"));
    // 모듈 명칭만 기재된 것은 호출이 아니다 — `pub mod sift;` 가 해당한다.
    assert!(!calls("pub mod sift;", "sift"));
}
