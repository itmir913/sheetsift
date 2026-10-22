import type { ErrorCode } from "./codes";

/**
 * 오류 코드를 사람이 읽을 말로.
 *
 * **같은 그릇이 앱마다 다른 말로 안내된다.** 바탕은 `SHEET_EMPTY` 만 돌려주고,
 * 그것을 "머리글 줄이 없습니다" 로 적을지 "빈 시트입니다" 로 적을지는 쓰는 쪽의
 * 몫이다. 여기는 **표를 열어 보는 화면**의 말투다.
 *
 * 보는 겹이 영영 내지 않을 코드에도 한 줄을 적는다 — 그릇의 코드가 한 enum 이라
 * 비워 두면 그날 화면에 코드가 그대로 뜬다. 그런 줄은 그렇게 적어 둔다.
 */
export const errors: Record<ErrorCode, string> = {
  UNKNOWN: "알 수 없는 오류입니다.",

  DB_QUERY_FAILED: "조회하지 못했습니다.",
  INVALID_SORT_KEY: "표에 없는 열을 가리켰습니다.",

  SHEET_WORKBOOK_UNREADABLE: "파일을 열지 못했습니다. xlsx 또는 csv 가 맞는지 확인해 주세요.",
  SHEET_UNREADABLE: "'{sheet}' 시트를 읽지 못했습니다.",
  SHEET_EMPTY: "'{sheet}' 시트에서 머리글 줄을 찾지 못했습니다.",
  SHEET_COLUMN_MISSING: "'{sheet}' 시트의 열을 맞추지 못했습니다.",
  SHEET_TABLE_FAILED: "읽어 온 자료를 표로 바꾸지 못했습니다.",
  SHEET_ENCODING_UNKNOWN:
    "csv 의 글자 표기를 알아내지 못했습니다. UTF-8 또는 CP949 로 저장해 주세요.",
  SHEET_SERIALIZE_FAILED: "자료를 내보내지 못했습니다.",

  // 아래는 이 화면이 내지 않는 코드다. 봉한 파일을 열거나 쓰는 자리가 없다.
  // 그래도 비워 두지 않는다 — 비면 그날 코드가 그대로 화면에 뜬다.
  DATA_SEAL_INVALID: "봉한 자료를 열지 못했습니다.",
  DATA_SIGNATURE_INVALID: "자료의 서명이 맞지 않습니다.",
  DATA_CONTAINER_UNKNOWN: "모르는 형식의 자료입니다.",
  VAULT_OPEN_FAILED: "보관함을 열지 못했습니다.",
  VAULT_FORMAT_INVALID: "보관함의 형식이 맞지 않습니다.",
  VAULT_PASSWORD_WRONG: "비밀번호가 맞지 않습니다.",
  VAULT_PASSWORD_TOO_SHORT: "비밀번호가 너무 짧습니다.",
  VAULT_VERSION_TOO_NEW: "이 화면이 모르는 판의 보관함입니다.",
  VAULT_CONTAINER_UNKNOWN: "모르는 형식의 보관함입니다.",
  VAULT_FILE_DAMAGED: "보관함 파일이 상했습니다.",
  VAULT_KEY_DERIVATION_FAILED: "보관함의 열쇠를 만들지 못했습니다.",
  VAULT_SEAL_FAILED: "보관함을 봉하지 못했습니다.",
  VAULT_WRITE_FAILED: "보관함을 쓰지 못했습니다.",
  SIFT_FORMAT_INVALID: "`.sift` 파일의 형식이 맞지 않습니다.",
  SIFT_SCHEMA_UNEXPECTED: "`.sift` 파일의 속이 우리가 만든 생김새가 아닙니다.",
  SIFT_META_INVALID: "`.sift` 파일에 담긴 열 정보를 읽지 못했습니다.",
  SIFT_VERSION_TOO_NEW: "이 화면이 모르는 판의 `.sift` 파일입니다.",
};

/** 화면의 말. 오류가 아닌 것들. */
export const ui = {
  loading: "불러오는 중",
  retry: "다시 시도",
  themeLight: "밝게",
  themeDark: "어둡게",
  themeSystem: "시스템",
} as const;
