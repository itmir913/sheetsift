<script setup lang="ts">
/**
 * 양식을 개방하기 이전의 판.
 *
 * **이 위치에만 설명이 존재한다.** 표를 열람하는 중에는 표가 높이를 전부
 * 사용하므로, 거기 안내 띠를 증설하면 그만큼 표가 축소된다. 이 도구가
 * 무엇인지·무엇을 개방할 수 있는지·자료가 어디로 가지 않는지를 판독하는 것은
 * 최초 1회라 시작 화면이 정위치다.
 *
 * **벌을 인지하지 않는다.** 창에서 구동하든 브라우저에서 구동하든 동일한 판을
 * 묘사한다. 개방하는 경로만 `FilePick` 이 능력으로 판정한다.
 */
import FilePick from "@/components/FilePick.vue";
import { t } from "@/i18n";

defineProps<{ busy?: boolean; note?: string | null; danger?: boolean }>();
const emit = defineEmits<{ pick: [file: File]; pickAt: [path: string, name: string] }>();

/** 열쇠를 산출하지 않고 **기재한다.** 문자열을 조립하면 열쇠가 실재하는지를
    타입이 판정하지 못해, 오기가 화면에 열쇠 그대로 표시된다. */
const USAGE = [
  ["start.usage.1.lead", "start.usage.1.text"],
  ["start.usage.2.lead", "start.usage.2.text"],
  ["start.usage.3.lead", "start.usage.3.text"],
  ["start.usage.4.lead", "start.usage.4.text"],
  ["start.usage.5.lead", "start.usage.5.text"],
] as const;

const PRIVACY = [
  ["start.privacy.1.lead", "start.privacy.1.text"],
  ["start.privacy.2.lead", "start.privacy.2.text"],
  ["start.privacy.3.lead", "start.privacy.3.text"],
] as const;

const REPO = "https://github.com/itmir913/sheetsift";
const KIT = "https://luminousky.com/teacher-utility-kit/";
const MAIL = "hello@luminousky.com";
</script>

<template>
  <!-- 광폭에서는 두 칸으로 전개한다. 한 칸으로 적재하면 1280px 에서도 하단이
       절단되어 수직 이동이 발생하는데, 정작 좌우는 공백이다. 파일을 수령하는
       자리를 좌측에 크게 배치하고, 판독할 것들을 우측에 적재한다. -->
  <div class="app-start">
    <div class="lg:col-span-2">
      <h2 class="text-3xl font-bold tracking-tight">{{ t("start.title") }}</h2>
      <p class="mt-2 text-pretty text-lg text-fg-muted">{{ t("start.lede") }}</p>
    </div>

    <!-- 수령 영역. 우측의 카드 둘과 동일한 높이를 점유한다. -->
    <div class="lg:col-start-1 lg:row-span-2 lg:row-start-2 lg:h-full">
      <FilePick
        :busy="busy"
        :note="note"
        :danger="danger"
        @pick="emit('pick', $event)"
        @pick-at="(p, n) => emit('pickAt', p, n)"
      />
    </div>

    <section class="app-card lg:col-start-2 lg:row-start-2">
      <h3 class="text-lg font-medium text-fg">{{ t("start.formats.title") }}</h3>
      <!-- 문장으로 기재한다. 항목으로 분절하면 **조건을 나열한 목록**으로
           판독되는데, 여기 기재하는 것은 개방 가능한 범위와 그 한계다. -->
      <div class="app-card-text">
        <p>{{ t("start.formats.1") }}</p>
        <p>{{ t("start.formats.2") }}</p>
        <p>{{ t("start.formats.3") }}</p>
      </div>
    </section>

    <section class="app-card lg:col-start-2 lg:row-start-3">
      <h3 class="text-lg font-medium text-fg">{{ t("start.usage.title") }}</h3>
      <div class="app-card-text">
        <p v-for="[lead, text] in USAGE" :key="lead">
          <strong class="block font-semibold text-fg">{{ t(lead) }}</strong>
          {{ t(text) }}
        </p>
      </div>
    </section>

    <section class="app-card lg:col-span-2">
      <h3 class="text-lg font-medium text-fg">{{ t("start.privacy.title") }}</h3>
      <div class="app-card-text">
        <p v-for="[lead, text] in PRIVACY" :key="lead">
          <strong class="block font-semibold text-fg">{{ t(lead) }}</strong>
          {{ t(text) }}
        </p>
      </div>
    </section>

    <!-- 만든 사람을 탐색하는 것도 최초 1회라 여기에만 존치한다.
         **고정하지 않는다** — 하단에 부착하면 협소한 높이에서 안내를 은폐한다. -->
    <footer class="app-foot lg:col-span-2">
      <p class="flex flex-wrap items-center justify-center gap-x-2 gap-y-1">
        <a class="app-link" :href="KIT" target="_blank" rel="noopener noreferrer">
          Teacher Utility Kit
        </a>
        <span class="hidden text-line sm:inline" aria-hidden="true">·</span>
        <a class="app-link" :href="REPO" target="_blank" rel="noopener noreferrer">
          {{ t("foot.repo") }}
        </a>
        <span class="hidden text-line sm:inline" aria-hidden="true">·</span>
        <a class="app-link" :href="`mailto:${MAIL}`">{{ MAIL }}</a>
      </p>
      <p class="mt-1">© luminousky</p>
    </footer>
  </div>
</template>
