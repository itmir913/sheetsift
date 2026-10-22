//! **부착한 문이 양측과 일치하는가.**
//!
//! 창을 기동하는 시험은 여기 배치하지 않는다. tauri 가 제공하는 모의 실행기는
//! 시험 실행 파일에 webview 를 결속하는데, 그 실행 파일이 **적재 시점에
//! 중단된다**(`STATUS_ENTRYPOINT_NOT_FOUND`) — 시험이 1건도 실행되지 않고
//! 사유는 코드와 무관하다. 실행하지 못하는 시험을 관문에 배치하면 그 조각은
//! 영구히 적색이다.
//!
//! 그래서 **소스를 글자로 판독한다.** 여기서 판정하는 것은 부착의 정합이고,
//! 문이 수행하는 작업은 `sheetsift-desk` 의 시험이 실행해 판정한다.
//!
//! 불일치는 **오류가 발생하지 않는 부류**다. 문 명칭을 변경하고 부착 목록을
//! 수정하지 않으면 컴파일은 통과하고 창에서만 중단된다. 화면의 호출부도
//! 동일하다 — TypeScript 는 문 명칭을 문자로 전달하므로 타입 검사가 판정하지
//! 못한다.

const COMMANDS: &str = include_str!("../src/commands.rs");
const LIB: &str = include_str!("../src/lib.rs");
const BOOK: &str = include_str!("../../../web/src/lib/book.tauri.ts");

/// `#[tauri::command]` 가 부착된 `pub fn` 들.
fn doors(src: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut marked = false;
    for line in src.lines().map(str::trim) {
        if line == "#[tauri::command]" {
            marked = true;
            continue;
        }
        let Some(rest) = line.strip_prefix("pub fn ") else { continue };
        if !marked {
            continue;
        }
        marked = false;
        let name: String =
            rest.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
        if !name.is_empty() {
            found.push(name);
        }
    }
    found.sort();
    found
}

/// `generate_handler![…]` 에 기재된 명칭들.
fn attached(src: &str) -> Vec<String> {
    let Some(at) = src.find("generate_handler![") else { return Vec::new() };
    let body = &src[at..];
    let to = body.find(']').unwrap_or(body.len());
    let mut found: Vec<String> = body[..to]
        .split(',')
        .filter_map(|p| p.trim().rsplit("::").next().map(str::to_string))
        .filter(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'))
        .collect();
    found.sort();
    found
}

/// 화면이 호출하는 문 명칭들(`call<…>("<명칭>"`).
fn called(src: &str) -> Vec<String> {
    const LEAD: &str = "call<";
    let mut found: Vec<String> = Vec::new();
    for (i, _) in src.match_indices(LEAD) {
        let rest = &src[i..];
        let Some(open) = rest.find("(\"") else { continue };
        let name: String = rest[open + 2..]
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        if !name.is_empty() && !found.contains(&name) {
            found.push(name);
        }
    }
    found.sort();
    found
}

/// **보전 대상을 탐색하지 못하면 통과가 아니라 실패다.**
#[test]
fn the_sources_are_there_to_look_at() {
    assert!(doors(COMMANDS).len() > 2, "문을 탐색하지 못했습니다 — 검사가 실명한 것입니다");
    assert!(attached(LIB).len() > 2, "부착 목록을 탐색하지 못했습니다 — 검사가 실명한 것입니다");
    assert!(called(BOOK).len() > 2, "화면의 호출부를 탐색하지 못했습니다 — 검사가 실명한 것입니다");
}

/// **기재한 문이 전부 부착되었는가.** 부착하지 않은 문은 창에서 "그런 명령이
/// 부재합니다" 로 중단된다.
#[test]
fn every_door_is_attached() {
    assert_eq!(doors(COMMANDS), attached(LIB), "기재한 문과 부착한 목록이 상이합니다");
}

/// **화면이 호출하는 명칭이 전부 실재하는가.**
#[test]
fn the_screen_calls_doors_that_exist() {
    let have = doors(COMMANDS);
    let calls = called(BOOK);
    let missing: Vec<&String> = calls.iter().filter(|c| !have.contains(c)).collect();
    assert!(missing.is_empty(), "부재하는 문을 호출합니다: {missing:?} (존재하는 것: {have:?})");
}

/* ── 그 검사들이 실제로 차단하는지 ──────────────────────────────── */

#[test]
fn the_check_catches_a_door_nobody_attached() {
    let doctored = "#[tauri::command]\npub fn brand_new(desk: Held) -> Result<()> {}\n";
    assert_eq!(doors(doctored), ["brand_new"]);
    assert!(!attached(LIB).contains(&"brand_new".to_string()));
}

#[test]
fn the_check_catches_a_screen_calling_a_gone_name() {
    let doctored = r#"return call<void>("open_the_wrong_thing", { path });"#;
    assert_eq!(called(doctored), ["open_the_wrong_thing"]);
    assert!(!doors(COMMANDS).contains(&"open_the_wrong_thing".to_string()));
}

/// 부착하지 않은 `pub fn` 은 문이 아니다. 그것까지 산정하면 보조 함수를 배치할
/// 수 없다.
#[test]
fn a_plain_function_is_not_a_door() {
    let doctored = "pub fn held(desk: &Held) -> Guard {}\n";
    assert!(doors(doctored).is_empty(), "{:?}", doors(doctored));
}
