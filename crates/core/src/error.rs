//! 오류 표현.
//!
//! **사람이 읽을 문구를 생성하지 않는다.** 코드와 치환 값만 반환하고, 문구는 앱이
//! 부여한다. 동일한 그릇이 앱마다 상이한 문구로 안내되어야 하기 때문이다 — 봉인이
//! 훼손된 경우 "재설치하십시오" 가 적절한 앱도, "파일을 재수신하십시오" 가 적절한
//! 앱도 존재한다.
//!
//! `detail` 에는 개발자용 영문 원문을 수용한다(로그·디버그 전용).

use std::collections::BTreeMap;

use serde::Serialize;

/// 이 크레이트가 발생시키는 오류 코드.
///
/// 직렬화된 명칭(`DATA_SEAL_INVALID`)이 앱의 문구 사전과 결합하는 열쇠다. 앱은
/// 자체 코드 옆에 이것을 그대로 수용해 노출한다 — 명칭을 한 벌 더 기재해 이전하면
/// 양자가 분기한다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    DbQueryFailed,
    /// 정렬·필터·값 목록이 표에 부재하는 열을 지시했다.
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
    /// 양식을 개방하지 못했다. zip 이 아니거나 내용이 손상되었다.
    SheetWorkbookUnreadable,
    /// 양식 내부의 시트 하나를 판독하지 못했다. **생략하지 않는다** — 묵시적으로
    /// 누락된 시트는 표 하나가 부재하는 DB 가 되고, 그것은 화면에서야 발현된다.
    SheetUnreadable,
    /// 시트에 헤더 행조차 부재한다.
    SheetEmpty,
    /// 정의가 지시하는 열이 시트에 부재한다.
    SheetColumnMissing,
    /// 표를 생성하거나 행을 삽입하지 못했다.
    SheetTableFailed,
    /// 시트의 열이 조회 엔진이 수용하는 수를 초과한다. **개방을 시도하기 전에
    /// 진술한다** — 그러지 않으면 열은 전부 묘사되는데 자료만 도달하지 않는다.
    SheetTooWide,
    /// csv 의 문자 표기를 판별하지 못했다. UTF-8 도 CP949 도 아니다.
    /// **훼손된 문자를 표시하지 않는다** — 그러면 사전이 묵시적으로 분기한다.
    SheetEncodingUnknown,
    /// 생성한 DB 를 바이트로 추출하지 못했다.
    SheetSerializeFailed,
    /// `.sift` 의 머리가 우리 것이 아니거나 내용이 손상되었다.
    #[cfg(feature = "sift")]
    SiftFormatInvalid,
    /// 해제한 내용이 우리가 생성한 구조가 아니다.
    #[cfg(feature = "sift")]
    SiftSchemaUnexpected,
    /// 수록된 지문·열 정보를 판독하지 못했다.
    #[cfg(feature = "sift")]
    SiftMetaInvalid,
    /// 이 앱이 인지하지 못하는 판의 `.sift` 다. **누락된 항목을 기본값으로 충전하지
    /// 않는다** — 그러면 그럴듯한 허위가 산출된다.
    #[cfg(feature = "sift")]
    SiftVersionTooNew,
    /// 봉인한 바이트를 기록하지 못했다. 기록은 앱이 수행하지만, 비밀번호 변경처럼
    /// 기록이 그릇의 약정에 포함되는 위치가 존재해 코드는 여기 배치한다.
    VaultWriteFailed,
    /// 구 판 파일을 상승시키는 조각이 실패했다. 그 판은 전부 철회된다.
    #[cfg(feature = "migrate")]
    VaultMigrationFailed,
}

#[derive(Debug, Clone, Serialize)]
pub struct Error {
    pub code: ErrorCode,
    /// 문구 템플릿의 `{이름}` 위치에 충전할 값.
    pub params: BTreeMap<String, String>,
    /// 개발자용 원문(영문). 사용자에게 노출되지 않는다.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

impl Error {
    pub fn new(code: ErrorCode) -> Self {
        Self { code, params: BTreeMap::new(), detail: None }
    }

    pub fn with(mut self, key: &str, value: impl ToString) -> Self {
        /* 치환 값도 절단한다. 화면이 송신한 값이 그대로 반환되는 항목이
           존재하고(요청이 지시한 시트 명칭 등), 그 값이 거대하면 문구에
           충전되어 화면을 점유한다. */
        const LIMIT: usize = 200;
        let text = value.to_string();
        let cut = match text.char_indices().nth(LIMIT) {
            Some((at, _)) => format!("{}…", &text[..at]),
            None => text,
        };
        self.params.insert(key.to_string(), cut);
        self
    }

    pub fn detail(mut self, detail: impl ToString) -> Self {
        /* **원문을 절단한다.** 조회가 중단되면 상자가 질의 전체를 사유에
           부착하는 경우가 존재하고, 조건에 값이 수만 개 기재된 질의는 그것이
           수백 KB 다. 그 문자는 화면으로 전송되고 기록에 적재된다 — 판독하는
           사람에게 유용한 것은 선두뿐이다. */
        const LIMIT: usize = 2000;
        let text = detail.to_string();
        self.detail = Some(match text.char_indices().nth(LIMIT) {
            Some((at, _)) => format!("{}…", &text[..at]),
            None => text,
        });
        self
    }
}

impl std::fmt::Display for Error {
    /// 로그용. 영문만 사용한다.
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
