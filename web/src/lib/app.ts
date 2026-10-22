/**
 * 이 화면의 명칭.
 *
 * **단일 위치에 배치한다.** 구 하네스는 동일한 어휘가 `index.html` 의 제목과
 * 머리글에 개별 기재되어 있었다. 그러면 명칭을 변경할 때 한쪽만 변경된 채
 * 잔존하는데, 빌드도 시험도 그것을 인지하지 못한다.
 */
export const APP_NAME = "sheetsift";

/** 머리글 옆에 기재하는 한 행. */
export const APP_TAGLINE = "자료는 이 브라우저 외부로 반출되지 않습니다";

/**
 * 브라우저 저장소 키의 접두사.
 *
 * 화면이 기억하는 것들(테마·열 배치)이 단일 명칭 하위에 집약된다. 동일한 위치에
 * 타 항목이 배포되어도 저장소를 공유하지 않는다.
 */
export const STORAGE_PREFIX = "sheetsift";

/** 저장소 키를 구성한다. */
export const storageKey = (name: string) => `${STORAGE_PREFIX}.${name}`;
