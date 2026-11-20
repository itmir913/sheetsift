/**
 * **창 단추를 묘사할지 판정하는 한 곳.**
 *
 * 세 환경에서 답이 상이하고, 셋 다 **오류가 발생하지 않는 부류**다.
 * - 브라우저에서 단추가 묘사되면 **눌러도 아무 작업도 수행하지 않는다.**
 * - 맥에서 묘사되면 운영체제의 신호등과 **두 벌**이 되고, 우리 것은 눌러 보기 이전의 모양도
 *   전체 화면도 수행하지 않는다.
 * - 윈도에서 묘사되지 않으면 **창을 폐쇄할 길이 전부 소멸한다.**
 *
 * 판정이 한 곳에 집약되어 있으므로 그 한 곳만 흔들어 넷을 전부 판정한다.
 */
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  onTitleBarDoubleClick,
  provideWindowFrame,
  reservesTrafficLights,
  showsWindowButtons,
  type WindowFrame,
} from "./useWindowFrame";

/** 창의 조작을 흉내 낸다. `nativeTitleBar` 는 **비동기**다 — 창에 물어야 알 수 있다. */
function frame(isDesktop: boolean, native: boolean, hold?: Promise<boolean>): WindowFrame {
  return {
    isDesktop,
    nativeTitleBar: () => hold ?? Promise.resolve(native),
    minimize: vi.fn(),
    toggleMaximize: vi.fn(),
    close: vi.fn(),
  };
}

/** 주입하고 비동기 판정이 도달하기를 기다린다. */
async function settled(f: WindowFrame) {
  provideWindowFrame(f);
  /* **큰 작업 하나를 기다린다.** `nextTick` 은 Vue 의 대기열만 비우고, 창에 묻는 약속은
     그쪽 대기열에 부재한다 — 작은 작업 둘로는 도달하지 않는 경우가 있다. */
  await new Promise((res) => setTimeout(res, 0));
  return { shows: showsWindowButtons.value, lights: reservesTrafficLights.value };
}

describe("who draws the window buttons", () => {
  it("a browser gets neither the buttons nor the reserved room", async () => {
    /* 눌러도 아무 작업도 수행하지 않는 단추가 보이는 것이 가장 흔한 결함이다. */
    expect(await settled(frame(false, false))).toEqual({ shows: false, lights: false });
  });

  it("a mac gets no buttons of ours, only room for the system ones", async () => {
    /* 운영체제가 신호등을 그린다. 우리가 또 그리면 두 벌이 된다. */
    expect(await settled(frame(true, true))).toEqual({ shows: false, lights: true });
  });

  it("windows gets our buttons and no reserved room", async () => {
    /* 제목 표시줄을 걷었으므로 우리가 그리지 않으면 창을 폐쇄할 길이 소멸한다. */
    expect(await settled(frame(true, false))).toEqual({ shows: true, lights: false });
  });

  it("the two are exclusive — no input makes both true", async () => {
    for (const [desktop, native] of [
      [false, false],
      [false, true],
      [true, false],
      [true, true],
    ] as const) {
      const got = await settled(frame(desktop, native));
      expect(got.shows && got.lights).toBe(false);
    }
  });

  it("nothing is drawn until the answer is in", async () => {
    /* `nativeTitleBar` 가 비동기라 최초 묘사 시점에는 답이 부재한다. 거짓으로 시작하면
       **맥에서 우리 단추가 한 프레임 점멸한 뒤 소멸하고**, 참으로 시작하면 윈도에서 자리가
       점멸한다. 모르는 동안에는 양쪽 모두 묘사하지 않는다. */
    let answer!: (yes: boolean) => void;
    const pending = new Promise<boolean>((res) => {
      answer = res;
    });
    provideWindowFrame(frame(true, false, pending));
    await new Promise((res) => setTimeout(res, 0));
    expect(showsWindowButtons.value).toBe(false);
    expect(reservesTrafficLights.value).toBe(false);

    answer(false);
    await pending;
    await new Promise((res) => setTimeout(res, 0));
    expect(showsWindowButtons.value).toBe(true);
  });

  it("an app that provides no frame draws nothing", async () => {
    /* `windowFrame` 은 선택 항목이다 — 넘기지 않은 앱은 창 조작이 부재하는 앱이다. */
    expect(await settled(frame(false, false))).toEqual({ shows: false, lights: false });
  });
});

describe("double-clicking the title bar toggles maximize", () => {
  /* **tauri 가 수행해 주지 않는다** — 그 쪽 문서는 손으로 기재하는 예시로만 제시한다. */
  let f: WindowFrame;

  beforeEach(async () => {
    f = frame(true, false);
    await settled(f);
  });

  it("toggles on the second click", () => {
    onTitleBarDoubleClick({ detail: 2 } as MouseEvent);
    expect(f.toggleMaximize).toHaveBeenCalledTimes(1);
  });

  it("leaves a single click alone — that one starts a drag", () => {
    /* 한 번 누름은 끌기의 개시다. 거기서 전환하면 창을 이동할 수 없다. */
    onTitleBarDoubleClick({ detail: 1 } as MouseEvent);
    expect(f.toggleMaximize).not.toHaveBeenCalled();
  });

  it("does nothing in a browser", async () => {
    const web = frame(false, false);
    await settled(web);
    onTitleBarDoubleClick({ detail: 2 } as MouseEvent);
    expect(web.toggleMaximize).not.toHaveBeenCalled();
  });
});
