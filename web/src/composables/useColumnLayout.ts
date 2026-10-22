import { computed, ref, watch, type Ref } from "vue";
import { storageKey } from "@/lib/app";
import { autoWidthRem, fontEpoch, headerWidthRem } from "@/lib/colWidth";
import type { ColumnDef, Row, ScreenDef } from "@/lib/grid";

/** 화면에 실제로 묘사되는 열. `srcIndex` 는 바탕이 전송한 행 배열에서의 원위치다. */
export interface LaidOutColumn extends ColumnDef {
  srcIndex: number;
  /** 열 폭(rem). 머리글을 측정해 결정하고, 사용자가 조작하면 그 값으로 피복한다. */
  width: number;
  /**
   * 잔여 공간을 **몇 배분** 수령할 열인가. `width` 는 그때 최소 폭이 된다.
   *
   * 그대로 유지하면 잔여 공간이 전체 우측 종단에 잔존해 최종 열만 확장된 것처럼
   * 인식된다. 배분을 부여하면 동일 의미의 열들(교과조합 4개, 원점수·표준점수·
   * 백분위·등급)이 동일한 폭으로 정렬되고, 문자가 장문인 열에 추가 배분할 수도
   * 있다.
   */
  flex?: number;
}

interface StoredLayout {
  order?: string[];
  widths?: Record<string, number>;
  pinned?: number;
}

/** 열 폭 한계(rem). 과도히 협소하면 정렬 표식이 라벨을 침범하고, 과도히 광범위하면 화면을 점유한다. */
export const MIN_WIDTH = 6;
export const MAX_WIDTH = 40;

/**
 * 좌측에 고정 가능한 열의 최대 수.
 *
 * `grid.css` 의 `[data-pin="N"]` 규칙 수와 일치해야 한다 — 규칙이 부재하는
 * 번호는 고정되지 않고 유실된다. 오류는 발생하지 않는다.
 * `crates/web/tests/screen.rs` 가 두 파일을 산정해 대조한다.
 */
export const MAX_PIN = 4;

/** 기준 열 하나는 기본으로 고정한다. 표를 수평 이동해도 무엇의 행인지 잔존한다. */
const DEFAULT_PIN = 1;

function clampWidth(rem: number): number {
  return Math.min(MAX_WIDTH, Math.max(MIN_WIDTH, rem));
}

/**
 * 사용자가 변경한 열 순서·폭·고정 수를 화면별로 기억한다.
 *
 * 기본값은 바탕이 제공한 열 정의에서 유래하고, 사람이 조작한 것만 피복한다.
 * 양식이 변경되어 열이 증감해도 저장된 순서를 그대로 적용하지 않고 현 정의와
 * 대조해 여과하므로, 차회 개방 시 화면이 훼손되지 않는다.
 *
 * 셋을 **단일 위치에 배치한다.** 동일한 단추가 화면에 따라 상이하게 작동하면
 * 고장으로 인식된다.
 */
export function useColumnLayout(screen: Ref<ScreenDef>) {
  const order = ref<string[]>([]);
  const widths = ref<Record<string, number>>({});
  const pinned = ref(DEFAULT_PIN);

  /* 머리글 지정이 열쇠에 포함된다. 지정이 변경되면 열의 개수도 명칭도 상이해져
     **상이한 표**가 되는데, 동일한 열쇠로 보관하면 선행 지정에서 조절한 폭과
     순서가 무관한 열에 적용된다. */
  const key = computed(() => {
    const h = screen.value.header;
    const span = h ? `${h.at}.${h.rows}` : "none";
    return storageKey(`cols.${screen.value.id}.${span}`);
  });

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
      /* 저장 내용이 훼손되었으면 기본값으로 진행한다. */
    }

    // 저장된 순서 중 현재도 존재하는 열만 존치하고, 신규 발생한 열은 후단에 부착한다.
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

    // 열이 감소한 양식에서는 저장된 수가 현 열 수를 초과할 수 있다.
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

  /** 원 정의에서의 위치. 바탕의 행 배열을 판독할 때 사용한다. */
  const srcIndexOf = computed(() => {
    const map = new Map<string, number>();
    screen.value.columns.forEach((c, i) => map.set(c.key, i));
    return map;
  });

  /**
   * 자료까지 참조해 산정한 폭. 화면 개방 시 **1회만** 충전한다.
   *
   * 페이지를 이동하거나 정렬·필터를 변경할 때 재산정하지 않는다. 그 시점에는
   * 표가 이미 사용자 앞에 묘사되어 있고 폭을 조작해 조절할 기회도 존재했다.
   * 거기서 폭이 재차 이동하면 조작하지 않은 것이 자동 변경되는 셈이다.
   */
  const auto = ref<Map<string, number> | null>(null);

  /** 측정값이 자료까지 참조한 것인가. 공백 결과로 산정한 것은 잠정이다. */
  const settled = ref(false);

  /** 폭을 전부 측정해 표를 묘사해도 되는가. 측정 이전에 묘사하면 폭이 1회 변동한다. */
  const ready = computed(() => auto.value !== null);

  /**
   * 조회 결과로 폭을 산정한다. **자료를 1회 참조한 이후에는 재산정하지 않는다.**
   *
   * 결과가 공백이거나 조회가 실패했을 때도 호출해야 한다 — 머리글만으로 산정해
   * 표를 묘사한다. 그렇게 산정한 것은 잠정이라, 이후 자료가 수반된 결과가
   * 도달하면 그 시점에 1회 재산정한다.
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

  /** 측정 이전에 사용하는 폭. 머리글이 전부 노출되는 만큼. */
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
   * 고정 가능한 열 수. 전부 고정하면 이동할 대상이 부재하므로 하나는 존치한다.
   */
  const pinLimit = computed(() => Math.min(MAX_PIN, columns.value.length - 1));

  function setPinned(n: number) {
    pinned.value = Math.max(0, Math.min(n, pinLimit.value));
    save();
  }

  /** `from` 위치의 열을 `to` 위치로 이전한다. */
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

  /** 조작이 종료된 시점에 1회만 저장한다(조작 중 매 프레임 저장하지 않는다). */
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

  watch(key, load, { immediate: true });

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
