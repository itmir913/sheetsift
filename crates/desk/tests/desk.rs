//! **창이 호출하는 경로가 실제로 작동하는가.**
//!
//! 파일을 판독하고 기록하는 작업은 이 껍질에만 소재하므로, 그것이 실재하는
//! 디스크에서 작동하는지를 여기서 판정한다. 조회의 정오는 바탕과 알맹이의
//! 시험이 판정한다 — 여기서 판정하는 것은 **경로를 수령해 개방하는 위치와,
//! 기록이 절단되지 않는 위치**다.

use std::path::{Path, PathBuf};

use sheetsift_desk::error::Fault;
use sheetsift_desk::{file, Desk};

/// 웹에는 파일 시스템이 부재해 실행 파일 내부에 수용하지만, 여기서는 그 반대다 —
/// **디스크에 실재해야** 판독 경로를 판정할 수 있어서, 수용한 것을 1회 기록한다.
const TINY_XLSX: &[u8] = include_bytes!("../../core/tests/fixtures/tiny.xlsx");

/// 시험마다 상이한 디렉터리. 동시에 실행되므로 명칭이 중복되면 상호 간섭한다.
fn scratch(tag: &str) -> PathBuf {
    let at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("시계가 역행했습니다")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("sheetsift-{tag}-{at}"));
    std::fs::create_dir_all(&dir).expect("임시 디렉터리를 생성하지 못했습니다");
    dir
}

fn code_of(fault: &Fault) -> String {
    let json = serde_json::to_value(fault).expect("오류를 직렬화하지 못했습니다");
    json["code"].as_str().expect("코드가 부재합니다").to_string()
}

/* ── 판독 ────────────────────────────────────────────────────────── */

#[test]
fn a_workbook_opens_from_a_path() {
    let dir = scratch("open");
    let path = dir.join("tiny.xlsx");
    std::fs::write(&path, TINY_XLSX).expect("양식을 기록하지 못했습니다");

    let mut desk = Desk::new();
    desk.open_workbook(&path).expect("양식을 개방하지 못했습니다");

    assert_eq!(desk.name(), Some("tiny.xlsx"));
    let catalog = desk.list_screens(&Default::default()).expect("목록을 수령하지 못했습니다");
    assert!(!catalog.screens.is_empty(), "표가 부재합니다");
}

/// 부재하는 경로는 **중단한다.** 공백 양식으로 개방하면 사용자는 파일이 손상된
/// 것으로 판독한다.
#[test]
fn a_path_that_is_not_there_is_thrown() {
    let dir = scratch("absent");
    let fault = Desk::new()
        .open_workbook(&dir.join("부재.xlsx"))
        .expect_err("부재하는 파일이 개방되었습니다");
    assert_eq!(code_of(&fault), "FILE_READ_FAILED");
}

/// **치환 값에 전체 경로를 기재하지 않는다.** 그 값은 화면에 표시되고 갈무리
/// 되어 반출되는데, 거기에는 사용자 명칭이 포함된다.
#[test]
fn the_error_carries_the_name_and_not_the_path() {
    let dir = scratch("privacy");
    let fault =
        Desk::new().open_workbook(&dir.join("부재.xlsx")).expect_err("개방되었습니다");
    let json = serde_json::to_value(&fault).expect("직렬화하지 못했습니다");
    assert_eq!(json["params"]["name"], "부재.xlsx");
    let text = json["params"].to_string();
    assert!(!text.contains("sheetsift-privacy"), "치환 값에 경로가 혼입되었습니다: {text}");
}

/// 개방하지 못했으면 **직전의 양식을 존치한다.**
#[test]
fn a_failed_open_keeps_the_one_that_is_open() {
    let dir = scratch("keep");
    let path = dir.join("tiny.xlsx");
    std::fs::write(&path, TINY_XLSX).expect("양식을 기록하지 못했습니다");

    let mut desk = Desk::new();
    desk.open_workbook(&path).expect("양식을 개방하지 못했습니다");
    let _ = desk.open_workbook(&dir.join("부재.xlsx")).expect_err("개방되었습니다");

    assert_eq!(desk.name(), Some("tiny.xlsx"));
}

/// 양식이 부재한 상태의 조회는 **중단한다.**
#[test]
fn a_query_without_a_workbook_is_thrown() {
    let fault = Desk::new().list_screens(&Default::default()).expect_err("응답했습니다");
    assert_eq!(code_of(&fault), "NO_WORKBOOK_OPEN");
}

/// 상한은 양식 이전에 도달한다. 기동 시 1회 수령하는 값이다.
#[test]
fn the_limits_answer_before_anything_is_open() {
    assert!(Desk::new().limits().max_page > 0);
}

/* ── 기록 ────────────────────────────────────────────────────────── */

#[test]
fn bytes_land_on_disk() {
    let dir = scratch("write");
    let path = dir.join("표.sift");
    file::write(&path, b"sealed").expect("기록하지 못했습니다");
    assert_eq!(std::fs::read(&path).expect("판독하지 못했습니다"), b"sealed");
}

/// **임시 파일을 존치하지 않는다.** 존치하면 사용자가 인지하지 못하는 파일이
/// 디렉터리에 적재된다.
#[test]
fn nothing_is_left_beside_the_file() {
    let dir = scratch("leftover");
    file::write(&dir.join("표.sift"), b"sealed").expect("기록하지 못했습니다");
    let left: Vec<String> = std::fs::read_dir(&dir)
        .expect("디렉터리를 판독하지 못했습니다")
        .map(|e| e.expect("항목을 판독하지 못했습니다").file_name().to_string_lossy().into_owned())
        .filter(|n| n != "표.sift")
        .collect();
    assert!(left.is_empty(), "잔여물이 존재합니다: {left:?}");
}

/// 기록하지 못하는 경로는 중단한다. 디렉터리를 지시하면 그렇다.
#[test]
fn a_path_that_cannot_be_written_is_thrown() {
    let dir = scratch("unwritable");
    let fault = file::write(&dir, b"sealed").expect_err("기록되었습니다");
    assert_eq!(code_of(&fault), "FILE_WRITE_FAILED");
}

/// **임시 명칭이 인접한 파일을 덮어쓰지 않는다.** 확장자를 교체하는 명명
/// (`표.tmp`)이나 단순 부가(`표.sift.tmp`)는 사용자가 배치한 파일과 중복할 수 있다.
#[test]
fn the_temporary_name_does_not_eat_a_neighbours_file() {
    let dir = scratch("neighbour");
    let neighbours = ["표.tmp", "표.sift.tmp", "표"];
    for n in neighbours {
        std::fs::write(dir.join(n), "사용자가 배치한 파일").expect("인접 파일을 기록하지 못했습니다");
    }
    let target = dir.join("표.sift");
    file::write(&target, "새로 기록한 것".as_bytes()).expect("기록하지 못했습니다");

    for n in neighbours {
        assert_eq!(
            std::fs::read(dir.join(n)).expect("인접 파일을 판독하지 못했습니다"),
            "사용자가 배치한 파일".as_bytes(),
            "인접한 {n} 를 덮어썼습니다"
        );
    }
    assert_eq!(std::fs::read(&target).expect("판독하지 못했습니다"), "새로 기록한 것".as_bytes());
    std::fs::remove_dir_all(&dir).ok();
}

/// **교체하지 못해도 임시 파일을 존치하지 않는다.** 디렉터리를 지시하면 임시
/// 파일은 기록되고 개명에서 중단된다. 치환 값에는 명칭 하나만 기재한다.
#[test]
fn a_failed_write_leaves_nothing_behind() {
    let dir = scratch("failed");
    let blocked = dir.join("점유.sift");
    std::fs::create_dir_all(blocked.join("내부")).expect("디렉터리를 생성하지 못했습니다");

    let fault = file::write(&blocked, b"sealed").expect_err("디렉터리 위에 기록되었습니다");
    let json = serde_json::to_value(&fault).expect("오류를 직렬화하지 못했습니다");
    assert_eq!(json["code"], "FILE_WRITE_FAILED");
    assert_eq!(json["params"]["name"], "점유.sift", "치환 값에 전체 경로가 기재되었습니다");

    let left: Vec<String> = std::fs::read_dir(&dir)
        .expect("디렉터리를 판독하지 못했습니다")
        .map(|e| e.expect("항목을 판독하지 못했습니다").file_name().to_string_lossy().into_owned())
        .filter(|n| n != "점유.sift")
        .collect();
    assert!(left.is_empty(), "잔여물이 존재합니다: {left:?}");
    std::fs::remove_dir_all(&dir).ok();
}

/// 교체 대상이 이미 존재하면 **교체한다.** 재기록하는 경로가 그것이다.
#[test]
fn an_existing_file_is_replaced() {
    let dir = scratch("replace");
    let path = dir.join("표.sift");
    file::write(&path, b"first").expect("기록하지 못했습니다");
    file::write(&path, b"second").expect("재기록하지 못했습니다");
    assert_eq!(std::fs::read(&path).expect("판독하지 못했습니다"), b"second");
}

/* ── 오류의 구조 ────────────────────────────────────────────────── */

/// **바탕의 오류와 이 껍질의 오류가 동일한 구조로 도달하는가.**
///
/// 상이하면 화면이 종류를 분기해 판독하게 되고, 그 분기는 코드가 증가하는 날
/// 한쪽만 수정된다.
#[test]
fn both_kinds_of_fault_look_the_same() {
    let dir = scratch("shape");
    let path = dir.join("훼손.xlsx");
    // zip 의 머리만 보유하고 알맹이가 부재한 바이트. 바탕이 중단시킨다 —
    // 임의의 문자는 csv 로 판독되므로 중단되지 않는다.
    std::fs::write(&path, b"PK broken").expect("기록하지 못했습니다");
    let from_core = Desk::new().open_workbook(&path).expect_err("개방되었습니다");
    let from_shell = file::read(Path::new("")).expect_err("판독되었습니다");

    for fault in [&from_core, &from_shell] {
        let json = serde_json::to_value(fault).expect("직렬화하지 못했습니다");
        assert!(json["code"].is_string(), "코드가 부재합니다: {json}");
        assert!(json["params"].is_object(), "치환 값이 부재합니다: {json}");
    }
    assert_ne!(code_of(&from_core), code_of(&from_shell));
}

/// 이 껍질의 코드와 바탕의 코드가 **중복하지 않는가.** 중복하면 사전의 한 행이
/// 두 사안을 진술하게 되고, 그중 하나는 영구히 부정확하다.
#[test]
fn the_shell_codes_do_not_collide_with_the_backend() {
    const SHELL_RS: &str = include_str!("../src/error.rs");
    const CORE_RS: &str = include_str!("../../core/src/error.rs");

    let ours = variants(SHELL_RS);
    let theirs = variants(CORE_RS);
    assert!(!ours.is_empty() && !theirs.is_empty(), "코드를 탐색하지 못했습니다 — 검사가 실명한 것입니다");
    let both: Vec<&String> = ours.iter().filter(|c| theirs.contains(c)).collect();
    assert!(both.is_empty(), "양측에 동일한 코드가 존재합니다: {both:?}");
}

/// `pub enum ErrorCode { … }` 의 변종 명칭들.
fn variants(src: &str) -> Vec<String> {
    let Some(from) = src.find("pub enum ErrorCode {") else { return Vec::new() };
    let body = &src[from..];
    let to = body.find("\n}").unwrap_or(body.len());
    body[..to]
        .lines()
        .skip(1)
        .map(str::trim)
        .filter(|l| l.ends_with(','))
        .map(|l| l.trim_end_matches(',').to_string())
        .collect()
}

/// **과대한 파일은 판독하지 않고 사유를 진술한다.**
///
/// 상한이 부재하면 할당이 실패하는 시점에 창이 사유 없이 폐쇄된다 — Rust 의
/// 할당 실패는 중단이라 포착할 수단이 부재하다.
#[test]
fn a_file_past_the_limit_says_so_instead_of_dying() {
    let dir = scratch("too_large");
    let path = dir.join("거대.csv");
    // 상한을 1바이트 초과하는 파일. 내용은 판독되지 않으므로 공백으로 충전한다.
    let file = std::fs::File::create(&path).unwrap();
    file.set_len(file::MAX_BYTES + 1).unwrap();
    drop(file);

    let mut desk = Desk::default();
    let err = desk.open_workbook(&path).expect_err("상한을 초과했는데 판독했습니다");
    let json = serde_json::to_value(&err).unwrap();
    assert_eq!(json["code"], "FILE_TOO_LARGE");
    assert_eq!(json["params"]["name"], "거대.csv", "치환 값에 전체 경로가 기재되었습니다");
    assert!(json["params"]["limit"].is_string());

    // 투입한 바이트도 동일한 상한을 경유한다.
    let big = vec![b' '; (file::MAX_BYTES + 1) as usize];
    let err = desk.open_bytes(&big, "투입.csv").expect_err("투입한 바이트에 상한이 부재합니다");
    assert_eq!(serde_json::to_value(&err).unwrap()["code"], "FILE_TOO_LARGE");

    std::fs::remove_dir_all(&dir).ok();
}

/// 그 검사가 실제로 차단하는지 판정한다.
#[test]
fn the_collision_check_catches_a_repeated_code() {
    let doctored = "pub enum ErrorCode {\n    DbQueryFailed,\n}";
    assert_eq!(variants(doctored), ["DbQueryFailed"]);
    assert!(variants(include_str!("../../core/src/error.rs")).contains(&"DbQueryFailed".to_string()));
}

/// 임시 디렉터리를 소거한다. 시험이 중단되면 존치되나, 그것은 임시 디렉터리의
/// 소관이다 — 중단된 시점의 상태를 판독할 수 있는 쪽이 양호하다.
#[test]
fn the_scratch_is_a_real_directory() {
    let dir = scratch("sanity");
    assert!(dir.is_dir());
    std::fs::remove_dir_all(&dir).expect("소거하지 못했습니다");
}
