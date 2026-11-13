import { nextTick } from "vue";
import { beforeEach, describe, expect, it, vi } from "vitest";

/* 시험에 쓰는 값이다. 저장소는 메모리 흉내이고 접두사는 시험 전용이다. */
const store = new Map<string, string>();
let blocked = false;
Object.assign(globalThis, {
  localStorage: {
    getItem: (k: string) => {
      if (blocked) throw new Error("blocked");
      return store.get(k) ?? null;
    },
    setItem: (k: string, v: string) => {
      if (blocked) throw new Error("blocked");
      store.set(k, v);
    },
  },
});

/* 모듈이 `installed` 와 `wrap` 을 모듈 수준으로 들고 있어, 시험마다 새로 반입한다.
   접두사를 든 `inject` 도 함께 새로 반입되므로 그쪽에 다시 주입한다. */
async function fresh() {
  vi.resetModules();
  (await import("../lib/inject")).provideStoragePrefix("test");
  return import("./useWrap");
}

beforeEach(() => {
  store.clear();
  blocked = false;
});

describe("useWrap", () => {
  it("defaults to off when nothing is stored", async () => {
    const m = await fresh();
    m.installWrap();
    expect(m.wrap.value).toBe(false);
  });

  it("writes the choice under <prefix>.wrap and reads it on the next install", async () => {
    const m = await fresh();
    m.installWrap();
    m.setWrap(true);
    await nextTick();
    expect(store.get("test.wrap")).toBe("1");

    const again = await fresh();
    again.installWrap();
    expect(again.wrap.value).toBe(true);
  });

  it("writes the off choice too, so it survives a default that is on elsewhere", async () => {
    const m = await fresh();
    m.installWrap();
    m.setWrap(true);
    await nextTick();
    m.setWrap(false);
    await nextTick();
    expect(store.get("test.wrap")).toBe("0");
  });

  /* 보관된 값은 `"1"` 하나만 켠 것으로 판독한다 — 그 외의 글자(수기로 변경한 값·
     판이 상이한 앱이 기재한 값)에서 묵시적으로 켜지지 않아야 한다. */
  it("treats anything but \"1\" as off", async () => {
    for (const bad of ["0", "true", "yes", "", "2", "01"]) {
      store.set("test.wrap", bad);
      const m = await fresh();
      m.installWrap();
      expect(m.wrap.value).toBe(false);
    }
  });

  it("does not throw when storage is blocked, and still changes for this session", async () => {
    blocked = true;
    const m = await fresh();
    m.installWrap();
    expect(m.wrap.value).toBe(false);
    m.setWrap(true);
    await nextTick();
    expect(m.wrap.value).toBe(true);
  });

  it("installs only once", async () => {
    const m = await fresh();
    m.installWrap();
    m.setWrap(true);
    await nextTick();
    /* A second install must not re-read storage over the live value. */
    store.set("test.wrap", "0");
    m.installWrap();
    expect(m.wrap.value).toBe(true);
  });

  /* 장착 이전에는 보관하지 않는다 — 접두사가 주입되지 않은 시점에 기재하면
     열쇠가 상이한 곳에 적재된다. */
  it("changes before install affect this session only", async () => {
    const m = await fresh();
    m.setWrap(true);
    await nextTick();
    expect(m.wrap.value).toBe(true);
    expect(store.has("test.wrap")).toBe(false);
  });
});
