/**
 * 화면. 바탕(wasm)에 글자를 보내고 글자를 받아 그린다.
 *
 * **사람이 읽을 문구는 전부 여기 있다.** 바탕은 오류 코드와 치환 값만
 * 돌려주고, 그것을 말로 바꾸는 일은 이쪽 몫이다 — 같은 그릇이 앱마다 다른
 * 말로 안내되어야 하기 때문이다.
 */
import init, { Book, limits } from "./pkg/sheetsift_web.js";

/**
 * 손으로 적으면 안 되는 값들. 바탕이 내준다 — 빈칸 표식을 여기 베껴 두면
 * 바탕이 바꾸는 날 값 선택이 조용히 0건이 된다.
 */
let LIMITS = { blankToken: "", maxValues: 0, maxPage: 0 };

/** 한 쪽에 보여 줄 줄 수. 바탕의 상한을 넘지 않는다. */
let PAGE = 200;

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

/**
 * 지금 들고 있는 것.
 *
 * `picked` 는 열마다 고른 값들(`Set`)이다. 비어 있으면 그 열은 거르지 않는다 —
 * **빈 `Set` 과 "아무것도 안 고름" 을 가르지 않는다.** 하나도 안 고른 것을
 * "전부 빼라" 로 읽으면 체크를 다 풀었을 때 화면이 통째로 비고, 사람은 자료가
 * 없는 줄 안다.
 */
const state = { book: null, columns: [], sorts: [], picked: new Map(), offset: 0, total: 0 };

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
  closePicker();
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
    // **정렬과 값 선택은 지우고 검색어는 둔다.** 그 둘은 이 시트의 열과 값을
    // 가리키므로 시트가 바뀌면 뜻이 없어진다. 전체 검색은 열 이름을 가리키지
    // 않아 시트가 바뀌어도 뜻이 있고, 무엇보다 검색창에 글자가 그대로 보인다 —
    // 보이는 것과 걸린 것이 달라지지 않는다.
    state.sorts = [];
    state.picked.clear();
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

    const name = document.createElement("span");
    name.className = "name";
    name.textContent = c.label;
    name.title = `${c.label} — 눌러서 정렬합니다`;
    name.addEventListener("click", () => toggleSort(c.key));
    th.append(name);

    if (c.guessed) {
      const mark = document.createElement("span");
      mark.className = "guessed";
      mark.textContent = " (종류 미확인)";
      th.append(mark);
    }

    const pick = document.createElement("button");
    pick.type = "button";
    pick.className = "pick-btn";
    pick.textContent = "▾";
    pick.title = `${c.label} — 값을 선택합니다`;
    pick.setAttribute("aria-label", `${c.label} 값 선택`);
    // 머리글을 누르면 정렬이므로, 단추까지 올라가지 않게 막는다.
    pick.addEventListener("click", (e) => {
      e.stopPropagation();
      openPicker(c, pick);
    });
    th.append(pick);

    th.dataset.key = c.key;
    if (state.picked.get(c.key)?.size) th.classList.add("filtered");
    el.head.append(th);
  }
}

/* ── 값 선택 ────────────────────────────────────────────────── */

/** 열려 있는 선택 패널. 한 번에 하나만 연다. */
let picker = null;

function closePicker() {
  picker?.node.remove();
  picker = null;
  document.removeEventListener("keydown", onPickerKey, true);
  document.removeEventListener("pointerdown", onPickerAway, true);
  el.wrap.removeEventListener("scroll", follow);
  window.removeEventListener("scroll", follow);
  window.removeEventListener("resize", follow);
}

/**
 * 단추를 따라간다. 단추가 **표 밖으로 밀려 나가면** 닫는다 — 보이지도 않는
 * 열의 값을 고르고 있는 꼴이 되지 않게.
 */
function follow() {
  if (!picker) return;
  // 붙잡고 있던 단추가 사라졌으면 더 따라갈 곳이 없다.
  if (!picker.anchor.isConnected) {
    closePicker();
    return;
  }
  const box = picker.anchor.getBoundingClientRect();
  const view = el.wrap.getBoundingClientRect();
  if (box.right <= view.left || box.left >= view.right) {
    closePicker();
    return;
  }
  place(picker.node, picker.anchor);
}

/**
 * 단추 아래, **단추의 오른쪽에 맞춰** 건다.
 *
 * 왼쪽에 맞추면 패널이 옆 열 위로 뻗는다 — 단추가 제 열의 오른쪽 끝에 있기
 * 때문이다. 어느 열의 것인지 한눈에 보여야 한다. 화면 왼쪽 밖으로 나가는
 * 경우에만 오른쪽으로 민다.
 */
function place(node, anchor) {
  const box = anchor.getBoundingClientRect();
  const width = node.getBoundingClientRect().width;
  const room = document.documentElement.clientWidth;
  const left = Math.min(Math.max(box.right - width, 8), room - width - 8);
  node.style.top = `${Math.round(box.bottom + window.scrollY + 4)}px`;
  node.style.left = `${Math.round(left + window.scrollX)}px`;
}

function onPickerKey(e) {
  if (e.key === "Escape") closePicker();
}

function onPickerAway(e) {
  if (picker && !picker.node.contains(e.target) && e.target !== picker.anchor) closePicker();
}

async function openPicker(column, anchor) {
  if (picker?.key === column.key) {
    closePicker();
    return;
  }
  closePicker();

  const node = document.createElement("div");
  node.className = "picker";
  node.setAttribute("role", "dialog");
  node.setAttribute("aria-label", `${column.label} 값 선택`);
  node.textContent = "불러오는 중입니다…";

  /* **표 바깥에 띄운다.** 표 안에 넣으면 그 칸이 넓어져 열 폭이 통째로
     움직인다 — 단추를 눌렀다고 화면이 흔들리면 안 된다. */
  document.body.append(node);
  place(node, anchor);

  picker = { key: column.key, node, anchor };
  document.addEventListener("keydown", onPickerKey, true);
  document.addEventListener("pointerdown", onPickerAway, true);
  /* 표를 굴리면 단추가 움직인다. **패널을 따라다니게 한다** — 그 자리에
     남으면 어느 열의 것인지 알 수 없다.

     굴릴 때 닫아 보았더니 더 나빴다. 브라우저가 단추를 보이게 굴려 주는
     일이 있는데(화면 밖의 열을 누를 때), 그 **스크롤 이벤트가 클릭보다
     늦게 도착**해서 막 연 패널이 그 자리에서 닫힌다. 눌렀는데 아무 일도
     안 일어난 것처럼 보이고, 오류는 나지 않는다. */
  el.wrap.addEventListener("scroll", follow, { passive: true });
  window.addEventListener("scroll", follow, { passive: true });
  window.addEventListener("resize", follow, { passive: true });

  await refill(column, "");
}

/**
 * 후보를 받아 다시 그린다.
 *
 * **좁히는 일은 바탕이 한다.** 받은 목록을 여기서 걸러 보여 주면, 값이 많아
 * 앞에서 잘린 경우 찾는 값이 그 뒤에 있을 때 "없습니다" 가 뜬다 — 사람은
 * 그런 값이 없는 줄 안다.
 */
async function refill(column, needle) {
  if (picker?.key !== column.key) return;
  let got;
  try {
    // **그 열 자신의 선택은 빼고** 묻는다. 그래야 체크를 풀었다 걸었다 해도
    // 후보가 사라지지 않는다 — 바탕이 그렇게 해 준다.
    got = JSON.parse(
      state.book.values(column.key, JSON.stringify(filters(column.key)), needle),
    );
  } catch (thrown) {
    picker.node.textContent = speak(thrown);
    picker.node.classList.add("bad");
    return;
  }
  if (picker?.key !== column.key) return; // 그 사이에 닫혔다
  drawPicker(picker.node, column, got, needle);
  // 알맹이가 들어오며 높이가 달라졌으니 다시 건다.
  place(picker.node, picker.anchor);
}

function drawPicker(node, column, got, needle) {
  node.replaceChildren();
  node.classList.remove("bad");
  const chosen = new Set(state.picked.get(column.key) ?? []);

  const head = document.createElement("div");
  head.className = "picker-head";
  const find = document.createElement("input");
  find.type = "search";
  find.placeholder = "값 확인";
  find.value = needle;
  head.append(find);
  node.append(head);

  const tools = document.createElement("div");
  tools.className = "picker-tools";
  const all = document.createElement("button");
  all.type = "button";
  all.textContent = "보이는 것 모두";  // 바탕이 좁혀 준 그 목록이다
  const none = document.createElement("button");
  none.type = "button";
  none.textContent = "선택 해제";
  tools.append(all, none);
  node.append(tools);

  if (got.truncated) {
    /* **잘렸다고 말한다.** 말하지 않으면 목록에 없는 값을 "없는 값" 으로
       읽는다. 보이는 것이 전부가 아니다. */
    const warn = document.createElement("p");
    warn.className = "picker-warn";
    warn.textContent =
      `값이 너무 많아 ${LIMITS.maxValues.toLocaleString("ko-KR")}개까지만 보여 줍니다.` +
      " 위에서 확인하면 나머지에서도 찾습니다.";
    node.append(warn);
  }

  const list = document.createElement("div");
  list.className = "picker-list";
  node.append(list);

  const rows = [];
  for (const v of got.values) {
    const label = document.createElement("label");
    const box = document.createElement("input");
    box.type = "checkbox";
    box.checked = chosen.has(v.value);
    box.addEventListener("change", () => {
      if (box.checked) chosen.add(v.value);
      else chosen.delete(v.value);
      apply(column.key, chosen);
    });
    const text = document.createElement("span");
    text.className = "picker-value";
    text.textContent = v.value;
    const count = document.createElement("span");
    count.className = "picker-count";
    count.textContent = v.count.toLocaleString("ko-KR");
    label.append(box, text, count);
    list.append(label);
    rows.push({ value: v.value, label, box });
  }

  if (rows.length === 0) {
    const empty = document.createElement("p");
    empty.className = "picker-warn";
    empty.textContent = needle ? `'${needle}' 에 맞는 값이 없습니다.` : "고를 값이 없습니다.";
    node.append(empty);
  }

  /* 치는 대로 묻지 않는다. 큰 표에서 글자마다 묻는 것은 손가락보다 느리다.
     커서 자리는 다시 그릴 때 끝으로 가므로 돌려 놓는다. */
  let typing;
  find.addEventListener("input", () => {
    clearTimeout(typing);
    const text = find.value;
    typing = setTimeout(async () => {
      await refill(column, text.trim());
      const again = picker?.node.querySelector(".picker-head input");
      if (again) {
        again.focus();
        again.setSelectionRange(again.value.length, again.value.length);
      }
    }, 200);
  });

  all.addEventListener("click", () => {
    for (const r of rows) {
      r.box.checked = true;
      chosen.add(r.value);
    }
    apply(column.key, chosen);
  });
  none.addEventListener("click", () => {
    for (const r of rows) r.box.checked = false;
    chosen.clear();
    apply(column.key, chosen);
  });

  find.focus();
}

/**
 * 고른 것을 반영한다. 하나도 안 고른 열은 거르지 않는다.
 *
 * **머리글을 통째로 다시 그리지 않는다.** 다시 그리면 열려 있는 패널이
 * 붙잡고 있던 단추가 DOM 에서 사라져, 패널이 제자리를 잃고 닫혀 버린다 —
 * 체크 하나 했을 뿐인데 패널이 없어진다. 바뀌는 것은 그 열의 표시 하나뿐이다.
 */
function apply(key, chosen) {
  if (chosen.size === 0) state.picked.delete(key);
  else state.picked.set(key, new Set(chosen));
  state.offset = 0;
  for (const th of el.head.children) {
    if (th.dataset.key === key) th.classList.toggle("filtered", chosen.size > 0);
  }
  draw();
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

/**
 * 지금 걸린 것 전부.
 *
 * `except` 를 주면 그 열의 선택만 뺀다 — 값 선택 목록을 물을 때 쓴다. 제
 * 필터까지 반영하면 체크를 풀었다 걸었다 하는 사이에 후보가 사라진다.
 */
function filters(except) {
  const out = [];
  // 전체 검색은 모든 열을 OR 로 묶는다.
  const text = el.q.value.trim();
  if (text && state.columns.length > 0) {
    out.push({ key: state.columns[0].key, keys: state.columns.map((c) => c.key), contains: text });
  }
  for (const [key, values] of state.picked) {
    if (key === except || values.size === 0) continue;
    out.push({ key, values: [...values] });
  }
  return out;
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
  .then(() => {
    LIMITS = JSON.parse(limits());
    // 바탕이 말없이 잘라 주는 쪽보다, 처음부터 그 안에서 고르는 쪽이 낫다.
    PAGE = Math.min(PAGE, LIMITS.maxPage);
    tell("");
  })
  .catch((e) => tell(`바탕을 띄우지 못했습니다: ${e}`, "bad"));
