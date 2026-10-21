//! 오류 표현.
//!
//! **사람이 읽을 문구를 만들지 않는다.** 코드와 치환 값만 돌려주고, 문구는 앱이
//! 붙인다. 같은 그릇이 앱마다 다른 말로 안내되어야 하기 때문이다 — 봉인이 깨졌을
//! 때 "다시 설치하세요" 가 맞는 앱도, "파일을 다시 받으세요" 가 맞는 앱도 있다.
//!
//! `detail` 에는 개발자용 영문 원문을 담는다(로그·디버그 전용).

use std::collections::BTreeMap;

use serde::Serialize;

/// 이 크레이트가 내는 오류 코드.
///
/// 직렬화한 이름(`DATA_SEAL_INVALID`)이 앱의 문구 사전과 맞물리는 열쇠다. 앱은
/// 제 코드 옆에 이것을 그대로 품어 내보낸다 — 이름을 한 벌 더 적어 옮기면 두 벌이
/// 갈라진다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    DbQueryFailed,
    /// 정렬·필터·값 목록이 표에 없는 열을 가리켰다.
    InvalidSortKey,
    DataSealInvalid,
    DataSignatureInvalid,
    DataContainerUnknown,
    VaultOpenFailed,
    VaultFormatInvalid,
    VaultPasswordWrong,
    VaultPasswordTooShort,
    VaultVersionTooNew,
    VaultContainerUnknown,
    VaultFileDamaged,
    VaultKeyDerivationFailed,
    VaultSealFailed,
    /// 양식을 열지 못했다. zip 이 아니거나 속이 상했다.
    SheetWorkbookUnreadable,
    /// 양식 안의 시트 하나를 읽지 못했다. **건너뛰지 않는다** — 조용히 빠진
    /// 시트는 표 하나가 없는 DB 가 되고, 그것은 화면에서야 드러난다.
    SheetUnreadable,
    /// 시트에 머리글 줄조차 없다.
    SheetEmpty,
    /// 정의가 가리키는 열이 시트에 없다.
    SheetColumnMissing,
    /// 표를 만들거나 줄을 넣지 못했다.
    SheetTableFailed,
    /// csv 의 글자 표기를 알아내지 못했다. UTF-8 도 CP949 도 아니다.
    /// **깨진 글자를 보이지 않는다** — 그러면 사전이 조용히 갈린다.
    SheetEncodingUnknown,
    /// 만든 DB 를 바이트로 꺼내지 못했다.
    SheetSerializeFailed,
    /// `.sift` 의 머리가 우리 것이 아니거나 속이 상했다.
    SiftFormatInvalid,
    /// 풀어 낸 알맹이가 우리가 만든 생김새가 아니다.
    SiftSchemaUnexpected,
    /// 담긴 지문·열 정보를 읽지 못했다.
    SiftMetaInvalid,
    /// 이 앱이 모르는 판의 `.sift` 다. **빠진 칸을 기본값으로 채우지 않는다** —
    /// 그러면 그럴듯한 거짓이 나온다.
    SiftVersionTooNew,
    /// 봉한 바이트를 쓰지 못했다. 쓰는 일은 앱이 하지만, 비밀번호 바꾸기처럼
    /// 쓰기가 그릇의 약속에 들어가는 자리가 있어 코드는 여기 둔다.
    VaultWriteFailed,
}

#[derive(Debug, Clone, Serialize)]
pub struct Error {
    pub code: ErrorCode,
    /// 문구 템플릿의 `{이름}` 자리에 채울 값.
    pub params: BTreeMap<String, String>,
    /// 개발자용 원문(영문). 사용자에게 보이지 않는다.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

impl Error {
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

impl std::fmt::Display for Error {
    /// 로그용. 영문만 쓴다.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.code)?;
        if let Some(d) = &self.detail {
            write!(f, ": {d}")?;
        }
        Ok(())
    }
}

impl std::error::Error for Error {}

impl From<rusqlite::Error> for Error {
    fn from(e: rusqlite::Error) -> Self {
        Error::new(ErrorCode::DbQueryFailed).detail(e)
    }
}

pub type Result<T> = std::result::Result<T, Error>;
