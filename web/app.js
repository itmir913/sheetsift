/**
 * 화면. 바탕(wasm)에 글자를 보내고 글자를 받아 그린다.
 *
 * **사람이 읽을 문구는 전부 여기 있다.** 바탕은 오류 코드와 치환 값만
 * 돌려주고, 그것을 말로 바꾸는 일은 이쪽 몫이다 — 같은 그릇이 앱마다 다른
 * 말로 안내되어야 하기 때문이다.
 */
import init, { Book } from "./pkg/sheetsift_web.js";

/** 한 쪽에 보여 줄 줄 수. */
const PAGE = 200;

/**
 * 오류 코드를 말로 바꾼다. **모르는 코드를 삼키지 않는다** — 코드 그대로
 * 보여 주어, 사전에 빠진 것이 그 자리에서 드러나게 한다.
 */
const SAY = {
  SHEET_WORKBOOK_UNREADABLE: "파일을 열지 못했습니다. xlsx 또는 csv 가 맞는지 확인해 주세요.",
  SHEET_UNREADABLE: "그 시트를 읽지 못했습니다.",
  SHEET_EMPTY: "시트에 머리글 줄이 없습니다.",
  SHEET_COLUMN_MISSING: "시트에 '{column}' 열이 없습니다.",
  SHEET_TABLE_FAILED: "읽어 온 자료를 표로 바꾸지 못했습니다.",
  SHEET_ENCODING_UNKNOWN:
    "CSV 의 글자 인코딩을 확인하지 못했습니다. UTF-8 또는 CP949 로 저장해 주세요.",
  SHEET_SERIALIZE_FAILED: "자료를 내보내지 못했습니다.",
  DB_QUERY_FAILED: "조회하지 못했습니다.",
  INVALID_SORT_KEY: "표에 없는 열을 가리켰습니다.",
};

/** wasm 이 던진 것을 사람이 읽을 말로. */
function speak(thrown) {
  let payload;
  try {
    payload = JSON.parse(typeof thrown === "string" ? thrown : String(thrown?.message ?? thrown));
  } catch {
    return String(thrown);
  }
  const template = SAY[payload.code];
  if (!template) return `알 수 없는 오류입니다 (${payload.code ?? "?"})`;
  return template.replace(/\{(\w+)\}/g, (whole, key) => payload.params?.[key] ?? whole);
}

/* ── 화면 조각 ──────────────────────────────────────────────── */

const $ = (id) => document.getElementById(id);
const el = { file: $("file"), drop: $("drop"), status: $("status"), sheets: $("sheets"),
  bar: $("bar"), q: $("q"), count: $("count"), sorts: $("sorts"), wrap: $("wrap"),
  head: $("head"), body: $("body"), pager: $("pager"), prev: $("prev"), next: $("next"),
  where: $("where") };

/** 지금 들고 있는 것. */
const state = { book: null, columns: [], sorts: [], offset: 0, total: 0 };

function tell(text, kind = "") {
  el.status.textContent = text;
  el.status.className = kind ? `status ${kind}` : "status";
}

function show(on) {
  for (const node of [el.sheets, el.bar, el.wrap, el.pager]) node.hidden = !on;
}

/* ── 파일 받기 ──────────────────────────────────────────────── */

async function take(file) {
  if (!file) return;
  tell(`${file.name} 을(를) 읽는 중입니다…`);
  show(false);
  try {
    const bytes = new Uint8Array(await file.arrayBuffer());
    state.book = new Book(bytes, file.name);
    const { sheets } = JSON.parse(state.book.sheets());
    drawSheets(sheets);
    if (sheets.length > 0) await openSheet(0);
  } catch (thrown) {
    state.book = null;
    tell(speak(thrown), "bad");
  }
}

el.file.addEventListener("change", () => take(el.file.files[0]));
for (const name of ["dragenter", "dragover"]) {
  el.drop.addEventListener(name, (e) => {
    e.preventDefault();
    el.drop.classList.add("over");
  });
}
for (const name of ["dragleave", "drop"]) {
  el.drop.addEventListener(name, () => el.drop.classList.remove("over"));
}
el.drop.addEventListener("drop", (e) => {
  e.preventDefault();
  take(e.dataTransfer?.files?.[0]);
});

/* ── 시트 ───────────────────────────────────────────────────── */

function drawSheets(names) {
  el.sheets.replaceChildren();
  names.forEach((name, at) => {
    const b = document.createElement("button");
    b.type = "button";
    b.textContent = name;
    b.dataset.at = String(at);
    b.addEventListener("click", () => openSheet(at));
    el.sheets.append(b);
  });
  el.sheets.hidden = names.length === 0;
}

async function openSheet(at) {
  for (const b of el.sheets.children) {
    b.setAttribute("aria-current", String(Number(b.dataset.at) === at));
  }
  try {
    const view = JSON.parse(state.book.openSheet(at));
    if (!view.headerFound) {
      show(false);
      el.sheets.hidden = false;
      tell(
        `'${view.sheet}' 에서 머리글 줄을 확인하지 못했습니다. 표가 아닌 시트일 수 있습니다.`,
        "warn",
      );
      return;
    }
    state.columns = view.columns;
    // **정렬은 지우고 검색어는 둔다.** 정렬은 그 시트의 열을 가리키므로 시트가
    // 바뀌면 뜻이 없어진다. 전체 검색은 열 이름을 가리키지 않아 시트가 바뀌어도
    // 뜻이 있고, 무엇보다 검색창에 글자가 그대로 보인다 — 보이는 것과 걸린
    // 것이 달라지지 않는다.
    state.sorts = [];
    state.offset = 0;
    drawHead();
    const guessed = view.columns.filter((c) => c.guessed);
    if (guessed.length > 0) {
      tell(
        `열의 종류를 확인하지 못한 곳이 있습니다: ${guessed.map((c) => c.label).join(", ")}.` +
          " 숫자와 글자가 섞여 있어 글자로 다룹니다.",
        "warn",
      );
    } else {
      tell("");
    }
    show(true);
    await draw();
  } catch (thrown) {
    show(false);
    el.sheets.hidden = false;
    tell(speak(thrown), "bad");
  }
}

/* ── 표 ─────────────────────────────────────────────────────── */

function drawHead() {
  el.head.replaceChildren();
  for (const c of state.columns) {
    const th = document.createElement("th");
    th.textContent = c.label;
    th.title = `${c.label} — 눌러서 정렬합니다`;
    if (c.guessed) {
      const mark = document.createElement("span");
      mark.className = "guessed";
      mark.textContent = " (종류 미확인)";
      th.append(mark);
    }
    th.addEventListener("click", () => toggleSort(c.key));
    el.head.append(th);
  }
}

function toggleSort(key) {
  const at = state.sorts.findIndex((s) => s.key === key);
  if (at < 0) state.sorts.unshift({ key, dir: "asc" });
  else if (state.sorts[at].dir === "asc") state.sorts[at].dir = "desc";
  else state.sorts.splice(at, 1);
  state.offset = 0;
  draw();
}

function drawSorts() {
  el.sorts.replaceChildren();
  state.sorts.forEach((s) => {
    const label = state.columns.find((c) => c.key === s.key)?.label ?? s.key;
    const b = document.createElement("button");
    b.type = "button";
    b.textContent = `${label} ${s.dir === "asc" ? "오름" : "내림"} ✕`;
    b.addEventListener("click", () => toggleSort(s.key));
    el.sorts.append(b);
  });
}

/** 전체 검색은 모든 열을 OR 로 묶는다. */
function filters() {
  const text = el.q.value.trim();
  if (!text || state.columns.length === 0) return [];
  return [{ key: state.columns[0].key, keys: state.columns.map((c) => c.key), contains: text }];
}

async function draw() {
  try {
    const ask = { sorts: state.sorts, filters: filters(), offset: state.offset, limit: PAGE };
    const page = JSON.parse(state.book.page(JSON.stringify(ask)));
    state.total = page.total;

    const body = document.createDocumentFragment();
    for (const row of page.rows) {
      const tr = document.createElement("tr");
      row.forEach((value, i) => {
        const td = document.createElement("td");
        const number = state.columns[i]?.kind === "number";
        if (number) td.className = "num";
        // 값이 없는 칸은 비워 둔다. 글자를 적으면 자료의 값과 섞인다.
        td.textContent = value === null || value === undefined ? "" : String(value);
        tr.append(td);
      });
      body.append(tr);
    }
    el.body.replaceChildren(body);

    el.count.textContent = `${page.total.toLocaleString("ko-KR")}행 · ${page.elapsedMs.toFixed(1)}ms`;
    drawSorts();
    drawPager(page);
  } catch (thrown) {
    tell(speak(thrown), "bad");
  }
}

function drawPager(page) {
  const from = state.total === 0 ? 0 : page.offset + 1;
  const to = Math.min(page.offset + PAGE, state.total);
  el.where.textContent = `${from.toLocaleString("ko-KR")}–${to.toLocaleString("ko-KR")}`;
  el.prev.disabled = page.offset === 0;
  el.next.disabled = to >= state.total;
}

el.prev.addEventListener("click", () => {
  state.offset = Math.max(0, state.offset - PAGE);
  draw();
});
el.next.addEventListener("click", () => {
  state.offset += PAGE;
  draw();
});

/* 검색은 치는 대로 돌리지 않는다. 큰 표에서 글자마다 조회하면 손가락보다
   느려진다. */
let typing;
el.q.addEventListener("input", () => {
  clearTimeout(typing);
  typing = setTimeout(() => {
    state.offset = 0;
    draw();
  }, 200);
});

/* ── 띄우기 ─────────────────────────────────────────────────── */

init()
  .then(() => tell(""))
  .catch((e) => tell(`바탕을 띄우지 못했습니다: ${e}`, "bad"));
