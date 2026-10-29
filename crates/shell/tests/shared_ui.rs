//! **공용 화면 패키지(`packages/sheetsift-ui`)가 제품의 것을 보유하지 않는가.**
//!
//! 그 패키지는 sheetsift 의 앱과 goe-navi 가 동일하게 사용한다. 그래서 제품마다
//! 상이한 것 — 문구, 색 값, 이 앱의 경계 — 를 보유하면 안 되고, 받는 앱이
//! 주입한다(`docs/shared-ui.md`). 보유해도 컴파일은 통과하고 이 앱의 화면도 정상이다.
//! **타 제품의 화면에서만** 발현된다 — 영어 화면에 한국어가 출현하거나, 제품의 색을
//! 피복했는데 일부만 추종하거나, 받는 쪽의 `@/` 가 엉뚱한 파일을 지시한다.
//!
//! 그래서 원본을 문자로 판독해 차단한다.
//!
//! | 판정 | 무엇 |
//! |---|---|
//! | 문구 | 주석 외부에 한글이 부재하고, 사람이 판독할 속성을 문자 그대로 기재하지 않는다 |
//! | 색 | `tokens.css` 외부에 색 값을 직접 기재하지 않는다 |
//! | 별칭 | `@/` 로 반입하지 않는다 |
//! | 경계 | 이 앱의 경계(`sheetsift-book`·`sheetsift-wasm`)를 반입하지 않는다 |
//! | 반출 | 뿌리 `package.json` 의 반출 목록이 패키지의 것과 동일하다 |
//!
//! **보전 대상을 탐색하지 못하면 통과가 아니라 실패다.** 의도적으로 훼손한 입력으로
//! 실제 차단하는지 동일 파일에서 함께 판정한다.

use std::fs;
use std::path::{Path, PathBuf};

const ROOT_PACKAGE: &str = include_str!("../../../package.json");
const UI_PACKAGE: &str = include_str!("../../../packages/sheetsift-ui/package.json");

/// 패키지 원본의 위치. 저장소 뿌리 기준이다.
const SRC: &str = "packages/sheetsift-ui/src";

/// 색의 기본값을 기재하는 유일한 파일.
const TOKENS: &str = "tokens.css";

/// 이 앱의 경계. 공용 화면은 받는 앱이 주입한 문만 호출한다.
const APP_ONLY: &[&str] = &["sheetsift-book", "sheetsift-wasm"];

/// 사람이 판독하는 속성. 문자 그대로 기재하면 문구가 패키지에 적재된다.
const SPOKEN_ATTRS: &[&str] = &["aria-label", "title", "placeholder", "alt"];

/// 색을 지정하는 함수 표기.
const COLOR_FUNCS: &[&str] =
    &["rgb(", "rgba(", "hsl(", "hsla(", "hwb(", "lab(", "lch(", "oklab(", "oklch(", "color-mix("];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap()
}

fn collect(at: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(at) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, out);
        } else if path
            .extension()
            .is_some_and(|e| ["ts", "vue", "css"].contains(&&*e.to_string_lossy()))
        {
            out.push(path);
        }
    }
}

/// 패키지 원본 전부. (파일 명칭, 본문)
fn sources() -> Vec<(String, String)> {
    let mut paths = Vec::new();
    collect(&root().join(SRC), &mut paths);
    paths
        .into_iter()
        .map(|p| {
            let name = p.file_name().unwrap().to_string_lossy().into_owned();
            let text = fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
            (name, text)
        })
        .collect()
}

/* ── 판독 ────────────────────────────────────────────────────────── */

/// `open` 에서 `close` 까지를 소거한다. 닫히지 않으면 끝까지 소거한다.
fn strip_between(src: &str, open: &str, close: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let mut rest = src;
    while let Some(at) = rest.find(open) {
        out.push_str(&rest[..at]);
        match rest[at + open.len()..].find(close) {
            Some(end) => rest = &rest[at + open.len() + end + close.len()..],
            None => return out,
        }
    }
    out.push_str(rest);
    out
}

/// 주석을 소거한다 — 블록 주석, 템플릿 주석, 행 주석.
///
/// 행 주석은 `//` 이후 행 끝까지다. 문자열 안의 `//`(주소 따위)까지 소거하는데,
/// 그것은 판정 대상을 **축소하는** 방향이라 오판으로 정지시키지 않는다.
fn strip_comments(src: &str) -> String {
    let s = strip_between(src, "/*", "*/");
    let s = strip_between(&s, "<!--", "-->");
    s.lines()
        .map(|l| match l.find("//") {
            Some(at) => &l[..at],
            None => l,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn is_hangul(c: char) -> bool {
    matches!(c, '\u{AC00}'..='\u{D7A3}' | '\u{1100}'..='\u{11FF}' | '\u{3130}'..='\u{318F}')
}

/// 주석 외부의 한글이 출현하는 행.
fn hangul_lines(src: &str) -> Vec<String> {
    strip_comments(src)
        .lines()
        .filter(|l| l.chars().any(is_hangul))
        .map(|l| l.trim().to_string())
        .collect()
}

/// 사람이 판독할 속성을 문자 그대로 기재한 위치. `:aria-label="…"` 처럼 결속한
/// 것은 표현식이라 해당하지 않는다.
fn literal_attrs(src: &str) -> Vec<String> {
    let body = strip_comments(src);
    let mut found = Vec::new();
    for attr in SPOKEN_ATTRS {
        let needle = format!("{attr}=\"");
        for (at, _) in body.match_indices(&needle) {
            let before = body[..at].chars().next_back();
            // 선행 문자가 공백이어야 속성이다. `:`·`-`·`@` 이면 결속이거나 타 명칭의 일부다.
            if before.is_some_and(char::is_whitespace) {
                let tail: String = body[at..].chars().take(40).collect();
                found.push(tail);
            }
        }
    }
    found
}

/// `#rgb` 형태의 색 값. 16진 3·4·6·8 자리이고 그 직후가 식별자 문자가 아니다.
fn hex_colors(src: &str) -> Vec<String> {
    let b = src.as_bytes();
    let mut found = Vec::new();
    for (i, _) in src.match_indices('#') {
        let digits = b[i + 1..].iter().take_while(|c| c.is_ascii_hexdigit()).count();
        let next = b.get(i + 1 + digits);
        let ends = next.is_none_or(|c| !(c.is_ascii_alphanumeric() || *c == b'_' || *c == b'-'));
        if [3, 4, 6, 8].contains(&digits) && ends {
            found.push(src[i..i + 1 + digits].to_string());
        }
    }
    found
}

/// 색 값을 직접 기재한 위치 — 16진 표기와 색 함수.
///
/// `.ts` 에서는 16진 표기를 판정하지 않는다. 그 안의 `#` 은 사설 항목(`#inner`)이고,
/// 색을 기재하는 위치는 문자열이라 색 함수 쪽이 포착한다.
fn color_values(name: &str, src: &str) -> Vec<String> {
    let body = strip_comments(src);
    let mut found = Vec::new();
    if !name.ends_with(".ts") {
        found.extend(hex_colors(&body));
    }
    for f in COLOR_FUNCS {
        // `rgb(` 는 `rgba(` 가 아닌 위치에서도 출현한다 — 선행 문자가 식별자 문자면 타 명칭이다.
        for (at, _) in body.match_indices(f) {
            let before = body[..at].chars().next_back();
            if !before.is_some_and(|c| c.is_alphanumeric() || c == '-') {
                found.push((*f).to_string());
            }
        }
    }
    found
}

/// `@/` 로 반입하는 위치.
fn alias_imports(src: &str) -> Vec<String> {
    let body = strip_comments(src);
    body.lines()
        .filter(|l| l.contains("\"@/") || l.contains("'@/"))
        .map(|l| l.trim().to_string())
        .collect()
}

/// 이 앱의 경계를 반입하는 위치.
fn app_imports(src: &str) -> Vec<String> {
    let body = strip_comments(src);
    let mut found = Vec::new();
    for name in APP_ONLY {
        for q in ['"', '\''] {
            if body.contains(&format!("{q}{name}{q}")) || body.contains(&format!("{q}{name}/")) {
                found.push((*name).to_string());
            }
        }
    }
    found
}

/// `package.json` 의 한 항목.
fn json(src: &str) -> serde_json::Value {
    serde_json::from_str(src).expect("package.json 을 해석하지 못했습니다")
}

/// 반출 목록. (열쇠, 대상)
fn exports(pkg: &serde_json::Value) -> Vec<(String, String)> {
    pkg["exports"]
        .as_object()
        .map(|m| {
            m.iter().map(|(k, v)| (k.clone(), v.as_str().unwrap_or_default().to_string())).collect()
        })
        .unwrap_or_default()
}

/* ── 판정 ────────────────────────────────────────────────────────── */

#[test]
fn the_sources_are_there_to_look_at() {
    let files = sources();
    for must in ["DataGrid.vue", "grid.ts", "inject.ts", TOKENS, "grid.css"] {
        assert!(
            files.iter().any(|(n, _)| n == must),
            "`{SRC}` 에서 `{must}` 를 탐색하지 못했습니다 — 검사가 실명한 것입니다"
        );
    }
}

/// **문구를 보유하지 않는가.** 주석 외부에 한글이 부재하고, 사람이 판독할 속성을
/// 문자 그대로 기재하지 않는다.
#[test]
fn the_package_carries_no_words_of_its_own() {
    let mut found = Vec::new();
    for (name, text) in sources() {
        for l in hangul_lines(&text) {
            found.push(format!("{name}: {l}"));
        }
        if name.ends_with(".vue") {
            for a in literal_attrs(&text) {
                found.push(format!("{name}: {a}"));
            }
        }
    }
    assert!(
        found.is_empty(),
        "공용 화면이 문구를 직접 보유합니다 — 열쇠로 치환하고 받는 앱의 사전에 기재하십시오: {found:#?}"
    );
}

/// **색 값을 직접 기재하지 않는가.** 기본값은 `tokens.css` 한 곳이다.
#[test]
fn colours_come_only_from_the_tokens() {
    let mut found = Vec::new();
    for (name, text) in sources() {
        if name == TOKENS {
            continue;
        }
        for c in color_values(&name, &text) {
            found.push(format!("{name}: {c}"));
        }
    }
    assert!(found.is_empty(), "`{TOKENS}` 외부에 색 값이 기재되었습니다 — 토큰을 사용하십시오: {found:#?}");
}

/// **`@/` 로 반입하지 않는가.** 받는 쪽의 `@/` 는 받는 쪽의 `src` 를 지시한다.
#[test]
fn the_package_uses_no_alias() {
    let mut found = Vec::new();
    for (name, text) in sources() {
        for l in alias_imports(&text) {
            found.push(format!("{name}: {l}"));
        }
    }
    assert!(found.is_empty(), "공용 화면이 `@/` 로 반입합니다 — 상대 경로로 기재하십시오: {found:#?}");
}

/// **이 앱의 경계를 반입하지 않는가.** 자료의 문은 받는 앱이 주입한다.
#[test]
fn the_package_does_not_reach_into_this_app() {
    let mut found = Vec::new();
    for (name, text) in sources() {
        for n in app_imports(&text) {
            found.push(format!("{name}: {n}"));
        }
    }
    assert!(found.is_empty(), "공용 화면이 이 앱의 경계를 반입합니다: {found:#?}");
}

/// **뿌리의 반출 목록이 패키지의 것과 동일한가.**
///
/// git 의존으로 받는 쪽은 저장소 뿌리의 `package.json` 을 판독하고, 이 앱은 작업공간의
/// 패키지 쪽을 판독한다. 양자가 분기하면 이 앱에서는 반입되는 경로가 받는 쪽에서는
/// 부재한다 — 이 앱에서는 아무것도 발현되지 않는다.
#[test]
fn the_root_exports_what_the_package_exports() {
    let root = json(ROOT_PACKAGE);
    let ui = json(UI_PACKAGE);
    let ui_exports = exports(&ui);
    assert!(!ui_exports.is_empty(), "패키지의 반출 목록을 탐색하지 못했습니다 — 검사가 실명한 것입니다");

    let expected: Vec<(String, String)> = ui_exports
        .iter()
        .map(|(k, v)| (k.clone(), v.replacen("./", "./packages/sheetsift-ui/", 1)))
        .collect();
    assert_eq!(exports(&root), expected, "뿌리와 패키지의 반출 목록이 상이합니다");

    // 받는 쪽에 도달하는 파일. 원본이 누락되면 반출 목록이 공백을 지시한다.
    let files: Vec<&str> =
        root["files"].as_array().into_iter().flatten().filter_map(|v| v.as_str()).collect();
    assert!(files.contains(&"packages/sheetsift-ui/src"), "뿌리의 `files` 가 패키지 원본을 포함하지 않습니다: {files:?}");
}

/// **`vue` 를 동반하지 않는가.** 두 벌이 혼합되면 반응성이 파손된다.
#[test]
fn vue_is_the_receivers_own() {
    for (what, src) in [("package.json", ROOT_PACKAGE), ("packages/sheetsift-ui/package.json", UI_PACKAGE)] {
        let pkg = json(src);
        assert!(pkg["dependencies"]["vue"].is_null(), "`{what}` 이 vue 를 dependencies 로 동반합니다");
        assert!(pkg["peerDependencies"]["vue"].is_string(), "`{what}` 의 peerDependencies 에 vue 가 부재합니다");
    }
}

/* ── 그 판정들이 실제로 차단하는지 ──────────────────────────────── */

#[test]
fn the_check_catches_a_word_written_into_the_template() {
    let doctored = "<template>\n  <!-- 설명은 통과한다 -->\n  <p>비어 있음</p>\n</template>\n";
    assert_eq!(hangul_lines(doctored), ["<p>비어 있음</p>"]);
}

#[test]
fn the_check_lets_a_comment_speak() {
    let doctored = "// 한국어 주석\n/* 블록\n 주석 */\nconst x = 1;\n";
    assert!(hangul_lines(doctored).is_empty());
}

#[test]
fn the_check_catches_a_literal_label() {
    let doctored = "<button aria-label=\"Close\" :title=\"t('x')\" data-title=\"x\">";
    assert_eq!(literal_attrs(doctored).len(), 1);
}

#[test]
fn the_check_catches_a_colour_value() {
    assert_eq!(color_values("a.css", ".x { color: #fff; }"), ["#fff"]);
    assert_eq!(color_values("a.vue", "<style>.x { background: rgb(0 0 0); }</style>"), ["rgb("]);
    assert_eq!(color_values("a.ts", "el.style.color = 'oklch(0.5 0 0)';"), ["oklch("]);
}

#[test]
fn the_check_tells_a_colour_from_what_only_looks_like_one() {
    // 토큰 참조, 사설 항목, 색이 아닌 16진 길이, 주석 안의 색.
    let doctored = ".x { color: var(--c-fg); } /* #fff */ #abcde #pv-head";
    assert!(color_values("a.css", doctored).is_empty(), "{:?}", color_values("a.css", doctored));
    assert!(color_values("a.ts", "this.#inner.call(); const n = 0xfff;").is_empty());
}

#[test]
fn the_check_catches_an_alias() {
    assert_eq!(alias_imports("import { t } from \"@/i18n\";").len(), 1);
    assert!(alias_imports("// import { t } from \"@/i18n\";").is_empty());
}

#[test]
fn the_check_catches_this_apps_boundary() {
    assert_eq!(app_imports("import { limits } from \"sheetsift-book\";"), ["sheetsift-book"]);
}

#[test]
fn the_check_catches_exports_that_drifted() {
    let ui = json(r#"{ "exports": { ".": "./src/index.ts", "./lib/*": "./src/lib/*.ts" } }"#);
    let root = json(r#"{ "exports": { ".": "./packages/sheetsift-ui/src/index.ts" } }"#);
    let expected: Vec<(String, String)> =
        exports(&ui).iter().map(|(k, v)| (k.clone(), v.replacen("./", "./packages/sheetsift-ui/", 1))).collect();
    assert_ne!(exports(&root), expected);
}
