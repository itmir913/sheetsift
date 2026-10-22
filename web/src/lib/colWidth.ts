/**
 * 열 폭을 **측정해** 결정한다.
 *
 * 종래에는 `manifest.json` 에 열마다 `width` 를 수동 기재했다. 그 수치는
 * 누군가 화면을 참조해 개략 결정한 것이라, 라벨이 한 문자만 증가해도 머리글이
 * 절단되었다 — 그런데 누구도 그 사실을 통지하지 않았다. 양식이 변경될 때마다
 * 재차 불일치하는 값이 정의 파일에 고정되어 있는 셈이었다.
 *
 * 그래서 기재하지 않고 측정한다. 실제 머리글 셀과 **동일한 클래스를 부착한
 * 셀**을 화면 외부에 하나 생성해 두고, 거기에 문자를 주입해 폭을 판독한다.
 * 여백·구분선·정렬 표식 공간이 전부 그 셀의 스타일에 이미 포함되어 있으므로,
 * 여기서 `2.625rem` 같은 수치를 별도 보유하지 않아도 된다. CSS 가 변경되면
 * 측정값도 추종해 변경된다.
 *
 * 본문 값도 함께 측정한다. 머리글만 측정하면 `전형방법` 처럼 **명칭은 단축인데
 * 내부에 문장이 수록된 열**이 서너 문자로 축소된다. 다만 **화면 개방 시 1회만**
 * 측정한다 — 페이지를 이동하거나 정렬·필터를 변경할 때마다 재산정하면, 사용자가
 * 이미 열람 중이던 표가 조작하지 않았는데 이동한다. 그 시점에는 이미 폭을 끌어
 * 조절할 기회가 존재했다.
 */
import { ref } from "vue";
import { cellText, type ColumnKind, type Row } from "@/lib/grid";

/** 측정값은 문자마다 1회만 산출한다. 동일한 라벨이 복수의 화면에 출현한다. */
const cache = new Map<string, number>();

let probeCell: HTMLElement | null = null;
let probeLabel: HTMLElement | null = null;
let probeBody: HTMLElement | null = null;

/**
 * 본문에서 측정할 행 수. 500행을 전부 측정하면 레이아웃을 그 수만큼 재산정하게
 * 된다. 균등 추출하면 최장 값은 대부분 포착된다.
 */
const SAMPLE_ROWS = 150;

/** 한 열에서 실제로 측정할 값의 수. 문자 수로 선별한 뒤 이만큼만 실측한다. */
const SAMPLE_VALUES = 3;

/**
 * 측정으로 산정 가능한 최대 폭(rem).
 *
 * 상한이 부재하면 문장이 수록된 열 하나가 화면을 전체 점유한다. 수동 조작 시의
 * 한계(MAX_WIDTH)보다 낮게 배치한다 — 자동으로는 여기까지만 확장하고, 추가
 * 열람이 필요하면 사용자가 조작해 확장한다.
 */
export const AUTO_MAX_WIDTH = 24;

/**
 * 서체가 변경될 때마다 상승한다. 이 값을 판독하는 computed 는 서체가 전부
 * 적재된 이후 재산출된다.
 *
 * 서체가 아직 적재되지 않은 시점에 측정하면 대체 서체의 폭이 산출된다.
 * Pretendard 는 한글 1문자가 대체 서체보다 광범위해, 그대로 유지하면 전 열이
 * 소폭 협소해진다.
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

  // 이미 존재하면 그것을 사용한다. 개발 중 모듈이 재적재되어도 측정 셀이 누적되지 않는다.
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

  // 정렬 표식은 정렬하지 않은 열에서도 공간을 점유한다. 측정 셀에도 존재해야
  // 그만큼을 가산한 폭이 산출된다.
  const mark = document.createElement("span");
  mark.className = "dg-sortmark";
  mark.textContent = "▲";

  cell.append(label, mark);

  // 본문 셀도 별도 측정한다. 여백이 머리글과 상이하고, 숫자 열은 자릿수를
  // 정합하느라 문자 폭까지 상이해진다.
  const body = document.createElement("div");
  body.className = "dg-cell";

  box.append(cell, body);

  // 후단에 공백 셀을 하나 배치한다. 이것이 부재하면 최종 셀이 `:last-child` 가
  // 되어 **우측 구분선이 부재하는 셀**로 측정된다 — 표 내부의 셀보다 1px 협소하게
  // 산출되고, 그만큼 종단 문자가 말줄임으로 전환된다.
  box.append(document.createElement("div"));

  document.body.append(box);
  probeCell = cell;
  probeLabel = label;
  probeBody = body;
}

/**
 * 머리글이 절단되지 않는 최소 열 폭(rem).
 *
 * 측정 대상은 문자가 아니라 **셀 전체**다 — 좌우 여백과 정렬 표식 공간이
 * 포함된 값이 산출된다.
 */
export function headerWidthRem(label: string): number {
  const hit = cache.get(label);
  if (hit !== undefined) return hit;

  ensureProbe();
  probeLabel!.textContent = label;
  const px = probeCell!.getBoundingClientRect().width;
  // 0.25rem 단위로 절상한다. 소수점 이하가 절사되어 1문자가 말줄임으로
  // 전환되는 사태를 차단한다.
  const rem = Math.ceil((px / rootFontPx()) * 4) / 4;

  cache.set(label, rem);
  return rem;
}

/** 문자가 광범위한 쪽인가. 실측할 값을 선별하는 데만 사용한다. */
function weigh(s: string): number {
  let w = 0;
  for (const ch of s) w += ch.codePointAt(0)! > 0x1100 ? 2 : 1;
  return w;
}

/** 본문 값 하나가 전부 노출되는 셀 폭(rem). */
function bodyWidthRem(text: string, kind: ColumnKind): number {
  ensureProbe();
  probeBody!.dataset.kind = kind;
  probeBody!.textContent = text;
  const px = probeBody!.getBoundingClientRect().width;
  return Math.ceil((px / rootFontPx()) * 4) / 4;
}

/** 500행에서 균등 추출한다. 무작위 추출하면 개방할 때마다 폭이 상이해진다. */
function sample(rows: Row[]): Row[] {
  if (rows.length <= SAMPLE_ROWS) return rows;
  const step = rows.length / SAMPLE_ROWS;
  const out: Row[] = [];
  for (let i = 0; i < SAMPLE_ROWS; i += 1) out.push(rows[Math.floor(i * step)]);
  return out;
}

/**
 * 머리글과 본문을 함께 참조해 산정한 열 폭(rem).
 *
 * 문자 수로 장문 값 수 개를 선별한 뒤 **그것만 실측한다.** 500행 × 40열을 전부
 * 측정하면 레이아웃을 2만 회 재산정하게 되는데, 선별해 측정하면 열마다 수 회면
 * 충분하다. 선별은 근사치여도 최종 산출되는 수치는 실측값이다.
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
