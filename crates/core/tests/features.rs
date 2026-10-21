//! **켜지 않은 층이 빠져도 컴파일이 되는가.**
//!
//! 이 바탕에는 켜야 들어오는 층이 있다(`sift`·`infer`). 켜지 않는 앱이
//! 실제로 있으므로, **그 조합이 컴파일되는지**가 지켜져야 한다. 그런데 그것은
//! `cargo test --workspace` 가 볼 수 없다 — 시험은 기본 기능으로 돌고,
//! 기본은 다 켠 것이다.
//!
//! 끄고 한 번 더 빌드하는 명령을 밖에 두는 길도 있지만, 돌리는 명령을 새로
//! 만들지 않는다. 밖에 두면 어디선가만 돌고 어디선가는 안 돌게 된다.
//!
//! 그래서 **그 조합이 깨질 수 있는 길 하나**를 글자로 막는다. 켜고 끄는
//! 것으로 깨지는 경우는 하나뿐이다 — **늘 들어오는 코드가 켜야 들어오는
//! 층을 부르는 것**이다. 그 길이 없으면 끄는 것은 코드를 덜어 내는 일일 뿐이다.
//!
//! 진짜 확인은 그 층을 끄는 앱의 빌드가 한다. 여기서 막는 것은 **그 앱이
//! 받아 보기 전에** 알 수 있는 부분이다.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// 켜야 들어오는 층. 이름이 곧 파일 이름이고 모듈 이름이다.
const GATED: [&str; 2] = ["sift", "infer"];

fn src() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src")
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("{} 을 읽지 못했습니다: {e}", path.display()))
        .replace("\r\n", "\n")
}

/// 줄이 그 층을 부르고 있는가. `sift::Foo` · `crate::infer::bar` 꼴을 본다.
///
/// 주석과 글 속의 이름은 세지 않는다 — 그러면 이 시험을 설명하는 주석에 제가
/// 걸린다. 뒤에 `::` 가 붙은 것만 부르는 것으로 본다.
fn calls(line: &str, module: &str) -> bool {
    let needle = format!("{module}::");
    let mut from = 0;
    while let Some(at) = line[from..].find(&needle) {
        let at = from + at;
        // 앞 글자가 이름의 일부면 다른 이름이다(`my_sift::` 같은 것).
        let before = line[..at].chars().next_back();
        let joined = before.is_some_and(|c| c.is_alphanumeric() || c == '_');
        // 주석 안이면 세지 않는다. 덩이 주석까지 보려면 파서가 되어야 하므로,
        // 줄 주석과 문서 주석만 본다 — 이 파일이 막으려는 것은 코드다.
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

    for entry in std::fs::read_dir(&dir).expect("src 를 열지 못했습니다") {
        let path = entry.expect("src 를 읽지 못했습니다").path();
        if path.extension().is_none_or(|e| e != "rs") {
            continue;
        }
        let stem = path.file_stem().unwrap().to_string_lossy().to_string();
        // 켜야 들어오는 층끼리는 서로 불러도 된다. 함께 빠지거나 함께 들어온다
        // ... 고 할 수 없다. 따로 켤 수 있으므로 서로도 부르지 않아야 한다.
        looked.insert(stem.clone());
        let source = read(&path);
        for (i, line) in source.split('\n').enumerate() {
            for module in GATED {
                if module == stem {
                    continue;
                }
                if calls(line, module) {
                    fail.push(format!(
                        "src/{stem}.rs:{} 가 켜야 들어오는 `{module}` 을 부릅니다\n\
                         \x20   그 층을 끈 앱에서 컴파일이 섭니다. `#[cfg(feature = \"{module}\")]` 를\n\
                         \x20   붙이거나, 부르는 쪽을 그 층 안으로 옮기세요.",
                        i + 1
                    ));
                }
            }
        }
    }

    // **찾지 못하면 통과가 아니라 실패다.** 파일이 옮겨졌는데 초록불이 뜨면
    // 이 시험은 아무것도 보지 않으면서 본다고 말하는 것이 된다.
    for module in GATED {
        assert!(
            looked.contains(module),
            "src/{module}.rs 를 찾지 못했습니다 — 이름이 바뀌었다면 이 시험도 함께 고치세요"
        );
    }
    assert!(
        looked.len() > GATED.len(),
        "늘 들어오는 파일을 하나도 보지 못했습니다 — 자리가 옮겨졌는지 보세요"
    );

    assert!(fail.is_empty(), "\n✗ {}\n", fail.join("\n✗ "));
    println!("✓ 늘 들어오는 코드가 켜야 들어오는 층을 부르지 않습니다 (파일 {}개)", looked.len());
}

/// 이 시험이 정말 막는지 본다. 막으려는 모양을 못 알아보면 조용히 통과한다.
#[test]
fn the_check_sees_a_real_call_and_ignores_prose() {
    assert!(calls("    let x = sift::seal(&conn)?;", "sift"));
    assert!(calls("    crate::infer::layout(&s, \"-\")", "infer"));
    // 주석과 글 속의 이름은 부르는 것이 아니다.
    assert!(!calls("// sift::seal 은 저쪽에 있다", "sift"));
    assert!(!calls("/// 자세한 것은 infer::layout 을 보라", "infer"));
    // 이름의 일부일 뿐인 것도 아니다.
    assert!(!calls("    my_sift::seal(&conn)", "sift"));
    // 모듈 이름만 적힌 것은 부르는 것이 아니다 — `pub mod sift;` 가 그렇다.
    assert!(!calls("pub mod sift;", "sift"));
}
