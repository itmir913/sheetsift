//! **비밀이 남는 길을 막는다 — 돌려 보는 시험이 볼 수 없는 자리들.**
//!
//! 암호화 코드에서 넘어진 자리는 매번 같은 부류였다. 잘못된 값이 아니라
//! **아무 일도 일어나지 않는 것**이다. 지우는 줄이 건너뛰어지고, 지우는 코드가
//! 컴파일에서 빠지고, 쓰고 나서 치우는 줄이 `?` 뒤로 밀린다. 셋 다 프로그램은
//! 멀쩡히 돌고, 결과도 맞고, 나머지 시험은 초록이다. **남는 것은 힙 위의 평문
//! 자료와 열쇠뿐이고, 그건 어떤 시험도 보지 못한다.**
//!
//! 그래서 글자로 본다. 여기 보는 것은 전부 "있어야 할 것이 없어져도 아무도
//! 모르는" 자리다.
//!
//! **찾지 못하면 통과가 아니라 실패다.** 지키려는 대상이 사라졌는데 초록불이
//! 뜨는 것이 이 시험의 유일한 위험이다.
//!
//! **여기까지가 글자로 볼 수 있는 한계다.** 복호화 자리는 부르는 이름
//! (`.decrypt(`·`_aead::open(`)과 그 위 몇 줄이라는 거리에 기댄다 — 그 안에서
//! 다른 값을 감싸고 평문은 맨 그릇으로 받는 코드는 이론상 지나간다. 파서가
//! 되지 않고는 못 막으므로 적어만 둔다. 진짜 방어는 `Zeroizing` 이라는 타입이고,
//! 이 시험은 그 타입이 벗겨지는 것을 알아채는 장치다.
//!
//! 앱에 남는 검사(비밀번호를 받는 명령에서 지우기, 작업 트리의 개인키)는 여기
//! 없다. 그 자리는 이 크레이트에 없고, 부르는 앱이 본다.

use std::path::{Path, PathBuf};

/// 복호화 자리 위로 `Zeroizing` 을 찾아 올라가는 줄 수.
const LOOKBACK: usize = 4;

fn crate_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn raw(path: &Path) -> String {
    std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("{} 을 읽지 못했습니다: {e}", path.display()))
        .replace("\r\n", "\n")
}

/// **주석을 지우고 본다.** 주석 안의 글자를 코드로 세면 시험이 열린 채로
/// 통과한다 — `// self.key.zeroize();` 로 막아 놓아도 "지운다" 고 읽는다.
/// 줄 수는 그대로 두어 자리를 짚을 수 있게 한다. 러스트의 덩이 주석은 겹칠 수
/// 있으므로 깊이를 센다.
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

/// 돌려주는 타입만 본다. 인자의 `&[u8; 32]` 를 돌려주는 값으로 세지 않으려고
/// 첫 `-> ` 뒤만 자른다.
fn return_type(line: &str) -> Option<&str> {
    line.find("-> ").map(|at| &line[at + 3..])
}

/// 막은 것을 모아 한꺼번에 알려 준다. 첫 실패에서 멈추면 고치고 다시 돌리기를
/// 되풀이하게 된다.
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

    /// 일부러 어긋낸 소스를 먹였을 때 **무엇이 막았는지** 보려고 둔다.
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

/// 금고 소스와 상자 파일을 받아 **글자로** 본다.
///
/// 파일을 읽지 않고 받기만 하는 까닭이 있다. 이 시험이 지키려는 것은 "있어야
/// 할 것이 없어져도 아무도 모르는" 자리인데, 그렇다면 **이 시험 자신이 그
/// 자리를 놓치는 것**도 똑같이 조용하다. 받아서 보게 해 두면 일부러 어긋낸
/// 소스를 먹여 서는지 볼 수 있다. 진짜 소스로 어긋내 보려면 컴파일이 먼저
/// 서므로, 그 길로는 확인할 수 없다.
fn audit(vault_rel: &str, vault_source: &str, manifests: &[(&str, String)]) -> Report {
    let source = strip_comments(vault_source);
    let lines: Vec<&str> = source.split('\n').collect();
    let mut r = Report::default();

    // ── 1. 복호화한 평문은 스스로 지워지는 그릇에 담는다 ────────────────────
    //
    // 복호화가 돌려주는 것은 봉투에 든 자료 전부다. 손으로 지우는 줄을 두면
    // 그 줄 앞에서 `?` 가 도는 길이 반드시 생긴다. `Zeroizing` 은 어느 길로
    // 나가든 떨어질 때 지운다.
    let sites: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, l)| l.contains(".decrypt(") || l.contains("_aead::open("))
        .map(|(i, _)| i)
        .collect();
    if sites.is_empty() {
        r.stop(
            format!("{vault_rel} 에서 복호화하는 자리를 찾지 못했습니다"),
            &["이름이 바뀌었다면 이 시험도 함께 고치세요 — 지키는 대상이 사라집니다."],
        );
    }
    for &i in &sites {
        let from = i.saturating_sub(LOOKBACK);
        let around = lines[from..=i].join("\n");
        if !around.contains("Zeroizing::new(") {
            r.stop(
                format!(
                    "{vault_rel}:{} 복호화한 평문이 스스로 지워지는 그릇에 담기지 않았습니다",
                    i + 1
                ),
                &[
                    "여기 든 것은 봉투에 담긴 자료 전부입니다. `zeroize::Zeroizing::new(...)` 로 감싸세요.",
                    "손으로 `zeroize()` 를 부르는 것으로는 모자랍니다 — 그 줄 앞에서 `?` 가 도는",
                    "길이 생기면 아무 말 없이 건너뜁니다. 돌려 보는 시험은 그것을 보지 못합니다.",
                ],
            );
        }
    }

    // 열쇠 사본도 마찬가지다. 맨 배열로 돌려주면 옮겨질 때마다 스택에 사본이
    // 남아, 잠근 뒤에도 거기 있다. **한 자리라도 맨 배열이면 선다** — 나머지가
    // 옳다고 통과시키면 새로 생긴 그 한 자리가 조용히 지나간다.
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
                format!("{vault_rel}:{} 열쇠를 맨 배열로 돌려주고 있습니다", i + 1),
                &[
                    "`Result<zeroize::Zeroizing<[u8; 32]>>` 로 돌려주세요.",
                    "맨 `[u8; 32]` 는 옮겨질 때마다 스택에 사본을 남기고, 잠근 뒤에도 거기 있습니다.",
                ],
            );
        }
    }
    if wrapped == 0 {
        r.stop(
            format!("{vault_rel} 에서 열쇠를 뽑아 돌려주는 자리를 찾지 못했습니다"),
            &["모양이 바뀌었다면 이 시험도 함께 고치세요 — 지키는 대상이 사라집니다."],
        );
    }
    r.pass.push(format!(
        "복호화한 평문과 열쇠는 스스로 지워지는 그릇에 담깁니다 (복호화 {}곳 · 열쇠 {wrapped}곳)",
        sites.len()
    ));

    // ── 2. 지우개가 컴파일에 남아 있다 ──────────────────────────────────────
    //
    // `aes-gcm` 과 `argon2` 안의 지우는 코드는 `zeroize` 기능을 켤 때만 들어간다.
    // 끄면 **아무 말 없이 빠지고**, 코드는 그대로 돌고, 열쇠만 메모리에 남는다.
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
                format!("{where_} 어디서도 {dep} 를 찾지 못했습니다"),
                &["크레이트를 갈았다면 이 시험도 함께 고치세요 — 지키는 대상이 사라집니다."],
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
                    format!("{rel} 의 {dep} 에 zeroize 기능이 꺼져 있습니다"),
                    &[
                        "features 에 \"zeroize\" 를 두세요.",
                        "끄면 크레이트 안의 지우는 코드가 통째로 빠집니다. 빌드도 시험도 그대로 통과하고,",
                        "열쇠만 메모리에 남습니다.",
                    ],
                );
            }
        }
    }
    r.pass
        .push(format!("지우는 코드가 컴파일에 들어갑니다 (크레이트 {seen}개)"));

    // ── 3. 열쇠는 떨어질 때 지우고, 찍힐 때 가린다 ──────────────────────────
    //
    // `Drop` 을 지우면 잠근 뒤에도 열쇠가 남는다. `#[derive(Debug)]` 를 붙이면
    // 열쇠가 로그와 패닉 메시지로 나간다. 둘 다 조용하다.
    match lines.iter().position(|l| l.contains("Drop for Vault")) {
        None => r.stop(
            format!("{vault_rel} 에 `impl Drop for Vault` 가 없습니다"),
            &["봉투가 떨어질 때 열쇠를 지우는 자리입니다. 없으면 잠근 뒤에도 메모리에 남습니다."],
        ),
        Some(at) => {
            let end = lines[at..]
                .iter()
                .position(|l| *l == "}")
                .map(|o| at + o)
                .unwrap_or(lines.len() - 1);
            if !lines[at..=end].join("\n").contains("zeroize()") {
                r.stop(
                    format!("{vault_rel} 의 `Drop` 이 열쇠를 지우지 않습니다"),
                    &["`self.key.zeroize();` 한 줄이 있어야 합니다."],
                );
            }
        }
    }
    if !lines.iter().any(|l| l.contains("std::fmt::Debug for Vault")) {
        r.stop(
            format!("{vault_rel} 에 손으로 쓴 `Debug` 가 없습니다"),
            &["파생시키면 열쇠와 솔트가 로그·패닉 메시지에 그대로 찍힙니다. 가리는 구현을 두세요."],
        );
    }
    let struct_at = lines.iter().position(|l| l.contains("struct Vault"));
    match struct_at {
        None => r.stop(
            format!("{vault_rel} 에서 `struct Vault` 를 찾지 못했습니다"),
            &["이름이 바뀌었다면 이 시험도 함께 고치세요 — 지키는 대상이 사라집니다."],
        ),
        Some(at) => {
            // 파생 목록은 선언 바로 앞줄에 붙는다. 여러 줄로 쓰기도 하므로
            // 속성이 아닌 줄을 만날 때까지 거슬러 올라간다.
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
                    &["열쇠가 로그와 패닉 메시지로 나갑니다. 손으로 쓴 구현만 두세요."],
                );
            }
        }
    }
    r.pass
        .push("열쇠는 떨어질 때 지워지고 찍힐 때 가려집니다".to_string());

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

/* ── 이 시험이 정말 막는지 본다 ──────────────────────────────────────────────
   위의 검사가 **아무것도 보지 않게 되는 것**이 가장 위험하다. 글자를 찾는
   방식이라, 찾는 모양을 잘못 적어도 소스가 멀쩡하면 초록불이 뜬다. 그래서
   일부러 어긋낸 소스를 먹여 서는지 본다. 진짜 소스를 고쳐서는 확인할 수
   없다 — 컴파일이 먼저 서기 때문이다. */

/// 모든 검사를 통과하는 가장 작은 소스. 아래 시험들은 여기서 한 군데씩만
/// 어긋내어, 막은 것이 **그 한 군데 때문**임을 분명히 한다.
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
        "온전한 소스를 막았습니다. 아래 시험들이 무엇 때문에 막혔는지도 믿을 수 없게 됩니다:\n{:#?}",
        r.fail
    );
}

#[test]
fn bare_plaintext_is_blocked() {
    let broken = SOUND_VAULT.replace("let plain = zeroize::Zeroizing::new(", "let plain = (");
    assert!(
        audit_vault(&broken).blocked("복호화한 평문이 스스로 지워지는 그릇에 담기지 않았습니다"),
        "평문을 맨 그릇으로 받는 것을 막지 못했습니다"
    );
}

#[test]
fn a_bare_key_return_is_blocked() {
    let broken = SOUND_VAULT.replace("Result<zeroize::Zeroizing<[u8; 32]>>", "Result<[u8; 32]>");
    assert!(
        audit_vault(&broken).blocked("열쇠를 맨 배열로 돌려주고 있습니다"),
        "열쇠를 맨 배열로 돌려주는 것을 막지 못했습니다"
    );
}

#[test]
fn a_vanished_decrypt_site_is_blocked() {
    let broken = SOUND_VAULT.replace("sheetsift_aead::open(", "sheetsift_aead::unseal(");
    assert!(
        audit_vault(&broken).blocked("복호화하는 자리를 찾지 못했습니다"),
        "지키는 대상이 사라진 것을 막지 못했습니다 — 가장 위험한 쪽입니다"
    );
}

#[test]
fn a_vanished_key_recipe_is_blocked() {
    let broken = SOUND_VAULT.replace("[u8; 32]", "[u8; 64]");
    assert!(
        audit_vault(&broken).blocked("열쇠를 뽑아 돌려주는 자리를 찾지 못했습니다"),
        "열쇠를 돌려주는 자리가 사라진 것을 막지 못했습니다"
    );
}

#[test]
fn an_emptied_drop_is_blocked() {
    let broken = SOUND_VAULT.replace("        self.key.zeroize();\n", "");
    assert!(
        audit_vault(&broken).blocked("`Drop` 이 열쇠를 지우지 않습니다"),
        "열쇠를 지우지 않는 `Drop` 을 막지 못했습니다"
    );
}

#[test]
fn a_missing_drop_is_blocked() {
    let broken = SOUND_VAULT.replace("Drop for Vault<F>", "Clone for Vault<F>");
    assert!(
        audit_vault(&broken).blocked("`impl Drop for Vault` 가 없습니다"),
        "`Drop` 이 아예 없는 것을 막지 못했습니다"
    );
}

#[test]
fn a_derived_debug_is_blocked() {
    let broken = SOUND_VAULT.replace("pub struct Vault<F: Format> {", "#[derive(Debug)]\npub struct Vault<F: Format> {");
    let r = audit_vault(&broken);
    assert!(
        r.blocked("`Debug` 를 파생하고 있습니다"),
        "열쇠를 찍는 `Debug` 파생을 막지 못했습니다"
    );
}

#[test]
fn a_missing_manual_debug_is_blocked() {
    let broken = SOUND_VAULT.replace("std::fmt::Debug for Vault<F>", "std::fmt::Display for Vault<F>");
    assert!(
        audit_vault(&broken).blocked("손으로 쓴 `Debug` 가 없습니다"),
        "가리는 `Debug` 가 없는 것을 막지 못했습니다"
    );
}

#[test]
fn a_vanished_struct_is_blocked() {
    let broken = SOUND_VAULT.replace("struct Vault<F: Format>", "struct Envelope<F: Format>");
    assert!(
        audit_vault(&broken).blocked("`struct Vault` 를 찾지 못했습니다"),
        "금고 자체가 사라진 것을 막지 못했습니다"
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
                .blocked(&format!("{dep} 에 zeroize 기능이 꺼져 있습니다")),
            "{dep} 의 지우개가 꺼진 것을 막지 못했습니다"
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
                .blocked(&format!("어디서도 {dep} 를 찾지 못했습니다")),
            "{dep} 가 사라진 것을 막지 못했습니다"
        );
    }
}

/// 주석 안의 글자를 코드로 세면 시험이 **열린 채로 통과한다.** 지우는 줄을
/// 주석으로 막아 놓아도 "지운다" 고 읽는다.
#[test]
fn a_commented_out_wipe_does_not_count() {
    let broken = SOUND_VAULT.replace("        self.key.zeroize();", "        // self.key.zeroize();");
    assert!(
        audit_vault(&broken).blocked("`Drop` 이 열쇠를 지우지 않습니다"),
        "주석으로 막아 놓은 지우기를 지운 것으로 세었습니다"
    );
    let block = SOUND_VAULT.replace("        self.key.zeroize();", "        /* self.key.zeroize(); */");
    assert!(
        audit_vault(&block).blocked("`Drop` 이 열쇠를 지우지 않습니다"),
        "덩이 주석으로 막아 놓은 지우기를 지운 것으로 세었습니다"
    );
}
