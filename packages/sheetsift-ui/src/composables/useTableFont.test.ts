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
/* 모듈이 `installed` 와 `tableFont` 를 모듈 수준으로 들고 있어, 시험마다 새로 반입한다.
   접두사를 든 `inject` 도 함께 새로 반입되므로 그쪽에 다시 주입한다. */
async function fresh() {
  vi.resetModules();
  (await import("../lib/inject")).provideStoragePrefix("test");
  return import("./useTableFont");
}

beforeEach(() => {
  store.clear();
  blocked = false;
});

describe("useTableFont", () => {
  it("defaults to 16px when nothing is stored", async () => {
    const m = await fresh();
    m.installTableFont();
    expect(m.tableFont.value).toBe(16);
  });

  it("moves one step at a time and stops at both ends", async () => {
    const m = await fresh();
    m.installTableFont();
    for (let i = 0; i < 10; i += 1) m.stepTableFont(1);
    expect(m.tableFont.value).toBe(22);
    for (let i = 0; i < 10; i += 1) m.stepTableFont(-1);
    expect(m.tableFont.value).toBe(14);
  });

  it("writes the choice under <prefix>.fontSize and reads it on the next install", async () => {
    const m = await fresh();
    m.installTableFont();
    m.stepTableFont(1);
    expect(store.get("test.fontSize")).toBe("18");
    const again = await fresh();
    again.installTableFont();
    expect(again.tableFont.value).toBe(18);
  });

  it("falls back to the default for a stored value that is not a step", async () => {
    for (const bad of ["17", "abc", "", "16.5", "-14"]) {
      store.set("test.fontSize", bad);
      const m = await fresh();
      m.installTableFont();
      expect(m.tableFont.value).toBe(16);
    }
  });

  it("does not throw when storage is blocked, and still changes for this session", async () => {
    blocked = true;
    const m = await fresh();
    m.installTableFont();
    expect(() => m.stepTableFont(1)).not.toThrow();
    expect(m.tableFont.value).toBe(18);
  });
});

describe("useTableFont edge cases", () => {
  it("installs only once", async () => {
    const m = await fresh();
    m.installTableFont();
    m.stepTableFont(1);
    /* A second install must not re-read storage over the live value. */
    store.set("test.fontSize", "14");
    m.installTableFont();
    expect(m.tableFont.value).toBe(18);
  });

  it("steps from the default even before install", async () => {
    const m = await fresh();
    m.stepTableFont(-1);
    expect(m.tableFont.value).toBe(14);
    expect(store.get("test.fontSize")).toBe("14");
  });

  it("keeps the default inside the steps", async () => {
    const m = await fresh();
    expect(m.TABLE_FONT_STEPS).toContain(m.DEFAULT_TABLE_FONT);
  });
});
