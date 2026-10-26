import { nextTick } from "vue";

/**
 * **묘사가 화면에 도달할 때까지 대기한다.**
 *
 * 브라우저 벌의 바탕은 즉시 응답한다 — `Book.open` 은 wasm 을 **동기로**
 * 호출하므로, 그 문이 반환할 때까지 주 흐름이 점유된다. 그 직전에 '읽는 중'
 * 으로 전환해도 Vue 가 DOM 을 갱신하는 것과 브라우저가 그것을 묘사하는 것은
 * 동일한 흐름이라, **전환이 화면에 도달하기 이전에 점유가 개시된다.** 사람이
 * 보는 것은 조작했는데 아무 작용도 발생하지 않다가 수 초 뒤 표가 출현하는
 * 것이고, 그동안 창은 응답하지 않는다.
 *
 * `nextTick` 만으로는 부족하다. 그것은 DOM 이 갱신된 시점이고 묘사된 시점이
 * 아니다. 그래서 프레임 둘을 경과시킨다 — 첫 프레임이 묘사를 요청하고, 다음
 * 프레임이 그것이 도달한 이후다. 마지막 `setTimeout` 은 그 프레임의 작업이
 * 종료된 이후로 이동시킨다.
 *
 * **점유 자체가 소멸하는 것은 아니다.** 그것을 소거하려면 바탕을 작업자
 * (worker)로 이전해야 하고, 그것은 별도 사유의 변경이다. 여기서 보전하는 것은
 * **점유되기 이전에 진술이 도달한다**는 것 하나다.
 */
export async function painted(): Promise<void> {
  await nextTick();
  if (typeof requestAnimationFrame !== "function") return;
  await new Promise<void>((resolve) => {
    requestAnimationFrame(() => {
      requestAnimationFrame(() => setTimeout(resolve, 0));
    });
  });
}
