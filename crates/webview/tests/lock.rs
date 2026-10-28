//! **tauri 와 이 상자가 동일한 판의 webview 결속을 사용하는가.**
//!
//! `harden` 은 tauri 가 반환한 컨트롤러를 이 상자가 기재한 `webview2-com` 의 타입으로
//! 변환한다. 판이 상이하면 동일 명칭의 상이한 타입이 된다. 잠금 파일에 동일 상자의
//! 판이 둘 존재하는 것이 그 징후라, **글자로 판독해 정지한다.**
//!
//! 잠금 파일은 운영체제와 무관하게 윈도 전용 의존까지 수록하므로, 이 시험은 어느
//! 러너에서도 동일하게 판정한다.
//!
//! **탐색하지 못하면 통과가 아니라 실패다.** 의도적으로 훼손한 입력으로 실제
//! 차단하는지도 함께 판정한다.

const LOCK: &str = include_str!("../Cargo.lock");

/// 판이 하나여야 하는 상자들.
const SINGLE: &[&str] = &["webview2-com", "windows"];

/// 잠금 파일에서 명칭이 `name` 인 패키지의 판들.
fn versions(lock: &str, name: &str) -> Vec<String> {
    let want = format!("name = \"{name}\"");
    let lines: Vec<&str> = lock.lines().map(str::trim).collect();
    let mut out = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if *line != want {
            continue;
        }
        if let Some(v) = lines.get(i + 1).and_then(|l| l.strip_prefix("version = \"")) {
            out.push(v.trim_end_matches('"').to_string());
        }
    }
    out
}

#[test]
fn tauri_and_this_crate_share_one_webview_binding() {
    assert!(
        !versions(LOCK, "tauri").is_empty(),
        "잠금 파일에서 tauri 를 탐색하지 못했습니다 — 검사가 실명한 것입니다"
    );
    for name in SINGLE {
        let found = versions(LOCK, name);
        assert_eq!(
            found.len(),
            1,
            "잠금 파일의 {name} 판이 하나가 아닙니다: {found:?}\n\
             tauri 의 판을 상승시켰다면 Cargo.toml 의 {name} 판도 추종시키십시오."
        );
    }
}

#[test]
fn the_check_catches_two_versions() {
    let doctored = "[[package]]\nname = \"windows\"\nversion = \"0.61.3\"\n\n\
                    [[package]]\nname = \"windows\"\nversion = \"0.62.2\"\n";
    assert_eq!(versions(doctored, "windows"), ["0.61.3", "0.62.2"]);
    assert!(versions(doctored, "webview2-com").is_empty());
}
