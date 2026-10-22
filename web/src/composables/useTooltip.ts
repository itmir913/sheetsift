/**
 * 마우스를 얹었을 때 뜨는 설명.
 *
 * 브라우저가 그려 주는 `title` 속성을 쓰지 않는다. 그 글상자는 **크기도 글꼴도
 * 우리가 정할 수 없어서**, 화면 전체가 Pretendard 에 `text-base` 인데 설명만
 * 작은 시스템 글꼴로 뜬다. 선생님이 표의 잘린 칸을 확인하려고 얹는 것이라
 * 오히려 본문보다 커야 한다.
 *
 * **요소마다 붙이지 않는다.** 표는 한 화면에 수천 칸이라, 칸마다 듣는 귀를
 * 달면 그 수만큼 늘어난다. 대신 `data-tip` 속성만 적어 두고 문서 하나가
 * 받아서 올라온 자리를 거슬러 찾는다(`closest`). 지금까지 `title` 을 적던
 * 자리에 이름만 바꿔 적으면 된다.
 *
 * 띄우는 일은 `AppTooltip` 이 한다. 여기서는 **무엇을 어디에** 만 정한다.
 */
import { reactive } from "vue";

/** 설명을 띄울 곳에 붙이는 속성. */
const ATTR = "data-tip";

/**
 * 처음 뜨기까지 기다리는 시간(ms). 표 위를 가로질러 갈 뿐인데 칸마다 글상자가
 * 튀어나오면 읽을 수 없다.
 */
const DELAY = 350;

/** 이미 떠 있을 때 옆 칸으로 옮기는 경우. 한 번 기다렸으므로 곧바로 따라간다. */
const DELAY_NEXT = 60;

export interface TooltipState {
  open: boolean;
  text: string;
  /** 가리키는 요소의 자리(창 기준). 어느 쪽에 붙일지는 `AppTooltip` 이 정한다. */
  anchor: { left: number; top: number; right: number; bottom: number };
}

export const tooltip = reactive<TooltipState>({
  open: false,
  text: "",
  anchor: { left: 0, top: 0, right: 0, bottom: 0 },
});

let timer: number | undefined;
let current: Element | null = null;

/* `Element` 로 받는다 — 차트의 점은 `<circle>` 이라 `HTMLElement` 가 아니다.
   여기서 쓰는 것(속성 읽기·붙어 있는지·자리 재기)은 전부 `Element` 에 있다. */
function textOf(el: Element): string {
  return (el.getAttribute(ATTR) ?? "").trim();
}

function show(el: Element): void {
  const text = textOf(el);
  // 뜨기를 기다리는 사이에 글자가 지워졌거나 화면에서 빠졌을 수 있다.
  if (!text || !el.isConnected) return;
  const r = el.getBoundingClientRect();
  tooltip.text = text;
  tooltip.anchor = { left: r.left, top: r.top, right: r.right, bottom: r.bottom };
  tooltip.open = true;
}

export function hideTooltip(): void {
  if (timer !== undefined) window.clearTimeout(timer);
  timer = undefined;
  current = null;
  tooltip.open = false;
}

function onOver(ev: PointerEvent): void {
  // 손가락으로 누르는 화면에서는 뜨지 않는다. 얹는다는 것이 없다.
  if (ev.pointerType !== "mouse") return;
  const target = ev.target instanceof Element ? ev.target.closest(`[${ATTR}]`) : null;
  const el = target && textOf(target) ? target : null;
  if (el === current) return;
  if (timer !== undefined) window.clearTimeout(timer);
  current = el;
  if (!el) {
    tooltip.open = false;
    return;
  }
  timer = window.setTimeout(() => show(el), tooltip.open ? DELAY_NEXT : DELAY);
}

function onKeyDown(ev: KeyboardEvent): void {
  if (ev.key === "Escape") hideTooltip();
}

let installed = false;

/**
 * 문서 하나에 귀를 단다. `AppTooltip` 이 붙을 때 한 번만 부른다.
 *
 * 스크롤·휠·누름에는 **곧바로 없앤다.** 떠 있는 글상자는 자리를 한 번 잡고
 * 나면 따라가지 않으므로, 표가 움직이면 엉뚱한 칸 옆에 남는다.
 */
export function installTooltip(): void {
  if (installed) return;
  installed = true;
  document.addEventListener("pointerover", onOver, true);
  document.addEventListener("pointerdown", hideTooltip, true);
  document.addEventListener("wheel", hideTooltip, { capture: true, passive: true });
  document.addEventListener("scroll", hideTooltip, true);
  document.addEventListener("keydown", onKeyDown, true);
  window.addEventListener("blur", hideTooltip);
}
