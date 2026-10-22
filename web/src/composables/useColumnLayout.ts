import { computed, ref, watch, type Ref } from "vue";
import { storageKey } from "@/lib/app";
import { autoWidthRem, fontEpoch, headerWidthRem } from "@/lib/colWidth";
import type { ColumnDef, Row, ScreenDef } from "@/lib/grid";

/** 화면에 실제로 그려지는 열. `srcIndex` 는 백엔드가 보낸 행 배열에서의 원래 위치다. */
export interface LaidOutColumn extends ColumnDef {
  srcIndex: number;
  /** 열 너비(rem). 머리글을 재서 정하고, 사용자가 끌면 그 값으로 덮는다. */
  width: number;
  /**
   * 남는 자리를 **몇 몫** 가져갈 열인가. `width` 는 그때 최소 너비가 된다.
   *
   * 그냥 두면 남는 자리가 통째로 오른쪽 끝에 남아 마지막 열만 늘어난 것처럼
   * 보인다. 몫을 매기면 같은 뜻의 열들(교과조합 넷, 원점수·표준점수·백분위·등급)이
   * 같은 폭으로 서고, 글자가 긴 열에 더 줄 수도 있다.
   */
  flex?: number;
}

interface StoredLayout {
  order?: string[];
  widths?: Record<string, number>;
  pinned?: number;
}

/** 열 너비 한계(rem). 너무 좁으면 정렬 표식이 라벨을 먹고, 너무 넓으면 화면을 잡아먹는다. */
export const MIN_WIDTH = 6;
export const MAX_WIDTH = 40;

/**
 * 왼쪽에 붙잡아 둘 수 있는 열의 최대 수.
 *
 * `grid.css` 의 `[data-pin="N"]` 규칙 수와 같아야 한다 — 규칙이 없는 번호는
 * 붙잡히지 않고 그냥 흘러간다. 오류는 나지 않는다. `crates/web/tests/screen.rs`
 * 가 두 파일을 세어서 본다.
 */
export const MAX_PIN = 4;

/** 기준 열 하나는 기본으로 붙잡아 둔다. 표를 옆으로 밀어도 무엇의 줄인지 남는다. */
const DEFAULT_PIN = 1;

function clampWidth(rem: number): number {
  return Math.min(MAX_WIDTH, Math.max(MIN_WIDTH, rem));
}

/**
 * 사용자가 바꾼 열 순서·너비·고정 수를 화면별로 기억한다.
 *
 * 기본값은 바탕이 내준 열 정의에서 오고, 사람이 손댄 것만 덮어쓴다. 양식이
 * 바뀌어 열이 늘거나 줄어도 저장된 차례를 그대로 쓰지 않고 지금 정의와
 * 대조해 걸러내므로, 다음에 열 때 화면이 깨지지 않는다.
 *
 * 셋을 **한자리에 둔다.** 같은 단추가 화면에 따라 다르게 굴면 고장으로 읽힌다.
 */
export function useColumnLayout(screen: Ref<ScreenDef>) {
  const order = ref<string[]>([]);
  const widths = ref<Record<string, number>>({});
  const pinned = ref(DEFAULT_PIN);

  const key = computed(() => storageKey(`cols.${screen.value.id}`));

  function defaultOrder(): string[] {
    return screen.value.columns.map((c) => c.key);
  }

  function load() {
    auto.value = null;
    settled.value = false;
    const defaults = defaultOrder();
    let stored: StoredLayout = {};
    try {
      const raw = localStorage.getItem(key.value);
      if (raw) stored = JSON.parse(raw) as StoredLayout;
    } catch {
      /* 저장 내용이 깨졌으면 기본값으로 간다. */
    }

    // 저장된 순서 중 지금도 존재하는 열만 남기고, 새로 생긴 열은 뒤에 붙인다.
    const known = new Set(defaults);
    const kept = (stored.order ?? []).filter((k) => known.has(k));
    const missing = defaults.filter((k) => !kept.includes(k));
    order.value = [...kept, ...missing];

    const w: Record<string, number> = {};
    for (const [k, v] of Object.entries(stored.widths ?? {})) {
      if (known.has(k) && Number.isFinite(v)) {
        w[k] = clampWidth(v);
      }
    }
    widths.value = w;

    // 열이 줄어든 양식에서는 저장된 수가 지금 열 수를 넘을 수 있다.
    const p = stored.pinned;
    pinned.value = Number.isInteger(p) && p! >= 0 ? Math.min(p!, MAX_PIN) : DEFAULT_PIN;
  }

  function save() {
    try {
      const payload: StoredLayout = {
        order: order.value,
        widths: widths.value,
        pinned: pinned.value,
      };
      localStorage.setItem(key.value, JSON.stringify(payload));
    } catch {
      /* 저장 실패는 무시한다. */
    }
  }

  /** 원래 정의에서의 위치. 백엔드 행 배열을 읽을 때 쓴다. */
  const srcIndexOf = computed(() => {
    const map = new Map<string, number>();
    screen.value.columns.forEach((c, i) => map.set(c.key, i));
    return map;
  });

  /**
   * 자료까지 보고 잡은 너비. 화면을 열 때 **딱 한 번** 채운다.
   *
   * 쪽을 넘기거나 정렬·필터를 바꿀 때 다시 잡지 않는다. 그때는 표가 이미
   * 사용자 앞에 그려져 있고 너비를 끌어 조절할 기회도 있었다. 거기서 폭이
   * 다시 움직이면 손대지 않은 것이 저절로 바뀌는 셈이다.
   */
  const auto = ref<Map<string, number> | null>(null);

  /** 잰 값이 자료까지 본 것인가. 빈 결과로 잡은 것은 잠정이다. */
  const settled = ref(false);

  /** 너비를 다 재어 표를 그려도 되는가. 재기 전에 그리면 폭이 한 번 튄다. */
  const ready = computed(() => auto.value !== null);

  /**
   * 조회 결과로 너비를 잡는다. **자료를 한 번 보고 나면 다시 잡지 않는다.**
   *
   * 결과가 비었거나 조회가 실패했을 때도 불러야 한다 — 머리글만으로 잡고
   * 표를 그린다. 그렇게 잡은 것은 잠정이라, 뒤에 자료가 딸린 결과가 오면
   * 그때 한 번 다시 잡는다.
   */
  function measure(rows: Row[]) {
    if (settled.value) return;
    const out = new Map<string, number>();
    screen.value.columns.forEach((c, at) => {
      out.set(c.key, clampWidth(autoWidthRem(c.label, c.kind, at, rows)));
    });
    auto.value = out;
    settled.value = rows.length > 0;
  }

  /** 아직 재기 전에 쓰는 폭. 머리글이 다 보이는 만큼. */
  const byHeader = computed<Map<string, number>>(() => {
    void fontEpoch.value;
    const out = new Map<string, number>();
    for (const c of screen.value.columns) {
      out.set(c.key, clampWidth(headerWidthRem(c.label)));
    }
    return out;
  });

  const columns = computed<LaidOutColumn[]>(() => {
    const byKey = new Map(screen.value.columns.map((c) => [c.key, c]));
    return order.value.flatMap((key) => {
      const col = byKey.get(key);
      if (!col) return [];
      return [
        {
          ...col,
          width:
            widths.value[key] ?? auto.value?.get(key) ?? byHeader.value.get(key) ?? MIN_WIDTH,
          srcIndex: srcIndexOf.value.get(key) ?? 0,
        },
      ];
    });
  });

  /**
   * 붙잡을 수 있는 열 수. 다 붙잡으면 밀 것이 없으므로 하나는 남긴다.
   */
  const pinLimit = computed(() => Math.min(MAX_PIN, columns.value.length - 1));

  function setPinned(n: number) {
    pinned.value = Math.max(0, Math.min(n, pinLimit.value));
    save();
  }

  /** `from` 위치의 열을 `to` 위치로 옮긴다. */
  function move(from: number, to: number) {
    if (from === to || from < 0 || to < 0) return;
    const next = [...order.value];
    const [k] = next.splice(from, 1);
    if (k === undefined) return;
    next.splice(to, 0, k);
    order.value = next;
    save();
  }

  function resize(key: string, width: number) {
    widths.value = {
      ...widths.value,
      [key]: clampWidth(width),
    };
  }

  /** 드래그가 끝났을 때 한 번만 저장한다(드래그 중 매 프레임 저장하지 않는다). */
  function commit() {
    save();
  }

  function reset() {
    order.value = defaultOrder();
    widths.value = {};
    pinned.value = DEFAULT_PIN;
    try {
      localStorage.removeItem(key.value);
    } catch {
      /* 무시 */
    }
  }

  const isCustomized = computed(() => {
    const d = defaultOrder();
    return (
      Object.keys(widths.value).length > 0 ||
      pinned.value !== DEFAULT_PIN ||
      order.value.length !== d.length ||
      order.value.some((k, i) => k !== d[i])
    );
  });

  watch(() => screen.value.id, load, { immediate: true });

  return {
    columns,
    order,
    widths,
    pinned,
    pinLimit,
    setPinned,
    ready,
    measure,
    move,
    resize,
    commit,
    reset,
    isCustomized,
  };
}
