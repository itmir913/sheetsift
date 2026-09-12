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

/* ── 교육청 엑셀의 참고 시트들 ──────────────────────────────────
   전부 "머리글 + 행" 짜리 표라서 그리드 하나면 끝난다. 화면마다 따로 만들
   것이 없고, 여기 정의만 있으면 정렬·열 검색·값 필터·틀 고정·인쇄가
   그대로 딸려 온다.
   열 이름은 양식의 머리글을 눌러 만든 것이라 길다. 화면에 보이는 이름은
   두 번째 인자로 따로 적는다. */

/// 학생부종합 전형 — 335행.
static JONGHAP_COLUMNS: &[ColumnDef] = &[
    text("region", "지역", "전형개요___지역", 8.5),
    text("univ", "대학명", "전형개요___대학명", 12.0),
    text("jh", "전형명", "전형개요___전형명", 14.0),
    text("gubun", "구분", "전형개요___구분", 13.0),
    text("quota", "모집인원", "전형개요___모집인원", 8.5),
    text("stage", "단계/일괄", "전형개요___단계_일괄", 9.0),
    text("stageRatio", "1단계 선발비율", "전형개요___1단계_선발비율", 11.0),
    text("method", "전형방법", "전형개요___전형방법", 22.0),
    text("itvWhen", "면접 시기", "면접일정_및_내용___면접_시기", 9.5),
    text("itvAnnounce", "1단계 발표", "면접일정_및_내용___1단계발표", 10.5),
    text("itvDate", "면접일자", "면접일정_및_내용___면접일자", 12.0),
    text("itvHow", "면접방법", "면접일정_및_내용___면접방법__서류_제시문_기타_", 14.0),
    text("minGrade", "수능최저 등급기준", "수능최저학력기준___등급기준", 11.0),
    text("minDetail", "수능최저 세부내용", "수능최저학력기준___세부내용", 20.0),
    text("evalAcademic", "학업역량", "서류_평가요소별_비율___학업역량", 8.5),
    text("evalCareer", "진로역량", "서류_평가요소별_비율___진로역량__탐구__자기계발_등_", 8.5),
    text("evalCommunity", "공동체역량", "서류_평가요소별_비율___공동체역량", 9.5),
    text("evalNote", "평가요소 및 배점", "평가요소_및_인재상___평가요소_및_배점", 24.0),
    text("ideal", "대학 인재상", "평가요소_및_인재상___대학_인재상", 24.0),
];

/// 정원외·특별전형 — 11,345행.
static TEUKBYEOL_COLUMNS: &[ColumnDef] = &[
    text("region", "지역", "기본사항___지역", 8.5),
    text("univ", "대학명", "기본사항___대학명", 12.0),
    text("mj", "모집단위", "_2027학년도_전형정보___모집단위", 16.0),
    text("gye", "계열", "_2027학년도_전형정보___계열", 8.5),
    text("jtype", "전형유형", "_2027학년도_전형정보___전형_유형", 9.0),
    text("jh", "전형명", "_2027학년도_전형정보___전형명", 16.0),
    text("unified", "통합선발", "_2027학년도_전형정보___통합선발_모집단위", 9.0),
    text("quota", "모집인원", "_2027학년도_전형정보___모집_인원", 8.5),
    text("qualMulti", "자격 2개 이상", "_2027학년도_전형정보___지원자격___지원자격_2개_이상", 10.5),
    text("qRural", "농어촌", "_2027학년도_전형정보___농어촌", 8.0),
    text("qVocational", "특성화고", "_2027학년도_전형정보___특성화고", 8.5),
    text("qBasic", "기/차/한", "_2027학년도_전형정보___기_차_한", 8.5),
    text("qMulticultural", "다문화", "_2027학년도_전형정보___다문화", 8.0),
    text("qManyChildren", "다자녀", "_2027학년도_전형정보___다자녀", 8.0),
    text("qOfficial", "공무원", "_2027학년도_전형정보___공무원__군인_경찰_소방_등_", 9.0),
    text("mj26", "2026 모집단위", "_2026학년도_전형정보___모집단위", 16.0),
    text("jtype26", "2026 전형유형", "_2026학년도_전형정보___전형유형", 10.0),
    text("jh26", "2026 전형명", "_2026학년도_전형정보___전형명", 16.0),
    text("q26Rural", "2026 농어촌", "_2026학년도_전형정보___지원자격___농어촌", 9.5),
    text("q26Vocational", "2026 특성화고", "_2026학년도_전형정보___특성화고", 10.0),
    text("q26Basic", "2026 기/차/한", "_2026학년도_전형정보___기_차_한", 10.0),
    text("quota26", "2026 모집인원", "_2026학년도_입시결과___모집인원", 10.0),
    text("compRate26", "2026 경쟁률", "_2026학년도_입시결과___경쟁률", 9.5),
    text("extra26", "2026 충원인원", "_2026학년도_입시결과___충원인원", 10.0),
    text("basis26", "대학제공기준", "_2026학년도_입시결과___대학제공기준", 10.5),
    text("result50", "입결 50%", "_2026학년도_입시결과___수시입시결과___0_5", 8.5),
    text("result70", "입결 70%", "_2026학년도_입시결과___0_7", 8.5),
    text("note26", "비고", "_2026학년도_입시결과___비고", 10.0),
];

/// 대학별 교과 반영 방법 — 900행.
static GYOGWA_COLUMNS: &[ColumnDef] = &[
    text("year", "연도", "연도", 7.0),
    text("region", "지역", "지역", 8.5),
    text("univ", "대학명", "대학명", 12.0),
    text("campus", "캠퍼스", "캠퍼스", 8.5),
    text("jh", "전형명", "전형명", 14.0),
    text("gye", "계열", "계열", 9.0),
    text("mj", "모집단위", "모집단위", 18.0),
    text("g1Subject", "일반선택 반영교과", "일반선택과목___반영교과", 14.0),
    text("g1Count", "일반선택 반영과목", "일반선택과목___반영과목", 11.0),
    text("g1Method", "일반선택 반영방법", "일반선택과목___반영방법", 24.0),
    text("g1Index", "활용지표", "일반선택과목___활용지표", 9.5),
    text("g1Unit", "이수단위", "일반선택과목___이수단위", 9.5),
    text("g2Subject", "진로선택 반영교과", "진로선택과목___반영교과", 14.0),
    text("g2Method", "진로선택 반영방법", "진로선택과목___반영방법", 24.0),
    text("g2Count", "진로선택 과목수", "진로선택과목___반영과목수", 11.0),
    text("g2A", "A", "진로선택과목___A", 7.0),
    text("g2B", "B", "진로선택과목___B", 7.0),
    text("g2C", "C", "진로선택과목___C", 7.0),
    text("g2Note", "진로선택 반영내용", "진로선택과목___반영내용", 11.0),
    text("note", "비고(변경/유지/신설)", "비고___변경_유지_신설", 22.0),
];

/// 무전공·자유전공 — 805행.
static JAYUL_COLUMNS: &[ColumnDef] = &[
    text("region", "지역", "지역", 8.5),
    text("univ", "대학명", "대학명", 12.0),
    text("type", "유형", "유형", 8.0),
    text("gye", "계열", "계열", 8.5),
    text("jtype", "전형유형", "전형유형", 9.0),
    text("jh", "전형명", "전형명", 14.0),
    text("mj", "2027 모집단위", "_2027_모집단위", 16.0),
    text("quota", "모집인원", "모집인원", 8.5),
    text("ratio", "반영비율", "반영비율", 22.0),
    text("detail", "2027 세부전공", "_2027_세부전공", 26.0),
];

/// 전형 일정 — 482행.
static ILJEONG_COLUMNS: &[ColumnDef] = &[
    text("date", "날짜", "날짜", 10.0),
    text("univ", "대학명", "대학명", 12.0),
    text("gubun", "구분", "구분", 8.0),
    text("jh", "전형명", "전형명", 14.0),
    text("what", "내용", "내용", 10.0),
    text("mj", "모집단위", "모집단위", 18.0),
    text("note", "비고", "비고", 22.0),
];

/// 2028 대입 변경 사항 — 985행.
static DAEIP2028_COLUMNS: &[ColumnDef] = &[
    text("region", "지역", "지역", 8.5),
    text("univ", "대학명", "대학명", 12.0),
    text("jtype", "전형유형", "전형유형", 9.0),
    text("category", "분류", "분류", 9.5),
    text("jh27", "2027 전형명", "_2027_전형명", 14.0),
    text("note27", "2027 주요 사항", "_2027_주요_사항", 24.0),
    text("changed", "27 대비 변경", "_27대비_변경여부", 10.0),
    text("jh28", "2028 전형명", "_2028_전형명", 14.0),
    text("note28", "2028 주요 사항", "_2028_주요_사항", 24.0),
    text("commonGrade", "공통 등급", "_2028_교과성적반영방법___공통___등급", 8.5),
    text("commonLevel", "공통 성취도", "_2028_교과성적반영방법___공통___성취도", 9.5),
    text("generalGrade", "일반 등급", "_2028_교과성적반영방법___일반___등급", 8.5),
    text("generalLevel", "일반 성취도", "_2028_교과성적반영방법___일반___성취도", 9.5),
    text("careerGrade", "진로 등급", "_2028_교과성적반영방법___진로___등급", 8.5),
    text("careerLevel", "진로 성취도", "_2028_교과성적반영방법___진로___성취도", 9.5),
    text("fusionGrade", "융합 등급", "_2028_교과성적반영방법___융합___등급", 8.5),
    text("fusionLevel", "융합 성취도", "_2028_교과성적반영방법___융합___성취도", 9.5),
    text("reflectNote", "교과반영 비고", "_2028_교과성적반영방법___융합___비고", 16.0),
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
    ScreenDef {
        id: "jonghap",
        title: "학생부종합 전형",
        description: "종합전형 개요와 면접 일정·서류 평가요소",
        base: "jonghap",
        columns: JONGHAP_COLUMNS,
    },
    ScreenDef {
        id: "teukbyeol",
        title: "특별전형",
        description: "정원외·특별전형과 지원자격, 2026 입시결과",
        base: "teukbyeol",
        columns: TEUKBYEOL_COLUMNS,
    },
    ScreenDef {
        id: "gyogwa",
        title: "교과 반영 방법",
        description: "대학별 일반선택·진로선택 과목 반영 방법",
        base: "gyogwa",
        columns: GYOGWA_COLUMNS,
    },
    ScreenDef {
        id: "jayul",
        title: "무전공·자유전공",
        description: "전공자율선택 모집단위와 세부전공",
        base: "jayul",
        columns: JAYUL_COLUMNS,
    },
    ScreenDef {
        id: "iljeong",
        title: "전형 일정",
        description: "대학별 1단계 발표·면접 등 전형 일정",
        base: "iljeong",
        columns: ILJEONG_COLUMNS,
    },
    ScreenDef {
        id: "daeip2028",
        title: "2028 대입 변경",
        description: "2027 대비 2028학년도 전형 변경 사항",
        base: "daeip2028",
        columns: DAEIP2028_COLUMNS,
    },
];

pub fn find(id: &str) -> Option<&'static ScreenDef> {
    SCREENS.iter().find(|s| s.id == id)
}
