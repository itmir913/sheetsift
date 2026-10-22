/**
 * 바탕(`sheetsift-core`)이 돌려주는 오류 코드 목록.
 *
 * ⚠ `crates/core/src/error.rs` 의 `ErrorCode` 와 반드시 같아야 한다.
 *    `crates/web/tests/messages.rs` 가 두 파일을 대조한다.
 *
 * **바탕은 사람이 읽을 문구를 만들지 않는다.** 코드와 치환 값만 넘기고, 말로
 * 바꾸는 일은 이 디렉터리의 사전이 한다.
 *
 * 쓰지 않는 코드까지 적는다. 그릇의 오류 코드가 한 enum 이라, 보는 겹이 영영
 * 내지 않을 코드도 여기 한 줄이 있어야 한다 — 없으면 그날 화면에 코드가
 * 그대로 뜬다.
 */
export const ERROR_CODES = [
  /** 바탕이 아니라 겹에서 난 것. 오류를 글자로조차 바꾸지 못한 자리. */
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
  "SHEET_WORKBOOK_UNREADABLE",
  "SHEET_UNREADABLE",
  "SHEET_EMPTY",
  "SHEET_COLUMN_MISSING",
  "SHEET_TABLE_FAILED",
  "SHEET_ENCODING_UNKNOWN",
  "SHEET_SERIALIZE_FAILED",
  "SIFT_FORMAT_INVALID",
  "SIFT_SCHEMA_UNEXPECTED",
  "SIFT_META_INVALID",
  "SIFT_VERSION_TOO_NEW",
] as const;

export type ErrorCode = (typeof ERROR_CODES)[number];

/** 바탕이 던진 것의 생김새. */
export interface AppErrorPayload {
  code: ErrorCode;
  /** 문구의 `{이름}` 자리에 채울 값. */
  params?: Record<string, string>;
  /** 개발자용 원문(영문). 사람에게 보이지 않는다. */
  detail?: string;
}
