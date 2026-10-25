import type { ErrorCode } from "./codes";

/**
 * 오류 코드를 사람이 판독할 문구로.
 *
 * **동일한 그릇이 앱마다 상이한 문구로 안내된다.** 바탕은 `SHEET_EMPTY` 만
 * 반환하고, 그것을 "헤더 행이 없습니다" 로 기재할지 "빈 시트입니다" 로
 * 기재할지는 쓰는 곳의 소관이다. 여기는 **표를 열어 보는 화면**의 어조다.
 *
 * 열람 겹이 영구히 발생시키지 않을 코드에도 한 행을 기재한다 — 그릇의 코드가
 * 단일 enum 이라 공백으로 두면 그 시점에 화면에 코드가 그대로 표시된다. 그러한
 * 행은 그렇게 기재해 둔다.
 */
export const errors: Record<ErrorCode, string> = {
  UNKNOWN: "알 수 없는 오류입니다.",

  DB_QUERY_FAILED: "조회하지 못했습니다.",
  INVALID_SORT_KEY: "정렬 기준으로 지정한 열을 찾지 못했습니다.",

  SHEET_WORKBOOK_UNREADABLE:
    "파일을 열지 못했습니다. xlsx·xls·csv 파일이 맞는지 확인해 주십시오.",
  SHEET_UNREADABLE: "'{sheet}' 시트를 읽지 못했습니다.",
  SHEET_EMPTY: "'{sheet}' 시트에서 헤더 행을 찾지 못했습니다.",
  SHEET_COLUMN_MISSING: "'{sheet}' 시트의 열을 맞추지 못했습니다.",
  SHEET_TABLE_FAILED: "읽어 온 자료를 표로 바꾸지 못했습니다.",
  SHEET_TOO_WIDE:
    "이 시트는 열이 {columns}개로, 열어 볼 수 있는 {limit}개를 초과합니다.",
  SHEET_ENCODING_UNKNOWN:
    "csv 의 문자 인코딩을 알 수 없습니다. UTF-8 또는 CP949 로 저장해 주십시오.",
  SHEET_SERIALIZE_FAILED: "자료를 내보내지 못했습니다.",

  // 하기는 이 화면이 발생시키지 않는 코드다. 봉인한 파일을 개방하거나 기록하는
  // 위치가 부재한다. 그래도 공백으로 두지 않는다 — 공백이면 그 시점에 코드가
  // 그대로 화면에 표시된다.
  DATA_SEAL_INVALID: "봉인된 자료를 열지 못했습니다.",
  DATA_SIGNATURE_INVALID: "자료의 서명이 맞지 않습니다.",
  DATA_CONTAINER_UNKNOWN: "알 수 없는 형식의 자료입니다.",
  VAULT_OPEN_FAILED: "보관함을 열지 못했습니다.",
  VAULT_FORMAT_INVALID: "보관함의 형식이 맞지 않습니다.",
  VAULT_PASSWORD_WRONG: "비밀번호가 맞지 않습니다.",
  VAULT_PASSWORD_TOO_SHORT: "비밀번호가 너무 짧습니다.",
  VAULT_VERSION_TOO_NEW: "이 화면이 알지 못하는 판의 보관함입니다.",
  VAULT_CONTAINER_UNKNOWN: "알 수 없는 형식의 보관함입니다.",
  VAULT_FILE_DAMAGED: "보관함 파일이 손상되었습니다.",
  VAULT_KEY_DERIVATION_FAILED: "보관함의 열쇠를 만들지 못했습니다.",
  VAULT_SEAL_FAILED: "보관함을 봉인하지 못했습니다.",
  // 하기 셋은 데스크톱 껍질만 발생시킨다. 브라우저에는 경로가 부재한다.
  FILE_READ_FAILED: "'{name}' 파일을 읽지 못했습니다. 파일이 있는지 확인해 주십시오.",
  FILE_WRITE_FAILED:
    "'{name}' 파일에 쓰지 못했습니다. 저장 위치의 권한과 남은 공간을 확인해 주십시오.",
  FILE_TOO_LARGE:
    "'{name}' 은 이 앱이 한 번에 열 수 있는 크기({limit}MB)를 넘습니다. 시트를 나누어 저장한 뒤 열어 주십시오.",
  NO_WORKBOOK_OPEN: "열어 둔 파일이 없습니다. 파일을 먼저 선택해 주십시오.",

  VAULT_WRITE_FAILED: "보관함을 저장하지 못했습니다.",
  SIFT_FORMAT_INVALID: "`.sift` 파일의 형식이 맞지 않습니다.",
  SIFT_SCHEMA_UNEXPECTED: "`.sift` 파일의 내용이 이 앱이 만든 구조가 아닙니다.",
  SIFT_META_INVALID: "`.sift` 파일에 담긴 열 정보를 읽지 못했습니다.",
  SIFT_VERSION_TOO_NEW: "이 화면이 알지 못하는 판의 `.sift` 파일입니다.",
};

/**
 * 화면의 문구. 오류가 아닌 것들.
 *
 * **열쇠는 `영역.대상.종류` 다.** 종류는 `aria`(보조기기), `tip`(포인터 설명),
 * `ph`(입력란 예시)이고, 생략하면 화면에 그대로 묘사되는 문구다.
 *
 * `_one`·`_other` 로 끝나는 쌍은 수에 따라 선택된다([`tn`]). **한국어에는 수에
 * 따른 형태 변화가 부재하나 영어에는 존재하므로**, 양측을 동일한 열쇠 집합으로
 * 유지하려면 한국어에도 동일한 쌍을 기재한다.
 *
 * 여기는 **화면의 어조**다. 주석과 문서의 한자어 문체를 유입시키지 않는다 —
 * 읽는 사람이 상이하다. `개방`이 아니라 `열기`, `부재합니다`가 아니라 `없습니다`.
 */
export const ui = {
  /* 앱 전역 */
  "app.tagline.web": "자료는 이 브라우저 밖으로 나가지 않습니다",
  "app.tagline.desktop": "자료는 이 컴퓨터 밖으로 나가지 않습니다",
  "app.starting": "시작하는 중…",
  "app.sheets": "시트 목록",
  "app.sheets.show.tip": "시트 목록 보기 (Ctrl+B)",
  "app.sheets.hide.tip": "시트 목록 숨기기 (Ctrl+B)",
  "app.sheet.noheader.tip": "{title} — 헤더 행을 찾지 못했습니다",
  "app.group.none": "묶음 없음",
  "app.group.missing.tip": "'{group}' 묶음이 목록에 없습니다",
  "app.file.nosheets": "이 파일에서 열어 볼 수 있는 표를 찾지 못했습니다.",
  "app.sheet.nocolumns":
    "이 시트에서 열을 찾지 못했습니다. 빈 시트이거나, 표가 아닌 내용만 있습니다.",
  "app.sheet.toowide": "이 시트는 열이 {columns}개로, 열어 볼 수 있는 범위를 초과합니다.",
  "app.header.assumed":
    "'{sheet}' 의 헤더를 찾지 못해 첫 행을 헤더로 가정했습니다. 위 도구 모음에서 지정해 주십시오.",

  /* 파일 수령 */
  "pick.drop": "xlsx·xls·csv 파일을 선택하거나 여기에 끌어다 놓으십시오",
  "pick.safe.web": "파일은 이 브라우저 안에서만 열립니다",
  "pick.safe.desktop": "파일은 이 컴퓨터 안에서만 열립니다",
  "pick.reading": "읽는 중…",
  "pick.change": "다른 파일 열기",
  "pick.change.tip": "다른 파일을 선택하거나 이 단추에 끌어다 놓으십시오",
  "pick.busy.tip": "파일을 읽고 있습니다",
  "pick.filter": "표 파일",

  /* 테마·언어 */
  "theme.aria": "테마",
  "theme.light": "밝게",
  "theme.dark": "어둡게",
  "theme.system": "시스템",
  "locale.aria": "언어",
  "locale.ko": "한국어",
  "locale.en": "English",

  /* 쪽 이동 */
  "pager.first": "첫 페이지",
  "pager.prev": "이전 페이지",
  "pager.next": "다음 페이지",
  "pager.last": "마지막 페이지",
  "pager.page.aria": "페이지 번호",
  "pager.of_one": "/ {n} 페이지",
  "pager.of_other": "/ {n} 페이지",

  /* 도구 모음 */
  "tools.pin": "왼쪽 고정",
  "tools.pin.less.aria": "고정할 열 줄이기",
  "tools.pin.more.aria": "고정할 열 늘리기",
  "tools.header": "헤더",
  "tools.header.single": "{a}행",
  "tools.header.range": "{a}–{b}행",
  "tools.header.up.aria": "헤더 시작 행 위로",
  "tools.header.up.tip": "헤더가 시작되는 행을 위로",
  "tools.header.down.aria": "헤더 시작 행 아래로",
  "tools.header.down.tip": "헤더가 시작되는 행을 아래로",
  "tools.header.less.aria": "헤더 행 수 줄이기",
  "tools.header.less.tip": "헤더로 쓸 행 수를 줄입니다",
  "tools.header.more.aria": "헤더 행 수 늘리기",
  "tools.header.more.tip": "헤더로 쓸 행 수를 늘립니다",
  "tools.print": "인쇄",
  "tools.print.busy": "준비하는 중…",
  "tools.print.tip": "지금 적용된 정렬·필터로 걸러진 전체를 인쇄합니다",
  "tools.columns.reset": "열 배치 되돌리기",
  "tools.columns.reset.tip": "열 순서와 폭을 처음 상태로",
  "tools.sorts.clear": "정렬 해제",
  "tools.filters.clear": "필터 해제",
  "tools.grouping": "천 단위 쉼표",
  "tools.grouping.tip": "숫자 칸에 1,000 처럼 쉼표를 넣습니다",
  "tools.sorts": "정렬",
  "tools.help": "도움말",
  "tools.help.text": "열 제목 클릭: 정렬 · 끌기: 순서 바꾸기 · 경계선 끌기: 폭 조절",
  "tools.sort.remove.tip": "{label} 정렬 해제",

  /* 표 */
  "column.letter": "{letter}열",
  "grid.status": "{from}–{to} / {total}",
  "grid.rows_one": "{n}행",
  "grid.rows_other": "{n}행",
  "grid.empty": "조건에 맞는 행이 없습니다. 필터를 풀어 보십시오.",
  "grid.head.tip": "{label}\n클릭: 오름차순 → 내림차순 → 해제 · 끌기: 순서 바꾸기",
  "grid.sort.asc.aria": "오름차순으로 정렬됨",
  "grid.sort.desc.aria": "내림차순으로 정렬됨",
  "grid.resize.aria": "{label} 폭 조절",
  "grid.search.ph": "검색",
  "grid.search.aria": "{label} 검색",
  "grid.filter.aria": "{label} 필터",
  "grid.filter.tip": "{label} 필터",

  /* 값 선택 */
  "filter.all": "전체",
  "filter.selected_one": "{n}개 선택",
  "filter.selected_other": "{n}개 선택",
  "filter.excluded_one": "{n}개 제외",
  "filter.excluded_other": "{n}개 제외",
  "filter.search": "값 검색",
  "filter.loading": "불러오는 중…",
  "filter.checkall.shown": "검색 결과 모두 선택",
  "filter.checkall": "모두 선택",
  "filter.none": "일치하는 값이 없습니다",
  "filter.blank": "(비어 있음)",
  "filter.truncated_one": "값의 종류가 많아 {n}개까지만 보여 줍니다. 검색으로 좁혀 주십시오.",
  "filter.truncated_other": "값의 종류가 많아 {n}개까지만 보여 줍니다. 검색으로 좁혀 주십시오.",
  "filter.cancel": "취소",
  "filter.apply": "적용",

  /* 인쇄 */
  "print.sorts": "정렬",
  "print.filters": "필터",
  "print.count": "행 수",
  "print.printed": "출력",
  "print.no": "#",
  "print.rows_one": "{n}행 인쇄",
  "print.rows_other": "{n}행 인쇄",
  "print.oftotal_one": "(조건에 맞는 전체 {n}행 가운데 일부)",
  "print.oftotal_other": "(조건에 맞는 전체 {n}행 가운데 일부)",
  "print.truncated_one": "조건에 맞는 행이 많아 위에서부터 {n}행만 인쇄합니다. 필터를 더 좁혀 주십시오.",
  "print.truncated_other":
    "조건에 맞는 행이 많아 위에서부터 {n}행만 인쇄합니다. 필터를 더 좁혀 주십시오.",
  "print.asc": "오름차순",
  "print.desc": "내림차순",
  "print.contains": '"{text}" 포함',
  "print.excluded": "{list} 제외",
  "print.selected.none": "선택 없음",
  "print.more": "{list} 외 {n}개",
} as const;

/** 화면 문구의 열쇠. **한국어 벌이 정본이다.** */
export type UiKey = keyof typeof ui;
