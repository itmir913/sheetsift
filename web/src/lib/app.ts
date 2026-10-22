/**
 * 이 화면이 무엇이라 불리는가.
 *
 * **한자리에 둔다.** 예전 하네스는 같은 낱말이 `index.html` 의 제목과 머리글에
 * 따로 적혀 있었다. 그러면 이름을 바꿀 때 한쪽만 바뀐 채 남는데, 빌드도 시험도
 * 그것을 모른다.
 */
export const APP_NAME = "sheetsift";

/** 머리글 옆에 적는 한 줄. */
export const APP_TAGLINE = "자료는 이 브라우저 밖으로 나가지 않습니다";

/**
 * 브라우저 저장소 키의 접두사.
 *
 * 화면이 기억하는 것들(테마·열 배치)이 한 이름 아래 모인다. 같은 자리에 다른
 * 것이 올라가도 저장소를 나눠 쓰지 않는다.
 */
export const STORAGE_PREFIX = "sheetsift";

/** 저장소 키를 짓는다. */
export const storageKey = (name: string) => `${STORAGE_PREFIX}.${name}`;
