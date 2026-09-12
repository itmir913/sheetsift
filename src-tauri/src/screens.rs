//! 화면 정의.
//!
//! "화면도 데이터다" — 조회 화면은 코드가 아니라 이 표의 항목 하나로 표현된다.
//! 지금은 Rust 상수지만 구조를 그대로 YAML/JSON 으로 뺄 수 있게 잡아 두었다.
//! 교육청 파일에 시트가 하나 늘어도 여기 항목 하나만 추가하면 화면이 생긴다.
//!
//! ## 사전 인코딩(Dict) 열
//! 대용량 표는 반복되는 문자열(대학명·전형명 등)을 `dict` 테이블의 정수 id 로
//! 바꿔 저장한다. 이때 **id 를 가나다순으로 부여**했기 때문에
//! `ORDER BY t.univ`(정수) 가 곧 가나다순 정렬이다.
//! 덕분에 정렬·집계에는 조인이 전혀 필요 없고, 화면에 내보낼 100행에만 조인한다.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ColumnKind {
    /// 문자열. 좌측 정렬, 부분일치 필터.
    Text,
    /// 숫자. 우측 정렬, 자릿수 맞춤.
    Number,
}

/// 값이 기준 테이블에 어떤 형태로 들어 있는지.
#[derive(Debug, Clone, Copy)]
pub enum Source {
    /// 기준 테이블의 열을 그대로 쓴다.
    Plain(&'static str),
    /// 사전 인코딩된 열. `col` 에 정수 id 가 있고 `alias` 로 dict 를 조인한다.
    Dict { col: &'static str, alias: &'static str },
}

impl Source {
    /// 기준 테이블 안에서의 열 참조. 정렬·필터에 쓴다.
    pub fn base_ref(&self) -> String {
        match self {
            Source::Plain(c) => format!("t.\"{c}\""),
            Source::Dict { col, .. } => format!("t.\"{col}\""),
        }
    }
    /// 화면에 내보낼 식. 사전 열은 조인한 문자열을 쓴다.
    pub fn display_ref(&self) -> String {
        match self {
            Source::Plain(c) => format!("t.\"{c}\""),
            Source::Dict { alias, .. } => format!("{alias}.s"),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnDef {
    /// 결과 컬럼 별칭이자 프런트에서 쓰는 키.
    pub key: &'static str,
    /// 화면에 보이는 이름.
    pub label: &'static str,
    #[serde(skip)]
    pub source: Source,
    pub kind: ColumnKind,
    /// 열 너비(rem).
    pub width: f32,
    pub sortable: bool,
    pub filterable: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreenDef {
    pub id: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    /// 기준 테이블 이름 (내부 상수).
    #[serde(skip)]
    pub base: &'static str,
    pub columns: &'static [ColumnDef],
}

impl ScreenDef {
    pub fn column(&self, key: &str) -> Option<&ColumnDef> {
        self.columns.iter().find(|c| c.key == key)
    }

    /// 화면에 필요한 dict 조인 절을 만든다. (중복 별칭 제거)
    pub fn dict_joins(&self) -> String {
        let mut seen: Vec<&str> = Vec::new();
        let mut out = String::new();
        for c in self.columns {
            if let Source::Dict { col, alias } = c.source {
                if seen.contains(&alias) {
                    continue;
                }
                seen.push(alias);
                out.push_str(&format!(
                    "\n     LEFT JOIN dict {alias} ON {alias}.id = t.\"{col}\""
                ));
            }
        }
        out
    }
}

const fn dict(
    key: &'static str,
    label: &'static str,
    col: &'static str,
    alias: &'static str,
    width: f32,
) -> ColumnDef {
    ColumnDef {
        key,
        label,
        source: Source::Dict { col, alias },
        kind: ColumnKind::Text,
        width,
        sortable: true,
        filterable: true,
    }
}

const fn text(key: &'static str, label: &'static str, col: &'static str, width: f32) -> ColumnDef {
    ColumnDef {
        key,
        label,
        source: Source::Plain(col),
        kind: ColumnKind::Text,
        width,
        sortable: true,
        filterable: true,
    }
}

const fn num(key: &'static str, label: &'static str, col: &'static str, width: f32) -> ColumnDef {
    ColumnDef {
        key,
        label,
        source: Source::Plain(col),
        kind: ColumnKind::Number,
        width,
        sortable: true,
        filterable: false,
    }
}

/// 모집단위별 지원경향 — 120,340행.
static MOJIP_COLUMNS: &[ColumnDef] = &[
    dict("bUniv", "기준 대학", "b_univ", "d1", 13.0),
    dict("bJh", "기준 전형", "b_jh", "d2", 9.0),
    dict("bGye", "기준 계열", "b_gye", "d3", 8.5),
    dict("bMj", "기준 모집단위", "b_mj", "d4", 13.0),
    dict("region", "지역", "region", "d5", 8.5),
    dict("univ", "대학", "univ", "d6", 11.0),
    dict("jh", "전형", "jh", "d7", 10.0),
    dict("gye", "계열", "gye", "d8", 8.5),
    dict("mj", "모집단위", "mj", "d9", 14.0),
    num("quota", "모집인원", "quota", 8.5),
    num("apply", "지원건수", "n_apply", 8.5),
    num("pass", "합격", "n_pass", 8.5),
    num("rate", "합격률", "rate", 8.5),
];

/// 전형별 지원경향 — 122,382행.
static JUNHYEONG_COLUMNS: &[ColumnDef] = &[
    dict("bUniv", "기준 대학", "b_univ", "d1", 13.0),
    dict("bType", "기준 전형유형", "b_type", "d2", 9.5),
    dict("bJh", "기준 전형", "b_jh", "d3", 9.0),
    dict("bGye", "기준 계열", "b_gye", "d4", 8.5),
    dict("region", "지역", "region", "d5", 8.5),
    dict("univ", "대학", "univ", "d6", 11.0),
    dict("jtype", "전형유형", "jtype", "d7", 8.5),
    dict("jh", "전형", "jh", "d8", 12.0),
    dict("gye", "계열", "gye", "d9", 8.5),
    num("apply", "지원건수", "n_apply", 8.5),
    num("pass", "합격", "n_pass", 8.5),
    num("rate", "합격률", "rate", 8.5),
];

/// 대학 입결 데이터 — 6,373행. 열이 넓은 표의 예.
static DATA_COLUMNS: &[ColumnDef] = &[
    text("region", "권역", "권역", 8.5),
    text("area", "지역", "지역", 8.5),
    text("city", "세부지역", "세부지역", 8.5),
    text("univ", "대학명", "대학명", 12.0),
    text("mj", "2027 수시 모집단위", "_2027_수시_모집단위", 16.0),
    text("gye", "계열", "계열", 8.5),
    text("gyogwa1Jh", "교과1 전형", "교과1_전형", 10.0),
    num("gyogwa1P50", "교과1 50%", "교과1_50_", 8.5),
    num("gyogwa1P70", "교과1 70%", "교과1_70_", 8.5),
    text("gyogwa2Jh", "교과2 전형", "교과2_전형", 10.0),
    num("gyogwa2P50", "교과2 50%", "교과2_50_", 8.5),
    num("gyogwa2P70", "교과2 70%", "교과2_70_", 8.5),
    text("jongJh", "종합1 전형", "종합1_전형", 10.0),
    num("jongP50", "종합1 50%", "종합1_50_", 8.5),
    num("jongP70", "종합1 70%", "종합1_70_", 8.5),
    num("jeongsi", "정시 백분위", "정시백분위", 9.0),
    text("suneung", "수능 반영영역", "_26정시반영영역", 18.0),
];

pub static SCREENS: &[ScreenDef] = &[
    ScreenDef {
        id: "mojipdanwibyeol",
        title: "모집단위별 지원경향",
        description: "기준 모집단위에 지원한 학생들이 함께 지원한 대학·모집단위",
        base: "mojipdanwibyeol",
        columns: MOJIP_COLUMNS,
    },
    ScreenDef {
        id: "junhyeongbyeol",
        title: "전형별 지원경향",
        description: "기준 전형에 지원한 학생들이 함께 지원한 대학·전형",
        base: "junhyeongbyeol",
        columns: JUNHYEONG_COLUMNS,
    },
    ScreenDef {
        id: "data",
        title: "대학 입결 데이터",
        description: "대학·모집단위별 교과 / 종합 / 정시 입시 결과",
        base: "data",
        columns: DATA_COLUMNS,
    },
];

pub fn find(id: &str) -> Option<&'static ScreenDef> {
    SCREENS.iter().find(|s| s.id == id)
}
