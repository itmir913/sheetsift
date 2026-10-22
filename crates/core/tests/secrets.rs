//! **비밀이 잔존하는 경로를 차단한다 — 실행하는 시험이 판정할 수 없는 위치들.**
//!
//! 암호화 코드에서 실패한 위치는 매번 동일한 부류였다. 부정확한 값이 아니라
//! **아무 작용도 발생하지 않는 것**이다. 소거 행이 생략되고, 소거 코드가
//! 컴파일에서 누락되고, 사용 후 정리하는 행이 `?` 후방으로 이동한다. 셋 모두
//! 프로그램은 정상 동작하고, 결과도 정확하고, 나머지 시험은 통과한다. **잔존하는
//! 것은 힙 상의 평문 자료와 열쇠뿐이고, 그것은 어떠한 시험도 판정하지 못한다.**
//!
//! 그래서 글자로 판정한다. 여기서 판정하는 것은 전부 "존재해야 할 것이 소멸해도
//! 누구도 인지하지 못하는" 위치다.
//!
//! **탐색하지 못하면 통과가 아니라 실패다.** 보전 대상이 소멸했는데 통과하는
//! 것이 이 시험의 유일한 위험이다.
//!
//! **여기까지가 글자로 판정 가능한 한계다.** 복호화 위치는 호출 명칭
//! (`.decrypt(`·`_aead::open(`)과 그 상단 수 행이라는 거리에 의존한다 — 그 범위
//! 내부에서 타 값을 포장하고 평문은 순수 그릇으로 수용하는 코드는 이론상
//! 통과한다. 파서가 되지 않고는 차단 불가하므로 기재만 해 둔다. 실제 방어는
//! `Zeroizing` 이라는 타입이고, 이 시험은 그 타입이 제거되는 것을 포착하는
//! 장치다.
//!
//! 앱에 잔존하는 검사(비밀번호를 수용하는 명령에서 소거, 작업 트리의 개인키)는
//! 여기 부재한다. 그 위치는 이 크레이트에 부재하고, 부르는 앱이 판정한다.

use std::path::{Path, PathBuf};

/// 복호화 위치 상단으로 `Zeroizing` 을 탐색하며 소급하는 행 수.
const LOOKBACK: usize = 4;

fn crate_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn raw(path: &Path) -> String {
    std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("{} 을 판독하지 못했습니다: {e}", path.display()))
        .replace("\r\n", "\n")
}

/// **주석을 제거하고 판정한다.** 주석 내부의 문자를 코드로 산정하면 시험이
/// 개방된 채로 통과한다 — `// self.key.zeroize();` 로 차단해 두어도 "소거한다"
/// 고 판독한다. 행 수는 그대로 유지해 위치를 지시할 수 있게 한다. 러스트의 블록
/// 주석은 중첩 가능하므로 깊이를 산정한다.
fn strip_comments(src: &str) -> String {
    let bytes: Vec<char> = src.chars().collect();
    let mut out = String::with_capacity(src.len());
    let mut line = false;
    let mut depth = 0usize;
    let mut in_str = false;
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        let next = bytes.get(i + 1).copied().unwrap_or('\0');
        if c == '\n' {
            line = false;
            out.push(c);
            i += 1;
            continue;
        }
        let live = !line && depth == 0;
        if live && !in_str && c == '"' {
            in_str = true;
        } else if live && in_str && c == '"' && (i == 0 || bytes[i - 1] != '\\') {
            in_str = false;
        }
        if !in_str {
            if live && c == '/' && next == '/' {
                line = true;
            }
            if !line && c == '/' && next == '*' {
                depth += 1;
            }
            if !line && depth > 0 && c == '*' && next == '/' {
                depth -= 1;
                out.push_str("  ");
                i += 2;
                continue;
            }
        }
        out.push(if line || depth > 0 { ' ' } else { c });
        i += 1;
    }
    out
}

/// 반환 타입만 판정한다. 인자의 `&[u8; 32]` 를 반환 값으로 산정하지 않으려고
/// 최초 `-> ` 후단만 절단한다.
fn return_type(line: &str) -> Option<&str> {
    line.find("-> ").map(|at| &line[at + 3..])
}

/// 차단한 것을 집약해 일괄 통지한다. 최초 실패에서 정지하면 수정과 재실행을
/// 반복하게 된다.
#[derive(Default)]
struct Report {
    fail: Vec<String>,
    pass: Vec<String>,
}

impl Report {
    fn stop(&mut self, title: impl Into<String>, why: &[&str]) {
        let mut text = title.into();
        for line in why {
            text.push_str("\n    ");
            text.push_str(line);
        }
        self.fail.push(text);
    }

    /// 의도적으로 훼손한 소스를 투입했을 때 **무엇이 차단했는지** 판정하려고 배치한다.
    fn blocked(&self, needle: &str) -> bool {
        self.fail.iter().any(|f| f.contains(needle))
    }

    fn finish(self) {
        for line in &self.pass {
            println!("✓ {line}");
        }
        if self.fail.is_empty() {
            return;
        }
        let mut message = String::from("\n");
        for item in &self.fail {
            message.push_str("✗ ");
            message.push_str(item);
            message.push_str("\n\n");
        }
        panic!("{message}");
    }
}

/// 금고 소스와 상자 파일을 수용해 **글자로** 판정한다.
///
/// 파일을 판독하지 않고 수용만 하는 사유가 존재한다. 이 시험이 보전하려는 것은
/// "존재해야 할 것이 소멸해도 누구도 인지하지 못하는" 위치인데, 그렇다면 **이
/// 시험 자체가 그 위치를 누락하는 것**도 동일하게 묵시적이다. 수용해 판정하게
/// 배치하면 의도적으로 훼손한 소스를 투입해 중단하는지 판정할 수 있다. 실제
/// 소스로 훼손하려면 컴파일이 선행 중단되므로, 그 경로로는 확인할 수 없다.
fn audit(vault_rel: &str, vault_source: &str, manifests: &[(&str, String)]) -> Report {
    let source = strip_comments(vault_source);
    let lines: Vec<&str> = source.split('\n').collect();
    let mut r = Report::default();

    // ── 1. 복호화한 평문은 자동 소거되는 그릇에 수용한다 ────────────────────
    //
    // 복호화가 반환하는 것은 봉투에 수용된 자료 전부다. 수동 소거 행을 배치하면
    // 그 행 이전에서 `?` 가 복귀하는 경로가 반드시 발생한다. `Zeroizing` 은 어느
    // 경로로 이탈하든 소멸 시점에 소거한다.
    let sites: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, l)| l.contains(".decrypt(") || l.contains("_aead::open("))
        .map(|(i, _)| i)
        .collect();
    if sites.is_empty() {
        r.stop(
            format!("{vault_rel} 에서 복호화하는 위치를 탐색하지 못했습니다"),
            &["명칭이 변경되었다면 이 시험도 함께 수정하십시오 — 보전 대상이 소멸합니다."],
        );
    }
    for &i in &sites {
        let from = i.saturating_sub(LOOKBACK);
        let around = lines[from..=i].join("\n");
        if !around.contains("Zeroizing::new(") {
            r.stop(
                format!(
                    "{vault_rel}:{} 복호화한 평문이 자동 소거되는 그릇에 수용되지 않았습니다",
                    i + 1
                ),
                &[
                    "여기 수용된 것은 봉투에 수용된 자료 전부입니다. `zeroize::Zeroizing::new(...)` 로 포장하십시오.",
                    "수동으로 `zeroize()` 를 호출하는 것으로는 불충분합니다 — 그 행 이전에서 `?` 가",
                    "복귀하는 경로가 발생하면 묵시적으로 생략합니다. 실행하는 시험은 그것을 판정하지 못합니다.",
                ],
            );
        }
    }

    // 열쇠 사본도 동일하다. 순수 배열로 반환하면 이전될 때마다 스택에 사본이
    // 잔존해, 잠금 이후에도 거기 존재한다. **한 위치라도 순수 배열이면 중단한다** —
    // 나머지가 정확하다고 통과시키면 신규 발생한 그 한 위치가 묵시적으로 통과한다.
    let mut wrapped = 0usize;
    for (i, line) in lines.iter().enumerate() {
        let Some(ret) = return_type(line) else { continue };
        if !ret.contains("[u8; 32]") {
            continue;
        }
        if ret.contains("Zeroizing<[u8; 32]>") {
            wrapped += 1;
        } else {
            r.stop(
                format!("{vault_rel}:{} 열쇠를 순수 배열로 반환하고 있습니다", i + 1),
                &[
                    "`Result<zeroize::Zeroizing<[u8; 32]>>` 로 반환하십시오.",
                    "순수 `[u8; 32]` 는 이전될 때마다 스택에 사본을 잔존시키고, 잠금 이후에도 거기 존재합니다.",
                ],
            );
        }
    }
    if wrapped == 0 {
        r.stop(
            format!("{vault_rel} 에서 열쇠를 도출해 반환하는 위치를 탐색하지 못했습니다"),
            &["구조가 변경되었다면 이 시험도 함께 수정하십시오 — 보전 대상이 소멸합니다."],
        );
    }
    r.pass.push(format!(
        "복호화한 평문과 열쇠는 자동 소거되는 그릇에 수용됩니다 (복호화 {}곳 · 열쇠 {wrapped}곳)",
        sites.len()
    ));

    // ── 2. 소거 코드가 컴파일에 잔존한다 ────────────────────────────────────
    //
    // `aes-gcm` 과 `argon2` 내부의 소거 코드는 `zeroize` 기능을 활성화할 때만
    // 포함된다. 비활성화하면 **묵시적으로 누락되고**, 코드는 그대로 동작하고,
    // 열쇠만 메모리에 잔존한다.
    let mut seen = 0usize;
    for dep in ["aes-gcm", "argon2"] {
        let needle = format!("{dep} =");
        let found: Vec<(&str, &str)> = manifests
            .iter()
            .flat_map(|(rel, text)| {
                text.split('\n')
                    .map(str::trim)
                    .filter(|l| l.starts_with(&needle))
                    .map(move |l| (*rel, l))
            })
            .collect();
        if found.is_empty() {
            let where_ = manifests.iter().map(|(rel, _)| *rel).collect::<Vec<_>>().join(" · ");
            r.stop(
                format!("{where_} 어디에서도 {dep} 를 탐색하지 못했습니다"),
                &["크레이트를 교체했다면 이 시험도 함께 수정하십시오 — 보전 대상이 소멸합니다."],
            );
            continue;
        }
        seen += 1;
        for (rel, line) in found {
            let on = line
                .find("features")
                .map(|at| &line[at..])
                .is_some_and(|rest| rest.contains("\"zeroize\""));
            if !on {
                r.stop(
                    format!("{rel} 의 {dep} 에 zeroize 기능이 비활성 상태입니다"),
                    &[
                        "features 에 \"zeroize\" 를 배치하십시오.",
                        "비활성화하면 크레이트 내부의 소거 코드가 전체 누락됩니다. 빌드도 시험도 그대로 통과하고,",
                        "열쇠만 메모리에 잔존합니다.",
                    ],
                );
            }
        }
    }
    r.pass
        .push(format!("소거 코드가 컴파일에 포함됩니다 (크레이트 {seen}개)"));

    // ── 3. 열쇠는 소멸 시점에 소거하고, 출력 시점에 은폐한다 ────────────────
    //
    // `Drop` 을 제거하면 잠금 이후에도 열쇠가 잔존한다. `#[derive(Debug)]` 를
    // 부착하면 열쇠가 로그와 패닉 메시지로 유출된다. 양자 모두 묵시적이다.
    match lines.iter().position(|l| l.contains("Drop for Vault")) {
        None => r.stop(
            format!("{vault_rel} 에 `impl Drop for Vault` 가 부재합니다"),
            &["봉투가 소멸할 때 열쇠를 소거하는 위치입니다. 부재하면 잠금 이후에도 메모리에 잔존합니다."],
        ),
        Some(at) => {
            let end = lines[at..]
                .iter()
                .position(|l| *l == "}")
                .map(|o| at + o)
                .unwrap_or(lines.len() - 1);
            if !lines[at..=end].join("\n").contains("zeroize()") {
                r.stop(
                    format!("{vault_rel} 의 `Drop` 이 열쇠를 소거하지 않습니다"),
                    &["`self.key.zeroize();` 한 행이 존재해야 합니다."],
                );
            }
        }
    }
    if !lines.iter().any(|l| l.contains("std::fmt::Debug for Vault")) {
        r.stop(
            format!("{vault_rel} 에 수동 작성한 `Debug` 가 부재합니다"),
            &["파생시키면 열쇠와 솔트가 로그·패닉 메시지에 그대로 출력됩니다. 은폐하는 구현을 배치하십시오."],
        );
    }
    let struct_at = lines.iter().position(|l| l.contains("struct Vault"));
    match struct_at {
        None => r.stop(
            format!("{vault_rel} 에서 `struct Vault` 를 탐색하지 못했습니다"),
            &["명칭이 변경되었다면 이 시험도 함께 수정하십시오 — 보전 대상이 소멸합니다."],
        ),
        Some(at) => {
            // 파생 목록은 선언 직전 행에 부착된다. 복수 행으로 기재하기도 하므로
            // 속성이 아닌 행을 조우할 때까지 소급한다.
            let mut i = at;
            let mut derived = false;
            while i > 0 {
                let prev = lines[i - 1].trim();
                if prev.is_empty() || prev.starts_with("#[") || prev.starts_with(')') {
                    if prev.contains("Debug") {
                        derived = true;
                    }
                    i -= 1;
                    continue;
                }
                break;
            }
            if derived {
                r.stop(
                    format!("{vault_rel} 의 `Vault` 가 `Debug` 를 파생하고 있습니다"),
                    &["열쇠가 로그와 패닉 메시지로 유출됩니다. 수동 작성한 구현만 배치하십시오."],
                );
            }
        }
    }
    r.pass
        .push("열쇠는 소멸 시점에 소거되고 출력 시점에 은폐됩니다".to_string());

    r
}

#[test]
fn secrets_cannot_quietly_stop_being_wiped() {
    let dir = crate_dir();
    let manifests = vec![
        (
            "crates/core/Cargo.toml",
            strip_comments(&raw(&dir.join("Cargo.toml"))),
        ),
        (
            "crates/aead/Cargo.toml",
            strip_comments(&raw(&dir.join("../aead/Cargo.toml"))),
        ),
    ];
    audit(
        "crates/core/src/vault.rs",
        &raw(&dir.join("src/vault.rs")),
        &manifests,
    )
    .finish();
}

/* ── 이 시험이 실제로 차단하는지 판정한다 ──────────────────────────────────
   상기 검사가 **아무것도 판정하지 않게 되는 것**이 최대 위험이다. 글자를
   탐색하는 방식이라, 탐색 형태를 부정확하게 기재해도 소스가 정상이면 통과한다.
   그래서 의도적으로 훼손한 소스를 투입해 중단하는지 판정한다. 실제 소스를
   수정해서는 확인할 수 없다 — 컴파일이 선행 중단되기 때문이다. */

/// 전 검사를 통과하는 최소 소스. 하기 시험들은 여기서 한 지점씩만 훼손해,
/// 차단이 **그 한 지점을 사유로** 발생했음을 명확히 한다.
const SOUND_VAULT: &str = r#"
fn derive_key(password: &str, salt: &[u8]) -> Result<zeroize::Zeroizing<[u8; 32]>> {
    let mut key = zeroize::Zeroizing::new([0u8; 32]);
    Ok(key)
}
pub struct Vault<F: Format> {
    key: [u8; 32],
}
impl<F: Format> Drop for Vault<F> {
    fn drop(&mut self) {
        self.key.zeroize();
    }
}
impl<F: Format> std::fmt::Debug for Vault<F> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Vault").field("key", &"<redacted>").finish()
    }
}
fn open(&self) -> Result<()> {
    let plain = zeroize::Zeroizing::new(
        sheetsift_aead::open(&key, &nonce, header, ciphertext)?,
    );
    Ok(())
}
"#;

fn sound_manifests() -> Vec<(&'static str, String)> {
    vec![
        (
            "core",
            "argon2 = { version = \"0.6\", features = [\"zeroize\"] }\n".to_string(),
        ),
        (
            "aead",
            "aes-gcm = { version = \"0.11\", features = [\"zeroize\"] }\n".to_string(),
        ),
    ]
}

fn audit_vault(source: &str) -> Report {
    audit("vault.rs", source, &sound_manifests())
}

#[test]
fn a_sound_source_passes() {
    let r = audit_vault(SOUND_VAULT);
    assert!(
        r.fail.is_empty(),
        "온전한 소스를 차단했습니다. 하기 시험들이 무엇을 사유로 차단되었는지도 신뢰할 수 없게 됩니다:\n{:#?}",
        r.fail
    );
}

#[test]
fn bare_plaintext_is_blocked() {
    let broken = SOUND_VAULT.replace("let plain = zeroize::Zeroizing::new(", "let plain = (");
    assert!(
        audit_vault(&broken).blocked("복호화한 평문이 자동 소거되는 그릇에 수용되지 않았습니다"),
        "평문을 순수 그릇으로 수용하는 것을 차단하지 못했습니다"
    );
}

#[test]
fn a_bare_key_return_is_blocked() {
    let broken = SOUND_VAULT.replace("Result<zeroize::Zeroizing<[u8; 32]>>", "Result<[u8; 32]>");
    assert!(
        audit_vault(&broken).blocked("열쇠를 순수 배열로 반환하고 있습니다"),
        "열쇠를 순수 배열로 반환하는 것을 차단하지 못했습니다"
    );
}

#[test]
fn a_vanished_decrypt_site_is_blocked() {
    let broken = SOUND_VAULT.replace("sheetsift_aead::open(", "sheetsift_aead::unseal(");
    assert!(
        audit_vault(&broken).blocked("복호화하는 위치를 탐색하지 못했습니다"),
        "보전 대상이 소멸한 것을 차단하지 못했습니다 — 최대 위험 요소입니다"
    );
}

#[test]
fn a_vanished_key_recipe_is_blocked() {
    let broken = SOUND_VAULT.replace("[u8; 32]", "[u8; 64]");
    assert!(
        audit_vault(&broken).blocked("열쇠를 도출해 반환하는 위치를 탐색하지 못했습니다"),
        "열쇠를 반환하는 위치가 소멸한 것을 차단하지 못했습니다"
    );
}

#[test]
fn an_emptied_drop_is_blocked() {
    let broken = SOUND_VAULT.replace("        self.key.zeroize();\n", "");
    assert!(
        audit_vault(&broken).blocked("`Drop` 이 열쇠를 소거하지 않습니다"),
        "열쇠를 소거하지 않는 `Drop` 을 차단하지 못했습니다"
    );
}

#[test]
fn a_missing_drop_is_blocked() {
    let broken = SOUND_VAULT.replace("Drop for Vault<F>", "Clone for Vault<F>");
    assert!(
        audit_vault(&broken).blocked("`impl Drop for Vault` 가 부재합니다"),
        "`Drop` 이 전무한 것을 차단하지 못했습니다"
    );
}

#[test]
fn a_derived_debug_is_blocked() {
    let broken = SOUND_VAULT.replace("pub struct Vault<F: Format> {", "#[derive(Debug)]\npub struct Vault<F: Format> {");
    let r = audit_vault(&broken);
    assert!(
        r.blocked("`Debug` 를 파생하고 있습니다"),
        "열쇠를 출력하는 `Debug` 파생을 차단하지 못했습니다"
    );
}

#[test]
fn a_missing_manual_debug_is_blocked() {
    let broken = SOUND_VAULT.replace("std::fmt::Debug for Vault<F>", "std::fmt::Display for Vault<F>");
    assert!(
        audit_vault(&broken).blocked("수동 작성한 `Debug` 가 부재합니다"),
        "은폐하는 `Debug` 가 부재한 것을 차단하지 못했습니다"
    );
}

#[test]
fn a_vanished_struct_is_blocked() {
    let broken = SOUND_VAULT.replace("struct Vault<F: Format>", "struct Envelope<F: Format>");
    assert!(
        audit_vault(&broken).blocked("`struct Vault` 를 탐색하지 못했습니다"),
        "금고 자체가 소멸한 것을 차단하지 못했습니다"
    );
}

#[test]
fn a_disabled_eraser_is_blocked() {
    for dep in ["argon2", "aes-gcm"] {
        let manifests: Vec<(&str, String)> = sound_manifests()
            .into_iter()
            .map(|(rel, text)| {
                if text.starts_with(dep) {
                    (rel, format!("{dep} = \"0\"\n"))
                } else {
                    (rel, text)
                }
            })
            .collect();
        assert!(
            audit("vault.rs", SOUND_VAULT, &manifests)
                .blocked(&format!("{dep} 에 zeroize 기능이 비활성 상태입니다")),
            "{dep} 의 소거 기능이 비활성화된 것을 차단하지 못했습니다"
        );
    }
}

#[test]
fn a_vanished_crate_is_blocked() {
    for dep in ["argon2", "aes-gcm"] {
        let manifests: Vec<(&str, String)> = sound_manifests()
            .into_iter()
            .filter(|(_, text)| !text.starts_with(dep))
            .collect();
        assert!(
            audit("vault.rs", SOUND_VAULT, &manifests)
                .blocked(&format!("어디에서도 {dep} 를 탐색하지 못했습니다")),
            "{dep} 가 소멸한 것을 차단하지 못했습니다"
        );
    }
}

/// 주석 내부의 문자를 코드로 산정하면 시험이 **개방된 채로 통과한다.** 소거
/// 행을 주석으로 차단해 두어도 "소거한다" 고 판독한다.
#[test]
fn a_commented_out_wipe_does_not_count() {
    let broken = SOUND_VAULT.replace("        self.key.zeroize();", "        // self.key.zeroize();");
    assert!(
        audit_vault(&broken).blocked("`Drop` 이 열쇠를 소거하지 않습니다"),
        "주석으로 차단한 소거를 소거한 것으로 산정했습니다"
    );
    let block = SOUND_VAULT.replace("        self.key.zeroize();", "        /* self.key.zeroize(); */");
    assert!(
        audit_vault(&block).blocked("`Drop` 이 열쇠를 소거하지 않습니다"),
        "블록 주석으로 차단한 소거를 소거한 것으로 산정했습니다"
    );
}
