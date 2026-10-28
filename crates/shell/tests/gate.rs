//! **관문의 정의는 하나이고, 반출은 그것을 경유해야 한다.**
//!
//! 워크플로가 셋이다.
//!
//! | 파일 | 무엇 |
//! |---|---|
//! | `gate.yml` | 관문의 정의. `workflow_call` 로만 작동한다 |
//! | `ci.yml` | 분기와 병합 요청. 관문을 부른다 |
//! | `publish.yml` | 반출. 수동 기동 전용이고, 관문을 부른 뒤 송출·굽기·게시한다 |
//!
//! 그리고 사람이 입력하는 명령은 `npm run ci` 하나다. 그 명령은 조각들(`ci:<무엇>`)을
//! 순차 호출하고, `gate.yml` 은 동일한 조각을 잡 하나씩 담당해 동시에 실행한다.
//! **어느 쪽도 검사를 글자로 별도 기재하지 않는다.**
//!
//! 기재했더니 한쪽만 수정되었다. 그 시점부터 반출되는 것과 평상시 참조하는 것이
//! **상이한 행을 경유한다** — 그리고 그것은 어디에서도 오류로 발현되지 않는다.
//! 더 열악하게는, 느슨해지는 쪽이 통상 반출하는 쪽이다. 그래서 양자를 여기서
//! 결속한다.
//!
//! **이 검사는 글자를 판독한다.** `package.json` 도 워크플로도 `cargo test` 가
//! 실행할 수 있는 것이 아니다. 그래서 **보전 대상을 탐색하지 못하면 통과가
//! 아니라 실패고**, 의도적으로 훼손한 입력으로 실제 차단하는지 함께 판정한다.

const GATE: &str = include_str!("../../../.github/workflows/gate.yml");
const CI: &str = include_str!("../../../.github/workflows/ci.yml");
const PUBLISH: &str = include_str!("../../../.github/workflows/publish.yml");
const PACKAGE: &str = include_str!("../../../package.json");

/// 게시하는 쪽이 부착하는 문구. 여기 수용하는 것으로 **그 파일이 소멸하면
/// 관문이 정지한다** — 소멸을 태그 시점에 인지하면 1회를 소모한다.
const NOTES: &str = include_str!("../../../.github/release-notes.md");

/// 관문을 부르는 행. 부르는 파일마다 이것이 소재해야 한다.
const CALL: &str = "uses: ./.github/workflows/gate.yml";

/// 관문을 부르는 파일들. 명칭은 진술에 사용한다.
const CALLERS: &[(&str, &str)] = &[("ci.yml", CI), ("publish.yml", PUBLISH)];

/// 반출하는 잡들. `publish.yml` 에 소재하고, 관문을 대기해야 한다.
///
/// 표지는 그 잡의 문서에 출현해야 하는 글자다. 명칭만 탐색하면 잡을 개칭한
/// 시점에 **판정 대상이 소거된 채로 통과**한다.
const OUTBOUND: &[(&str, &str)] = &[
    ("pages", "actions/deploy-pages"),
    ("bake", "npm run bake"),
    ("ship", "gh release create"),
];

/// 반출하는 잡들 중 최종. 타 반출 잡 전부를 대기한다.
const LAST: &str = "ship";

/// 태그가 아닌 것에서 정지시키는 잡.
const TAG_ONLY: &str = "tag-only";

/// 관문을 담당하지 않는 잡. 관문을 부르는 잡은 이 명칭이다.
const CALL_JOB: &str = "gate";

/// 검사 도구들. 부르는 파일에 기재하면 관문의 정의가 두 벌이 된다.
const CHECK_TOOLS: &[&str] = &[
    "cargo test",
    "cargo clippy",
    "cargo fmt",
    "vue-tsc",
    "eslint",
    "npm run check",
    "npm run test",
    "npm test",
];

/* ── 판독 ────────────────────────────────────────────────────────── */

/// `package.json` 의 `"<명칭>":` 로 기재된 스크립트 명칭들.
fn script_names(src: &str) -> Vec<String> {
    let Some(at) = src.find("\"scripts\"") else { return Vec::new() };
    let body = &src[at..];
    let to = body.find("\n  }").unwrap_or(body.len());
    body[..to]
        .lines()
        .skip(1)
        .filter_map(|l| l.trim().strip_prefix('"')?.split('"').next().map(str::to_string))
        .collect()
}

/// 한 스크립트가 기재한 행.
fn script_body<'a>(src: &'a str, name: &str) -> Option<&'a str> {
    let at = src.find(&format!("\"{name}\":"))?;
    let after = &src[at + name.len() + 3..];
    let from = after.find('"')? + 1;
    let len = after[from..].find('"')?;
    Some(&after[from..from + len])
}

/// 관문의 조각들(`ci:<무엇>`). `ci` 자체는 조각이 아니다.
fn parts_declared(src: &str) -> Vec<String> {
    script_names(src).into_iter().filter(|n| n.starts_with("ci:")).collect()
}

/// 임의의 문서에서 `npm run ci:<무엇>` 으로 호출되는 조각들.
fn parts_called(src: &str) -> Vec<String> {
    const LEAD: &str = "npm run ci:";
    let mut found: Vec<String> = Vec::new();
    for (i, _) in src.match_indices(LEAD) {
        let name: String = src[i + LEAD.len()..]
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '-')
            .collect();
        if name.is_empty() {
            continue;
        }
        let part = format!("ci:{name}");
        if !found.contains(&part) {
            found.push(part);
        }
    }
    found
}

/// 워크플로의 잡들. 명칭과 그 잡이 수용한 문서.
///
/// 잡은 `jobs:` 하단에 **2칸** 들여쓴 명칭이다. 더 깊이 들여쓴 행은 그 잡의
/// 내부라 다음 잡이 출현할 때까지 함께 수용한다.
///
/// **행을 글자로 탐색하지 않는다.** `include_str!` 은 파일을 원형 그대로
/// 수용하므로 윈도에서는 행 종단이 `\r\n` 이다. `"\njobs:\n"` 을 탐색하면
/// 거기서는 영구히 탐색하지 못하고, 그러면 이 검사는 **어떠한 잡도 판정하지
/// 못한 채 진술할 내용이 부재해진다.** 1회 그렇게 발생했다 — `lines()` 는 `\r`
/// 을 제거하므로 그것으로 산정한다.
///
/// **주석은 잡의 문서가 아니다.** 잡 상단의 주석은 그 직전의 잡에 부착되므로,
/// 수용하면 수호하는 글자를 **산문이 충족시킨다.**
fn jobs(src: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let mut inside = false;
    for line in src.lines() {
        if !inside {
            inside = line.trim_end() == "jobs:";
            continue;
        }
        // 들여쓰지 않은 행이 출현하면 `jobs:` 외부다.
        if !line.trim().is_empty() && !line.starts_with(' ') {
            break;
        }
        let head = line.starts_with("  ")
            && !line.starts_with("   ")
            && line.trim_end().ends_with(':')
            && !line.trim_start().starts_with('#');
        if head {
            out.push((line.trim().trim_end_matches(':').to_string(), String::new()));
        } else if line.trim_start().starts_with('#') {
            continue;
        } else if let Some(last) = out.last_mut() {
            last.1.push_str(line);
            last.1.push('\n');
        }
    }
    out
}

/// 한 잡의 문서.
fn job(src: &str, name: &str, whose: &str) -> String {
    jobs(src)
        .into_iter()
        .find(|(n, _)| n == name)
        .unwrap_or_else(|| panic!("{whose} 에서 `{name}` 잡을 탐색하지 못했습니다 — 검사가 실명한 것입니다"))
        .1
}

/// 한 잡이 대기하는 잡들 (`needs: [a, b]` 또는 `needs: a`).
fn needs(block: &str) -> Vec<String> {
    let Some(at) = block.find("needs:") else { return Vec::new() };
    let Some(line) = block[at + "needs:".len()..].lines().next() else { return Vec::new() };
    line.trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// 전원 하나가 기재한 문서 (`on:` 하단의 2칸 들여쓴 명칭).
fn trigger(src: &str, name: &str) -> Option<String> {
    let mut inside = false;
    let mut mine = false;
    let mut out = String::new();
    for line in src.lines() {
        if !inside {
            // `on:` 은 YAML 에서 참으로 판독되는 열쇠라 인용하는 판도 존재한다.
            let t = line.trim_end();
            inside = t == "on:" || t == "\"on\":" || t == "'on':";
            continue;
        }
        if !line.trim().is_empty() && !line.starts_with(' ') {
            break;
        }
        let head = line.starts_with("  ")
            && !line.starts_with("   ")
            && line.trim_end().ends_with(':')
            && !line.trim_start().starts_with('#');
        if head {
            if mine {
                break;
            }
            mine = line.trim().trim_end_matches(':') == name;
            continue;
        }
        if mine && !line.trim_start().starts_with('#') {
            out.push_str(line);
            out.push('\n');
        }
    }
    mine.then_some(out)
}

/// 기재된 전원들.
fn triggers(src: &str) -> Vec<String> {
    let mut inside = false;
    let mut out = Vec::new();
    for line in src.lines() {
        if !inside {
            let t = line.trim_end();
            inside = t == "on:" || t == "\"on\":" || t == "'on':";
            continue;
        }
        if !line.trim().is_empty() && !line.starts_with(' ') {
            break;
        }
        if line.starts_with("  ")
            && !line.starts_with("   ")
            && line.trim_end().ends_with(':')
            && !line.trim_start().starts_with('#')
        {
            out.push(line.trim().trim_end_matches(':').to_string());
        }
    }
    out
}

/* ── 실명하지 않았는가 ──────────────────────────────────────────── */

#[test]
fn the_sources_are_there_to_look_at() {
    assert!(
        !parts_declared(PACKAGE).is_empty(),
        "package.json 에서 관문의 조각을 탐색하지 못했습니다 — 검사가 실명한 것입니다"
    );
    assert!(
        script_body(PACKAGE, "ci").is_some(),
        "`ci` 스크립트를 탐색하지 못했습니다 — 검사가 실명한 것입니다"
    );
    assert!(!jobs(GATE).is_empty(), "`gate.yml` 에서 잡을 탐색하지 못했습니다 — 검사가 실명한 것입니다");
    assert!(
        !jobs(PUBLISH).is_empty(),
        "`publish.yml` 에서 잡을 탐색하지 못했습니다 — 검사가 실명한 것입니다"
    );
    assert!(
        !parts_called(GATE).is_empty(),
        "`gate.yml` 에서 조각을 호출하는 위치를 탐색하지 못했습니다 — 검사가 실명한 것입니다"
    );
}

/* ── 단일 명령이 전부인가 ───────────────────────────────────────── */

/// **`npm run ci` 가 조각 전부를 호출하는가.**
///
/// 하나를 누락하면 사람이 입력하는 단일 명령만 묵시적으로 적게 판정하게 된다.
/// 액션은 그대로 전부 실행되어 통과하므로, 그 차이는 커밋을 전송한 이후에야
/// 발현된다.
#[test]
fn the_one_command_runs_every_part() {
    let called = parts_called(script_body(PACKAGE, "ci").expect("`ci` 스크립트가 부재합니다"));
    let missing: Vec<String> =
        parts_declared(PACKAGE).into_iter().filter(|p| !called.contains(p)).collect();
    assert!(missing.is_empty(), "`npm run ci` 가 호출하지 않는 조각이 존재합니다: {missing:?}");
}

/// **`npm run ci` 가 부재하는 조각을 호출하지 않는가.**
///
/// 기존 조각 하나를 오기로 치환하면, 그 조각이 호출되지 않는 채로 남은 조각들이
/// 전부 통과한다 — 액션은 전부 실행하므로 전송한 이후에야 발현된다.
#[test]
fn the_one_command_calls_nothing_that_is_not_a_part() {
    let declared = parts_declared(PACKAGE);
    let called = parts_called(script_body(PACKAGE, "ci").expect("`ci` 스크립트가 부재합니다"));
    let unknown: Vec<String> = called.into_iter().filter(|p| !declared.contains(p)).collect();
    assert!(unknown.is_empty(), "`npm run ci` 가 부재하는 조각을 호출합니다: {unknown:?}");
}

/// **조각마다 `gate.yml` 에 잡이 존재하는가.**
///
/// 조각을 증설하고 `gate.yml` 을 수정하지 않으면 그 검사는 로컬에서만 작동한다 —
/// 그리고 도구가 부재하는 환경에서 작동하지 못하는 조각(wasm)이면 **어디에서도
/// 작동하지 않는다.**
#[test]
fn the_gate_runs_every_part() {
    let in_gate = parts_called(GATE);
    let missing: Vec<String> =
        parts_declared(PACKAGE).into_iter().filter(|p| !in_gate.contains(p)).collect();
    assert!(missing.is_empty(), "`gate.yml` 이 실행하지 않는 조각이 존재합니다: {missing:?}");
}

/// 부재하는 조각을 호출하지 않는가. 명칭을 변경한 시점에 구 명칭이 잔존하면 그
/// 잡은 `npm` 이 인지하지 못하는 스크립트를 호출하며 중단한다 — 지연되어,
/// 무관한 문구로.
#[test]
fn the_gate_calls_nothing_that_is_not_a_part() {
    let declared = parts_declared(PACKAGE);
    let extra: Vec<String> =
        parts_called(GATE).into_iter().filter(|p| !declared.contains(p)).collect();
    assert!(extra.is_empty(), "package.json 에 부재하는 조각을 `gate.yml` 이 호출합니다: {extra:?}");
}

/* ── 관문의 정의가 하나인가 ─────────────────────────────────────── */

/// **부르는 파일들이 관문을 호출하는가.**
#[test]
fn every_caller_calls_the_gate() {
    for (name, src) in CALLERS {
        assert!(src.contains(CALL), "`{name}` 이 관문을 호출하지 않습니다");
    }
}

/// **부르는 파일들이 검사를 직접 기재하지 않는가.**
///
/// 기재하면 관문의 정의가 두 벌이 되고, 한쪽만 수정된다. 그 시점부터 느슨해지는
/// 쪽은 통상 반출하는 쪽이다 — 검사가 정지한 판이 그대로 반출되고 게시는 정상
/// 성공한다.
#[test]
fn no_caller_spells_out_a_check_of_its_own() {
    let mut spelled: Vec<String> = Vec::new();
    for (name, src) in CALLERS {
        for line in src.lines().map(str::trim) {
            if line.starts_with('#') {
                continue;
            }
            if let Some(tool) = CHECK_TOOLS.iter().find(|t| line.contains(**t)) {
                spelled.push(format!("{name}: `{tool}`"));
            }
            if !parts_called(line).is_empty() {
                spelled.push(format!("{name}: `{line}`"));
            }
        }
    }
    assert!(
        spelled.is_empty(),
        "관문을 부르는 파일이 검사를 직접 기재했습니다 — `{CALL}` 로 호출하십시오: {spelled:?}"
    );
}

/// **`gate.yml` 이 부르는 것으로만 작동하는가.**
///
/// 자체 전원을 기재하면 관문이 독립해 작동하고, 그러면 부르는 쪽이 통과시킨 것과
/// 상이한 결과가 녹색으로 병존한다.
#[test]
fn the_gate_runs_only_when_it_is_called() {
    assert_eq!(
        triggers(GATE),
        ["workflow_call"],
        "`gate.yml` 이 `workflow_call` 외의 전원을 기재했습니다"
    );
}

/* ── 반출이 관문을 경유하는가 ───────────────────────────────────── */

/// **반출하는 잡이 관문을 대기하는가.**
///
/// 그 행이 소거되면 검사가 정지한 판이 그대로 반출되고 게시는 정상 성공한다.
#[test]
fn what_goes_out_has_passed_the_gate() {
    for (name, marker) in OUTBOUND {
        let block = job(PUBLISH, name, "`publish.yml`");
        assert!(
            block.contains(marker),
            "`{name}` 에서 `{marker}` 를 탐색하지 못했습니다 — 검사가 실명한 것입니다"
        );
        assert!(
            needs(&block).contains(&CALL_JOB.to_string()),
            "`{name}` 이 관문(`{CALL_JOB}`)을 대기하지 않습니다"
        );
    }
}

/// **게시가 반출하는 것 전부를 대기하는가.**
///
/// 화면이 송출되지 않은 판에서 Release 가 부착되면 그것은 절반만 발생한
/// 반출이고, 부착된 이후에 그것을 진술하는 위치는 부재하다.
///
/// **시간을 소모하지 않는다.** `needs` 는 순차 실행이 아니다 — 송출과 굽기는
/// 그대로 병렬로 작동하고, 게시는 지연된 쪽이 종료된 시점에 시작한다.
#[test]
fn publishing_waits_for_everything_else_that_goes_out() {
    let waits = needs(&job(PUBLISH, LAST, "`publish.yml`"));
    assert!(!waits.is_empty(), "`{LAST}` 가 대기하는 잡이 부재합니다 — 검사가 실명한 것입니다");
    let missing: Vec<&str> = OUTBOUND
        .iter()
        .map(|(n, _)| *n)
        .filter(|n| *n != LAST && !waits.contains(&n.to_string()))
        .collect();
    assert!(missing.is_empty(), "게시가 대기하지 않는 반출 잡이 존재합니다: {missing:?}");
}

/* ── 반출을 사람만 기동하는가 ───────────────────────────────────── */

/// **`publish.yml` 이 수동 기동 전용인가.**
///
/// 전원을 하나라도 증설하면 — 특히 `push` — **판을 표식하려 붙인 태그가 그대로
/// 게시된다.** 아무것도 그것을 진술하지 않는다.
#[test]
fn publishing_starts_only_by_hand() {
    assert_eq!(
        triggers(PUBLISH),
        ["workflow_dispatch"],
        "`publish.yml` 이 수동 기동 외의 전원을 기재했습니다 — 반출이 저절로 발생합니다"
    );
}

/// **태그가 아닌 것에서 정지시키는 잡이 소재하고, 관문이 그것을 대기하는가.**
///
/// 생략하는 것이 아니라 **정지시킨다.** 묵시적으로 아무것도 수행하지 않은 판은
/// 게시되었다고 판독된다.
#[test]
fn publishing_refuses_anything_that_is_not_a_tag() {
    let guard = job(PUBLISH, TAG_ONLY, "`publish.yml`");
    assert!(
        guard.contains("github.ref_type"),
        "`{TAG_ONLY}` 가 ref 의 종별을 판정하지 않습니다 — 검사가 실명한 것입니다"
    );
    assert!(guard.contains("exit 1"), "`{TAG_ONLY}` 가 정지하지 않습니다 — 생략은 통과로 판독됩니다");
    assert!(
        needs(&job(PUBLISH, CALL_JOB, "`publish.yml`")).contains(&TAG_ONLY.to_string()),
        "관문이 `{TAG_ONLY}` 를 대기하지 않습니다 — 분기에서도 반출이 진행됩니다"
    );
}

/* ── 반출하는 것의 명칭이 한 벌인가 ─────────────────────────────── */

/// 임의의 문서가 기재한 산출물의 명칭들.
///
/// 판을 기재하는 자리는 쪽마다 상이하다(`$version` 과 `{version}`). 그것을 하나로
/// 통일해 비교한다. **판을 기재하지 않은 것은 산출물이 아니다** —
/// `sheetsift-tauri.exe` 는 상자의 실행 파일이고 반출하지 않는다.
fn asset_names(src: &str) -> Vec<String> {
    const LEAD: &str = "sheetsift-";
    let mut out: Vec<String> = Vec::new();
    for (i, _) in src.match_indices(LEAD) {
        let raw: String = src[i..]
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || "-._${}".contains(*c))
            .collect();
        let name = raw.replace("$version", "{}").replace("{version}", "{}");
        if !name.contains("{}") || out.contains(&name) {
            continue;
        }
        out.push(name);
    }
    out.sort();
    out
}

/// 게시 직전에 존재를 확인하는 조각들 (`for want in …`).
fn wants(src: &str) -> Vec<String> {
    let Some(at) = src.find("for want in ") else { return Vec::new() };
    let Some(line) = src[at + "for want in ".len()..].lines().next() else { return Vec::new() };
    line.trim().trim_end_matches("; do").split_whitespace().map(str::to_string).collect()
}

/// **구운 것과 게시 문구가 동일한 명칭을 기재하는가.**
///
/// 명칭이 세 곳에 기재된다 — 굽는 단계, 게시 직전의 확인, 그리고 사람이 판독하는
/// 문구. 한쪽만 수정되면 **게시 쪽은 부존재하는 파일을 안내하고**, 그것은
/// 어디에서도 오류로 출현하지 않는다. 내려받는 사람만 인지한다.
#[test]
fn the_release_page_names_what_was_baked() {
    let made = asset_names(PUBLISH);
    assert!(
        !made.is_empty(),
        "`publish.yml` 에서 산출물의 명칭을 탐색하지 못했습니다 — 검사가 실명한 것입니다"
    );
    let told = asset_names(NOTES);
    assert!(
        !told.is_empty(),
        "게시 문구에서 산출물의 명칭을 탐색하지 못했습니다 — 검사가 실명한 것입니다"
    );
    assert_eq!(made, told, "구운 것과 게시 문구가 기재한 명칭이 상이합니다");
    assert!(NOTES.contains("{version}"), "게시 문구에 판을 기재하는 자리가 부재합니다");

    let wants = wants(PUBLISH);
    assert!(
        !wants.is_empty(),
        "게시 직전에 확인하는 조각을 탐색하지 못했습니다 — 검사가 실명한 것입니다"
    );
    for want in &wants {
        assert!(
            made.iter().any(|m| m.contains(want.as_str())),
            "`{want}` 를 확인하는데 그런 명칭의 산출물이 부재합니다"
        );
    }
    for m in &made {
        assert!(
            wants.iter().any(|w| m.contains(w.as_str())),
            "`{m}` 이 게시 직전에 확인되지 않습니다"
        );
    }
}

/* ── 수령하는 것이 구운 것뿐인가 ────────────────────────────────── */

/// `upload-artifact` 가 기재한 명칭들.
fn artifact_names(src: &str) -> Vec<String> {
    const LEAD: &str = "name: bundle-";
    src.match_indices(LEAD)
        .map(|(i, _)| format!("bundle-{}", src[i + LEAD.len()..].lines().next().unwrap_or("").trim()))
        .collect()
}

/// `download-artifact` 가 기재한 패턴.
fn artifact_pattern(src: &str) -> Option<String> {
    let at = src.find("pattern: ")?;
    Some(src[at + "pattern: ".len()..].lines().next()?.trim().to_string())
}

/// **수령하는 것을 한정했는가, 그리고 그 한정이 올린 것과 합치하는가.**
///
/// 명칭을 한정하지 않으면 그 판이 산출한 것 **전부**를 수령한다 — 화면을 송출
/// 하는 잡도 산출물을 하나 생성하므로, 그것이 Release 에 부착된다. 실제
/// 발생했고, 존재를 확인하는 쪽은 **각 판이 소재하는지만 판정해 그것을 포착하지
/// 못했다.**
#[test]
fn what_is_published_is_what_was_baked() {
    let names = artifact_names(PUBLISH);
    assert!(
        !names.is_empty(),
        "올리는 산출물의 명칭을 탐색하지 못했습니다 — 검사가 실명한 것입니다"
    );
    let pattern = artifact_pattern(PUBLISH)
        .expect("내려받는 패턴을 기재하지 않았습니다 — 그 판의 산출물 전부를 수령합니다");
    let prefix = pattern
        .strip_suffix('*')
        .unwrap_or_else(|| panic!("패턴 `{pattern}` 이 접두로 한정하지 않습니다"));
    let stray: Vec<&String> = names.iter().filter(|n| !n.starts_with(prefix)).collect();
    assert!(stray.is_empty(), "패턴 `{pattern}` 이 수령하지 못하는 산출물이 존재합니다: {stray:?}");
}

/* ── 도구의 판이 한 값인가 ──────────────────────────────────────── */

/// `package.json` 이 기재한 `@tauri-apps/cli@<판>` 의 판들.
fn cli_versions(src: &str) -> Vec<String> {
    const LEAD: &str = "@tauri-apps/cli@";
    src.match_indices(LEAD)
        .map(|(i, _)| {
            src[i + LEAD.len()..]
                .chars()
                .take_while(|c| c.is_ascii_digit() || *c == '.')
                .collect::<String>()
        })
        .collect()
}

/// **굽는 쪽과 구동하는 쪽이 동일한 도구를 경유하는가.**
///
/// 고정하지 않은 도구는 아무것도 변경하지 않은 날에 변동한다. 그래서 고정하는데,
/// 고정한 값이 두 곳에 기재되면 **한쪽만 상승한다** — 그 시점부터 구워지는 것과
/// 창을 개방해 판독하는 것이 상이한 도구를 경유한다.
#[test]
fn the_tool_is_pinned_to_one_version() {
    let found = cli_versions(PACKAGE);
    assert!(
        !found.is_empty(),
        "`package.json` 에서 도구의 판을 탐색하지 못했습니다 — 검사가 실명한 것입니다"
    );
    // **판을 전치하지 않은 위치까지 산정한다.** `cli_versions` 는 `@` 가 후속하는
    // 것만 탐색하므로, `@` 자체가 부재한 호출은 계수되지 않은 채로 통과한다.
    assert_eq!(
        PACKAGE.matches("@tauri-apps/cli").count(),
        found.len(),
        "판을 고정하지 않고 도구를 호출하는 위치가 존재합니다"
    );
    assert!(found.iter().all(|v| !v.is_empty()), "판을 고정하지 않은 위치가 존재합니다: {found:?}");
    let first = &found[0];
    assert!(found.iter().all(|v| v == first), "도구의 판이 복수입니다: {found:?}");
}

/// **워크플로가 도구의 판을 기재하지 않는가.**
#[test]
fn no_workflow_pins_a_version_of_its_own() {
    for (name, src) in [("gate.yml", GATE), ("ci.yml", CI), ("publish.yml", PUBLISH)] {
        assert!(
            !src.contains("@tauri-apps/cli@"),
            "`{name}` 이 도구의 판을 직접 기재했습니다 — `package.json` 의 스크립트를 호출하십시오"
        );
    }
}

/* ── 부재하는 스크립트를 호출하지 않는가 ────────────────────────── */

/// 임의의 문서가 **뿌리의** `package.json` 에서 호출하는 스크립트들.
///
/// `working-directory` 를 기재한 단계는 제외한다 — 그 단계의 `npm run build` 는
/// `web/package.json` 의 것이고, 뿌리에는 부재하는 것이 정상이다.
///
/// **단계를 적립해서 판정한다.** 행을 순차 판정하면 `working-directory` 가 `run`
/// 보다 하단에 기재된 단계를 뿌리로 오판하므로, 단계가 종료된 시점에 그 단계
/// 전체를 판독한다 — 기재 순서를 가정하지 않는다.
fn scripts_called(src: &str) -> Vec<String> {
    const LEAD: &str = "npm run ";
    let mut out: Vec<String> = Vec::new();
    let mut step = String::new();

    let take = |step: &str, out: &mut Vec<String>| {
        if step.contains("working-directory:") {
            return;
        }
        for (i, _) in step.match_indices(LEAD) {
            let name: String = step[i + LEAD.len()..]
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == ':' || *c == '-')
                .collect();
            if name.is_empty() || out.contains(&name) {
                continue;
            }
            out.push(name);
        }
    };

    for line in src.lines() {
        let t = line.trim_start();
        if t.starts_with('#') {
            continue;
        }
        // 단계의 선두. `- name:` · `- run:` · `- uses:` 가 그것이다.
        if t.starts_with("- ") && t.contains(':') {
            take(&step, &mut out);
            step.clear();
        }
        step.push_str(line);
        step.push('\n');
    }
    take(&step, &mut out);
    out
}

/// **부재하는 스크립트를 호출하지 않는가.**
///
/// 조각(`ci:…`)은 이미 판정한다. 그런데 굽는 잡은 조각이 아닌 스크립트를
/// 호출하고(`bake`), 그 명칭을 오기하거나 `package.json` 에서 개칭하면 **태그를
/// 1회 소모한 이후에** 발현된다.
#[test]
fn no_workflow_calls_a_script_that_is_not_declared() {
    let declared = script_names(PACKAGE);
    for (name, src) in [("gate.yml", GATE), ("publish.yml", PUBLISH)] {
        let called = scripts_called(src);
        assert!(
            !called.is_empty(),
            "`{name}` 에서 스크립트를 호출하는 위치를 탐색하지 못했습니다 — 검사가 실명한 것입니다"
        );
        let unknown: Vec<String> = called.into_iter().filter(|s| !declared.contains(s)).collect();
        assert!(
            unknown.is_empty(),
            "`{name}` 이 package.json 에 부재하는 스크립트를 호출합니다: {unknown:?}"
        );
    }
}

/* ── 그 판정들이 실제로 차단하는지 ──────────────────────────────── */

/// 조각 하나를 누락한 `ci` 를 투입한다.
#[test]
fn the_check_catches_a_part_the_one_command_forgot() {
    let doctored = concat!(
        "{\n  \"scripts\": {\n",
        "    \"ci\": \"npm run ci:gate\",\n",
        "    \"ci:gate\": \"cargo test --workspace\",\n",
        "    \"ci:wasm\": \"cargo test --target wasm32-unknown-unknown\"\n",
        "  }\n}\n"
    );
    assert_eq!(parts_declared(doctored), ["ci:gate", "ci:wasm"]);
    let called = parts_called(script_body(doctored, "ci").expect("`ci` 가 부재합니다"));
    assert_eq!(called, ["ci:gate"]);
}

/// 조각 하나를 오기한 `ci` 를 투입한다.
#[test]
fn the_check_catches_a_misspelled_part_in_the_one_command() {
    let doctored = concat!(
        "{\n  \"scripts\": {\n",
        "    \"ci\": \"npm run ci:gate && npm run ci:wsam\",\n",
        "    \"ci:gate\": \"cargo test --workspace\",\n",
        "    \"ci:wasm\": \"cargo test --target wasm32-unknown-unknown\"\n",
        "  }\n}\n"
    );
    let declared = parts_declared(doctored);
    let called = parts_called(script_body(doctored, "ci").expect("`ci` 가 부재합니다"));
    let unknown: Vec<&String> = called.iter().filter(|p| !declared.contains(p)).collect();
    assert_eq!(unknown, [&"ci:wsam".to_string()]);
}

/// 관문을 부르지 않는 파일을 투입한다.
#[test]
fn the_check_catches_a_caller_that_stopped_calling() {
    let doctored = "jobs:\n  gate:\n    runs-on: ubuntu-latest\n    steps:\n      - run: npm run ci\n";
    assert!(!doctored.contains(CALL));
}

/// 검사를 직접 기재한 파일을 투입한다.
#[test]
fn the_check_catches_a_check_written_into_a_caller() {
    let doctored = concat!(
        "jobs:\n",
        "  gate:\n    uses: ./.github/workflows/gate.yml\n",
        "  extra:\n    steps:\n      - run: cargo test --workspace\n",
        "      - run: npm run ci:web\n"
    );
    let mut spelled: Vec<String> = Vec::new();
    for line in doctored.lines().map(str::trim) {
        if line.starts_with('#') {
            continue;
        }
        if CHECK_TOOLS.iter().any(|t| line.contains(*t)) || !parts_called(line).is_empty() {
            spelled.push(line.to_string());
        }
    }
    assert_eq!(spelled, ["- run: cargo test --workspace", "- run: npm run ci:web"]);
}

/// 자체 전원을 기재한 관문을 투입한다.
#[test]
fn the_check_catches_a_gate_that_runs_on_its_own() {
    let doctored = "on:\n  workflow_call:\n  push:\n    branches: [main]\njobs:\n";
    assert_eq!(triggers(doctored), ["workflow_call", "push"]);
}

/// 전원을 증설한 반출을 투입한다.
#[test]
fn the_check_catches_a_publish_a_push_can_start() {
    let doctored = "on:\n  workflow_dispatch:\n  push:\n    tags: ['*']\njobs:\n";
    assert_eq!(triggers(doctored), ["workflow_dispatch", "push"]);
}

/// 관문을 대기하지 않는 반출 잡을 투입한다.
#[test]
fn the_check_catches_an_outbound_job_that_skips_the_gate() {
    let doctored = concat!(
        "\njobs:\n",
        "  gate:\n    needs: tag-only\n    uses: ./.github/workflows/gate.yml\n",
        "  pages:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/deploy-pages@v5\n"
    );
    assert_eq!(needs(&job(doctored, "pages", "투입")), Vec::<String>::new());
    assert_eq!(needs(&job(doctored, "gate", "투입")), ["tag-only"]);
}

/// 굽기만 대기하는 게시를 투입한다.
#[test]
fn the_check_catches_a_publish_that_leaves_the_screen_behind() {
    let doctored = concat!(
        "\njobs:\n",
        "  pages:\n    needs: gate\n    steps:\n      - uses: actions/deploy-pages@v5\n",
        "  bake:\n    needs: gate\n    steps:\n      - run: npm run bake\n",
        "  ship:\n    needs: [gate, bake]\n    steps:\n      - run: gh release create x\n"
    );
    let waits = needs(&job(doctored, LAST, "투입"));
    let missing: Vec<&str> = OUTBOUND
        .iter()
        .map(|(n, _)| *n)
        .filter(|n| *n != LAST && !waits.contains(&n.to_string()))
        .collect();
    assert_eq!(missing, ["pages"]);
}

/// 정지하지 않는 수호 잡을 투입한다.
#[test]
fn the_check_catches_a_guard_that_only_skips() {
    let doctored = concat!(
        "\njobs:\n",
        "  tag-only:\n    if: github.ref_type == 'tag'\n    steps:\n      - run: echo ok\n"
    );
    let guard = job(doctored, TAG_ONLY, "투입");
    assert!(guard.contains("github.ref_type"));
    assert!(!guard.contains("exit 1"), "생략하는 것은 통과로 판독됩니다");
}

/// 한쪽만 개칭한 산출물을 투입한다.
#[test]
fn the_check_catches_an_asset_the_release_page_does_not_name() {
    let action = "          Compress-Archive -DestinationPath \"dist/sheetsift-$version-windows-setup.zip\"\n";
    let notes = "| `sheetsift-{version}-windows-portable.zip` | zip |\n";
    assert_eq!(asset_names(action), ["sheetsift-{}-windows-setup.zip"]);
    assert_eq!(asset_names(notes), ["sheetsift-{}-windows-portable.zip"]);
    assert_ne!(asset_names(action), asset_names(notes));
}

/// 판을 기재하지 않은 것을 산출물로 오판하지 않는가.
#[test]
fn the_check_tells_an_asset_from_the_crates_own_executable() {
    let doctored = "$exe = \"crates/tauri/target/release/sheetsift-tauri.exe\"\n";
    assert_eq!(asset_names(doctored), Vec::<String>::new());
}

/// 한정하지 않은 내려받기를 투입한다.
#[test]
fn the_check_catches_a_download_that_takes_everything() {
    let doctored = "      - uses: actions/download-artifact@v8\n        with:\n          path: artifacts\n          merge-multiple: true\n";
    assert!(artifact_pattern(doctored).is_none());
}

/// 접두가 상이한 패턴을 투입한다.
#[test]
fn the_check_catches_a_pattern_that_misses_what_was_uploaded() {
    let doctored = concat!(
        "          name: bundle-windows\n",
        "          name: bundle-macos\n",
        "          pattern: dist-*\n"
    );
    let names = artifact_names(doctored);
    assert_eq!(names, ["bundle-windows", "bundle-macos"]);
    let pattern = artifact_pattern(doctored).expect("패턴이 부재합니다");
    let prefix = pattern.strip_suffix('*').expect("접두가 부재합니다");
    assert_eq!(names.iter().filter(|n| !n.starts_with(prefix)).count(), names.len());
}

/// 상이한 판을 투입한다.
#[test]
fn the_check_catches_a_second_pin_that_moved() {
    let doctored = concat!(
        "{\n  \"scripts\": {\n",
        "    \"bake\": \"npx @tauri-apps/cli@2.9.1 build\",\n",
        "    \"dev:tauri\": \"npx @tauri-apps/cli@2.8.0 dev\"\n",
        "  }\n}\n"
    );
    let found = cli_versions(doctored);
    assert_eq!(found, ["2.9.1", "2.8.0"]);
    assert!(!found.iter().all(|v| v == &found[0]));
}

/// 오기한 스크립트를 투입한다.
#[test]
fn the_check_catches_a_misspelled_script() {
    let doctored = "      - run: npm run bkae -- --bundles nsis\n";
    assert_eq!(scripts_called(doctored), ["bkae"]);
    assert!(!script_names(PACKAGE).contains(&"bkae".to_string()));
}

/// 타 그릇의 스크립트를 뿌리의 것으로 오판하지 않는가. 기재 순서를 가정하지
/// 않는지도 함께 판정한다.
#[test]
fn the_check_tells_the_root_scripts_from_the_screens_own() {
    for doctored in [
        concat!(
            "      - name: Build the screen\n        working-directory: web\n        run: npm run build\n",
            "      - name: Bake\n        run: npm run bake -- --bundles nsis\n"
        ),
        concat!(
            "      - name: Build the screen\n        run: npm run build\n        working-directory: web\n",
            "      - name: Bake\n        run: npm run bake -- --bundles nsis\n"
        ),
    ] {
        assert_eq!(scripts_called(doctored), ["bake"]);
    }
}

/* ── 판독 자체가 실명하지 않았는가 ──────────────────────────────── */

/// **행 종단이 `\r\n` 이어도 판독하는가.**
///
/// `include_str!` 은 파일을 원형 그대로 수용한다. 윈도에서 체크아웃하면 행
/// 종단이 `\r\n` 이라, 행을 글자로 탐색하던 시점에는 **어떠한 잡도 탐색하지
/// 못하고** 검사가 "실명한 것입니다" 를 출력했다. 러너(리눅스)에서는 정상이라,
/// 수정하는 사람만 중단된다.
#[test]
fn the_check_reads_a_workflow_with_windows_line_endings() {
    let crlf = "name: ci\r\non:\r\n  workflow_call:\r\njobs:\r\n  gate:\r\n    steps:\r\n      - run: npm run ci:gate\r\n";
    assert_eq!(jobs(crlf).iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>(), ["gate"]);
    assert_eq!(triggers(crlf), ["workflow_call"]);
}

/// 잡을 구분하는 판정이 실명하지 않았는가. 들여쓰기만 판정해 분할하므로, 내부의
/// `with:` 를 잡으로 산정하면 목록이 왜곡된다.
#[test]
fn the_check_tells_a_job_from_what_is_inside_one() {
    let doctored = "\njobs:\n  gate:\n    steps:\n      - uses: x\n        with:\n          key: gate\n  wasm:\n    steps:\n      - run: npm run ci:wasm\n";
    assert_eq!(
        jobs(doctored).iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>(),
        ["gate", "wasm"]
    );
}

/// 주석이 잡의 문서에 수용되지 않는가.
///
/// 잡 상단의 주석은 그 직전의 잡에 부착된다. 수용하면 수호하는 글자를 **산문이
/// 충족시킨다.**
#[test]
fn the_check_does_not_let_a_comment_stand_in_for_a_job() {
    let doctored = concat!(
        "\njobs:\n",
        "  pages:\n    steps:\n      - uses: actions/deploy-pages@v5\n",
        "  # github.ref_type 를 판정한다고 기재만 한 주석\n",
        "  bake:\n    steps:\n      - run: npm run bake\n"
    );
    let pages = job(doctored, "pages", "투입");
    assert!(!pages.contains("github.ref_type"), "상단의 주석이 직전 잡의 문서에 수용되었습니다");
    assert_eq!(
        jobs(doctored).iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>(),
        ["pages", "bake"]
    );
}

/// 전원을 판별하는 판정이 그 내부와 타 전원을 혼동하지 않는가.
#[test]
fn the_check_tells_one_trigger_from_the_next() {
    let doctored = concat!(
        "on:\n",
        "  push:\n    branches: ['**']\n",
        "  pull_request:\n",
        "jobs:\n  gate:\n    steps:\n      - run: npm run ci:gate\n"
    );
    let push = trigger(doctored, "push").expect("push 가 부재합니다");
    assert_eq!(push.trim(), "branches: ['**']");
    assert!(!push.contains("pull_request"));
    assert!(!push.contains("jobs"));
    assert_eq!(triggers(doctored), ["push", "pull_request"]);
}

/// `needs: a` 한 개와 `needs: [a, b]` 양측을 판독하는가.
#[test]
fn the_check_reads_both_shapes_of_the_waiting_line() {
    assert_eq!(needs("    needs: gate\n"), ["gate"]);
    assert_eq!(needs("    needs: [gate, pages, bake]\n"), ["gate", "pages", "bake"]);
}
