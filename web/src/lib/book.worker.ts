/**
 * **바탕을 수행하는 작업자.** 브라우저 벌에서만 실재한다.
 *
 * wasm 은 동기로 작동한다 — 호출하면 반환할 때까지 그 스레드를 점유한다. 그것을
 * 주 흐름에서 호출하면 수십 MB 양식을 판독하는 수 초 동안 화면이 묘사도 조작도
 * 수령하지 못하고, 브라우저가 「응답 없음」을 기재한다. 그래서 **점유하는 것을
 * 별도 스레드로 이전한다.**
 *
 * 여기에 논리를 배치하지 않는다. 수령한 것을 그대로 wasm 에 전달하고 반환한
 * 문자를 그대로 회신하는 행뿐이다 — 해석도 분기도 경계(`book.wasm.ts`)가
 * 수행한다. 두 위치가 각자 판독하면 일방만 수정된다.
 *
 * **개방한 양식은 여기 잔류한다.** 주 흐름으로 반출할 수 없는 것이라(wasm 의
 * 손잡이다) 주고받는 것은 요청과 응답의 문자뿐이다. 창 벌이 양식을 명령 겹에
 * 보유하는 것과 동일한 분업이다.
 */
import init, { Book as WasmBook, limits as wasmLimits } from "sheetsift-wasm";

/** 주 흐름이 요청하는 작업. 명칭은 경계가 반출하는 문과 동일한 벌이다. */
export type WorkerAsk =
  | { id: number; op: "start" }
  | { id: number; op: "open"; bytes: Uint8Array; name: string }
  | { id: number; op: "listScreens" | "queryRows" | "columnValues"; request: string };

/**
 * 회신. **중단한 것도 회신으로 반환한다** — 작업자의 예외는 주 흐름의 예외가
 * 아니라, 발생시킨 채로 두면 요청한 쪽이 영구히 대기한다.
 */
export type WorkerReply =
  | { id: number; ok: true; value: string }
  | { id: number; ok: false; error: string };

let book: WasmBook | null = null;

/** 개방한 양식이 부재하는데 호출한 위치. **묵시적으로 공백을 반환하지 않는다.** */
function opened(): WasmBook {
  if (!book) throw new Error("no workbook is open");
  return book;
}

function perform(ask: WorkerAsk): string {
  switch (ask.op) {
    case "start":
      return wasmLimits();
    case "open":
      /* 선행 양식은 폐기한다. 두 벌을 보유하면 어느 것을 조회 중인지가 분기한다. */
      book = new WasmBook(ask.bytes, ask.name);
      return "null";
    case "listScreens":
      return opened().listScreens(ask.request);
    case "queryRows":
      return opened().queryRows(ask.request);
    case "columnValues":
      return opened().columnValues(ask.request);
  }
}

/**
 * wasm 을 기동한다. **1회만 기동한다.**
 *
 * 기동이 완료되기 이전에 도달한 요청도 동일한 약속을 대기한다 — 주 흐름이
 * 순서를 보전하더라도, 여기서 재기동하면 선행 기동이 구성한 것이 소멸한다.
 */
let started: Promise<void> | null = null;
function start(): Promise<void> {
  started ??= init().then(() => undefined);
  return started;
}

self.onmessage = async (ev: MessageEvent<WorkerAsk>) => {
  const ask = ev.data;
  let reply: WorkerReply;
  try {
    await start();
    reply = { id: ask.id, ok: true, value: perform(ask) };
  } catch (e) {
    /* 겹은 오류를 JSON 문자로 발생시킨다. 그 문자를 그대로 운반하고, 해석은
       경계가 수행한다 — 여기서 해석하면 판독하는 위치가 두 벌이 된다. */
    reply = { id: ask.id, ok: false, error: typeof e === "string" ? e : String((e as Error)?.message ?? e) };
  }
  self.postMessage(reply);
};
