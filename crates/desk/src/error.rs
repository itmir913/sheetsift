//! 이 껍질이 발생시키는 오류.
//!
//! **바탕의 코드를 한 벌 더 기재하지 않는다.** `sheetsift-core` 의
//! [`ErrorCode`](sheetsift_core::ErrorCode) 는 그대로 통과시키고, 여기 배치하는
//! 것은 **바탕이 발생시킬 수 없는 것**뿐이다 — 바탕은 파일 시스템을 인지하지
//! 않으므로 파일을 판독하지 못한 사유를 진술할 수단이 부재하다.
//!
//! 코드를 바탕의 enum 에 증설하지 않는 사유가 그것이다. 거기 증설하면 파일을
//! 판독할 일이 영구히 부재한 웹 껍질까지 그 문구를 한 행 기재해야 하고, 그
//! 문구는 영영 화면에 출현할 수 없다.
//!
//! **직렬화한 구조는 바탕의 것과 동일하다**(`code` · `params` · `detail`).
//! 부르는 곳은 양자를 구별하지 않고 단일 사전으로 문구를 부여한다.

use std::collections::BTreeMap;

use serde::Serialize;

/// 이 껍질이 발생시키는 오류 코드. 바탕의 것과 중복하지 않는다.
///
/// 직렬화된 명칭(`FILE_READ_FAILED`)이 앱의 문구 사전과 결합하는 열쇠다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    /// 경로의 파일을 판독하지 못했다. 부재하거나, 권한이 부족하거나, 타 프로그램이
    /// 점유하고 있다.
    FileReadFailed,
    /// 파일이 과대해 개방하지 않았다. 판독하면 메모리가 고갈되고, 그 시점에
    /// **창이 사유 없이 폐쇄된다** — Rust 의 할당 실패는 포착할 수 없다.
    FileTooLarge,
    /// 경로에 기록하지 못했다. **부분적으로 기록된 파일을 존치하지 않는다** —
    /// 기록은 임시 명칭으로 수행하고 완료된 이후에 교체한다.
    FileWriteFailed,
    /// 개방한 양식이 부재한 상태에서 조회를 요청받았다. 창이 양식을 개방하기
    /// 이전에 화면이 복원된 경우가 그것이다.
    NoWorkbookOpen,
}

/// 이 껍질의 오류. 구조는 바탕의 [`Error`](sheetsift_core::Error) 와 동일하다.
#[derive(Debug, Clone, Serialize)]
pub struct ShellError {
    pub code: ErrorCode,
    /// 문구 템플릿의 `{이름}` 위치에 충전할 값.
    pub params: BTreeMap<String, String>,
    /// 개발자용 원문(영문). 사용자에게 노출되지 않는다.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

impl ShellError {
    pub fn new(code: ErrorCode) -> Self {
        Self { code, params: BTreeMap::new(), detail: None }
    }

    pub fn with(mut self, key: &str, value: impl ToString) -> Self {
        self.params.insert(key.to_string(), value.to_string());
        self
    }

    pub fn detail(mut self, detail: impl ToString) -> Self {
        self.detail = Some(detail.to_string());
        self
    }
}

/// 창으로 반환하는 오류. 바탕의 것이거나 이 껍질의 것이다.
///
/// `untagged` 인 사유는 **양자가 동일한 구조로 도달해야** 하기 때문이다. 종류를
/// 부착하면 화면이 그것을 분기해 판독하게 되고, 그 분기는 코드가 증가하는 날
/// 한쪽만 수정된다.
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum Fault {
    Core(sheetsift_core::Error),
    Shell(ShellError),
}

impl From<sheetsift_core::Error> for Fault {
    fn from(e: sheetsift_core::Error) -> Self {
        Fault::Core(e)
    }
}

impl From<ShellError> for Fault {
    fn from(e: ShellError) -> Self {
        Fault::Shell(e)
    }
}

impl std::fmt::Display for Fault {
    /// 로그용. 영문만 사용한다.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Fault::Core(e) => write!(f, "{e}"),
            Fault::Shell(e) => {
                write!(f, "{:?}", e.code)?;
                if let Some(d) = &e.detail {
                    write!(f, ": {d}")?;
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for Fault {}

pub type Result<T> = std::result::Result<T, Fault>;
