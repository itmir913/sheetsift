//! **화면이 바탕과 맞물려 있는가.**
//!
//! 화면은 TypeScript 라 `cargo test` 가 돌려 볼 수 없다. 그래서 **소스를 글자로
//! 읽는다** — 여기서 보는 것들은 어긋나도 오류가 나지 않고, 화면에 틀린 것이
//! 그럴듯하게 떠 있는 쪽으로 조용히 흘러가기 때문이다.
//!
//! **지키는 대상을 찾지 못하면 통과가 아니라 실패다.** 파일 이름이 바뀌거나
//! 찾는 글귀가 사라지면, 아무것도 못 보는 검사가 조용히 초록불이 된다. 그래서
//! 일부러 어긋낸 입력을 먹여 정말 서는지도 같은 파일에서 본다.

/* ── 오류 코드 사전 ──────────────────────────────────────────────── */

const ERROR_RS: &str = include_str!("../../core/src/error.rs");
const CODES_TS: &str = include_str!("../../../web/src/i18n/codes.ts");
const MESSAGES_TS: &str = include_str!("../../../web/src/i18n/messages.ko.ts");

/// 겹이 오류를 글자로조차 바꾸지 못한 자리에서 내는 코드. 바탕의 enum 에는
/// 없고 `bindings.rs` 가 만든다.
const SHELL_ONLY: &str = "UNKNOWN";

/// `ErrorCode` 의 변종 이름을 serde 가 내보내는 글자로. `#[serde(rename_all =
/// "SCREAMING_SNAKE_CASE")]` 와 같은 규칙이다.
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

/// `error.rs` 의 `ErrorCode` 변종들을 찾는다.
///
/// `#[cfg(feature = ...)]` 이 붙은 것도 센다. 이 저장소의 기본은 다 켠 것이고,
/// **끄는 앱도 그 코드의 문구를 적어야 한다** — 코드가 한 enum 이라서다.
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

/// `codes.ts` 의 `ERROR_CODES` 에 적힌 글자들.
fn listed_codes(src: &str) -> Vec<String> {
    let from = match src.find("export const ERROR_CODES = [") {
        Some(at) => at,
        None => return Vec::new(),
    };
    let body = &src[from..];
    let to = body.find("] as const;").unwrap_or(body.len());
    body[..to]
        .lines()
        .filter_map(|l| {
            let l = l.trim();
            l.strip_prefix('"').and_then(|r| r.split('"').next()).map(str::to_string)
        })
        .collect()
}

/// 사전(`messages.ko.ts`)이 문구를 적어 둔 코드들.
///
/// 값이 **다음 줄로 넘어간 것도 센다.** 긴 문구는 줄을 넘기는데, 같은 줄에
/// 따옴표가 있는 것만 세면 그런 줄이 "문구가 없다" 로 잡힌다 — 멀쩡한 사전을
/// 두고 검사가 서는 쪽이라, 아무도 못 믿게 된다.
fn spoken_codes(src: &str) -> Vec<String> {
    let lines: Vec<&str> = src.lines().map(str::trim).collect();
    let mut found = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let Some((name, rest)) = line.split_once(':') else { continue };
        if name.is_empty() || !name.chars().all(|c| c.is_ascii_uppercase() || c == '_') {
            continue;
        }
        // 값이 있어야 문구다. 같은 줄에 없으면 바로 다음 줄에 있어야 한다.
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
        "바탕의 ErrorCode 를 찾지 못했습니다 — 검사가 눈먼 것입니다"
    );
    assert!(
        !listed_codes(CODES_TS).is_empty(),
        "화면의 ERROR_CODES 를 찾지 못했습니다 — 검사가 눈먼 것입니다"
    );
    assert!(
        !spoken_codes(MESSAGES_TS).is_empty(),
        "화면의 문구 사전을 찾지 못했습니다 — 검사가 눈먼 것입니다"
    );
}

/// **바탕이 낼 수 있는 코드가 화면 목록에 다 있는가.**
///
/// 빠지면 그 코드가 오는 날 화면에 코드 글자가 그대로 뜬다. 오류는 나지 않는다.
#[test]
fn the_screen_lists_every_backend_code() {
    let listed = listed_codes(CODES_TS);
    let missing: Vec<String> =
        backend_codes(ERROR_RS).into_iter().filter(|c| !listed.contains(c)).collect();
    assert!(missing.is_empty(), "화면 목록에 없는 바탕 코드가 있습니다: {missing:?}");
}

/// 화면 목록에 **바탕에 없는 코드**가 남아 있지 않은가. 이름을 바꾼 날 옛
/// 이름이 사전에 그대로 남으면, 그 줄은 영영 화면에 뜰 수 없다.
#[test]
fn the_screen_lists_nothing_the_backend_cannot_send() {
    let backend = backend_codes(ERROR_RS);
    let extra: Vec<String> = listed_codes(CODES_TS)
        .into_iter()
        .filter(|c| c != SHELL_ONLY && !backend.contains(c))
        .collect();
    assert!(extra.is_empty(), "바탕에 없는 코드가 화면 목록에 있습니다: {extra:?}");
}

/// **목록에 있는 코드마다 문구가 있는가.** 하나라도 비면 그날 코드가 뜬다.
#[test]
fn every_listed_code_has_something_to_say() {
    let spoken = spoken_codes(MESSAGES_TS);
    let silent: Vec<String> =
        listed_codes(CODES_TS).into_iter().filter(|c| !spoken.contains(c)).collect();
    assert!(silent.is_empty(), "문구가 없는 코드가 있습니다: {silent:?}");
}

/* 아래 셋은 **이 검사가 정말 막는지**를 본다. 글자를 읽는 검사는 제 눈이
   멀어도 조용하다. */

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
    let doctored = "  SHEET_EMPTY: \"머리글 줄이 없습니다.\",\n  notACode: \"x\",\n";
    assert_eq!(spoken_codes(doctored), ["SHEET_EMPTY"]);
}

/// 줄을 넘긴 문구도 읽는가. 읽지 못하면 **멀쩡한 사전을 두고 검사가 선다** —
/// 한 번 그러고 나면 아무도 그 검사를 믿지 않는다.
#[test]
fn the_check_reads_a_message_wrapped_onto_the_next_line() {
    let doctored = "  SHEET_ENCODING_UNKNOWN:\n    \"글자 표기를 알아내지 못했습니다.\",\n";
    assert_eq!(spoken_codes(doctored), ["SHEET_ENCODING_UNKNOWN"]);
}

/// 값이 없는 줄은 문구가 아니다. 그것까지 세면 빈 칸을 못 잡는다.
#[test]
fn the_check_does_not_count_a_code_with_nothing_after_it() {
    assert_eq!(spoken_codes("  SHEET_EMPTY:\n\n  OTHER: \"x\",\n"), ["OTHER"]);
}

/* ── 붙잡을 수 있는 열 수 ────────────────────────────────────────── */

const LAYOUT_TS: &str = include_str!("../../../web/src/composables/useColumnLayout.ts");
const GRID_CSS: &str = include_str!("../../../web/src/styles/grid.css");

/// 화면이 붙잡을 수 있다고 믿는 열 수(`MAX_PIN`).
fn max_pin(src: &str) -> Option<usize> {
    let at = src.find("export const MAX_PIN = ")?;
    src[at..].split('=').nth(1)?.trim().trim_end_matches(';').split(';').next()?.trim().parse().ok()
}

/// `grid.css` 가 실제로 붙잡아 주는 번호의 수.
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

/// **붙잡는다고 해 놓고 흘러가지 않는가.**
///
/// 규칙이 없는 번호는 조용히 흘러간다. 단추는 눌리고 숫자도 올라가는데 열만
/// 안 붙잡힌다 — 어디에서도 오류가 나지 않는다.
#[test]
fn the_screen_cannot_pin_more_columns_than_the_css_holds() {
    let declared = max_pin(LAYOUT_TS).expect("MAX_PIN 을 찾지 못했습니다 — 검사가 눈먼 것입니다");
    let rules = pin_rules(GRID_CSS);
    assert!(rules > 0, "grid.css 에서 고정 규칙을 찾지 못했습니다 — 검사가 눈먼 것입니다");
    assert!(
        declared <= rules,
        "MAX_PIN 은 {declared} 인데 grid.css 가 붙잡아 주는 번호는 {rules} 개뿐입니다"
    );
}

#[test]
fn the_check_catches_a_pin_the_css_does_not_hold() {
    assert_eq!(max_pin("export const MAX_PIN = 9;\n"), Some(9));
    assert_eq!(pin_rules("[data-pin=\"0\"] {} [data-pin=\"1\"] {} [data-pin=\"0\"] {}"), 2);
}
