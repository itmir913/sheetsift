<script setup lang="ts">
/**
 * 화면 하나. 양식을 수령하고, 그 내부의 시트를 좌측에 배열하고, 선택한 것을
 * 표로 묘사한다.
 *
 * **시트 하나가 조회 화면 하나다.** goe-navi 는 무엇으로 묘사할지를 정의 파일이
 * 결정했으나 열람 겹에는 그 파일이 부재한다 — 그래도 `kind` 를 참조해 선정한다.
 * 화면 id 를 참조해 선정하면, 시트 명칭을 변경하는 즉시 묵시적으로 상이한 화면이
 * 개방된다.
 */
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
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
import { AppError, start } from "sheetsift-book";
import { APP_NAME, APP_TAGLINE } from "@/lib/app";
import { ui } from "@/i18n";

const KIND_VIEW = { grid: DataGrid } as const;

const groupNames = ref<string[]>([]);
const screens = ref<ScreenDef[]>([]);
const current = ref<ScreenDef | null>(null);

/**
 * 사용자가 지정한 시트별 머리글. **기재된 것만 보관한다** — 기재하지 않은
 * 시트는 바탕이 추정한 것을 적용하고, 그 추정은 바탕이 담당한다.
 */
const headers = ref<Record<string, HeaderSpec>>({});

const fileName = ref("");
const errorMessage = ref<string | null>(null);
/** 양식은 개방되었으나 그 시트 하나가 공백인 것 — 오류가 아니라 안내다. */
const note = ref<string | null>(null);
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
    errorMessage.value = e instanceof AppError ? e.message : String(e);
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
  errorMessage.value = null;
  note.value = null;
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
    if (!screens.value.length) note.value = "이 파일에서 열람 가능한 표를 탐색하지 못했습니다.";
  } catch (e) {
    errorMessage.value = e instanceof AppError ? e.message : String(e);
    fileName.value = "";
  } finally {
    busy.value = false;
  }
}

/** 머리글을 판별하지 못한 시트는 표가 부재한다. **공백 표로 묘사하지 않는다.** */
const headless = computed(() => current.value !== null && !current.value.headerFound);

/**
 * 머리글 지정을 변경한다. 목록 전체를 재요청해 **열 정의를 교체한다.**
 *
 * 열의 개수도 명칭도 종류도 함께 변경되므로, 현재 화면이 보유한 열 정의를
 * 부분적으로 수정할 수 없다. 바탕이 재산출한 것으로 전체 교체한다.
 */
async function setHeader(id: string, spec: HeaderSpec) {
  headers.value = { ...headers.value, [id]: spec };
  busy.value = true;
  errorMessage.value = null;
  try {
    const catalog = await listScreens(headers.value);
    groupNames.value = catalog.groups;
    screens.value = catalog.screens;
    current.value = catalog.screens.find((s) => s.id === id) ?? current.value;
  } catch (e) {
    errorMessage.value = e instanceof AppError ? e.message : String(e);
  } finally {
    busy.value = false;
  }
}

/**
 * 머리글을 판별하지 못한 시트에 제시할 최초 지정.
 *
 * **자동 적용하지 않는다.** 사람이 조작한 이후에 적용된다 — 추정에 실패한
 * 시트에 임의의 값을 적용해 표를 묘사하면, 그 표의 정확성을 누구도 확인하지
 * 않는다.
 */
const FIRST_GUESS: HeaderSpec = { at: 0, rows: 1 };
</script>

<template>
  <!-- no-print: 인쇄 시 앱 화면은 은폐하고 인쇄 뷰만 존치한다 -->
  <div class="no-print flex h-screen flex-col bg-canvas text-fg">
    <header
      class="flex shrink-0 items-center justify-between gap-4 border-b border-line bg-surface px-4 py-3"
    >
      <div class="flex min-w-0 items-center gap-3">
        <h1 class="shrink-0 text-xl font-bold tracking-tight">{{ APP_NAME }}</h1>
        <p class="truncate text-fg-subtle">{{ APP_TAGLINE }}</p>
      </div>

      <p v-if="fileName" class="min-w-0 flex-1 truncate text-right text-fg-muted" :data-tip="fileName">
        {{ fileName }}
      </p>

      <ThemeToggle />
    </header>

    <div class="flex min-h-0 flex-1">
      <!-- 시트 목록. 양식 개방 이전에는 공백이므로 공간도 점유하지 않는다. -->
      <nav v-if="screens.length" class="app-side" aria-label="시트">
        <div class="app-side-inner">
          <div class="app-side-scroll">
            <template v-for="g in groups" :key="g.name">
              <p class="app-side-group">{{ g.name }}</p>
              <ul class="flex flex-col gap-1">
                <li v-for="item in g.items" :key="item.id">
                  <button
                    type="button"
                    class="w-full truncate rounded-lg px-3 py-2 text-left transition-colors"
                    :class="
                      current?.id === item.id
                        ? 'bg-accent text-accent-fg font-medium'
                        : 'text-fg-muted hover:bg-surface-hover hover:text-fg'
                    "
                    :data-tip="
                      item.headerFound ? item.title : `${item.title} — 머리글 행을 탐색하지 못했습니다`
                    "
                    @click="current = item"
                  >
                    {{ item.title }}
                  </button>
                </li>
              </ul>
            </template>

            <!-- 묶음에 포함되지 못한 시트. 묵시적으로 소멸하면 인지하지 못하므로 표시한다. -->
            <template v-if="orphans.length">
              <p class="app-side-group text-danger">묶음 없음</p>
              <ul class="flex flex-col gap-1">
                <li v-for="item in orphans" :key="item.id">
                  <button
                    type="button"
                    class="w-full truncate rounded-lg px-3 py-2 text-left transition-colors"
                    :class="
                      current?.id === item.id
                        ? 'bg-accent text-accent-fg font-medium'
                        : 'text-fg-muted hover:bg-surface-hover hover:text-fg'
                    "
                    :data-tip="`묶음 '${item.group}' 이 목록에 부재합니다`"
                    @click="current = item"
                  >
                    {{ item.title }}
                  </button>
                </li>
              </ul>
            </template>
          </div>
        </div>
      </nav>

      <!-- 수직 스크롤이 발생하는 유일한 위치. 표의 열 머리글만 상단에 고정된다. -->
      <main ref="mainEl" class="app-main" :style="mainVars">
        <p v-if="starting" class="app-fixed pt-5 text-fg-muted">{{ ui.loading }}…</p>

        <template v-else>
          <div class="app-fixed shrink-0 pt-5 pb-1">
            <FilePick
              :busy="busy"
              :note="errorMessage ?? note"
              :danger="errorMessage !== null"
              @pick="pick"
              @pick-at="pickAt"
            />
          </div>

          <template v-if="current">
            <div class="app-fixed shrink-0 pt-4 pb-1">
              <h2 class="truncate text-lg font-semibold" :data-tip="current.title">
                {{ current.title }}
              </h2>
              <p v-if="current.description" class="text-fg-muted">{{ current.description }}</p>
            </div>

            <div v-if="headless" class="app-fixed flex flex-col items-start gap-3">
              <p class="text-fg-muted">
                이 시트의 머리글을 판별하지 못했습니다. 열 명칭이 몇 번째 행에 몇 행으로
                기재되어 있는지 지정해 주십시오 — 원본에서 열 명칭이 노출되는 행입니다.
              </p>
              <button
                type="button"
                class="rounded-md bg-accent px-4 py-2 font-medium text-accent-fg hover:bg-accent-hover"
                @click="setHeader(current.id, FIRST_GUESS)"
              >
                1행부터 1행으로 열람
              </button>
            </div>
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

    <!-- 포인터 접촉 시 표시되는 설명. 화면 전체에 하나. 공간을 점유하지 않는다. -->
    <AppTooltip />
  </div>
</template>
