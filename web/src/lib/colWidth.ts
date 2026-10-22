/**
 * 열 너비를 **재서** 정한다.
 *
 * 예전에는 `manifest.json` 에 열마다 `width` 를 손으로 적어 두었다. 그 숫자는
 * 누군가 화면을 보고 눈대중으로 정한 것이라, 라벨이 한 글자만 길어져도
 * 머리글이 잘렸다 — 그런데 아무도 그 사실을 알려 주지 않았다. 양식이 바뀔
 * 때마다 다시 어긋나는 값이 정의 파일에 박혀 있는 셈이었다.
 *
 * 그래서 적지 않고 잰다. 진짜 머리글 칸과 **같은 클래스를 단 칸**을 화면
 * 밖에 하나 만들어 두고, 거기에 글자를 넣어 폭을 읽는다. 여백·가름선·정렬
 * 표식 자리가 전부 그 칸의 스타일에 이미 들어 있으므로, 여기서 `2.625rem`
 * 같은 숫자를 따로 들고 있지 않아도 된다. CSS 가 바뀌면 잰 값도 따라 바뀐다.
 *
 * 본문 값도 같이 잰다. 머리글만 재면 `전형방법` 처럼 **이름은 짧은데 안에
 * 문장이 들어 있는 열**이 서너 글자로 줄어든다. 다만 **화면을 열 때 딱 한
 * 번만** 잰다 — 쪽을 넘기거나 정렬·필터를 바꿀 때마다 다시 잡으면, 사용자가
 * 이미 보고 있던 표가 손도 대지 않았는데 움직인다. 그때는 이미 너비를 끌어
 * 조절할 기회가 있었다.
 */
import { ref } from "vue";
import { cellText, type ColumnKind, type Row } from "@/lib/grid";

/** 잰 값은 글자마다 한 번만 구한다. 같은 라벨이 여러 화면에 나온다. */
const cache = new Map<string, number>();

let probeCell: HTMLElement | null = null;
let probeLabel: HTMLElement | null = null;
let probeBody: HTMLElement | null = null;

/**
 * 본문에서 재 볼 행 수. 500행을 다 재면 레이아웃을 그 수만큼 다시 잡게 된다.
 * 고르게 뽑아 보면 가장 긴 값은 거의 걸린다.
 */
const SAMPLE_ROWS = 150;

/** 한 열에서 실제로 재 볼 값의 수. 글자 수로 추린 뒤 이만큼만 진짜로 잰다. */
const SAMPLE_VALUES = 3;

/**
 * 재서 잡을 수 있는 최대 너비(rem).
 *
 * 상한이 없으면 문장이 든 열 하나가 화면을 통째로 차지한다. 손으로 끌 때의
 * 한계(MAX_WIDTH)보다 낮게 둔다 — 자동으로는 여기까지만 넓히고, 더 보고
 * 싶으면 사용자가 끌어서 늘린다.
 */
export const AUTO_MAX_WIDTH = 24;

/**
 * 글꼴이 바뀔 때마다 오른다. 이 값을 읽는 computed 는 글꼴이 다 실린 뒤
 * 다시 계산된다.
 *
 * 글꼴이 아직 안 실렸을 때 재면 대체 글꼴의 폭이 나온다. Pretendard 는
 * 한글 한 글자가 대체 글꼴보다 넓어서, 그대로 두면 모든 열이 조금씩 좁다.
 */
export const fontEpoch = ref(0);

if (typeof document !== "undefined" && document.fonts) {
  document.fonts.ready.then(() => {
    cache.clear();
    fontEpoch.value += 1;
  });
}

function rootFontPx(): number {
  const v = parseFloat(getComputedStyle(document.documentElement).fontSize);
  return Number.isFinite(v) && v > 0 ? v : 16;
}

function ensureProbe() {
  if (probeCell?.isConnected) return;

  // 이미 서 있으면 그것을 쓴다. 개발 중 모듈이 다시 실려도 재는 칸이 쌓이지 않는다.
  const standing = document.querySelector<HTMLElement>(".dg-probe");
  if (standing) {
    probeCell = standing.querySelector<HTMLElement>(".dg-hcell");
    probeLabel = standing.querySelector<HTMLElement>(".dg-hlabel");
    probeBody = standing.querySelector<HTMLElement>(".dg-cell");
    if (probeCell && probeLabel && probeBody) return;
    standing.remove();
  }

  const box = document.createElement("div");
  box.className = "dg dg-probe";
  box.setAttribute("aria-hidden", "true");

  const cell = document.createElement("div");
  cell.className = "dg-hcell";

  const label = document.createElement("span");
  label.className = "dg-hlabel";

  // 정렬 표식은 정렬하지 않은 열에서도 자리를 차지한다. 재는 칸에도 있어야
  // 그만큼을 더한 폭이 나온다.
  const mark = document.createElement("span");
  mark.className = "dg-sortmark";
  mark.textContent = "▲";

  cell.append(label, mark);

  // 본문 칸도 따로 잰다. 여백이 머리글과 다르고, 숫자 열은 자릿수를 맞추느라
  // 글자 폭까지 달라진다.
  const body = document.createElement("div");
  body.className = "dg-cell";

  box.append(cell, body);

  // 뒤에 빈 칸을 하나 세워 둔다. 이게 없으면 마지막 칸이 `:last-child` 가 되어
  // **오른쪽 가름선이 없는 칸**으로 재어진다 — 표 안의 칸보다 1px 좁게 나오고,
  // 그만큼 끝 글자가 말줄임으로 바뀐다.
  box.append(document.createElement("div"));

  document.body.append(box);
  probeCell = cell;
  probeLabel = label;
  probeBody = body;
}

/**
 * 머리글이 잘리지 않는 최소 열 너비(rem).
 *
 * 재는 것은 글자가 아니라 **칸 전체**다 — 좌우 여백과 정렬 표식 자리가
 * 포함된 값이 나온다.
 */
export function headerWidthRem(label: string): number {
  const hit = cache.get(label);
  if (hit !== undefined) return hit;

  ensureProbe();
  probeLabel!.textContent = label;
  const px = probeCell!.getBoundingClientRect().width;
  // 0.25rem 단위로 올림한다. 소수점 아래가 떨어져 나가 한 글자가 말줄임으로
  // 바뀌는 일을 막는다.
  const rem = Math.ceil((px / rootFontPx()) * 4) / 4;

  cache.set(label, rem);
  return rem;
}

/** 글자가 넓은 쪽인가. 진짜로 잴 값을 추리는 데만 쓴다. */
function weigh(s: string): number {
  let w = 0;
  for (const ch of s) w += ch.codePointAt(0)! > 0x1100 ? 2 : 1;
  return w;
}

/** 본문 값 하나가 다 보이는 칸 너비(rem). */
function bodyWidthRem(text: string, kind: ColumnKind): number {
  ensureProbe();
  probeBody!.dataset.kind = kind;
  probeBody!.textContent = text;
  const px = probeBody!.getBoundingClientRect().width;
  return Math.ceil((px / rootFontPx()) * 4) / 4;
}

/** 500행에서 고르게 뽑는다. 무작위로 뽑으면 열 때마다 폭이 달라진다. */
function sample(rows: Row[]): Row[] {
  if (rows.length <= SAMPLE_ROWS) return rows;
  const step = rows.length / SAMPLE_ROWS;
  const out: Row[] = [];
  for (let i = 0; i < SAMPLE_ROWS; i += 1) out.push(rows[Math.floor(i * step)]);
  return out;
}

/**
 * 머리글과 본문을 함께 보고 잡은 열 너비(rem).
 *
 * 글자 수로 긴 값 몇 개를 추린 뒤 **그것만 진짜로 잰다.** 500행 × 40열을 다
 * 재면 레이아웃을 2 만 번 다시 잡게 되는데, 추려서 재면 열마다 몇 번이면 된다.
 * 추리는 것은 어림이어도 마지막에 나오는 숫자는 잰 값이다.
 */
export function autoWidthRem(
  label: string,
  kind: ColumnKind,
  at: number,
  rows: Row[],
): number {
  let widest = headerWidthRem(label);

  const seen = new Set<string>();
  const picked: { text: string; w: number }[] = [];
  for (const r of sample(rows)) {
    const text = cellText({ kind }, r[at]);
    if (!text || seen.has(text)) continue;
    seen.add(text);
    const w = weigh(text);
    if (picked.length < SAMPLE_VALUES) {
      picked.push({ text, w });
      picked.sort((a, b) => a.w - b.w);
    } else if (w > picked[0].w) {
      picked[0] = { text, w };
      picked.sort((a, b) => a.w - b.w);
    }
  }

  for (const p of picked) {
    const rem = bodyWidthRem(p.text, kind);
    if (rem > widest) widest = rem;
  }

  return Math.min(AUTO_MAX_WIDTH, widest);
}
