//! 화면 정의 — **데이터에서 읽는다.**
//!
//! 조회 화면은 코드가 아니라 `data/screens.json` 의 항목 하나다. 빌드할 때
//! `build_db` 가 그것을 navi.db 의 `screens_meta` 에 넣어 두고, 앱은 기동하면서
//! 여기로 읽어 온다. 교육청 자료에 표가 하나 늘어도 Rust 를 고칠 일이 없다.
//!
//! ## 사전 인코딩(Dict) 열
//! 대용량 표는 반복되는 문자열(대학명·전형명 등)을 `dict` 테이블의 정수 id 로
//! 바꿔 저장한다. 이때 **id 를 가나다순으로 부여**했기 때문에
//! `ORDER BY t.univ`(정수) 가 곧 가나다순 정렬이다.
//! 덕분에 정렬·집계에는 조인이 전혀 필요 없고, 화면에 내보낼 행에만 조인한다.

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult, ErrorCode};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ColumnKind {
    /// 문자열. 좌측 정렬, 부분일치 필터.
    Text,
    /// 숫자. 우측 정렬, 자릿수 맞춤.
    Number,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnDef {
    /// 결과 컬럼 별칭이자 프런트에서 쓰는 키.
    pub key: String,
    /// 화면에 보이는 이름.
    pub label: String,
    /// 기준 테이블의 열 이름.
    #[serde(skip_serializing)]
    pub column: String,
    pub kind: ColumnKind,
    /// 열 너비(rem).
    pub width: f32,
    pub sortable: bool,
    pub filterable: bool,
    /// 사전 인코딩 열인가. 그렇다면 값은 정수 id 이고 dict 를 조인해야 한다.
    #[serde(skip_serializing)]
    pub dict: bool,
    /// dict 조인에 쓸 별칭. 읽어 들이면서 열마다 하나씩 매긴다.
    #[serde(skip)]
    pub alias: String,
}

impl ColumnDef {
    /// 기준 테이블 안에서의 열 참조. 정렬·필터에 쓴다.
    pub fn base_ref(&self) -> String {
        format!("t.\"{}\"", self.column)
    }
    /// 화면에 내보낼 식. 사전 열은 조인한 문자열을 쓴다.
    pub fn display_ref(&self) -> String {
        if self.dict {
            format!("{}.s", self.alias)
        } else {
            self.base_ref()
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreenDef {
    pub id: String,
    pub title: String,
    pub description: String,
    /// 왼쪽 목록에서 묶이는 자리. 묶음의 순서는 화면 쪽이 정한다.
    pub group: String,
    /// 기준 테이블 이름 (내부 값).
    #[serde(rename = "table", skip_serializing)]
    pub base: String,
    pub columns: Vec<ColumnDef>,
}

impl ScreenDef {
    pub fn column(&self, key: &str) -> Option<&ColumnDef> {
        self.columns.iter().find(|c| c.key == key)
    }

    /// 화면에 필요한 dict 조인 절을 만든다. (중복 별칭 제거)
    pub fn dict_joins(&self) -> String {
        let mut seen: Vec<&str> = Vec::new();
        let mut out = String::new();
        for c in &self.columns {
            if !c.dict || seen.contains(&c.alias.as_str()) {
                continue;
            }
            seen.push(&c.alias);
            out.push_str(&format!(
                "\n     LEFT JOIN dict {} ON {}.id = t.\"{}\"",
                c.alias, c.alias, c.column
            ));
        }
        out
    }
}

/// navi.db 에 들어 있는 화면 정의 묶음.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Screens {
    /// 왼쪽 목록의 묶음 순서.
    pub groups: Vec<String>,
    pub screens: Vec<ScreenDef>,
}

impl Screens {
    pub fn find(&self, id: &str) -> Option<&ScreenDef> {
        self.screens.iter().find(|s| s.id == id)
    }
}

/// `screens_meta` 에서 읽어 온다. 같은 열을 두 번 조인하지 않도록
/// 사전 열마다 별칭(d1, d2 …)을 여기서 매긴다.
pub fn load(conn: &Connection) -> AppResult<Screens> {
    let json: String = conn
        .query_row("SELECT json FROM screens_meta LIMIT 1", [], |r| r.get(0))
        .map_err(|e| {
            AppError::new(ErrorCode::DbEmbedInvalid)
                .detail(format!("screens_meta 를 읽지 못했습니다: {e}"))
        })?;

    let mut screens: Screens = serde_json::from_str(&json).map_err(|e| {
        AppError::new(ErrorCode::DbEmbedInvalid)
            .detail(format!("화면 정의를 해석하지 못했습니다: {e}"))
    })?;

    for s in &mut screens.screens {
        let mut alias_of: Vec<(String, String)> = Vec::new();
        for c in &mut s.columns {
            if !c.dict {
                continue;
            }
            let found = alias_of.iter().find(|(col, _)| *col == c.column).map(|(_, a)| a.clone());
            c.alias = match found {
                Some(a) => a,
                None => {
                    let a = format!("d{}", alias_of.len() + 1);
                    alias_of.push((c.column.clone(), a.clone()));
                    a
                }
            };
        }
    }
    Ok(screens)
}
