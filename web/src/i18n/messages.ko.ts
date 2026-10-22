import type { ErrorCode } from "./codes";

/**
 * 오류 코드를 사람이 판독할 문구로.
 *
 * **동일한 그릇이 앱마다 상이한 문구로 안내된다.** 바탕은 `SHEET_EMPTY` 만
 * 반환하고, 그것을 "머리글 행이 부재합니다" 로 기재할지 "공백 시트입니다" 로
 * 기재할지는 쓰는 곳의 소관이다. 여기는 **표를 개방해 열람하는 화면**의 어조다.
 *
 * 열람 겹이 영구히 발생시키지 않을 코드에도 한 행을 기재한다 — 그릇의 코드가
 * 단일 enum 이라 공백으로 두면 그 시점에 화면에 코드가 그대로 표시된다. 그러한
 * 행은 그렇게 기재해 둔다.
 */
export const errors: Record<ErrorCode, string> = {
  UNKNOWN: "미상의 오류입니다.",

  DB_QUERY_FAILED: "조회하지 못했습니다.",
  INVALID_SORT_KEY: "표에 부재하는 열을 지시했습니다.",

  SHEET_WORKBOOK_UNREADABLE: "파일을 개방하지 못했습니다. xlsx 또는 csv 가 맞는지 확인해 주십시오.",
  SHEET_UNREADABLE: "'{sheet}' 시트를 판독하지 못했습니다.",
  SHEET_EMPTY: "'{sheet}' 시트에서 머리글 행을 탐색하지 못했습니다.",
  SHEET_COLUMN_MISSING: "'{sheet}' 시트의 열을 정합하지 못했습니다.",
  SHEET_TABLE_FAILED: "판독한 자료를 표로 변환하지 못했습니다.",
  SHEET_ENCODING_UNKNOWN:
    "csv 의 문자 표기를 판별하지 못했습니다. UTF-8 또는 CP949 로 저장해 주십시오.",
  SHEET_SERIALIZE_FAILED: "자료를 반출하지 못했습니다.",

  // 하기는 이 화면이 발생시키지 않는 코드다. 봉인한 파일을 개방하거나 기록하는
  // 위치가 부재한다. 그래도 공백으로 두지 않는다 — 공백이면 그 시점에 코드가
  // 그대로 화면에 표시된다.
  DATA_SEAL_INVALID: "봉인한 자료를 개방하지 못했습니다.",
  DATA_SIGNATURE_INVALID: "자료의 서명이 일치하지 않습니다.",
  DATA_CONTAINER_UNKNOWN: "미지 형식의 자료입니다.",
  VAULT_OPEN_FAILED: "보관함을 개방하지 못했습니다.",
  VAULT_FORMAT_INVALID: "보관함의 형식이 일치하지 않습니다.",
  VAULT_PASSWORD_WRONG: "비밀번호가 일치하지 않습니다.",
  VAULT_PASSWORD_TOO_SHORT: "비밀번호가 너무 짧습니다.",
  VAULT_VERSION_TOO_NEW: "이 화면이 인지하지 못하는 판의 보관함입니다.",
  VAULT_CONTAINER_UNKNOWN: "미지 형식의 보관함입니다.",
  VAULT_FILE_DAMAGED: "보관함 파일이 손상되었습니다.",
  VAULT_KEY_DERIVATION_FAILED: "보관함의 열쇠를 생성하지 못했습니다.",
  VAULT_SEAL_FAILED: "보관함을 봉인하지 못했습니다.",
  // 하기 셋은 데스크톱 껍질만 발생시킨다. 브라우저에는 경로가 부재한다.
  FILE_READ_FAILED: "'{name}' 을(를) 판독하지 못했습니다. 파일이 존재하는지 확인해 주십시오.",
  FILE_WRITE_FAILED: "'{name}' 에 기록하지 못했습니다. 위치의 권한과 잔여 용량을 확인해 주십시오.",
  NO_WORKBOOK_OPEN: "개방한 양식이 부재합니다. 파일을 먼저 선택해 주십시오.",

  VAULT_WRITE_FAILED: "보관함을 기록하지 못했습니다.",
  SIFT_FORMAT_INVALID: "`.sift` 파일의 형식이 일치하지 않습니다.",
  SIFT_SCHEMA_UNEXPECTED: "`.sift` 파일의 내용이 우리가 생성한 구조가 아닙니다.",
  SIFT_META_INVALID: "`.sift` 파일에 수록된 열 정보를 판독하지 못했습니다.",
  SIFT_VERSION_TOO_NEW: "이 화면이 인지하지 못하는 판의 `.sift` 파일입니다.",
};

/** 화면의 문구. 오류가 아닌 것들. */
export const ui = {
  loading: "준비 중",
  retry: "재시도",
  themeLight: "밝게",
  themeDark: "어둡게",
  themeSystem: "시스템",
} as const;
