//! **화면이 바탕과 결합되어 있는가.**
//!
//! 화면은 TypeScript 라 `cargo test` 가 실행할 수 없다. 그래서 **소스를 글자로
//! 판독한다** — 여기서 판정하는 것들은 불일치해도 오류가 발생하지 않고, 화면에
//! 상이한 것이 그럴듯하게 표시되는 방향으로 묵시적으로 진행되기 때문이다.
//!
//! **보전 대상을 탐색하지 못하면 통과가 아니라 실패다.** 파일 명칭이 변경되거나
//! 탐색하는 문구가 소멸하면, 아무것도 판정하지 못하는 검사가 묵시적으로
//! 통과한다. 그래서 의도적으로 훼손한 입력을 투입해 실제로 중단하는지도 동일한
//! 파일에서 판정한다.

/* ── 오류 코드 사전 ──────────────────────────────────────────────── */

const ERROR_RS: &str = include_str!("../../core/src/error.rs");
/// 데스크톱 껍질이 자체로 발생시키는 코드. 바탕은 파일 시스템을 인지하지 않아
/// 파일을 판독하지 못한 사유를 진술할 수단이 부재하다.
const DESK_RS: &str = include_str!("../../desk/src/error.rs");
const CODES_TS: &str = include_str!("../../../web/src/i18n/codes.ts");
const MESSAGES_TS: &str = include_str!("../../../web/src/i18n/messages.ko.ts");

/// 겹이 오류를 문자로조차 변환하지 못한 위치에서 발생시키는 코드. 바탕의 enum
/// 에는 부재하고 `sheetsift-wasm` 의 껍질이 생성한다.
const SHELL_ONLY: &str = "UNKNOWN";

/// 화면이 수용해야 하는 코드 전부 — 바탕의 것과 데스크톱 껍질의 것.
///
/// **화면은 단일 벌이다.** 브라우저에서 구동하든 창에서 구동하든 동일한 사전이
/// 문구를 부여하므로, 일방만 발생시키는 코드도 거기 한 행이 존재해야 한다.
fn every_code() -> Vec<String> {
    let mut all = backend_codes(ERROR_RS);
    all.extend(backend_codes(DESK_RS));
    all
}

/// `ErrorCode` 의 변종 명칭을 serde 가 반출하는 문자로. `#[serde(rename_all =
/// "SCREAMING_SNAKE_CASE")]` 와 동일한 규칙이다.
fn screaming(variant: &str) -> String {
    let mut out = String::new();
    for (i, c) in variant.char_indices() {
        if c.is_ascii_uppercase() && i > 0 {
            out.push('_');
        }
        out.push(c.to_ascii_uppercase());
    }
    out
}

/// `error.rs` 의 `ErrorCode` 변종들을 탐색한다.
///
/// `#[cfg(feature = ...)]` 이 부착된 것도 산정한다. 이 저장소의 기본은 전부
/// 활성화한 상태이고, **비활성화하는 앱도 그 코드의 문구를 기재해야 한다** —
/// 코드가 단일 enum 이라서다.
fn backend_codes(src: &str) -> Vec<String> {
    let from = match src.find("pub enum ErrorCode {") {
        Some(at) => at,
        None => return Vec::new(),
    };
    let body = &src[from..];
    let to = body.find("\n}").unwrap_or(body.len());
    body[..to]
        .lines()
        .skip(1)
        .map(str::trim)
        .filter(|l| l.ends_with(','))
        .map(|l| screaming(l.trim_end_matches(',')))
        .collect()
}

/// `codes.ts` 의 `ERROR_CODES` 에 기재된 문자들.
///
/// 행마다 하나씩 기재되어 있으나 **행을 산정하지 않는다.** 한 행에 복수가
/// 기재되어도 동일하게 판독해야 한다 — 서식을 변경하는 시점에 검사가 "목록이
/// 공백이다" 로 중단되면, 수정한 사람은 자신이 무엇을 훼손했는지 규명하지
/// 못한다.
fn listed_codes(src: &str) -> Vec<String> {
    let from = match src.find("export const ERROR_CODES = [") {
        Some(at) => at,
        None => return Vec::new(),
    };
    let body = &src[from..];
    let to = body.find("] as const;").unwrap_or(body.len());
    // 따옴표 쌍을 순차 추출한다. 코드 형태(대문자와 밑줄)인 것만 존치해, 주석에
    // 따옴표가 혼입되어도 목록이 증가하지 않는다.
    body[..to]
        .split('"')
        .skip(1)
        .step_by(2)
        .filter(|s| !s.is_empty() && s.chars().all(|c| c.is_ascii_uppercase() || c == '_'))
        .map(str::to_string)
        .collect()
}

/// 사전(`messages.ko.ts`)이 문구를 기재한 코드들.
///
/// 값이 **다음 행으로 이월된 것도 산정한다.** 장문은 행을 이월하는데, 동일한
/// 행에 따옴표가 존재하는 것만 산정하면 그러한 행이 "문구가 부재한다" 로
/// 포착된다 — 정상인 사전을 두고 검사가 중단되는 쪽이라, 누구도 신뢰하지 않게
/// 된다.
fn spoken_codes(src: &str) -> Vec<String> {
    let lines: Vec<&str> = src.lines().map(str::trim).collect();
    let mut found = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let Some((name, rest)) = line.split_once(':') else { continue };
        if name.is_empty() || !name.chars().all(|c| c.is_ascii_uppercase() || c == '_') {
            continue;
        }
        // 값이 존재해야 문구다. 동일한 행에 부재하면 직후 행에 존재해야 한다.
        let here = rest.trim();
        let said = if here.is_empty() {
            lines.get(i + 1).is_some_and(|n| n.starts_with('"'))
        } else {
            here.starts_with('"')
        };
        if said {
            found.push(name.to_string());
        }
    }
    found
}

#[test]
fn the_sources_are_there_to_look_at() {
    assert!(
        !backend_codes(ERROR_RS).is_empty(),
        "바탕의 ErrorCode 를 탐색하지 못했습니다 — 검사가 실명한 것입니다"
    );
    assert!(
        !backend_codes(DESK_RS).is_empty(),
        "데스크톱 껍질의 ErrorCode 를 탐색하지 못했습니다 — 검사가 실명한 것입니다"
    );
    assert!(
        !listed_codes(CODES_TS).is_empty(),
        "화면의 ERROR_CODES 를 탐색하지 못했습니다 — 검사가 실명한 것입니다"
    );
    assert!(
        !spoken_codes(MESSAGES_TS).is_empty(),
        "화면의 문구 사전을 탐색하지 못했습니다 — 검사가 실명한 것입니다"
    );
}

/// **바탕이 발생시킬 수 있는 코드가 화면 목록에 전부 존재하는가.**
///
/// 누락되면 그 코드가 도달하는 시점에 화면에 코드 문자가 그대로 표시된다. 오류는 발생하지 않는다.
#[test]
fn the_screen_lists_every_backend_code() {
    let listed = listed_codes(CODES_TS);
    let missing: Vec<String> = every_code().into_iter().filter(|c| !listed.contains(c)).collect();
    assert!(missing.is_empty(), "화면 목록에 부재하는 코드가 존재합니다: {missing:?}");
}

/// 화면 목록에 **바탕에 부재하는 코드**가 잔존하지 않는가. 명칭을 변경한 시점에
/// 구 명칭이 사전에 그대로 잔존하면, 그 행은 영구히 화면에 표시될 수 없다.
#[test]
fn the_screen_lists_nothing_the_backend_cannot_send() {
    let backend = every_code();
    let extra: Vec<String> = listed_codes(CODES_TS)
        .into_iter()
        .filter(|c| c != SHELL_ONLY && !backend.contains(c))
        .collect();
    assert!(extra.is_empty(), "바탕에 부재하는 코드가 화면 목록에 존재합니다: {extra:?}");
}

/// **목록에 존재하는 코드마다 문구가 존재하는가.** 하나라도 공백이면 그 시점에 코드가 표시된다.
#[test]
fn every_listed_code_has_something_to_say() {
    let spoken = spoken_codes(MESSAGES_TS);
    let silent: Vec<String> =
        listed_codes(CODES_TS).into_iter().filter(|c| !spoken.contains(c)).collect();
    assert!(silent.is_empty(), "문구가 부재하는 코드가 존재합니다: {silent:?}");
}

/* 하기 셋은 **이 검사가 실제로 차단하는지**를 판정한다. 글자를 판독하는 검사는
   자체가 실명해도 묵시적이다. */

#[test]
fn the_check_catches_a_code_the_screen_never_heard_of() {
    let doctored = "pub enum ErrorCode {\n    DbQueryFailed,\n    BrandNewThing,\n}\n";
    let found = backend_codes(doctored);
    assert_eq!(found, ["DB_QUERY_FAILED", "BRAND_NEW_THING"]);
    assert!(!listed_codes(CODES_TS).contains(&found[1]));
}

#[test]
fn the_check_catches_a_code_with_no_words_for_it() {
    let doctored = r#"export const ERROR_CODES = ["DB_QUERY_FAILED", "NOBODY_WROTE_THIS"] as const;"#;
    let listed = listed_codes(doctored);
    assert_eq!(listed, ["DB_QUERY_FAILED", "NOBODY_WROTE_THIS"]);
    let spoken = spoken_codes(MESSAGES_TS);
    assert!(spoken.contains(&listed[0]));
    assert!(!spoken.contains(&listed[1]));
}

#[test]
fn the_check_reads_a_dictionary_line() {
    let doctored = "  SHEET_EMPTY: \"헤더 행이 부재합니다.\",\n  notACode: \"x\",\n";
    assert_eq!(spoken_codes(doctored), ["SHEET_EMPTY"]);
}

/// 행을 이월한 문구도 판독하는가. 판독하지 못하면 **정상인 사전을 두고 검사가
/// 중단된다** — 1회 그렇게 발생하면 누구도 그 검사를 신뢰하지 않는다.
#[test]
fn the_check_reads_a_message_wrapped_onto_the_next_line() {
    let doctored = "  SHEET_ENCODING_UNKNOWN:\n    \"문자 표기를 판별하지 못했습니다.\",\n";
    assert_eq!(spoken_codes(doctored), ["SHEET_ENCODING_UNKNOWN"]);
}

/// 값이 부재하는 행은 문구가 아니다. 그것까지 산정하면 공백 항목을 포착하지 못한다.
#[test]
fn the_check_does_not_count_a_code_with_nothing_after_it() {
    assert_eq!(spoken_codes("  SHEET_EMPTY:\n\n  OTHER: \"x\",\n"), ["OTHER"]);
}

/* ── 고정 가능한 열 수 ──────────────────────────────────────────── */

const LAYOUT_TS: &str = include_str!("../../../web/src/composables/useColumnLayout.ts");
const GRID_CSS: &str = include_str!("../../../web/src/styles/grid.css");

/// 화면이 고정 가능하다고 판단하는 열 수(`MAX_PIN`).
fn max_pin(src: &str) -> Option<usize> {
    let at = src.find("export const MAX_PIN = ")?;
    src[at..].split('=').nth(1)?.trim().trim_end_matches(';').split(';').next()?.trim().parse().ok()
}

/// `grid.css` 가 실제로 고정하는 번호의 수.
fn pin_rules(src: &str) -> usize {
    let mut seen: Vec<&str> = Vec::new();
    for (_, rest) in src.match_indices("[data-pin=\"").map(|(i, _)| (i, &src[i + 11..])) {
        if let Some(n) = rest.split('"').next() {
            if n.chars().all(|c| c.is_ascii_digit()) && !n.is_empty() && !seen.contains(&n) {
                seen.push(n);
            }
        }
    }
    seen.len()
}

/// **고정한다고 선언하고 유실되지 않는가.**
///
/// 규칙이 부재하는 번호는 묵시적으로 유실된다. 단추는 작동하고 수치도 상승하는데
/// 열만 고정되지 않는다 — 어디에서도 오류가 발생하지 않는다.
#[test]
fn the_screen_cannot_pin_more_columns_than_the_css_holds() {
    let declared = max_pin(LAYOUT_TS).expect("MAX_PIN 을 탐색하지 못했습니다 — 검사가 실명한 것입니다");
    let rules = pin_rules(GRID_CSS);
    assert!(rules > 0, "grid.css 에서 고정 규칙을 탐색하지 못했습니다 — 검사가 실명한 것입니다");
    assert!(
        declared <= rules,
        "MAX_PIN 은 {declared} 인데 grid.css 가 고정하는 번호는 {rules} 개뿐입니다"
    );
}

#[test]
fn the_check_catches_a_pin_the_css_does_not_hold() {
    assert_eq!(max_pin("export const MAX_PIN = 9;\n"), Some(9));
    assert_eq!(pin_rules("[data-pin=\"0\"] {} [data-pin=\"1\"] {} [data-pin=\"0\"] {}"), 2);
}

/* ── 경계의 두 벌 ───────────────────────────────────────────────── */

const BOOK_WASM: &str = include_str!("../../../web/src/lib/book.wasm.ts");
const BOOK_TAURI: &str = include_str!("../../../web/src/lib/book.tauri.ts");

/// `export` 로 반출하는 명칭들. 함수·클래스·구조·상수를 전부 수용한다.
fn exported(src: &str) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    for line in src.lines().map(str::trim) {
        let Some(rest) = line.strip_prefix("export ") else { continue };
        let rest = rest.strip_prefix("async ").unwrap_or(rest);
        let Some(after) = ["function ", "class ", "interface ", "const ", "type "]
            .iter()
            .find_map(|k| rest.strip_prefix(k))
        else {
            continue;
        };
        let name: String =
            after.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
        if !name.is_empty() && !found.contains(&name) {
            found.push(name);
        }
    }
    found.sort();
    found
}

/// 클래스가 반출하는 정적 메서드와 인스턴스 메서드 명칭.
fn methods(src: &str) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    for line in src.lines().map(str::trim) {
        let line = line.strip_prefix("static ").unwrap_or(line);
        let line = line.strip_prefix("async ").unwrap_or(line);
        let Some((head, _)) = line.split_once('(') else { continue };
        if head.is_empty() || !head.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            continue;
        }
        if ["if", "for", "while", "catch", "switch", "return", "constructor"].contains(&head) {
            continue;
        }
        if !found.contains(&head.to_string()) {
            found.push(head.to_string());
        }
    }
    found.sort();
    found
}

#[test]
fn the_two_boundaries_are_there_to_look_at() {
    assert!(!exported(BOOK_WASM).is_empty(), "브라우저 벌의 반출을 탐색하지 못했습니다 — 검사가 실명한 것입니다");
    assert!(!exported(BOOK_TAURI).is_empty(), "창 벌의 반출을 탐색하지 못했습니다 — 검사가 실명한 것입니다");
    assert!(!methods(BOOK_WASM).is_empty(), "메서드를 탐색하지 못했습니다 — 검사가 실명한 것입니다");
}

/// **경계의 두 벌이 동일한 것을 반출하는가.**
///
/// 화면은 `sheetsift-book` 이라는 명칭으로 반입하고 어느 쪽인지 인지하지
/// 않는다. 일방에만 존재하는 문이 발생하면 **그 벌에서만 화면이 중단되고**,
/// 타입 검사는 기준으로 삼은 일방만 참조하므로 통과한다.
#[test]
fn both_boundaries_export_the_same_things() {
    assert_eq!(exported(BOOK_WASM), exported(BOOK_TAURI), "경계의 두 벌이 상이한 것을 반출합니다");
}

/// 문 명칭도 동일해야 한다. 반출하는 것이 동일해도 그 내부가 상이하면 동일한
/// 사안이 발생한다.
#[test]
fn both_boundaries_offer_the_same_doors() {
    let wasm = methods(BOOK_WASM);
    let tauri = methods(BOOK_TAURI);
    let only_wasm: Vec<&String> = wasm.iter().filter(|m| !tauri.contains(m)).collect();
    let only_tauri: Vec<&String> = tauri.iter().filter(|m| !wasm.contains(m)).collect();
    assert!(
        only_wasm.is_empty() && only_tauri.is_empty(),
        "일방에만 존재하는 문이 있습니다 — 브라우저: {only_wasm:?}, 창: {only_tauri:?}"
    );
}

/// 그 검사가 실제로 차단하는지 판정한다.
#[test]
fn the_check_catches_a_door_on_only_one_side() {
    let doctored = "export function onlyHere(): void {}\n";
    assert_eq!(exported(doctored), ["onlyHere"]);
    assert!(!exported(BOOK_WASM).contains(&"onlyHere".to_string()));
}

/* ── 문서의 제목과 설명 ─────────────────────────────────────────── */

const INDEX_HTML: &str = include_str!("../../../web/index.html");
const APP_TS: &str = include_str!("../../../web/src/lib/app.ts");

/// 쌍으로 감싼 값 하나를 추출한다.
fn between<'a>(src: &'a str, open: &str, close: &str) -> Option<&'a str> {
    let at = src.find(open)? + open.len();
    let rest = &src[at..];
    Some(&rest[..rest.find(close)?])
}

/// 화면이 자신을 호칭하는 명칭(`APP_NAME`).
fn app_name(src: &str) -> Option<&str> {
    between(src, "export const APP_NAME = \"", "\"")
}

/// **탭의 제목이 화면의 명칭과 분기하지 않는가.**
///
/// 제목은 `index.html` 에 기재한다 — 탐색기와 공유 카드는 기동 이전의 문서를
/// 판독하므로 켠 뒤에 부여하면 공백으로 수집된다. 그 대신 명칭이 두 곳에
/// 존재하게 되고, `APP_NAME` 만 변경하면 머리글과 탭이 상이한 명칭을 묘사하는데
/// **어디에서도 오류가 발생하지 않는다.**
#[test]
fn the_tab_title_carries_the_name_the_screen_uses() {
    let name = app_name(APP_TS).expect("APP_NAME 을 탐색하지 못했습니다 — 검사가 실명한 것입니다");
    let title = between(INDEX_HTML, "<title>", "</title>")
        .expect("<title> 을 탐색하지 못했습니다 — 검사가 실명한 것입니다");
    assert!(title.contains(name), "탭의 제목 '{title}' 에 화면의 명칭 '{name}' 이 부재합니다");
}

/// 설명이 부재하면 공유 카드와 탐색 결과가 공백이다. 화면은 정상적으로
/// 묘사되므로 실행하는 시험으로는 발견하지 못한다.
#[test]
fn the_document_states_what_this_is() {
    let meta = between(INDEX_HTML, "name=\"description\"", "/>")
        .expect("설명을 탐색하지 못했습니다 — 검사가 실명한 것입니다");
    let text = between(meta, "content=\"", "\"").unwrap_or("").trim();
    assert!(text.len() > 40, "설명이 지나치게 단축되었습니다: '{text}'");
}

/// 그 검사들이 실제로 차단하는지 판정한다.
#[test]
fn the_check_catches_a_title_that_lost_the_name() {
    assert_eq!(app_name("export const APP_NAME = \"Other\";\n"), Some("Other"));
    let doctored = "<title>Something else</title>";
    let title = between(doctored, "<title>", "</title>").expect("훼손한 입력의 제목");
    assert!(!title.contains("SheetSift"));
    assert!(between(doctored, "name=\"description\"", "/>").is_none());
}
