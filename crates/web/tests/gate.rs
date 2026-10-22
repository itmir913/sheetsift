//! **관문은 한 명령이다.**
//!
//! 사람은 `npm run ci` 하나를 친다. 그 명령은 조각들(`ci:gate`·`ci:wasm`·
//! `ci:web`)을 차례로 부르고, 액션은 같은 조각을 잡 하나씩 맡아 한꺼번에
//! 돌린다. 어느 쪽도 검사를 글자로 따로 적지 않는다.
//!
//! 적었더니 한쪽만 고쳐졌다. 그때부터 나가는 것과 평소 보는 것이 **다른 줄을
//! 지난다** — 그리고 그것은 어디에서도 오류로 나타나지 않는다. 그래서 둘을
//! 여기서 붙들어 둔다.
//!
//! - 조각을 하나 늘리고 액션을 안 고치면 선다.
//! - 액션이 조각 하나를 빼먹으면 선다.
//! - `npm run ci` 가 조각 전부를 부르지 않으면 선다.
//! - 배포가 조각 하나라도 안 기다리면 선다.
//!
//! **이 검사는 글자를 읽는다.** `package.json` 도 워크플로도 `cargo test` 가
//! 돌려 볼 수 있는 것이 아니다. 그래서 **지키는 대상을 찾지 못하면 통과가
//! 아니라 실패고**, 일부러 어긋낸 입력으로 정말 막는지 함께 본다.

const ACTION: &str = include_str!("../../../.github/workflows/ci.yml");
const PACKAGE: &str = include_str!("../../../package.json");

/// 배포하는 잡. 관문이 아니라 나가는 자리다.
const DEPLOY_JOB: &str = "pages";

/* ── 읽어 내기 ───────────────────────────────────────────────────── */

/// `package.json` 의 `"<이름>":` 로 적힌 스크립트 이름들.
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

/// 한 스크립트가 적어 둔 줄.
fn script_body<'a>(src: &'a str, name: &str) -> Option<&'a str> {
    let at = src.find(&format!("\"{name}\":"))?;
    let after = &src[at + name.len() + 3..];
    let from = after.find('"')? + 1;
    let len = after[from..].find('"')?;
    Some(&after[from..from + len])
}

/// 관문의 조각들(`ci:<무엇>`). `ci` 자신은 조각이 아니다.
fn parts_declared(src: &str) -> Vec<String> {
    script_names(src).into_iter().filter(|n| n.starts_with("ci:")).collect()
}

/// 어떤 글에서 `npm run ci:<무엇>` 으로 불리는 조각들.
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

/// 워크플로의 잡들. 이름과 그 잡이 품은 글.
///
/// 잡은 `jobs:` 아래 **두 칸** 들여 쓴 이름이다. 더 깊이 들어간 줄은 그 잡의
/// 속이라 다음 잡이 나올 때까지 함께 담는다.
///
/// **줄을 글자로 찾지 않는다.** `include_str!` 은 파일을 있는 그대로 담으므로
/// 윈도에서는 줄 끝이 `\r\n` 이다. `"\njobs:\n"` 을 찾으면 거기서는 영영 못
/// 찾고, 그러면 이 검사는 **아무 잡도 못 본 채 할 말이 없어진다.** 한 번 그렇게
/// 났다 — `lines()` 는 `\r` 을 떼 주므로 그것으로 센다.
fn jobs(src: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let mut inside = false;
    for line in src.lines() {
        if !inside {
            inside = line.trim_end() == "jobs:";
            continue;
        }
        // 들여쓰지 않은 줄이 나오면 `jobs:` 밖이다.
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

/// 한 잡이 기다리는 잡들 (`needs: [a, b]`).
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

/// 관문을 맡은 잡들. 조각을 하나라도 부르는 잡이 그것이다.
fn gate_jobs(src: &str) -> Vec<String> {
    jobs(src)
        .into_iter()
        .filter(|(n, b)| n != DEPLOY_JOB && !parts_called(b).is_empty())
        .map(|(n, _)| n)
        .collect()
}

/* ── 눈이 멀지 않았는가 ──────────────────────────────────────────── */

#[test]
fn the_sources_are_there_to_look_at() {
    assert!(
        !parts_declared(PACKAGE).is_empty(),
        "package.json 에서 관문의 조각을 찾지 못했습니다 — 검사가 눈먼 것입니다"
    );
    assert!(
        script_body(PACKAGE, "ci").is_some(),
        "`ci` 스크립트를 찾지 못했습니다 — 검사가 눈먼 것입니다"
    );
    assert!(!jobs(ACTION).is_empty(), "워크플로에서 잡을 찾지 못했습니다 — 검사가 눈먼 것입니다");
    assert!(
        !parts_called(ACTION).is_empty(),
        "워크플로에서 조각을 부르는 자리를 찾지 못했습니다 — 검사가 눈먼 것입니다"
    );
}

/* ── 한 명령이 전부인가 ──────────────────────────────────────────── */

/// **`npm run ci` 가 조각 전부를 부르는가.**
///
/// 하나를 빼먹으면 사람이 치는 한 명령만 조용히 덜 보게 된다. 액션은 그대로
/// 다 돌아 초록불이라, 그 차이는 커밋을 밀어 넣고 나서야 드러난다.
#[test]
fn the_one_command_runs_every_part() {
    let called = parts_called(script_body(PACKAGE, "ci").expect("`ci` 스크립트가 없습니다"));
    let missing: Vec<String> =
        parts_declared(PACKAGE).into_iter().filter(|p| !called.contains(p)).collect();
    assert!(missing.is_empty(), "`npm run ci` 가 부르지 않는 조각이 있습니다: {missing:?}");
}

/// **조각마다 돌리는 자리가 액션에 있는가.**
///
/// 조각을 늘려 놓고 액션을 안 고치면 그 검사는 이 컴퓨터에서만 돈다 — 그리고
/// 이 컴퓨터에서 못 도는 조각(wasm)이면 **아무 데서도 돌지 않는다.**
#[test]
fn the_action_runs_every_part() {
    let in_action = parts_called(ACTION);
    let missing: Vec<String> =
        parts_declared(PACKAGE).into_iter().filter(|p| !in_action.contains(p)).collect();
    assert!(missing.is_empty(), "액션이 돌리지 않는 조각이 있습니다: {missing:?}");
}

/// 액션이 **없는 조각**을 부르지 않는가. 이름을 바꾼 날 옛 이름이 남으면 그
/// 잡은 `npm` 이 모르는 스크립트를 부르며 선다 — 늦게, 엉뚱한 말로.
#[test]
fn the_action_calls_nothing_that_is_not_a_part() {
    let declared = parts_declared(PACKAGE);
    let extra: Vec<String> =
        parts_called(ACTION).into_iter().filter(|p| !declared.contains(p)).collect();
    assert!(extra.is_empty(), "package.json 에 없는 조각을 액션이 부릅니다: {extra:?}");
}

/// **검사를 액션에 손으로 적지 않는가.**
///
/// 조각을 부르는 대신 `cargo test` 를 거기 직접 적으면 두 벌이 된다. 그러면
/// 한쪽만 고쳐지고, 나가는 것과 평소 보는 것이 다른 줄을 지난다.
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
        "액션이 검사를 직접 적었습니다 — package.json 의 조각을 부르세요: {spelled:?}"
    );
}

/* ── 나가는 것이 관문을 지났는가 ─────────────────────────────────── */

/// **배포가 조각 전부를 기다리는가.**
///
/// `needs` 한 줄이 그것을 지킨다. 관문 잡을 하나 늘리고 그 줄을 안 늘리면,
/// **그 검사가 서는 판이 그대로 올라가고 배포는 멀쩡히 성공한다.**
#[test]
fn what_goes_out_has_passed_every_part() {
    let all = jobs(ACTION);
    let deploy = all
        .iter()
        .find(|(n, _)| n == DEPLOY_JOB)
        .unwrap_or_else(|| panic!("`{DEPLOY_JOB}` 잡을 찾지 못했습니다 — 검사가 눈먼 것입니다"));
    assert!(
        deploy.1.contains("actions/deploy-pages"),
        "배포하는 자리를 찾지 못했습니다 — 검사가 눈먼 것입니다"
    );

    let gates = gate_jobs(ACTION);
    assert!(!gates.is_empty(), "관문 잡을 찾지 못했습니다 — 검사가 눈먼 것입니다");

    let waits = needs(&deploy.1);
    let unwaited: Vec<&String> = gates.iter().filter(|n| !waits.contains(n)).collect();
    assert!(unwaited.is_empty(), "배포가 기다리지 않는 관문 잡이 있습니다: {unwaited:?}");
}

/* ── 그 검사들이 정말 막는지 ─────────────────────────────────────── */

/// 조각 하나를 빼먹은 `ci` 를 먹여 본다.
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
    let called = parts_called(script_body(doctored, "ci").expect("`ci` 가 없습니다"));
    assert_eq!(called, ["ci:gate"]);
}

/// 조각 하나를 빼먹은 액션을 먹여 본다.
#[test]
fn the_check_catches_an_action_that_skips_a_part() {
    let doctored = "      - run: npm run ci:gate\n";
    assert_eq!(parts_called(doctored), ["ci:gate"]);
}

/// 검사를 손으로 적어 둔 액션을 먹여 본다.
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

/// 조각 하나를 안 기다리는 배포를 먹여 본다.
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
    let deploy = all.iter().find(|(n, _)| n == DEPLOY_JOB).expect("pages 가 없습니다");
    assert_eq!(needs(&deploy.1), ["gate"]);
}

/// **줄 끝이 `\r\n` 이어도 읽는가.**
///
/// `include_str!` 은 파일을 있는 그대로 담는다. 윈도에서 체크아웃하면 줄 끝이
/// `\r\n` 이라, 줄을 글자로 찾던 때는 **아무 잡도 못 찾고** 검사가 "눈먼 것
/// 입니다" 를 외쳤다. 러너(리눅스)에서는 멀쩡해서, 고치는 사람만 선다.
#[test]
fn the_check_reads_a_workflow_with_windows_line_endings() {
    let crlf = "name: ci\r\njobs:\r\n  gate:\r\n    steps:\r\n      - run: npm run ci:gate\r\n";
    assert_eq!(jobs(crlf).iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>(), ["gate"]);
    assert_eq!(gate_jobs(crlf), ["gate"]);
}

/// 잡을 가르는 눈이 멀지 않았는가. 들여쓰기만 보고 자르므로, 속의 `with:` 를
/// 잡으로 세면 관문 잡 목록이 엉뚱해진다.
#[test]
fn the_check_tells_a_job_from_what_is_inside_one() {
    let doctored = "\njobs:\n  gate:\n    steps:\n      - uses: x\n        with:\n          key: gate\n  wasm:\n    steps:\n      - run: npm run ci:wasm\n";
    assert_eq!(
        jobs(doctored).iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>(),
        ["gate", "wasm"]
    );
}
