//! **파일을 판독하고 기록한다.** 이 저장소에서 그것을 수행하는 유일한 위치다.
//!
//! `sheetsift-core` 는 바이트를 수령하고 바이트를 반환한다. 경로라는 개념이
//! 거기 부재한 사유는 웹에도 부재하기 때문이고, 그래서 **경로를 인지하는
//! 작업은 전부 여기 소재한다.**
//!
//! ## 경로를 화면으로 전달하지 않는다
//!
//! 오류의 치환 값에 기재하는 것은 **파일 명칭 하나**다. 전체 경로에는 사용자
//! 명칭이 포함되고, 그것은 화면에 표시되고 갈무리되어 반출된다. 개발자가
//! 판독할 원문(`detail`)에는 운영체제가 진술한 사유를 그대로 수용하되 그것은
//! 사용자에게 노출되지 않는다.

use std::path::Path;

use crate::error::{ErrorCode, Result, ShellError};

/// 경로가 지시하는 파일의 명칭. 경로가 디렉터리로 종단되면 공백이다.
pub fn name_of(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

/// 파일 전체를 수령한다.
///
/// **용량을 사전에 판정하지 않는다.** 판정해도 판독하는 시점에 상이할 수 있고,
/// 무엇이 과대한지는 이 겹이 아니라 부르는 곳이 인지한다.
pub fn read(path: &Path) -> Result<Vec<u8>> {
    std::fs::read(path).map_err(|e| {
        ShellError::new(ErrorCode::FileReadFailed).with("name", name_of(path)).detail(e).into()
    })
}

/// 바이트를 경로에 기록한다. **부분적으로 기록된 파일을 존치하지 않는다.**
///
/// 기록 도중에 전원이 차단되거나 디스크가 충만하면, 직접 기록하는 방식은 절단된
/// 파일을 그 경로에 존치한다. `.sift` 는 봉인한 그릇이라 절단되면 개방되지
/// 않는데, **사용자에게는 기록이 완료된 것으로 보인다** — 원본을 삭제한 이후에
/// 발현된다.
///
/// 그래서 임시 명칭으로 기록하고 완료된 이후에 교체한다. 교체는 동일한
/// 디렉터리 내부라 이전이 아니라 개명이다.
pub fn write(path: &Path, bytes: &[u8]) -> Result<()> {
    let failed = |e: std::io::Error| -> crate::error::Fault {
        ShellError::new(ErrorCode::FileWriteFailed).with("name", name_of(path)).detail(e).into()
    };
    let temp = temp_beside(path);
    std::fs::write(&temp, bytes).map_err(failed)?;
    if let Err(e) = std::fs::rename(&temp, path) {
        // 교체하지 못했으면 임시 파일을 존치하지 않는다. 존치하면 사용자가
        // 인지하지 못하는 파일이 디렉터리에 적재된다.
        let _ = std::fs::remove_file(&temp);
        return Err(failed(e));
    }
    Ok(())
}

/// 동일한 디렉터리의 임시 명칭. **타 디렉터리에 배치하지 않는다** — 개명은
/// 동일한 파일 시스템 내부에서만 성립하고, 임시 디렉터리는 상이한 장치일 수 있다.
///
/// 교체 대상과 동일한 명칭으로 개시해, 어느 작업의 잔여물인지 판별 가능하게 한다.
fn temp_beside(path: &Path) -> std::path::PathBuf {
    let mut name = std::ffi::OsString::from(".");
    name.push(path.file_name().unwrap_or_default());
    name.push(".sheetsift-partial");
    path.with_file_name(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 임시 명칭이 **동일한 디렉터리**에 산출되는가. 타 장치로 이탈하면 개명이
    /// 성립하지 않는다.
    #[test]
    fn the_temp_name_stays_in_the_same_folder() {
        let path = Path::new("C:/some/where/표.sift");
        let temp = temp_beside(path);
        assert_eq!(temp.parent(), path.parent());
        assert_ne!(temp.file_name(), path.file_name());
    }

    #[test]
    fn a_folder_path_has_no_name() {
        assert_eq!(name_of(Path::new("/")), "");
    }
}
