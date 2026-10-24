//! **관문은 단일 명령이다.**
//!
//! 사람은 `npm run ci` 하나를 입력한다. 그 명령은 조각들(`ci:<무엇>`)을 순차
//! 호출하고, 액션은 동일한 조각을 잡 하나씩 담당해 동시에
//! 실행한다. 어느 쪽도 검사를 글자로 별도 기재하지 않는다.
//!
//! 기재했더니 한쪽만 수정되었다. 그 시점부터 반출되는 것과 평상시 참조하는 것이
//! **상이한 행을 경유한다** — 그리고 그것은 어디에서도 오류로 발현되지 않는다.
//! 그래서 양자를 여기서 결속한다.
//!
//! - 조각을 하나 증가시키고 액션을 수정하지 않으면 중단한다.
//! - 액션이 조각 하나를 누락하면 중단한다.
//! - `npm run ci` 가 조각 전부를 호출하지 않으면 중단한다.
//! - 배포가 조각 하나라도 대기하지 않으면 중단한다.
//!
//! **이 검사는 글자를 판독한다.** `package.json` 도 워크플로도 `cargo test` 가
//! 실행할 수 있는 것이 아니다. 그래서 **보전 대상을 탐색하지 못하면 통과가
//! 아니라 실패고**, 의도적으로 훼손한 입력으로 실제 차단하는지 함께 판정한다.

const ACTION: &str = include_str!("../../../.github/workflows/ci.yml");
const PACKAGE: &str = include_str!("../../../package.json");

/// 배포하는 잡. 관문이 아니라 반출하는 위치다.
const DEPLOY_JOB: &str = "pages";

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
        } else if let Some(last) = out.last_mut() {
            last.1.push_str(line);
            last.1.push('\n');
        }
    }
    out
}

/// 한 잡이 대기하는 잡들 (`needs: [a, b]`).
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

/// 관문을 담당한 잡들. 조각을 하나라도 호출하는 잡이 그것이다.
fn gate_jobs(src: &str) -> Vec<String> {
    jobs(src)
        .into_iter()
        .filter(|(n, b)| n != DEPLOY_JOB && !parts_called(b).is_empty())
        .map(|(n, _)| n)
        .collect()
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
    assert!(!jobs(ACTION).is_empty(), "워크플로에서 잡을 탐색하지 못했습니다 — 검사가 실명한 것입니다");
    assert!(
        !parts_called(ACTION).is_empty(),
        "워크플로에서 조각을 호출하는 위치를 탐색하지 못했습니다 — 검사가 실명한 것입니다"
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

/// **조각마다 실행하는 위치가 액션에 존재하는가.**
///
/// 조각을 증가시키고 액션을 수정하지 않으면 그 검사는 로컬에서만 동작한다 — 그리고
/// 도구가 부재하는 환경에서 동작하지 못하는 조각(wasm)이면 **어디에서도 동작하지
/// 않는다.**
#[test]
fn the_action_runs_every_part() {
    let in_action = parts_called(ACTION);
    let missing: Vec<String> =
        parts_declared(PACKAGE).into_iter().filter(|p| !in_action.contains(p)).collect();
    assert!(missing.is_empty(), "액션이 실행하지 않는 조각이 존재합니다: {missing:?}");
}

/// 액션이 **부재하는 조각**을 호출하지 않는가. 명칭을 변경한 시점에 구 명칭이
/// 잔존하면 그 잡은 `npm` 이 인지하지 못하는 스크립트를 호출하며 중단한다 —
/// 지연되어, 무관한 문구로.
#[test]
fn the_action_calls_nothing_that_is_not_a_part() {
    let declared = parts_declared(PACKAGE);
    let extra: Vec<String> =
        parts_called(ACTION).into_iter().filter(|p| !declared.contains(p)).collect();
    assert!(extra.is_empty(), "package.json 에 부재하는 조각을 액션이 호출합니다: {extra:?}");
}

/// **검사를 액션에 수동 기재하지 않는가.**
///
/// 조각을 호출하는 대신 `cargo test` 를 거기 직접 기재하면 두 벌이 된다. 그러면
/// 한쪽만 수정되고, 반출되는 것과 평상시 참조하는 것이 상이한 행을 경유한다.
#[test]
fn the_action_spells_out_no_check_of_its_own() {
    let spelled: Vec<&str> = ACTION
        .lines()
        .map(str::trim)
        .filter(|l| !l.starts_with('#'))
        .filter(|l| l.contains("cargo test") || l.contains("cargo clippy"))
        .collect();
    assert!(
        spelled.is_empty(),
        "액션이 검사를 직접 기재했습니다 — package.json 의 조각을 호출하십시오: {spelled:?}"
    );
}

/* ── 반출되는 것이 관문을 통과했는가 ────────────────────────────── */

/// **배포가 조각 전부를 대기하는가.**
///
/// `needs` 한 행이 그것을 보전한다. 관문 잡을 하나 증가시키고 그 행을 증가시키지
/// 않으면, **그 검사가 중단되는 판이 그대로 반출되고 배포는 정상 성공한다.**
#[test]
fn what_goes_out_has_passed_every_part() {
    let all = jobs(ACTION);
    let deploy = all
        .iter()
        .find(|(n, _)| n == DEPLOY_JOB)
        .unwrap_or_else(|| panic!("`{DEPLOY_JOB}` 잡을 탐색하지 못했습니다 — 검사가 실명한 것입니다"));
    assert!(
        deploy.1.contains("actions/deploy-pages"),
        "배포하는 위치를 탐색하지 못했습니다 — 검사가 실명한 것입니다"
    );

    let gates = gate_jobs(ACTION);
    assert!(!gates.is_empty(), "관문 잡을 탐색하지 못했습니다 — 검사가 실명한 것입니다");

    let waits = needs(&deploy.1);
    let unwaited: Vec<&String> = gates.iter().filter(|n| !waits.contains(n)).collect();
    assert!(unwaited.is_empty(), "배포가 대기하지 않는 관문 잡이 존재합니다: {unwaited:?}");
}

/* ── 그 검사들이 실제로 차단하는지 ──────────────────────────────── */

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

/// 조각 하나를 누락한 액션을 투입한다.
#[test]
fn the_check_catches_an_action_that_skips_a_part() {
    let doctored = "      - run: npm run ci:gate\n";
    assert_eq!(parts_called(doctored), ["ci:gate"]);
}

/// 검사를 수동 기재한 액션을 투입한다.
#[test]
fn the_check_catches_a_check_written_into_the_action() {
    let doctored = "jobs:\n  gate:\n    steps:\n      - run: cargo test --workspace\n";
    let spelled: Vec<&str> = doctored
        .lines()
        .map(str::trim)
        .filter(|l| !l.starts_with('#') && l.contains("cargo test"))
        .collect();
    assert_eq!(spelled, ["- run: cargo test --workspace"]);
}

/// 조각 하나를 대기하지 않는 배포를 투입한다.
#[test]
fn the_check_catches_a_deploy_that_does_not_wait_for_a_part() {
    let doctored = concat!(
        "\njobs:\n",
        "  gate:\n    steps:\n      - run: npm run ci:gate\n",
        "  wasm:\n    steps:\n      - run: npm run ci:wasm\n",
        "  pages:\n    needs: [gate]\n    steps:\n      - uses: actions/deploy-pages@v4\n"
    );
    assert_eq!(gate_jobs(doctored), ["gate", "wasm"]);
    let all = jobs(doctored);
    let deploy = all.iter().find(|(n, _)| n == DEPLOY_JOB).expect("pages 가 부재합니다");
    assert_eq!(needs(&deploy.1), ["gate"]);
}

/// **행 종단이 `\r\n` 이어도 판독하는가.**
///
/// `include_str!` 은 파일을 원형 그대로 수용한다. 윈도에서 체크아웃하면 행
/// 종단이 `\r\n` 이라, 행을 글자로 탐색하던 시점에는 **어떠한 잡도 탐색하지
/// 못하고** 검사가 "실명한 것입니다" 를 출력했다. 러너(리눅스)에서는 정상이라,
/// 수정하는 사람만 중단된다.
#[test]
fn the_check_reads_a_workflow_with_windows_line_endings() {
    let crlf = "name: ci\r\njobs:\r\n  gate:\r\n    steps:\r\n      - run: npm run ci:gate\r\n";
    assert_eq!(jobs(crlf).iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>(), ["gate"]);
    assert_eq!(gate_jobs(crlf), ["gate"]);
}

/// 잡을 구분하는 판정이 실명하지 않았는가. 들여쓰기만 판정해 분할하므로, 내부의
/// `with:` 를 잡으로 산정하면 관문 잡 목록이 왜곡된다.
#[test]
fn the_check_tells_a_job_from_what_is_inside_one() {
    let doctored = "\njobs:\n  gate:\n    steps:\n      - uses: x\n        with:\n          key: gate\n  wasm:\n    steps:\n      - run: npm run ci:wasm\n";
    assert_eq!(
        jobs(doctored).iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>(),
        ["gate", "wasm"]
    );
}
