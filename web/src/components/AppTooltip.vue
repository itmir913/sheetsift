<script setup lang="ts">
/**
 * 부유 설명 1개. 앱 전체에 하나뿐이라 App.vue 가 1회만 배치한다.
 *
 * 위치는 **묘사한 뒤 측정해** 결정한다. 문자 길이에 따라 폭이 상이하므로 사전
 * 산출이 불가하다. 측정 이전에는 은폐해 두어(`data-placed`) 무관한 위치에서
 * 이동하는 것이 노출되지 않게 한다.
 */
import { computed, nextTick, onMounted, ref, watch } from "vue";
import { installTooltip, tooltip } from "@/composables/useTooltip";

/** 지시 대상과 설명 사이 간격(px). */
const GAP = 8;
/** 창 경계에 이만큼은 존치한다(px). */
const EDGE = 8;

const el = ref<HTMLElement | null>(null);
const x = ref(0);
const y = ref(0);
const placed = ref(false);

const tipVars = computed(() => ({ "--tip-x": `${x.value}px`, "--tip-y": `${y.value}px` }));

/**
 * 개행으로 분할 기재한 설명은 **첫 행이 대상의 명칭이고 나머지가 안내**다
 * (`grid.head.tip` 의 열 명칭과 조작 안내). 동일한 농도로 묘사하면 양자가
 * 한 문장으로 판독되어, 접촉할 때마다 전부를 재판독하게 된다.
 */
const head = computed(() => tooltip.text.split("\n")[0]);
const hint = computed(() => tooltip.text.slice(head.value.length + 1));

onMounted(installTooltip);

watch(
  () => [tooltip.open, tooltip.text, tooltip.anchor],
  async () => {
    if (!tooltip.open) {
      placed.value = false;
      return;
    }
    await nextTick();
    place();
  },
);

/**
 * **상단에 부착한다.** 마우스는 지시 대상의 중앙에 위치하고, 커서 도형은 하단
 * 으로 연장된다. 하단에 표시하면 그 커서가 문자 상자 위에 중첩되어 첫 문자를
 * 은폐한다.
 *
 * 창 상단에 여유가 부재하면 하단으로 이전한다. 좌우는 창 내부로 조정한다.
 */
function place(): void {
  const box = el.value?.getBoundingClientRect();
  if (!box) return;
  const a = tooltip.anchor;

  const right = Math.max(EDGE, window.innerWidth - box.width - EDGE);
  x.value = Math.min(Math.max((a.left + a.right) / 2 - box.width / 2, EDGE), right);

  const above = a.top - box.height - GAP;
  y.value = above < EDGE
    ? Math.min(a.bottom + GAP, Math.max(EDGE, window.innerHeight - box.height - EDGE))
    : above;

  placed.value = true;
}
</script>

<template>
  <Teleport to="body">
    <div
      v-if="tooltip.open"
      ref="el"
      class="tip no-print"
      role="tooltip"
      :data-placed="placed"
      :style="tipVars"
    >
      <span class="block">{{ head }}</span>
      <span v-if="hint" class="tip-hint">{{ hint }}</span>
    </div>
  </Teleport>
</template>
