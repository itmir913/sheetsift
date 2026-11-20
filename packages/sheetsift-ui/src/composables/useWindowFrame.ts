/**
 * **창틀 — 운영체제의 제목 표시줄을 앱이 대신 묘사하는가.**
 *
 * 제목 표시줄을 걷은 앱(`decorations: false`)은 최소화·최대화·닫기를 직접 묘사해야 한다.
 * 그런데 **맥은 걷지 않는다** — 운영체제의 신호등 단추를 존치하고 우리 머리를 그 위에
 * 겹친다. 그 단추는 눌러 보기 이전의 모양·전체 화면·보조 키 누름을 사용자가 기대하는
 * 것이라, 흉내 내면 반드시 어딘가 어긋난다.
 *
 * **판정은 「맥인가」가 아니라 「운영체제가 제목 표시줄을 그리는가」다.** 우리가 단추를
 * 묘사하는 사유가 그것이기 때문이다. 그 판정의 정본은 창의 `decorations` 설정이고, 받는
 * 앱이 그 창에 물어 [`WindowFrame.nativeTitleBar`] 로 전달한다 — **설정과 판정이 동일한
 * 값에서 산출되므로 갈릴 수 없다.** 운영체제의 명칭으로 판정하면 리눅스에서 신호등을
 * 사용하기로 하는 시점에 양자가 어긋나고, 그것은 그 운영체제에서만 발현된다.
 *
 * **판정하는 위치는 이 파일 하나다.** 화면마다 분산하면 사본이 되고, 어느 화면은
 * 브라우저에서 단추가 묘사되는 날이 온다 — 그 단추는 눌러도 아무 작업도 수행하지 않는데
 * 오류가 발생하지 않는다.
 */
import { computed, readonly, ref, shallowRef, type Ref } from "vue";

/**
 * 받는 앱이 주입하는 창의 조작. **이 패키지는 창을 인지하지 않는다** — tauri 도
 * 운영체제도 반입하지 않고, 받는 앱의 경계가 그것을 담당한다.
 */
export interface WindowFrame {
  /** 데스크톱 창인가. 브라우저면 거짓이고, 그때는 창 단추도 자리 비움도 부재한다. */
  isDesktop: boolean;
  /**
   * **운영체제의 제목 표시줄이 존치하는가.** 참이면 우리 단추를 묘사하지 않고, 그 단추가
   * 앉을 자리를 머리의 좌측에 비운다. 비동기인 사유는 창에 물어야 하기 때문이다.
   */
  nativeTitleBar: () => Promise<boolean>;
  minimize: () => void;
  toggleMaximize: () => void;
  close: () => void;
}

/* **반응형으로 보유한다.** 통상 주입은 장착 이전 1회뿐이라 평범한 변수로도 성립하나,
   그러면 주입 자체가 파생을 무효화하지 못해 **재주입했을 때 직전의 답이 잔존한다.**
   시험이 그 상태를 밟았고, 그것은 제품에서도 창을 교체하는 날 동일하게 발현된다. */
const frame = shallowRef<WindowFrame | null>(null);

/**
 * **판정의 3태.** `null` 은 「아직 모른다」다 — [`WindowFrame.nativeTitleBar`] 가 비동기라
 * 최초 묘사 시점에는 답이 부재한다. 거짓으로 시작하면 **맥에서 우리 단추가 한 프레임
 * 점멸한 뒤 소멸하고**, 참으로 시작하면 윈도에서 자리가 점멸한다. 모르는 동안에는 **양쪽
 * 모두 묘사하지 않는다.**
 */
const nativeBar = ref<boolean | null>(null);

/**
 * 받는 앱이 1회 주입한다. **주입하지 않으면 창 조작이 부재하는 앱**이고(브라우저 전용),
 * 단추도 자리 비움도 묘사되지 않는다.
 */
export function provideWindowFrame(f: WindowFrame): void {
  frame.value = f;
  nativeBar.value = null;
  if (!f.isDesktop) return;
  void f.nativeTitleBar().then((yes) => {
    nativeBar.value = yes;
  });
}

/** 창 단추를 묘사하는가. 데스크톱이고 **운영체제가 제목 표시줄을 그리지 않을 때**만 참이다. */
export const showsWindowButtons: Readonly<Ref<boolean>> = readonly(
  computed(() => frame.value?.isDesktop === true && nativeBar.value === false),
);

/**
 * 머리의 좌측에 운영체제의 단추가 앉을 자리를 비우는가. [`showsWindowButtons`] 와
 * **배타다** — 양자가 동시에 참이 되는 입력은 부재한다.
 */
export const reservesTrafficLights: Readonly<Ref<boolean>> = readonly(
  computed(() => frame.value?.isDesktop === true && nativeBar.value === true),
);

function held(): WindowFrame {
  const f = frame.value;
  if (!f) throw new Error("sheetsift-ui: the window frame was not provided");
  return f;
}

export function minimizeWindow(): void {
  held().minimize();
}

export function toggleMaximizeWindow(): void {
  held().toggleMaximize();
}

export function closeWindow(): void {
  held().close();
}

/**
 * 제목 표시줄을 **두 번 눌러 최대화를 전환한다.**
 *
 * **tauri 가 수행해 주지 않는다.** `data-tauri-drag-region` 이 두 번 누름을 최대화로
 * 전환한다는 진술은 tauri 의 문서에 부재하고, 그 쪽은 그것을 **손으로 기재하는 예시**로만
 * 제시한다. 그래서 여기 적는다 — 적은 것이 소멸하는 것은 조용하므로 시험이 결속을 본다.
 *
 * 끌기 영역을 보유한 요소에만 결속한다. 조작 가능한 자손 위의 누름은 그 자손이 대상이라
 * 여기 도달하지 않는다.
 */
export function onTitleBarDoubleClick(ev: MouseEvent): void {
  if (!showsWindowButtons.value && !reservesTrafficLights.value) return;
  if (ev.detail !== 2) return;
  held().toggleMaximize();
}
