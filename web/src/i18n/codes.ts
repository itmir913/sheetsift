/**
 * 화면에 도달할 수 있는 오류 코드 목록.
 *
 * ⚠ `crates/core/src/error.rs` 와 `crates/desk/src/error.rs` 의 `ErrorCode`
 *    양자와 반드시 일치해야 한다. `crates/shell/tests/screen.rs` 가 대조한다.
 *
 * **바탕은 사람이 판독할 문구를 생성하지 않는다.** 코드와 치환 값만 전달하고,
 * 문구로 변환하는 작업은 이 디렉터리의 사전이 수행한다.
 *
 * 사용하지 않는 코드까지 기재한다. 그릇의 오류 코드가 단일 enum 이라, 열람 겹이
 * 영구히 발생시키지 않을 코드도 여기 한 행이 존재해야 한다 — 부재하면 그 시점에
 * 화면에 코드가 그대로 표시된다.
 */
export const ERROR_CODES = [
  /** 바탕이 아니라 겹에서 발생한 것. 오류를 문자로조차 변환하지 못한 위치. */
  "UNKNOWN",
  "DB_QUERY_FAILED",
  "INVALID_SORT_KEY",
  "DATA_SEAL_INVALID",
  "DATA_SIGNATURE_INVALID",
  "DATA_CONTAINER_UNKNOWN",
  "VAULT_OPEN_FAILED",
  "VAULT_FORMAT_INVALID",
  "VAULT_PASSWORD_WRONG",
  "VAULT_PASSWORD_TOO_SHORT",
  "VAULT_VERSION_TOO_NEW",
  "VAULT_CONTAINER_UNKNOWN",
  "VAULT_FILE_DAMAGED",
  "VAULT_KEY_DERIVATION_FAILED",
  "VAULT_SEAL_FAILED",
  "VAULT_WRITE_FAILED",
  "FILE_TOO_LARGE",
  "SHEET_WORKBOOK_UNREADABLE",
  "SHEET_UNREADABLE",
  "SHEET_EMPTY",
  "SHEET_COLUMN_MISSING",
  "SHEET_TABLE_FAILED",
  "SHEET_TOO_WIDE",
  "SHEET_ENCODING_UNKNOWN",
  "SHEET_SERIALIZE_FAILED",
  "SIFT_FORMAT_INVALID",
  "SIFT_SCHEMA_UNEXPECTED",
  "SIFT_META_INVALID",
  "SIFT_VERSION_TOO_NEW",

  // 하기는 **데스크톱 껍질만** 발생시킨다(`crates/desk`). 브라우저에는 경로라는
  // 개념이 부재하므로 영구히 도달하지 않으나, 화면이 단일 벌이라 여기 존재해야
  // 한다 — 부재하면 창에서 코드가 그대로 표시된다.
  "FILE_READ_FAILED",
  "FILE_WRITE_FAILED",
  "NO_WORKBOOK_OPEN",
] as const;

export type ErrorCode = (typeof ERROR_CODES)[number];

/** 바탕이 발생시킨 것의 구조. */
export interface AppErrorPayload {
  code: ErrorCode;
  /** 문구의 `{이름}` 위치에 충전할 값. */
  params?: Record<string, string>;
  /** 개발자용 원문(영문). 사람에게 노출되지 않는다. */
  detail?: string;
}
