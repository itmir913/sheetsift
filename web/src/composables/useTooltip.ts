/**
 * 포인터 접촉 시 표시되는 설명.
 *
 * 브라우저가 묘사하는 `title` 속성을 사용하지 않는다. 그 문자 상자는 **크기도
 * 서체도 우리가 결정할 수 없어서**, 화면 전체가 Pretendard 에 `text-base` 인데
 * 설명만 소형 시스템 서체로 표시된다. 사용자가 표의 절단된 셀을 확인하려고
 * 접촉하는 것이라 오히려 본문보다 대형이어야 한다.
 *
 * **요소마다 부착하지 않는다.** 표는 한 화면에 수천 셀이라, 셀마다 수신기를
 * 부착하면 그 수만큼 증가한다. 대신 `data-tip` 속성만 기재해 두고 문서 하나가
 * 수신해 전파된 위치를 소급 탐색한다(`closest`). 종래 `title` 을 기재하던
 * 위치에 명칭만 변경해 기재하면 된다.
 *
 * 표시 작업은 `AppTooltip` 이 수행한다. 여기서는 **무엇을 어디에** 만 결정한다.
 */
import { reactive } from "vue";

/** 설명을 표시할 위치에 부착하는 속성. */
const ATTR = "data-tip";

/**
 * 최초 표시까지 대기하는 시간(ms). 표 위를 횡단할 뿐인데 셀마다 문자 상자가
 * 출현하면 판독할 수 없다.
 */
const DELAY = 350;

/** 이미 표시된 상태에서 인접 셀로 이동하는 경우. 1회 대기했으므로 즉시 추종한다. */
const DELAY_NEXT = 60;

export interface TooltipState {
  open: boolean;
  text: string;
  /** 지시 요소의 위치(창 기준). 어느 방향에 부착할지는 `AppTooltip` 이 결정한다. */
  anchor: { left: number; top: number; right: number; bottom: number };
}

export const tooltip = reactive<TooltipState>({
  open: false,
  text: "",
  anchor: { left: 0, top: 0, right: 0, bottom: 0 },
});

let timer: number | undefined;
let current: Element | null = null;

/* `Element` 로 수용한다 — 차트의 점은 `<circle>` 이라 `HTMLElement` 가 아니다.
   여기서 사용하는 것(속성 판독·부착 여부·위치 측정)은 전부 `Element` 에 존재한다. */
function textOf(el: Element): string {
  return (el.getAttribute(ATTR) ?? "").trim();
}

function show(el: Element): void {
  const text = textOf(el);
  // 표시를 대기하는 사이에 문자가 소거되었거나 화면에서 이탈했을 수 있다.
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
  // 터치로 조작하는 화면에서는 표시되지 않는다. 접촉이라는 개념이 부재한다.
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
 * 문서 하나에 수신기를 부착한다. `AppTooltip` 이 부착될 때 1회만 호출한다.
 *
 * 스크롤·휠·조작 시에는 **즉시 제거한다.** 표시된 문자 상자는 위치를 1회 확정한
 * 이후 추종하지 않으므로, 표가 이동하면 무관한 셀 옆에 잔존한다.
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
