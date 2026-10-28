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

/// 게시하는 쪽이 부착하는 문구. 여기 수용하는 것으로 **그 파일이 소멸하면
/// 관문이 정지한다** — 소멸을 태그 시점에 인지하면 1회를 소모한다.
const NOTES: &str = include_str!("../../../.github/release-notes.md");

/// **반출하는 잡들과, 그 잡을 탐색한 것을 입증하는 표지.**
///
/// 관문이 아니다 — 검사하지 않고 생성하거나 송출한다. 그래서 관문 목록에서
/// 제외하고, 대신 **조각 전부를 대기하는지**를 여기 등재된 잡마다 판정한다.
///
/// 표지는 그 잡의 문서에 출현해야 하는 글자다. 명칭만 탐색하면 잡을 개칭한
/// 시점에 **판정 대상이 소거된 채로 통과**한다 — 표지가 부재하면 실명한 것으로
/// 간주해 정지시킨다.
///
/// 잡을 하나 증가시키면 여기 한 행을 증가시킨다. 누락한 것은
/// `every_job_is_either_a_gate_or_registered_as_outbound` 가 차단한다 — 조각을
/// 호출하지도 않고 여기 미등재인 잡은 **이 파일의 어떤 판정에도 포착되지
/// 않으므로**, 그 상태 자체를 불통과로 판정한다.
const OUTBOUND: &[(&str, &str)] = &[
    // 화면을 송출한다.
    ("pages", "actions/deploy-pages"),
    // 데스크톱을 굽는다.
    ("bake", "npm run bake"),
    // 구워진 것을 게시한다.
    ("ship", "gh release create"),
];

/// 반출하는 잡인가.
fn outbound(name: &str) -> bool {
    OUTBOUND.iter().any(|(n, _)| *n == name)
}

/// 관문 잡에 기재하면 안 되는 도구들.
///
/// 조각을 호출하는 대신 검사를 직접 기재하면 두 벌이 되고, 한쪽만 수정된다.
/// `package.json` 의 조각이 실행하는 것이 전부 여기 포함되어야 한다 — 신규
/// 도구를 조각에 도입하면 이 목록에도 한 행을 증가시킨다.
const CHECK_TOOLS: &[&str] = &[
    "cargo test",
    "cargo clippy",
    "cargo fmt",
    "vue-tsc",
    "eslint",
    "npm run check",
    "npm run build",
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
            // **주석은 잡의 문서가 아니다.**
            //
            // 잡 하단의 판정들은 글자를 탐색한다. 그런데 `bake:` 상단의 주석은
            // 그 직전의 잡(`pages`)에 부착되므로, 주석을 수용하면 **산문으로
            // 판정을 충족시킬 수 있다** — 태그로 한정하는 `if` 를 삭제해도
            // 상단의 주석이 `refs/tags/` 를 기재하고 있으면 통과한다.
            continue;
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
        .filter(|(n, b)| !outbound(n) && !parts_called(b).is_empty())
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

/// 한 잡이 대기하지 않은 관문 잡들.
fn unwaited(block: &str, gates: &[String]) -> Vec<String> {
    let waits = needs(block);
    gates.iter().filter(|n| !waits.contains(n)).cloned().collect()
}

/// **반출하는 잡이 조각 전부를 대기하는가.**
///
/// `needs` 한 행이 그것을 보전한다. 관문 잡을 하나 증가시키고 그 행을 증가시키지
/// 않으면, **그 검사가 중단되는 판이 그대로 반출되고 배포는 정상 성공한다.**
///
/// **타 잡을 경유한 대기는 산정하지 않는다.** 이 검사는 글자를 판독하므로
/// `ship` 이 `bake` 를 대기하고 `bake` 가 관문을 대기하는 것을 추적할 수 없다 —
/// 추적하는 것으로 가장하면 `bake` 의 그 행이 소거된 시점에 묵시적으로 통과한다.
/// 그래서 반출하는 잡은 **관문 잡 전부를 자기 행에 기재해야 한다.**
#[test]
fn what_goes_out_has_passed_every_part() {
    let all = jobs(ACTION);
    let gates = gate_jobs(ACTION);
    assert!(!gates.is_empty(), "관문 잡을 탐색하지 못했습니다 — 검사가 실명한 것입니다");

    for (name, marker) in OUTBOUND {
        let job = all
            .iter()
            .find(|(n, _)| n == name)
            .unwrap_or_else(|| panic!("`{name}` 잡을 탐색하지 못했습니다 — 검사가 실명한 것입니다"));
        assert!(
            job.1.contains(marker),
            "`{name}` 에서 `{marker}` 를 탐색하지 못했습니다 — 검사가 실명한 것입니다"
        );

        let missing = unwaited(&job.1, &gates);
        assert!(missing.is_empty(), "`{name}` 이 대기하지 않는 관문 잡이 존재합니다: {missing:?}");
    }
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
    let gates = gate_jobs(doctored);
    assert_eq!(gates, ["gate", "wasm"]);
    let all = jobs(doctored);
    let deploy = all.iter().find(|(n, _)| n == "pages").expect("pages 가 부재합니다");
    assert_eq!(unwaited(&deploy.1, &gates), ["wasm"]);
}

/// **타 반출 잡을 경유해 대기한 것을 통과시키지 않는가.**
///
/// `ship` 이 `bake` 하나만 대기하면 GitHub 은 그것으로 관문을 경과시킨다. 그러나
/// `bake` 의 행이 축소되는 시점에 `ship` 은 아무것도 대기하지 않게 되고, 글자를
/// 판독하는 이 검사는 그 경유를 추적할 수 없다. 그래서 경유를 불통과로 판정하는지
/// 확인한다.
#[test]
fn the_check_catches_a_ship_that_waits_only_through_the_bake() {
    let doctored = concat!(
        "\njobs:\n",
        "  gate:\n    steps:\n      - run: npm run ci:gate\n",
        "  wasm:\n    steps:\n      - run: npm run ci:wasm\n",
        "  bake:\n    needs: [gate, wasm]\n    steps:\n      - run: npx @tauri-apps/cli build\n",
        "  ship:\n    needs: [bake]\n    steps:\n      - run: gh release create x\n"
    );
    let gates = gate_jobs(doctored);
    assert_eq!(gates, ["gate", "wasm"]);
    let all = jobs(doctored);
    let bake = all.iter().find(|(n, _)| n == "bake").expect("bake 가 부재합니다");
    assert_eq!(unwaited(&bake.1, &gates), Vec::<String>::new());
    let ship = all.iter().find(|(n, _)| n == "ship").expect("ship 가 부재합니다");
    assert_eq!(unwaited(&ship.1, &gates), ["gate", "wasm"]);
}

/// **표지를 탐색하지 못하면 실명한 것으로 판정하는가.**
///
/// 반출하는 잡을 개칭하면 `OUTBOUND` 의 명칭은 어떤 잡과도 합치하지 않는다. 그
/// 시점에 판정 대상이 소거된 채로 통과하면 안 된다 — 표지가 그것을 차단한다.
#[test]
fn the_check_catches_an_outbound_job_that_lost_its_marker() {
    let doctored = "\njobs:\n  ship:\n    needs: [gate]\n    steps:\n      - run: echo nothing\n";
    let all = jobs(doctored);
    let ship = all.iter().find(|(n, _)| n == "ship").expect("ship 가 부재합니다");
    let marker = OUTBOUND.iter().find(|(n, _)| *n == "ship").expect("ship 가 미등재입니다").1;
    assert!(!ship.1.contains(marker));
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

/* ── 조각과 잡이 분기하지 않는가 ────────────────────────────────── */

/// 관문을 담당하는 잡들의 문서. 반출하는 잡은 관문이 아니다.
fn gate_blocks(src: &str) -> Vec<(String, String)> {
    jobs(src).into_iter().filter(|(n, _)| !outbound(n)).collect()
}

/// **`npm run ci` 가 부재하는 조각을 호출하지 않는가.**
///
/// 조각의 명칭을 오기하면 `npm` 이 그 시점에 중단하므로 입력한 사람은 인지한다.
/// 그러나 **기존 조각 하나를 오기로 치환**하면, 그 조각이 호출되지 않는 채로
/// 남은 조각들이 전부 통과한다 — 액션은 전부 실행하므로 전송한 이후에야
/// 발현된다.
#[test]
fn the_one_command_calls_nothing_that_is_not_a_part() {
    let declared = parts_declared(PACKAGE);
    let called = parts_called(script_body(PACKAGE, "ci").expect("`ci` 스크립트가 부재합니다"));
    let unknown: Vec<String> = called.into_iter().filter(|p| !declared.contains(p)).collect();
    assert!(unknown.is_empty(), "`npm run ci` 가 부재하는 조각을 호출합니다: {unknown:?}");
}

/// **조각마다 관문 잡이 존재하는가.**
///
/// 액션 전체를 판독하면, 배포하는 잡이 호출하기만 해도 통과한다. 그 잡은
/// `main` 에서만 작동하므로 병합을 요청하는 판은 **그 조각을 경과하지 않는다.**
#[test]
fn every_part_runs_in_a_gate_job() {
    let in_gates: Vec<String> =
        gate_blocks(ACTION).into_iter().flat_map(|(_, b)| parts_called(&b)).collect();
    let missing: Vec<String> =
        parts_declared(PACKAGE).into_iter().filter(|p| !in_gates.contains(p)).collect();
    assert!(missing.is_empty(), "관문 잡이 실행하지 않는 조각이 존재합니다: {missing:?}");
}

/// **관문 잡이 검사를 직접 기재하지 않는가.**
///
/// `cargo test` 만 차단하면 타 도구로 치환된 시점에 통과한다 — 화면의 타입
/// 검사를 액션에 직접 기재하면 `package.json` 의 조각과 두 벌이 되고, 로컬에서
/// 입력하는 명령과 반출되는 것이 상이한 행을 경유한다.
#[test]
fn a_gate_job_spells_out_no_check_of_its_own() {
    let mut spelled: Vec<String> = Vec::new();
    for (name, block) in gate_blocks(ACTION) {
        for line in block.lines().map(str::trim) {
            if line.starts_with('#') || line.contains("npm run ci:") {
                continue;
            }
            if let Some(tool) = CHECK_TOOLS.iter().find(|t| line.contains(**t)) {
                spelled.push(format!("{name}: `{tool}`"));
            }
        }
    }
    assert!(
        spelled.is_empty(),
        "관문 잡이 검사를 직접 기재했습니다 — package.json 의 조각을 호출하십시오: {spelled:?}"
    );
}

/// 그 판정들이 실제로 차단하는지 확인한다.
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

#[test]
fn the_check_catches_a_part_that_only_the_deploy_runs() {
    let doctored = concat!(
        "\njobs:\n",
        "  gate:\n    steps:\n      - run: npm run ci:gate\n",
        "  pages:\n    needs: [gate]\n    steps:\n      - run: npm run ci:web\n",
        "      - uses: actions/deploy-pages@v5\n"
    );
    let in_gates: Vec<String> =
        gate_blocks(doctored).into_iter().flat_map(|(_, b)| parts_called(&b)).collect();
    assert_eq!(in_gates, ["ci:gate"]);
    assert!(!in_gates.contains(&"ci:web".to_string()));
}

#[test]
fn the_check_catches_a_type_check_written_into_a_gate_job() {
    let doctored = concat!(
        "\njobs:\n",
        "  web:\n    steps:\n      - run: npx vue-tsc -b\n",
        "  pages:\n    needs: [web]\n    steps:\n      - uses: actions/deploy-pages@v5\n"
    );
    let mut spelled: Vec<String> = Vec::new();
    for (name, block) in gate_blocks(doctored) {
        for line in block.lines().map(str::trim) {
            if line.starts_with('#') || line.contains("npm run ci:") {
                continue;
            }
            if let Some(tool) = CHECK_TOOLS.iter().find(|t| line.contains(**t)) {
                spelled.push(format!("{name}: `{tool}`"));
            }
        }
    }
    assert_eq!(spelled, ["web: `vue-tsc`"]);
}

/* ── 반출하는 잡이 전부 판별되었는가 ────────────────────────────── */

/// **잡은 관문이거나 반출하는 것이거나 둘 중 하나다.**
///
/// `gate_jobs` 는 조각을 호출하는 잡을 관문으로 판별한다. 그래서 조각을 호출하지
/// 않고 `OUTBOUND` 에도 미등재인 잡은 **이 파일의 어떤 판정에도 포착되지
/// 않는다** — 대기하는 행이 전무해도 녹색이다. 반출하는 잡이 그 부류라, 가장
/// 수호해야 하는 것이 누락된다.
#[test]
fn every_job_is_either_a_gate_or_registered_as_outbound() {
    let all = jobs(ACTION);
    assert!(!all.is_empty(), "워크플로에서 잡을 탐색하지 못했습니다 — 검사가 실명한 것입니다");
    let stray: Vec<String> = all
        .into_iter()
        .filter(|(n, b)| !outbound(n) && parts_called(b).is_empty())
        .map(|(n, _)| n)
        .collect();
    assert!(
        stray.is_empty(),
        "관문도 아니고 `OUTBOUND` 에 미등재인 잡이 존재합니다 — 등재하십시오: {stray:?}"
    );
}

/// 미등재 잡을 투입한다.
#[test]
fn the_check_catches_a_job_that_is_neither() {
    let doctored = concat!(
        "\njobs:\n",
        "  gate:\n    steps:\n      - run: npm run ci:gate\n",
        "  publish:\n    steps:\n      - run: cargo publish\n"
    );
    let stray: Vec<String> = jobs(doctored)
        .into_iter()
        .filter(|(n, b)| !outbound(n) && parts_called(b).is_empty())
        .map(|(n, _)| n)
        .collect();
    assert_eq!(stray, ["publish"]);
}

/* ── 반출을 태그가 기동하는가 ───────────────────────────────────── */

/// **반출하는 잡은 태그로 한정된다.**
///
/// 그 행이 소거되면 분기에 푸시하는 것마다 화면이 송출되고 앱이 구워진다 —
/// 그것은 오류로 출현하지 않는다. 정상 성공하는 채로 반출하는 사유가 소멸할
/// 뿐이다.
#[test]
fn what_goes_out_starts_only_on_a_tag() {
    let all = jobs(ACTION);
    for (name, _) in OUTBOUND {
        let job = all
            .iter()
            .find(|(n, _)| n == name)
            .unwrap_or_else(|| panic!("`{name}` 잡을 탐색하지 못했습니다 — 검사가 실명한 것입니다"));
        assert!(job.1.contains("refs/tags/"), "`{name}` 이 태그로 한정되지 않았습니다");
    }
}

/// 한정하지 않은 잡을 투입한다.
#[test]
fn the_check_catches_an_outbound_job_that_any_push_starts() {
    let doctored = "\njobs:\n  pages:\n    if: github.ref == 'refs/heads/main'\n    steps:\n      - uses: actions/deploy-pages@v5\n";
    let all = jobs(doctored);
    let job = all.iter().find(|(n, _)| n == "pages").expect("pages 가 부재합니다");
    assert!(!job.1.contains("refs/tags/"));
}

/* ── 산출물의 명칭이 한 벌인가 ──────────────────────────────────── */

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

/// 게시하는 잡이 존재를 확인하는 조각들 (`for want in …`).
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
    let made = asset_names(ACTION);
    assert!(
        !made.is_empty(),
        "워크플로에서 산출물의 명칭을 탐색하지 못했습니다 — 검사가 실명한 것입니다"
    );
    let told = asset_names(NOTES);
    assert!(
        !told.is_empty(),
        "게시 문구에서 산출물의 명칭을 탐색하지 못했습니다 — 검사가 실명한 것입니다"
    );
    assert_eq!(made, told, "구운 것과 게시 문구가 기재한 명칭이 상이합니다");
    assert!(NOTES.contains("{version}"), "게시 문구에 판을 기재하는 자리가 부재합니다");

    // 게시 직전의 확인도 동일한 명칭을 가리켜야 한다.
    let wants = wants(ACTION);
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

/// 한쪽만 개칭한 것을 투입한다.
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

/* ── 부재하는 스크립트를 호출하지 않는가 ────────────────────────── */

/// 임의의 문서가 **뿌리의** `package.json` 에서 호출하는 스크립트들.
///
/// `working-directory` 를 기재한 단계는 제외한다 — 그 단계의 `npm run build` 는
/// `web/package.json` 의 것이고, 뿌리에는 부재하는 것이 정상이다.
///
/// **단계를 적립해서 판정한다.** 행을 순차 판정하면 `working-directory` 가 `run`
/// 보다 하단에 기재된 단계를 뿌리로 오판하므로, 단계가 종료된 시점에 그 단계
/// 전체를 판독한다 — 기재 순서를 가정하지 않는다.
///
/// 주석은 산정하지 않는다. 호출하는 위치만 판별한다.
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
        if t.starts_with("- ") && (t.contains(':')) {
            take(&step, &mut out);
            step.clear();
        }
        step.push_str(line);
        step.push('\n');
    }
    take(&step, &mut out);
    out
}

/// **액션이 부재하는 스크립트를 호출하지 않는가.**
///
/// 조각(`ci:…`)은 이미 판정한다. 그런데 굽는 잡은 조각이 아닌 스크립트를
/// 호출하고(`bake`), 그 명칭을 오기하거나 `package.json` 에서 개칭하면 **태그를
/// 1회 소모한 이후에** 발현된다.
#[test]
fn the_action_calls_no_script_that_is_not_declared() {
    let declared = script_names(PACKAGE);
    let called = scripts_called(ACTION);
    assert!(
        !called.is_empty(),
        "액션에서 스크립트를 호출하는 위치를 탐색하지 못했습니다 — 검사가 실명한 것입니다"
    );
    let unknown: Vec<String> = called.into_iter().filter(|s| !declared.contains(s)).collect();
    assert!(unknown.is_empty(), "package.json 에 부재하는 스크립트를 액션이 호출합니다: {unknown:?}");
}

/// 오기한 액션을 투입한다.
#[test]
fn the_check_catches_a_misspelled_script_in_the_action() {
    let doctored = "      - run: npm run bkae -- --bundles nsis\n";
    assert_eq!(scripts_called(doctored), ["bkae"]);
    assert!(!script_names(PACKAGE).contains(&"bkae".to_string()));
}

/// 타 그릇의 스크립트를 뿌리의 것으로 오판하지 않는가. 그리고 그 제외가
/// 과도해 **전부를 소거하지는 않는가** — 소거하면 이 판정은 실명한다.
#[test]
fn the_check_tells_the_root_scripts_from_the_screens_own() {
    let doctored = concat!(
        "      - name: Build the screen\n",
        "        working-directory: web\n",
        "        run: npm run build\n",
        "      - name: Bake\n",
        "        run: npm run bake -- --bundles nsis\n"
    );
    assert_eq!(scripts_called(doctored), ["bake"]);
}

/// 기재 순서를 가정하지 않는가. `working-directory` 가 `run` 보다 하단에
/// 기재되어도 타 그릇의 것으로 판별해야 한다.
#[test]
fn the_check_tells_them_apart_whatever_order_the_step_is_written_in() {
    let doctored = concat!(
        "      - name: Build the screen\n",
        "        run: npm run build\n",
        "        working-directory: web\n",
        "      - name: Bake\n",
        "        run: npm run bake -- --bundles nsis\n"
    );
    assert_eq!(scripts_called(doctored), ["bake"]);
}

/// 주석이 잡의 문서에 수용되지 않는가.
///
/// `bake:` 상단의 주석은 그 직전의 잡에 부착된다. 수용하면 태그로 한정하는
/// `if` 를 삭제해도 **상단의 주석이 판정을 충족시킨다.**
#[test]
fn the_check_does_not_let_a_comment_stand_in_for_a_job() {
    let doctored = concat!(
        "\njobs:\n",
        "  pages:\n    if: github.ref == 'refs/heads/main'\n    steps:\n      - uses: actions/deploy-pages@v5\n",
        "  # 반출은 refs/tags/ 가 기동한다\n",
        "  bake:\n    steps:\n      - run: npm run bake\n"
    );
    let all = jobs(doctored);
    let pages = all.iter().find(|(n, _)| n == "pages").expect("pages 가 부재합니다");
    assert!(
        !pages.1.contains("refs/tags/"),
        "상단의 주석이 `pages` 의 문서에 수용되었습니다 — 산문으로 판정이 충족됩니다"
    );
    assert_eq!(all.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>(), ["pages", "bake"]);
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
/// 판을 고정하지 않은 도구는 아무것도 변경하지 않은 날에 변동한다. 그래서
/// 고정하는데, 고정한 값이 두 곳에 기재되면 **한쪽만 상승한다** — 그 시점부터
/// 구워지는 것과 창을 개방해 판독하는 것이 상이한 도구를 경유하고, 그것은
/// 어디에서도 오류로 출현하지 않는다.
#[test]
fn the_tool_is_pinned_to_one_version() {
    let found = cli_versions(PACKAGE);
    assert!(
        !found.is_empty(),
        "`package.json` 에서 도구의 판을 탐색하지 못했습니다 — 검사가 실명한 것입니다"
    );
    // **판을 전치하지 않은 위치까지 산정한다.** `cli_versions` 는 `@` 가 후속하는
    // 것만 탐색하므로, `@` 자체가 부재한 `npx @tauri-apps/cli build` 는 계수되지
    // 않은 채로 통과한다 — 고정하지 않은 것이 그 형태다.
    assert_eq!(
        PACKAGE.matches("@tauri-apps/cli").count(),
        found.len(),
        "판을 고정하지 않고 도구를 호출하는 위치가 존재합니다"
    );
    assert!(found.iter().all(|v| !v.is_empty()), "판을 고정하지 않은 위치가 존재합니다: {found:?}");
    let first = &found[0];
    assert!(found.iter().all(|v| v == first), "도구의 판이 복수입니다: {found:?}");
}

/// **액션이 도구의 판을 기재하지 않는가.**
///
/// 기재하면 `package.json` 의 것과 두 벌이 되고, 반출되는 것과 평시 입력하는 것이
/// 상이한 도구를 경유한다.
#[test]
fn the_action_pins_no_version_of_its_own() {
    assert!(
        !ACTION.contains("@tauri-apps/cli@"),
        "액션이 도구의 판을 직접 기재했습니다 — `package.json` 의 스크립트를 호출하십시오"
    );
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

/* ── 태그가 단독으로 기동하지 않는가 ────────────────────────────── */

/// 전원 하나가 기재한 문서 (`on:` 하단의 2칸 들여쓴 명칭).
///
/// `jobs` 와 동일한 판독이다 — 행을 글자로 탐색하지 않고 들여쓰기로 산정한다.
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

/// **태그를 푸시하는 것이 단독으로 반출을 기동하지 않는가.**
///
/// 반출하는 잡은 수동 기동과 태그를 동시에 요구한다. 그런데 `on.push` 에
/// `tags:` 를 기재하면 **태그를 푸시하는 순간 그 조건이 충족되고**, 판을 표식하려
/// 붙인 태그가 그대로 게시된다 — 아무것도 그것을 진술하지 않는다. 그 한 행이
/// 소재하지 않는 것이 수호 대상이다.
#[test]
fn a_pushed_tag_starts_nothing_on_its_own() {
    let push = trigger(ACTION, "push").expect("`push` 전원을 탐색하지 못했습니다 — 검사가 실명한 것입니다");
    assert!(
        push.contains("branches:"),
        "`push` 가 분기를 기재하지 않았습니다 — 검사가 실명한 것입니다"
    );
    assert!(
        !push.contains("tags:"),
        "`on.push` 가 태그를 기재했습니다 — 태그를 푸시하는 것만으로 반출됩니다"
    );
    assert!(
        trigger(ACTION, "workflow_dispatch").is_some(),
        "수동 기동을 기재하지 않았습니다 — 반출할 수단이 부재합니다"
    );
}

/// **반출하는 잡이 수동 기동까지 요구하는가.**
///
/// 태그만 판정하면 `on.push` 에 `tags:` 가 재기재되는 시점에 자동으로 반출된다.
/// 두 조건을 양측에서 수호한다.
#[test]
fn what_goes_out_requires_a_hand() {
    let all = jobs(ACTION);
    for (name, _) in OUTBOUND {
        let job = all
            .iter()
            .find(|(n, _)| n == name)
            .unwrap_or_else(|| panic!("`{name}` 잡을 탐색하지 못했습니다 — 검사가 실명한 것입니다"));
        assert!(
            job.1.contains("github.event_name == 'workflow_dispatch'"),
            "`{name}` 이 수동 기동을 요구하지 않습니다"
        );
    }
}

/// 태그를 기재한 전원을 투입한다.
#[test]
fn the_check_catches_a_tag_filter_that_came_back() {
    let doctored = concat!(
        "name: ci\n",
        "on:\n",
        "  push:\n    branches: ['**']\n    tags: ['[0-9]+.[0-9]+.[0-9]+']\n",
        "  workflow_dispatch:\n",
        "jobs:\n",
    );
    let push = trigger(doctored, "push").expect("push 가 부재합니다");
    assert!(push.contains("tags:"));
    assert!(trigger(doctored, "workflow_dispatch").is_some());
    assert!(trigger(doctored, "pull_request").is_none());
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
}
