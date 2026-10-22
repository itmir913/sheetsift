<script setup lang="ts">
/**
 * 떠 있는 설명 한 개. 앱 전체에 하나뿐이라 App.vue 가 한 번만 놓는다.
 *
 * 자리는 **그려 놓고 재서** 정한다. 글자 길이에 따라 폭이 달라지므로 미리
 * 계산할 수 없다. 재기 전에는 감춰 두어(`data-placed`) 엉뚱한 자리에서
 * 옮겨 가는 것이 보이지 않게 한다.
 */
import { computed, nextTick, onMounted, ref, watch } from "vue";
import { installTooltip, tooltip } from "@/composables/useTooltip";

/** 가리키는 것과 설명 사이 틈(px). */
const GAP = 8;
/** 창 가장자리에 이만큼은 남긴다(px). */
const EDGE = 8;

const el = ref<HTMLElement | null>(null);
const x = ref(0);
const y = ref(0);
const placed = ref(false);

const tipVars = computed(() => ({ "--tip-x": `${x.value}px`, "--tip-y": `${y.value}px` }));

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
 * **위에 붙인다.** 마우스는 얹은 것의 한가운데에 있고, 커서 그림은 아래로
 * 뻗는다. 아래에 띄우면 그 커서가 글상자 위에 겹쳐 첫 글자를 가린다.
 *
 * 창 위쪽에 자리가 없으면 아래로 넘긴다. 좌우는 창 안으로 당긴다.
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
      {{ tooltip.text }}
    </div>
  </Teleport>
</template>
