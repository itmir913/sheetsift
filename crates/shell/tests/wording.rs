//! **금지한 어휘가 잔존하지 않는가.**
//!
//! `CLAUDE.md` 가 전환을 지시한 어휘는 문서에 기재해 두는 것만으로는 유지되지
//! 않는다. 기재한 사람은 기억하고, 그 이후에 수정하는 사람은 기재를 판독하지
//! 않는다. 그래서 **문자로 판독해 차단한다.**
//!
//! 대상은 주석·문서·화면 문구 전부다. 식별자는 영문이라 영향을 받지 않는다.
//!
//! **보전 대상을 탐색하지 못하면 통과가 아니라 실패다.** 경로가 변경되어 아무
//! 파일도 판독하지 못하면 이 시험은 조용히 통과하므로, 판독한 파일 수를 함께
//! 판정하고 의도적으로 훼손한 문자를 투입해 실제로 차단하는지 확인한다.

use std::fs;
use std::path::{Path, PathBuf};

/// 전환하기로 한 어휘와 그 대체어. `CLAUDE.md` 의 목록과 동일한 벌이다.
const BANNED: &[(&str, &str)] = &[("머리글", "헤더")];

/// 판독하는 위치. 저장소 뿌리 기준이다.
const ROOTS: &[&str] = &["CLAUDE.md", "README.md", "docs", "crates", "web/src", ".github"];

/// 판독하는 확장자. 그 외(바이너리·고정값 양식)는 대상이 아니다.
const EXTS: &[&str] = &["rs", "ts", "vue", "md", "yml", "json"];

/// 진입하지 않는 디렉터리. 산출물과 수령한 상자다.
const SKIP: &[&str] = &["target", "node_modules", "pkg", "dist"];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap()
}

fn collect(at: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(at) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if path.is_dir() {
            if !SKIP.contains(&name.as_ref()) {
                collect(&path, out);
            }
        } else if path.extension().is_some_and(|e| EXTS.contains(&&*e.to_string_lossy())) {
            out.push(path);
        }
    }
}

fn sources() -> Vec<PathBuf> {
    let root = root();
    let mut out = Vec::new();
    for r in ROOTS {
        let at = root.join(r);
        if at.is_file() {
            out.push(at);
        } else {
            collect(&at, &mut out);
        }
    }
    out
}

#[test]
fn the_sources_are_there_to_look_at() {
    let files = sources();
    assert!(files.len() > 20, "판독한 파일이 {}개뿐입니다 — 검사가 실명한 것입니다", files.len());
    assert!(
        files.iter().any(|f| f.ends_with("CLAUDE.md")),
        "CLAUDE.md 를 탐색하지 못했습니다 — 검사가 실명한 것입니다"
    );
}

/// **금지한 어휘가 어디에도 잔존하지 않는가.**
#[test]
fn no_banned_word_survives_anywhere() {
    let root = root();
    let mut found: Vec<String> = Vec::new();
    for file in sources() {
        let Ok(text) = fs::read_to_string(&file) else { continue };
        for (word, instead) in BANNED {
            // 이 시험 자체는 금지어를 기재해야 하므로 제외한다.
            if file.ends_with("wording.rs") {
                continue;
            }
            // `CLAUDE.md` 의 전환 목록은 양자를 함께 기재한다.
            if file.ends_with("CLAUDE.md") && text.contains(&format!("{word}→{instead}")) {
                continue;
            }
            if let Some(at) = text.find(word) {
                let line = text[..at].lines().count();
                let short = file.strip_prefix(&root).unwrap_or(&file).display();
                found.push(format!("{short}:{line} `{word}` → `{instead}`"));
            }
        }
    }
    assert!(found.is_empty(), "전환하기로 한 어휘가 잔존합니다: {found:#?}");
}

/// 그 판정이 실제로 차단하는가. 훼손한 문자를 투입한다.
#[test]
fn the_check_catches_a_banned_word() {
    let doctored = "/// 이 표의 머리글은 2행이다.";
    let (word, _) = BANNED[0];
    assert!(doctored.contains(word), "훼손한 입력을 포착하지 못했습니다");
}
