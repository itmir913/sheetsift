//! **창이 호출하는 문.** 명칭과 요청의 항목 명칭은 브라우저 껍질의 것과 동일한
//! 벌이다(`sheetsift-wasm`).
//!
//! 여기 소재하는 것은 상태를 확보해 [`Desk`] 를 호출하는 행뿐이다. 분기도
//! 변환도 배치하지 않는다 — 배치하면 창이 실재해야 판정 가능한 코드가 그만큼
//! 증가한다.
//!
//! 반환하는 오류는 [`Fault`](sheetsift_desk::Fault) 그대로다. 구조가 바탕의
//! 것과 동일해(`code`·`params`·`detail`) 화면은 단일 사전으로 문구를 부여한다.

use std::path::PathBuf;
use std::sync::Mutex;

use sheetsift_desk::{Desk, Result};
use sheetsift_desk::grid::{ColumnValues, Page};
use sheetsift_shell::{Ask, CatalogAsk, Limits, ScreenCatalog, ValuesAsk};
use tauri::State;

/// 창이 보유하는 상태. 명령이 복수의 스레드에서 도달한다.
type Held<'a> = State<'a, Mutex<Desk>>;

/// **잠금이 훼손되었으면 중단한다.** 타 명령이 패닉한 상태를 그대로 경과시키면,
/// 무엇이 개방되어 있는지 인지하지 못한 채 조회가 진행된다.
fn held<'a>(desk: &'a Held<'a>) -> std::sync::MutexGuard<'a, Desk> {
    desk.lock().expect("상태가 훼손되었습니다")
}

/// 선택 대화상자가 반환한 경로의 양식을 개방한다. **바이트를 경유시키지 않는다.**
#[tauri::command]
pub fn open_workbook(desk: Held, path: PathBuf) -> Result<()> {
    held(&desk).open_workbook(&path)
}

/// 창에 투입한 파일을 개방한다. 그 경로는 웹과 동일하게 바이트로 도달한다.
///
/// **바이트를 원시 본문으로 수령한다.** 숫자 배열로 수령하면 tauri 가 그것을
/// JSON 으로 해석하므로, 수십 MB 양식에서 바이트당 수십 배의 중간 구조가
/// 생성되고 그 시점에 창이 메모리를 고갈시킨다. 파일 명칭은 그 경로로 함께
/// 전달할 수 없어 머리에 부착된다.
#[tauri::command]
pub fn open_bytes(desk: Held, request: tauri::ipc::Request<'_>) -> Result<()> {
    let tauri::ipc::InvokeBody::Raw(bytes) = request.body() else {
        return Err(sheetsift_desk::ShellError::new(sheetsift_desk::ErrorCode::FileReadFailed)
            .detail("expected a raw body")
            .into());
    };
    let name = request
        .headers()
        .get("x-file-name")
        .and_then(|v| v.to_str().ok())
        .and_then(percent_decode)
        .unwrap_or_default();
    held(&desk).open_bytes(bytes, &name)
}

/// 머리에 부호화되어 도달한 파일 명칭을 복호한다.
///
/// 머리는 ASCII 만 수용하므로 화면이 `encodeURIComponent` 로 부호화해 송신한다.
/// **복호하지 못하면 공백으로 충전하지 않는다** — 명칭이 공백이면 시트 묶음이
/// 명칭을 상실하고, 그 사유를 어디에서도 판별할 수 없다.
fn percent_decode(raw: &str) -> Option<String> {
    let mut out: Vec<u8> = Vec::with_capacity(raw.len());
    let mut rest = raw.as_bytes();
    while let Some((&b, tail)) = rest.split_first() {
        if b == b'%' {
            let hex = tail.get(..2)?;
            out.push(u8::from_str_radix(std::str::from_utf8(hex).ok()?, 16).ok()?);
            rest = &tail[2..];
        } else {
            out.push(b);
            rest = tail;
        }
    }
    String::from_utf8(out).ok()
}

#[tauri::command]
pub fn list_screens(desk: Held, request: CatalogAsk) -> Result<ScreenCatalog> {
    held(&desk).list_screens(&request)
}

#[tauri::command]
pub fn query_rows(desk: Held, request: Ask) -> Result<Page> {
    held(&desk).query_rows(&request)
}

#[tauri::command]
pub fn column_values(desk: Held, request: ValuesAsk) -> Result<ColumnValues> {
    held(&desk).column_values(&request)
}

/// 화면이 임의로 기재하면 안 되는 값들. 기동 시 1회 수령한다.
#[tauri::command]
pub fn limits(desk: Held) -> Limits {
    held(&desk).limits()
}

#[cfg(test)]
mod tests {
    use super::percent_decode;

    /// 머리에 부호화되어 도달한 명칭을 복호하는가. 한글 명칭이 그 경로를
    /// 경유한다 — 머리는 ASCII 만 수용한다.
    #[test]
    fn a_name_in_the_header_comes_back_whole() {
        assert_eq!(percent_decode("%ED%91%9C.csv").as_deref(), Some("표.csv"));
        assert_eq!(percent_decode("plain.xlsx").as_deref(), Some("plain.xlsx"));
        assert_eq!(percent_decode("a%20b.csv").as_deref(), Some("a b.csv"));
    }

    /// **복호하지 못한 것을 공백으로 충전하지 않는다.** 명칭이 공백이면 시트
    /// 묶음이 명칭을 상실하고 그 사유를 판별할 수 없다.
    #[test]
    fn a_broken_header_decodes_to_nothing_at_all() {
        assert_eq!(percent_decode("%ZZ"), None);
        assert_eq!(percent_decode("%E"), None);
        assert_eq!(percent_decode("%ED%91"), None, "완결되지 않은 문자");
    }
}
