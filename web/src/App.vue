<script setup lang="ts">
/**
 * 화면 하나. 양식을 받고, 그 안의 시트를 왼쪽에 늘어놓고, 고른 것을 표로 그린다.
 *
 * **시트 하나가 조회 화면 하나다.** goe-navi 는 무엇으로 그릴지를 정의 파일이
 * 정했지만 보는 겹에는 그 파일이 없다 — 그래도 `kind` 를 보고 고른다. 화면
 * id 를 보고 고르면, 시트 이름을 바꾸는 순간 조용히 다른 화면이 열린다.
 */
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import AppTooltip from "@/components/AppTooltip.vue";
import ThemeToggle from "@/components/ThemeToggle.vue";
import FilePick from "@/components/FilePick.vue";
import DataGrid from "@/components/DataGrid.vue";
import { listScreens, openBook, type HeaderSpec, type ScreenDef } from "@/lib/grid";
import { AppError, start } from "@/lib/book";
import { APP_NAME, APP_TAGLINE } from "@/lib/app";
import { ui } from "@/i18n";

const KIND_VIEW = { grid: DataGrid } as const;

const groupNames = ref<string[]>([]);
const screens = ref<ScreenDef[]>([]);
const current = ref<ScreenDef | null>(null);

/**
 * 사용자가 지정한 시트별 머리글. **기재된 것만 보관한다** — 기재하지 않은
 * 시트는 바탕이 추정한 것을 사용하고, 그 추정은 바탕이 책임진다.
 */
const headers = ref<Record<string, HeaderSpec>>({});

const fileName = ref("");
const errorMessage = ref<string | null>(null);
/** 양식은 열렸는데 그 시트 하나가 비어 있는 것 — 오류가 아니라 안내다. */
const note = ref<string | null>(null);
const busy = ref(false);
/** wasm 을 아직 켜는 중인가. 켜지기 전에는 파일을 받아도 할 수 있는 일이 없다. */
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

/** 묶음에 들지 못한 시트. **조용히 사라지면 안 된다.** */
const orphans = computed(() => {
  const known = new Set(groupNames.value);
  return screens.value.filter((s) => !known.has(s.group));
});

/* 본문이 가로로도 스크롤한다(표가 화면보다 넓다). 표가 아닌 것들은 가로
   스크롤과 무관하게 제자리에 있어야 하므로 `sticky; left: 0` 에 **보이는 폭**
   을 줘야 한다. 그 폭을 여기서 재서 넘긴다. */
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
 * 고른 양식을 연다.
 *
 * 읽지 못한 것은 **그냥 넘어가지 않는다.** 앞서 열어 둔 표를 그대로 두고
 * 오류만 띄우면, 사람은 새 파일을 보고 있다고 믿은 채 옛 자료를 읽는다.
 */
async function pick(file: File) {
  busy.value = true;
  errorMessage.value = null;
  note.value = null;
  screens.value = [];
  groupNames.value = [];
  current.value = null;
  // 양식이 교체되면 앞 양식의 지정은 의미가 없다. 시트 명칭이 우연히 일치하면
  // 전혀 다른 표에 그 지정이 적용된다.
  headers.value = {};
  fileName.value = file.name;
  try {
    const catalog = await openBook(new Uint8Array(await file.arrayBuffer()), file.name);
    groupNames.value = catalog.groups;
    screens.value = catalog.screens;
    current.value = groups.value[0]?.items[0] ?? screens.value[0] ?? null;
    if (!screens.value.length) note.value = "이 파일에서 볼 수 있는 표를 찾지 못했습니다.";
  } catch (e) {
    errorMessage.value = e instanceof AppError ? e.message : String(e);
    fileName.value = "";
  } finally {
    busy.value = false;
  }
}

/** 머리글을 판별하지 못한 시트는 표가 없다. **빈 표로 그리지 않는다.** */
const headless = computed(() => current.value !== null && !current.value.headerFound);

/**
 * 머리글 지정을 변경한다. 목록 전체를 재요청해 **열 정의를 교체한다.**
 *
 * 열의 개수도 명칭도 종류도 함께 변경되므로, 지금 화면이 보유한 열 정의를
 * 부분적으로 고칠 수 없다. 바탕이 다시 산출한 것으로 통째로 교체한다.
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
 * **자동으로 적용하지 않는다.** 사람이 누른 뒤에 적용된다 — 추정에 실패한
 * 시트에 임의의 값을 적용해 표를 그리면, 그 표가 맞는지 아무도 확인하지 않는다.
 */
const FIRST_GUESS: HeaderSpec = { at: 0, rows: 1 };
</script>

<template>
  <!-- no-print: 인쇄할 때 앱 화면은 감추고 인쇄 뷰만 남긴다 -->
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
      <!-- 시트 목록. 양식을 열기 전에는 비어 있으므로 자리도 차지하지 않는다. -->
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
                      item.headerFound ? item.title : `${item.title} — 머리글 줄을 찾지 못했습니다`
                    "
                    @click="current = item"
                  >
                    {{ item.title }}
                  </button>
                </li>
              </ul>
            </template>

            <!-- 묶음에 들지 못한 시트. 조용히 사라지면 못 알아채므로 띄운다. -->
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
                    :data-tip="`묶음 '${item.group}' 이 목록에 없습니다`"
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

      <!-- 세로 스크롤이 일어나는 유일한 곳. 표의 열 머리글만 위에 붙는다. -->
      <main ref="mainEl" class="app-main" :style="mainVars">
        <p v-if="starting" class="app-fixed pt-5 text-fg-muted">{{ ui.loading }}…</p>

        <template v-else>
          <div class="app-fixed shrink-0 pt-5 pb-1">
            <FilePick
              :busy="busy"
              :note="errorMessage ?? note"
              :danger="errorMessage !== null"
              @pick="pick"
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
                이 시트의 머리글을 판별하지 못했습니다. 열 이름이 몇 번째 줄에 몇 줄로
                적혀 있는지 지정해 주세요 — 원본에서 열 이름이 보이는 줄입니다.
              </p>
              <button
                type="button"
                class="rounded-md bg-accent px-4 py-2 font-medium text-accent-fg hover:bg-accent-hover"
                @click="setHeader(current.id, FIRST_GUESS)"
              >
                1행부터 1줄로 보기
              </button>
            </div>
            <!-- 무엇으로 그릴지는 화면 정의의 kind 가 정한다. -->
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

    <!-- 얹었을 때 뜨는 설명. 화면 전체에 하나. 자리를 차지하지 않는다. -->
    <AppTooltip />
  </div>
</template>
