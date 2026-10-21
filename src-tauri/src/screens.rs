//! 화면 정의 — **데이터에서 읽는다.**
//!
//! 조회 화면은 코드가 아니라 `data/manifest.json` 의 항목 하나다. 빌드할 때
//! `build_db` 가 그것을 navi.bin 의 `screens_meta` 에 넣어 두고, 앱은 기동하면서
//! 여기로 읽어 온다. 교육청 자료에 표가 하나 늘어도 Rust 를 고칠 일이 없다.
//!
//! 열 하나가 조회에서 무엇을 뜻하는지(사전 인코딩 열 포함)는
//! [`sheetsift_core::grid`] 에 있다. 여기는 그 위에 이 앱의 화면 — 묶음, 종류,
//! 성적 검색의 열 — 을 얹는다.

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult, ErrorCode};

pub use sheetsift_core::grid::ColumnKind;
use sheetsift_core::grid::Column;

/// 열 하나. 조회에 쓰는 몫은 [`Column`] 이고, 여기서는 성적 검색이 쓰는 두
/// 칸만 더한다.
///
/// 조회 엔진은 `sheetsift-core` 의 것이라 이 앱의 칸을 모른다. 두 칸을 그쪽에
/// 넣으면 범용 조회 도구가 성적 검색의 말을 알게 된다. 그래서 감싸서 **같은
/// JSON 한 덩이**로 읽고(`flatten`), 나머지 코드는 `Deref` 로 그 안을 그대로 본다.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnDef {
    #[serde(flatten)]
    pub grid: Column,
    /// 등급제 토글에 따라 갈아 끼우는 컷 열의 이름(`search.cuts` 의 열쇠).
    #[serde(default, skip_serializing)]
    pub cut: String,
    /// 계산으로 만드는 열(`score` 환산점수 · `gap` 차이값).
    ///
    /// **이것만 화면에도 보낸다.** 열 이름이 아니라 구실이라, 화면이 이걸 보고
    /// "무엇을 재어 가능성을 가르는가" 를 열 라벨로 적어 줄 수 있다. 이름을
    /// 화면에 적어 두면 양식에서 머리글을 바꿀 때 설명만 옛말이 된다.
    #[serde(default)]
    pub compute: String,
}

impl std::ops::Deref for ColumnDef {
    type Target = Column;
    fn deref(&self) -> &Column {
        &self.grid
    }
}

impl std::ops::DerefMut for ColumnDef {
    fn deref_mut(&mut self) -> &mut Column {
        &mut self.grid
    }
}

/// 무엇으로 그리는 화면인가. **어느 화면이 차트인지는 데이터가 정한다** —
/// 코드가 화면 id 를 보고 고르면 id 를 바꾸는 순간 조용히 표로 바뀐다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ScreenKind {
    /// 보통 표. `kind` 를 적지 않은 화면은 이것이다.
    #[default]
    Grid,
    /// 범위 차트(+표). `chart` 가 어느 열이 어떤 구실인지 말해 준다.
    RangeChart,
    /// 성적 검색 — 유일하게 계산을 하는 화면.
    Search,
    /// 학생 선택 — 한 학생을 고르고 그 학생의 추이를 본다. 표를 읽지 않는다.
    Student,
    /// 학생 데이터 관리. 표를 읽지 않는다.
    Admin,
    /// 프로그램 안내. 표를 읽지 않는다.
    Guide,
}

/// 성적 검색에서 **코드가 뜻을 아는 열**. 나머지는 그냥 실어 나른다.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyColumns {
    /// 학생의 환산점수와 견줄 입결 백분위.
    pub jeongsi: String,
    /// 이 행이 어느 환산규칙을 쓰는지 가리키는 열.
    pub rule_code: String,
}

/// 왼쪽 조건 상자가 거르는 열.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchFilters {
    pub region: String,
    pub gye: String,
    pub univ: String,
    pub mj: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BandDef {
    pub key: String,
    pub label: String,
    /// 이 밴드에 들어가는 격차의 하한. 마지막 줄은 하한이 없다.
    #[serde(default)]
    pub min: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchDef {
    /// 계산해서 채우는 '가능성' 칸의 열 key. 화면이 쓴다.
    pub band_column: String,
    #[serde(skip_serializing)]
    pub key_columns: KeyColumns,
    #[serde(skip_serializing)]
    pub filters: SearchFilters,
    /// 등급제 이름 → (컷 이름 → 양식의 열 이름). 화면의 토글이 하나를 고른다.
    #[serde(skip_serializing)]
    pub cuts: std::collections::BTreeMap<String, std::collections::BTreeMap<String, String>>,
    pub bands: Vec<BandDef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreenDef {
    pub id: String,
    #[serde(default)]
    pub kind: ScreenKind,
    pub title: String,
    pub description: String,
    /// 왼쪽 목록에서 묶이는 자리. 묶음의 순서는 화면 쪽이 정한다.
    pub group: String,
    /// 기준 테이블 이름 (내부 값).
    #[serde(rename = "table", skip_serializing)]
    pub base: String,
    pub columns: Vec<ColumnDef>,
    /// 범위 차트가 어느 열을 어떤 구실로 쓰는지. 그림 화면에만 있다.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chart: Option<serde_json::Value>,
    /// 성적 검색이 쓰는 열과 밴드. 성적 검색 화면에만 있다.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub search: Option<SearchDef>,
}

impl sheetsift_core::grid::Table for ScreenDef {
    fn id(&self) -> &str {
        &self.id
    }
    fn base(&self) -> &str {
        &self.base
    }
    fn columns(&self) -> Box<dyn Iterator<Item = &Column> + '_> {
        Box::new(self.columns.iter().map(|c| &c.grid))
    }
}

/// navi.bin 에 들어 있는 화면 정의 묶음.
/// 양식이 "없음" 을 적을 때 쓰는 글자. navi.bin 에 없으면(옛 파일) 이 값.
fn default_blank() -> String {
    "-".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Screens {
    /// 왼쪽 목록의 묶음 순서.
    pub groups: Vec<String>,
    /// 왼쪽 목록 아래 칸(프로그램 자체를 다루는 메뉴)의 묶음.
    #[serde(default, rename = "footGroups")]
    pub foot_groups: Vec<String>,
    /// 값이 없는 칸에 양식이 적어 두는 글자. **코드에 박지 않는다.**
    #[serde(default = "default_blank")]
    pub blank: String,
    /// 이 자료가 언제 것인가. 옛 navi.bin 에는 없으므로 비어 있을 수 있다.
    #[serde(default, rename = "dataVersion")]
    pub data_version: String,
    /// 숫자 칸을 몇 자리로 보여 줄 것인가. **양식이 아니라 계산이 정한다** —
    /// 환산점수를 확정하는 자릿수와 같아야 화면의 숫자와 밴드 라벨이 같은
    /// 수를 본다. 그래서 manifest 에서 읽지 않고 늘 코드에서 채운다.
    #[serde(skip_deserializing, default = "fixed_decimals")]
    pub decimals: u32,
    pub screens: Vec<ScreenDef>,
}

fn fixed_decimals() -> u32 {
    crate::score::FIXED_DECIMALS
}

impl Screens {
    pub fn find(&self, id: &str) -> Option<&ScreenDef> {
        self.screens.iter().find(|s| s.id == id)
    }

    /// 성적 검색 화면의 정의. 계산에 쓸 열 이름이 여기 들어 있다.
    pub fn search(&self) -> AppResult<(&ScreenDef, &SearchDef)> {
        self.screens
            .iter()
            .find_map(|s| s.search.as_ref().map(|d| (s, d)))
            .ok_or_else(|| {
                AppError::new(ErrorCode::DbEmbedInvalid)
                    .detail("manifest.json 에 search 를 쓰는 화면이 없습니다")
            })
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
        sheetsift_core::grid::assign_aliases(s.columns.iter_mut().map(|c| &mut c.grid));
    }
    Ok(screens)
}
