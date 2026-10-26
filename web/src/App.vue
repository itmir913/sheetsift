<script setup lang="ts">
/**
 * 화면 하나. 양식을 수령하고, 그 내부의 시트를 좌측에 배열하고, 선택한 것을
 * 표로 묘사한다.
 *
 * **시트 하나가 조회 화면 하나다.** goe-navi 는 무엇으로 묘사할지를 정의 파일이
 * 결정했으나 열람 겹에는 그 파일이 부재한다 — 그래도 `kind` 를 참조해 선정한다.
 * 화면 id 를 참조해 선정하면, 시트 명칭을 변경하는 즉시 묵시적으로 상이한 화면이
 * 개방된다.
 *
 * **개방하면 표가 출현한다.** 사람이 파일을 선택한 행위가 곧 열람 요청이라,
 * 그 사이에 단추를 1회 더 배치할 사유가 부재하다. 헤더 추정이 실패한 시트는
 * 1행을 적용해 묘사하고, 상이하면 사람이 조작부에서 이동시킨다. **그 경우에만
 * 1회 통지한다** — 판별에 실패한 시트는 선두 자료 행이 열 명칭으로 소비되므로,
 * 아무 진술도 하지 않으면 그 소비를 인지할 경로가 전무하다. 통지는 소멸하는
 * 것이라 판 내부에 잔류하지 않는다.
 */
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import AppTooltip from "@/components/AppTooltip.vue";
import ThemeToggle from "@/components/ThemeToggle.vue";
import FilePick from "@/components/FilePick.vue";
import DataGrid from "@/components/DataGrid.vue";
import {
  listScreens,
  openBook,
  openBookAt,
  type HeaderSpec,
  type ScreenCatalog,
  type ScreenDef,
} from "@/lib/grid";
import { start } from "sheetsift-book";
import { useFault } from "@/composables/useFault";
import LocaleToggle from "@/components/LocaleToggle.vue";
import StartScreen from "@/components/StartScreen.vue";
import { APP_NAME, storageKey } from "@/lib/app";
import { formatNumber, t } from "@/i18n";

const KIND_VIEW = { grid: DataGrid } as const;

const groupNames = ref<string[]>([]);
const screens = ref<ScreenDef[]>([]);
const current = ref<ScreenDef | null>(null);

/**
 * 사용자가 지정한 시트별 헤더. **기재된 것만 보관한다** — 기재하지 않은
 * 시트는 바탕이 추정한 것을 적용하고, 그 추정은 바탕이 담당한다.
 */
const headers = ref<Record<string, HeaderSpec>>({});

/* ── 소멸하는 통지 ─────────────────────────────────────────────
   판 내부에 문구를 상주시키지 않는다. 적중한 추정에까지 표식을 부착하면 매
   시트에 경고가 출현해 아무도 판독하지 않게 된다. */

const toast = ref<string | null>(null);
let toastTimer: ReturnType<typeof setTimeout> | null = null;

function notify(text: string) {
  toast.value = text;
  if (toastTimer) clearTimeout(toastTimer);
  toastTimer = setTimeout(() => (toast.value = null), 7000);
}

onBeforeUnmount(() => {
  if (toastTimer) clearTimeout(toastTimer);
});

const fileName = ref("");
/* 오류는 코드와 치환 값으로 보유한다. 문구로 보유하면 언어 전환을 추종하지 않는다. */
const { message: errorMessage, capture: captureFault, clear: clearFault } = useFault();
/**
   * 양식은 개방되었으나 열람 가능한 표가 부재한 것 — 오류가 아니라 안내다.
   *
   * **문구를 보유하지 않고 상태에서 산출한다.** 보유하면 언어를 전환해도 그
   * 한 문장만 선행 언어로 잔존한다.
   */
const note = computed(() =>
  fileName.value && !busy.value && screens.value.length === 0 && errorMessage.value === null
    ? t("app.file.nosheets")
    : null,
);
const busy = ref(false);
/** wasm 을 아직 기동 중인가. 기동 이전에는 파일을 수령해도 수행 가능한 작업이 부재한다. */
const starting = ref(true);

interface NavGroup {
  name: string;
  items: ScreenDef[];
}

const groups = computed<NavGroup[]>(() =>
  groupNames.value
    .map((name) => ({ name, items: screens.value.filter((s) => s.group === name) }))
    .filter((g) => g.items.length > 0),
);

/** 묶음에 포함되지 못한 시트. **묵시적으로 소멸하면 안 된다.** */
const orphans = computed(() => {
  const known = new Set(groupNames.value);
  return screens.value.filter((s) => !known.has(s.group));
});

/**
 * 좌측 목록을 배치하는가.
 *
 * **시트가 하나면 선택할 것이 부재하다.** 항목 하나짜리 목록에 폭을 15rem
 * 할당하면 표가 그만큼 축소되고, 그 시트의 명칭은 표 상단의 제목이 이미
 * 기재한다.
 */
const hasNav = computed(() => (narrow.value ? true : screens.value.length > 1));

/* ── 좌측 목록 개폐 ─────────────────────────────────────────────
   개폐 상태를 기억한다. 열람하는 사람의 작업 방식은 파일마다 변동하지 않는다 —
   매회 개폐하게 하면 그 조작이 열람 절차의 일부가 된다. */

const SIDE_KEY = storageKey("side");

function readCollapsed(): boolean {
  try {
    return localStorage.getItem(SIDE_KEY) === "1";
  } catch {
    /* 저장소 접근이 차단된 환경에서도 동작해야 한다. */
    return false;
  }
}

const stored = ref(readCollapsed());

/**
 * 폭이 협소한 기기에서는 목록이 **본문 위에 포개져** 개폐된다.
 *
 * 240px 짜리 목록이 상주하면 표에 남는 폭이 100px 대가 된다. 그렇다고 접은 채
 * 두면 시트를 선택할 경로가 부재하므로, 조작하면 그 자리에서 본문 위로
 * 전개하고 선택하면 폐쇄한다.
 *
 * **상태를 보관된 선택과 분리해 보유한다.** 양자를 하나로 보유했더니, 협소한
 * 폭에서 조작해도 보관된 값이 접힘이면 그대로 접힌 채였다 — 목록을 개방할
 * 경로가 전무했다.
 */
const NARROW = 768;
const narrow = ref(typeof window !== "undefined" && window.innerWidth < NARROW);
const openedNarrow = ref(false);

function onResize() {
  const now = window.innerWidth < NARROW;
  if (now === narrow.value) return;
  narrow.value = now;
  // 폭이 변경되면 포개진 목록은 폐쇄한다. 광폭으로 복귀했을 때 보관된 선택이
  // 그대로 적용되어야 한다.
  openedNarrow.value = false;
}

const sideCollapsed = computed({
  get: () => (narrow.value ? !openedNarrow.value : stored.value),
  set: (v: boolean) => {
    if (narrow.value) openedNarrow.value = !v;
    else stored.value = v;
  },
});

watch(stored, (v) => {
  try {
    localStorage.setItem(SIDE_KEY, v ? "1" : "0");
  } catch {
    /* 저장 실패는 무시한다. */
  }
});

/**
 * 개폐 단축키. 입력란에 문자를 기재하는 중에는 작동하지 않는다 — 열 검색란에
 * `b` 를 기재하는 것이 목록 개폐로 해석되면 안 된다.
 */
function onKeydown(ev: KeyboardEvent) {
  if (!(ev.ctrlKey || ev.metaKey) || ev.altKey || ev.key.toLowerCase() !== "b") return;
  const el = ev.target as HTMLElement | null;
  if (el && (el.isContentEditable || /^(input|textarea|select)$/i.test(el.tagName))) return;
  if (!hasNav.value) return;
  ev.preventDefault();
  sideCollapsed.value = !sideCollapsed.value;
}

onMounted(() => {
  window.addEventListener("keydown", onKeydown);
  window.addEventListener("resize", onResize);
});
onBeforeUnmount(() => {
  window.removeEventListener("keydown", onKeydown);
  window.removeEventListener("resize", onResize);
});

/* 본문이 수평으로도 스크롤한다(표가 화면보다 광범위하다). 표가 아닌 것들은
   수평 스크롤과 무관하게 정위치에 존재해야 하므로 `sticky; left: 0` 에 **가시
   폭**을 부여해야 한다. 그 폭을 여기서 측정해 전달한다. */
const mainEl = ref<HTMLElement | null>(null);
const viewportWidth = ref(0);
const mainVars = computed(() => ({ "--app-vw": `${viewportWidth.value}px` }));

let observer: ResizeObserver | null = null;
onMounted(() => {
  if (!mainEl.value) return;
  viewportWidth.value = mainEl.value.clientWidth;
  observer = new ResizeObserver(() => {
    viewportWidth.value = mainEl.value?.clientWidth ?? 0;
  });
  observer.observe(mainEl.value);
});
onBeforeUnmount(() => observer?.disconnect());

onMounted(async () => {
  try {
    await start();
  } catch (e) {
    captureFault(e);
  } finally {
    starting.value = false;
  }
});

/**
 * 선택한 양식을 개방한다.
 *
 * 판독하지 못한 것은 **그대로 통과시키지 않는다.** 선행 개방한 표를 그대로
 * 유지하고 오류만 표시하면, 사람은 신규 파일을 열람 중이라고 신뢰한 채 구
 * 자료를 판독한다.
 */
async function pick(file: File) {
  await opened(file.name, async () =>
    openBook(new Uint8Array(await file.arrayBuffer()), file.name),
  );
}

/**
 * 선택 대화상자가 반환한 경로를 개방한다. **바이트를 경유시키지 않는다.**
 *
 * 창에서만 도달한다(`canPickFile`). 개방 이후의 처리는 투입한 파일과 동일한
 * 위치를 경유한다 — 두 벌이 되면 일방만 수정된다.
 */
async function pickAt(path: string, name: string) {
  await opened(name, () => openBookAt(path));
}

async function opened(name: string, open: () => Promise<ScreenCatalog>) {
  busy.value = true;
  clearFault();
  screens.value = [];
  groupNames.value = [];
  current.value = null;
  // 양식이 교체되면 선행 양식의 지정은 무의미하다. 시트 명칭이 우연히 일치하면
  // 전혀 상이한 표에 그 지정이 적용된다.
  headers.value = {};
  fileName.value = name;
  try {
    const catalog = await open();
    groupNames.value = catalog.groups;
    screens.value = catalog.screens;
    current.value = groups.value[0]?.items[0] ?? screens.value[0] ?? null;
  } catch (e) {
    captureFault(e);
    fileName.value = "";
  } finally {
    busy.value = false;
  }
  await shown();
}

/**
 * 선택한 시트를 묘사한다. **표가 즉시 출현해야 한다.**
 *
 * 추정이 실패한 시트는 1행을 적용한다. 그 지정이 적중한다는 의미가 아니라,
 * 사람이 조작부에서 원본과 대조해 이동시킬 **기준점**을 제시하는 것이다 —
 * 공백 화면과 단추만 배치하면 어느 행이 헤더인지 판별할 근거 자체가 부재하다.
 */
async function shown() {
  const s = current.value;
  if (!s || s.headerFound) return;
  await setHeader(s.id, FIRST_GUESS);
  // **열이 생성된 경우에만 통지한다.** 공백 시트는 첫 행을 적용해도 표가
  // 부재해, 본문이 "열을 찾지 못했습니다" 를 기재한다. 거기에 "첫 행을
  // 헤더로 가정했습니다" 를 병기하면 두 문구가 서로를 부정한다.
  if (current.value?.headerFound) notify(t("app.header.assumed", { sheet: s.title }));
}

function select(item: ScreenDef) {
  if (narrow.value) sideCollapsed.value = true;
  if (current.value?.id === item.id) return;
  current.value = item;
  void shown();
}

/** 헤더를 판별하지 못한 시트는 표가 부재한다. **공백 표로 묘사하지 않는다.** */
const headless = computed(() => current.value !== null && !current.value.headerFound);

/* 열이 조회 엔진의 상한을 초과하는 시트. **표를 묘사하지 않는다** — 열은
   도달해 묘사할 수 있으나 자료는 영구히 도달하지 않는다. */
const tooWide = computed(() => current.value?.tooWide === true);

/**
 * 헤더 지정을 변경한다. 목록 전체를 재요청해 **열 정의를 교체한다.**
 *
 * 열의 개수도 명칭도 종류도 함께 변경되므로, 현재 화면이 보유한 열 정의를
 * 부분적으로 수정할 수 없다. 바탕이 재산출한 것으로 전체 교체한다.
 */
async function setHeader(id: string, spec: HeaderSpec) {
  headers.value = { ...headers.value, [id]: spec };
  busy.value = true;
  clearFault();
  try {
    const catalog = await listScreens(headers.value);
    groupNames.value = catalog.groups;
    screens.value = catalog.screens;
    current.value = catalog.screens.find((s) => s.id === id) ?? current.value;
  } catch (e) {
    captureFault(e);
  } finally {
    busy.value = false;
  }
}

/**
 * 헤더를 판별하지 못한 시트에 적용하는 최초 지정. 사람이 조작부에서
 * 이동시킬 기준점이다.
 */
const FIRST_GUESS: HeaderSpec = { at: 0, rows: 1 };
</script>

<template>
  <!-- no-print: 인쇄 시 앱 화면은 은폐하고 인쇄 뷰만 존치한다 -->
  <div class="no-print flex h-screen flex-col bg-canvas text-fg">
    <header class="app-head">
      <!-- 좌측 목록 개폐. 목록을 배치하는 양식에서만 출현한다 — 조작해도 아무
           작용이 없는 단추를 상주시키지 않는다. -->
      <button
        v-if="hasNav"
        type="button"
        class="app-head-btn app-head-icon"
        :aria-expanded="!sideCollapsed"
        aria-controls="app-side"
        :aria-label="t('app.sheets')"
        :data-tip="t(sideCollapsed ? 'app.sheets.show.tip' : 'app.sheets.hide.tip')"
        @click="sideCollapsed = !sideCollapsed"
      >
        <span aria-hidden="true">☰</span>
      </button>

      <h1 class="shrink-0 text-xl font-bold tracking-tight">{{ APP_NAME }}</h1>

      <p
        v-if="fileName"
        class="min-w-0 flex-1 truncate text-right font-medium"
        :data-tip="fileName"
      >
        {{ fileName }}
      </p>
      <span v-else class="flex-1"></span>

      <!-- **협소한 폭에서는 설정을 헤더에 배열하지 않는다.** 분절 선택기
           둘과 단추 하나가 상주하면 헤더가 세 행이 되고 파일 명칭이 2~3자만
           노출된다. 그것들은 목록 하단으로 이양한다(하기). -->
      <template v-if="!narrow">
        <!-- 개방 이후의 교체 경로. 투입(drag-drop)도 여기서 수령한다. -->
        <FilePick v-if="fileName" compact :busy="busy" @pick="pick" @pick-at="pickAt" />
        <LocaleToggle />
        <ThemeToggle />
      </template>
    </header>

    <div class="flex min-h-0 flex-1">
      <!-- 시트 목록. 양식 개방 이전에는 공백이므로 공간도 점유하지 않는다. -->
      <nav
        v-if="hasNav"
        id="app-side"
        class="app-side"
        :data-collapsed="sideCollapsed"
        :data-overlay="narrow"
        :inert="sideCollapsed || undefined"
        :aria-label="t('app.sheets')"
      >
        <div class="app-side-inner">
          <div class="app-side-scroll">
            <!-- 양식을 개방하기 이전. **공백으로 존치하지 않는다** — 협소한
                 폭에서는 설정만 하단에 존재하는 빈 판이 되어, 목록이 소멸한
                 것인지 개방하지 않은 것인지를 판별할 수 없다. -->
            <p v-if="!groups.length && !orphans.length" class="app-side-empty">
              {{ t("app.sheets.empty") }}
            </p>

            <template v-for="g in groups" :key="g.name">
              <p class="app-side-group">{{ g.name }}</p>
              <ul class="flex flex-col gap-1">
                <li v-for="item in g.items" :key="item.id">
                  <button
                    type="button"
                    class="app-side-item"
                    :data-current="current?.id === item.id"
                    :data-tip="
                      item.headerFound
                        ? item.title
                        : t('app.sheet.noheader.tip', { title: item.title })
                    "
                    @click="select(item)"
                  >
                    {{ item.title }}
                  </button>
                </li>
              </ul>
            </template>

            <!-- 묶음에 포함되지 못한 시트. 묵시적으로 소멸하면 인지하지 못하므로 표시한다. -->
            <template v-if="orphans.length">
              <p class="app-side-group text-danger">{{ t("app.group.none") }}</p>
              <ul class="flex flex-col gap-1">
                <li v-for="item in orphans" :key="item.id">
                  <button
                    type="button"
                    class="app-side-item"
                    :data-current="current?.id === item.id"
                    :data-tip="t('app.group.missing.tip', { group: item.group })"
                    @click="select(item)"
                  >
                    {{ item.title }}
                  </button>
                </li>
              </ul>
            </template>
          </div>

          <!-- 협소한 폭에서만 출현한다. 헤더에서 이양된 것들이다. -->
          <div v-if="narrow" class="app-side-foot">
            <FilePick
              v-if="fileName"
              compact
              :busy="busy"
              @pick="pick"
              @pick-at="pickAt"
            />
            <LocaleToggle />
            <ThemeToggle />
          </div>
        </div>
      </nav>

      <!-- 포개진 목록의 바깥. 조작하면 폐쇄한다. -->
      <div
        v-if="narrow && !sideCollapsed"
        class="app-scrim"
        @click="sideCollapsed = true"
      ></div>

      <!-- 수직 스크롤이 발생하는 유일한 위치. 표의 열 헤더만 상단에 고정된다. -->
      <main ref="mainEl" class="app-main" :style="mainVars">
        <p v-if="starting" class="app-fixed pt-5 text-fg-muted">{{ t("app.starting") }}</p>

        <!-- 개방 이전. 본문의 전부가 이 판이다 — 이 도구가 무엇인지와 자료가
             어디로 가지 않는지를 진술하는 위치가 여기 하나다. -->
        <div v-else-if="!fileName" class="app-fixed">
          <StartScreen
            :busy="busy"
            :note="errorMessage ?? note"
            :danger="errorMessage !== null"
            @pick="pick"
            @pick-at="pickAt"
          />
        </div>

        <template v-else>
          <p
            v-if="errorMessage ?? note"
            class="app-fixed shrink-0 pt-4"
            :class="errorMessage ? 'text-danger' : 'text-fg-muted'"
            role="status"
            aria-live="polite"
          >
            {{ errorMessage ?? note }}
          </p>

          <template v-if="current">
            <div class="app-fixed shrink-0 pt-4 pb-1">
              <h2 class="truncate text-lg font-semibold" :data-tip="current.title">
                {{ current.title }}
              </h2>
              <p v-if="current.description" class="text-fg-muted">{{ current.description }}</p>
            </div>

            <!-- 1행을 적용해도 열을 판별하지 못한 시트. 공백 시트가 그렇다.
                 **단추를 배치하지 않는다** — 조작할 대상이 부재하다. -->
            <p v-if="headless" class="app-fixed text-fg-muted">
              {{ t("app.sheet.nocolumns") }}
            </p>
            <!-- 열이 상한을 초과하는 시트. 조작할 대상이 부재해 단추도 부재하다. -->
            <p v-else-if="tooWide" class="app-fixed text-danger">
              {{ t("app.sheet.toowide", { columns: formatNumber(current.columns.length) }) }}
            </p>
            <!-- 무엇으로 묘사할지는 화면 정의의 kind 가 결정한다. -->
            <component
              :is="KIND_VIEW[current.kind]"
              v-else
              :key="`${current.id}:${current.header?.at}:${current.header?.rows}`"
              :screen="current"
              @set-header="(s: HeaderSpec) => setHeader(current!.id, s)"
            />
          </template>
        </template>
      </main>
    </div>

    <!-- 소멸하는 통지. 화면 전체에 하나이고 공간을 점유하지 않는다. -->
    <Transition name="app-toast">
      <button
        v-if="toast"
        type="button"
        class="app-toast"
        role="status"
        aria-live="polite"
        @click="toast = null"
      >
        {{ toast }}
      </button>
    </Transition>

    <!-- 포인터 접촉 시 표시되는 설명. 화면 전체에 하나. 공간을 점유하지 않는다. -->
    <AppTooltip />
  </div>
</template>
