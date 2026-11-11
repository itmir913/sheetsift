//! **낡은 wasm 으로 화면을 기동하는 경로를 차단해 두었는가.**
//!
//! `web/pkg` 는 `npm run pkg` 가 산출한다. Rust 를 수정한 뒤 그것을 실행하지
//! 않고 vite 를 직접 기동하면 화면이 **구 엔진**과 대화하는데, 어디에서도 오류가
//! 발생하지 않는다. 실측으로 19분 낡은 산출물을 두고 「값 목록이 범위를 반영하지
//! 않는다」는 결함이 보고되었고, 재컴파일하니 정상이었다.
//!
//! 그 자리를 `web/stalePkg.ts` 가 **정지시킨다.** 이 시험이 판정하는 것은 그것이
//! 제자리에 장착되어 있는가다.
//!
//! **이 검사는 글자를 판독한다.** vite 의 설정은 `cargo test` 가 실행할 수 있는
//! 것이 아니다. 그래서 **보전 대상을 탐색하지 못하면 통과가 아니라 실패고**,
//! 의도적으로 훼손한 입력으로 실제 차단하는지 함께 판정한다.
//!
//! 그리고 열거 방향이 한 가지 더 있다. `stalePkg.ts` 는 wasm 묶음에 유입되지
//! **않는** 상자를 열거한다(`OUTSIDE`) — 유입되는 쪽을 열거하면 상자가 하나
//! 증가할 때 묵시적으로 산정에서 누락되고, 그 상자를 수정한 사람은 아무 경고
//! 없이 구 엔진을 본다. 그 목록이 실제 의존 관계와 합치하는지를 여기서 판정한다.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

/// 수용하는 것으로 **그 파일이 소멸하면 관문이 정지한다.**
const VITE: &str = include_str!("../../../web/vite.config.ts");
const STALE: &str = include_str!("../../../web/stalePkg.ts");

/// 관문을 장착하는 행. `vite.config.ts` 에 소재해야 한다.
const MOUNT: &str = "freshPkg(";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().expect("저장소의 뿌리")
}

/* ── 판정들. 훼손한 입력으로 자체를 재판정할 수 있게 함수로 분리한다 ── */

/// 관문이 `plugins` 에 장착되어 있는가.
fn mounted(vite: &str) -> bool {
    let Some(at) = vite.find("plugins:") else { return false };
    let line_end = vite[at..].find('\n').map_or(vite.len(), |n| at + n);
    vite[at..line_end].contains(MOUNT)
}

/// 낡았을 때 **정지하는가.** 기록만 하면 기동 로그에 섞여 판독되지 않는다.
fn stops(stale: &str) -> bool {
    let Some(at) = stale.find("buildStart()") else { return false };
    let body_end = stale[at..].find("\n    },").map_or(stale.len(), |n| at + n);
    let body = &stale[at..body_end];
    body.contains("throw new Error") && !body.contains("console.")
}

/// `stalePkg.ts` 가 열거한, wasm 묶음 외부의 상자들.
fn outside(stale: &str) -> BTreeSet<String> {
    let at = stale.find("export const OUTSIDE = [").expect("OUTSIDE 를 탐색하지 못했습니다");
    let list = &stale[at..][stale[at..].find('[').unwrap() + 1..];
    let list = &list[..list.find(']').expect("목록이 폐쇄되지 않았습니다")];
    list.split(',')
        .map(|s| s.trim().trim_matches('"').to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// `crates/` 의 상자 전부.
fn all_crates(root: &Path) -> BTreeSet<String> {
    fs::read_dir(root.join("crates"))
        .expect("crates 를 판독하지 못했습니다")
        .map(|e| e.expect("항목"))
        .filter(|e| e.path().join("Cargo.toml").is_file())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect()
}

/// `crates/wasm` 에서 `path` 의존성을 따라 도달하는 상자 전부(자기 자신 포함).
///
/// **의존 관계를 글자로 열거하지 않고 추적한다.** 열거하면 의존성이 하나
/// 증가할 때 이 시험까지 함께 수정해야 하고, 수정하지 않으면 통과한다.
fn reachable(root: &Path) -> BTreeSet<String> {
    let mut seen = BTreeSet::new();
    let mut todo = vec!["wasm".to_string()];
    while let Some(dir) = todo.pop() {
        if !seen.insert(dir.clone()) {
            continue;
        }
        let toml = fs::read_to_string(root.join("crates").join(&dir).join("Cargo.toml"))
            .unwrap_or_else(|e| panic!("{dir} 의 Cargo.toml 을 판독하지 못했습니다: {e}"));
        for line in toml.lines() {
            // `sheetsift-core = { path = "../core" }` 꼴에서 상자의 디렉터리를 취한다.
            let Some(at) = line.find("path = \"../") else { continue };
            let rest = &line[at + "path = \"../".len()..];
            let name = &rest[..rest.find('"').expect("경로가 폐쇄되지 않았습니다")];
            todo.push(name.trim_end_matches('/').to_string());
        }
    }
    seen
}

/* ── 시험 ─────────────────────────────────────────────────────── */

#[test]
fn the_guard_is_mounted_on_vite() {
    assert!(mounted(VITE), "vite.config.ts 의 plugins 에 {MOUNT} 가 부재합니다");
}

#[test]
fn the_guard_stops_instead_of_logging() {
    assert!(stops(STALE), "낡은 산출물을 정지가 아니라 기록으로 처리하고 있습니다");
}

/// `OUTSIDE` 가 실제로 **유입되지 않는 상자 전부**인가. 상자가 증감하면 정지한다.
#[test]
fn the_crates_outside_the_bundle_are_listed_exactly() {
    let root = root();
    let all = all_crates(&root);
    let inside = reachable(&root);
    let expected: BTreeSet<String> = all.difference(&inside).cloned().collect();
    assert_eq!(
        outside(STALE),
        expected,
        "wasm 묶음에 유입되지 않는 상자가 변동했습니다. \
         stalePkg.ts 의 OUTSIDE 를 맞추십시오 (유입되는 것: {inside:?})"
    );
    assert!(inside.contains("core") && inside.contains("shell"), "{inside:?}");
}

/// 화면이 적재하는 산출물의 명칭과, 관문이 시각을 재는 산출물의 명칭이 **동일한
/// 쌍**인가. 갈리면 관문이 적재되지 않는 파일의 시각을 재게 된다.
#[test]
fn the_guard_measures_what_the_screen_loads() {
    let at = VITE.find("./pkg/").expect("vite 의 wasm 별칭을 탐색하지 못했습니다");
    let rest = &VITE[at + "./pkg/".len()..];
    let loaded = &rest[..rest.find('"').expect("별칭이 폐쇄되지 않았습니다")];
    assert!(STALE.contains(loaded), "관문이 {loaded} 의 시각을 재지 않습니다");
    let pair = loaded.replace(".js", "_bg.wasm");
    assert!(STALE.contains(&pair), "관문이 {pair} 의 시각을 재지 않습니다");
}

/// **이 검사 자체가 실명하지 않는가.** 훼손한 입력으로 판정이 실제 차단하는지를
/// 확인한다 — 글자를 판독하는 검사는 보전 대상을 탐색하지 못해도 통과할 수 있다.
#[test]
fn a_damaged_source_is_caught() {
    assert!(!mounted("  plugins: [vue(), tailwindcss()],"), "관문이 부재한데 통과했습니다");
    assert!(!mounted(&VITE.replace(MOUNT, "noop(")), "개칭된 관문을 장착으로 판정했습니다");
    assert!(
        !stops("    buildStart() {\n      if (stalePkg(root)) console.warn(\"stale\");\n    },"),
        "기록만 하는데 정지로 판정했습니다"
    );
    assert!(!stops("const x = 1;"), "관문의 본문이 부재한데 통과했습니다");
    assert!(
        outside(&STALE.replace("\"desk\", ", "")) != outside(STALE),
        "목록이 변동했는데 동일하게 판독했습니다"
    );
}
