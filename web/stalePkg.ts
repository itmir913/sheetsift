/**
 * **낡은 wasm 으로 화면을 구동하지 않는다.**
 *
 * `web/pkg` 는 `npm run pkg` 가 산출한다. Rust 를 수정한 뒤 그것을 실행하지
 * 않고 개발 서버만 기동하면 화면이 **구 엔진**과 대화하는데, 어디에서도 오류가
 * 발생하지 않는다 — 실측으로 19분 낡은 산출물을 두고 「값 목록이 범위를 반영
 * 하지 않는다」는 결함을 보고한 사례가 있다. 재빌드하니 정상이었다.
 *
 * 화면만 수정하는 날에는 vite 가 반영하므로 이 함정이 **보이지 않다가, 엔진을
 * 수정한 날에만** 묵시적으로 상이한 결과를 제시한다. 그래서 묵시적으로 오류가
 * 발생하는 쪽이 아니라 **정지하는 쪽**을 선택한다.
 *
 * `npm run dev:web` 과 `npm run build:web` 은 `npm run pkg` 를 선행하므로 이
 * 관문에 포착되지 않는다. 포착되는 것은 vite 를 직접 기동하는 경로다 —
 * `npm run dev --workspace web`, 그리고 **창의 개발 기동**
 * (`tauri.conf.json` 의 `beforeDevCommand` 가 그 경로다).
 */
import { existsSync, readdirSync, statSync } from "node:fs";
import { join, resolve } from "node:path";

/**
 * **wasm 묶음에 유입되지 않는 상자.** 나머지 `crates/*` 전부를 원본으로 산정한다.
 *
 * 열거하는 방향을 이렇게 둔 사유는 파손되는 방향이다. 유입되는 쪽을 열거하면
 * 상자가 하나 증가할 때 **묵시적으로 산정에서 누락**되고, 그 시점부터 그 상자를
 * 수정한 사람은 아무 경고 없이 구 엔진을 본다. 제외하는 쪽을 열거하면 반대로
 * 과잉 경고가 되는데, 그것은 똑똑히 출현한다.
 *
 * `crates/shell/tests/pkg_freshness.rs` 가 이 목록이 실제 의존 관계와 합치하는지
 * 를 판정한다 — 상자가 증감하면 그 시험이 정지한다.
 */
export const OUTSIDE = ["desk", "tauri", "webview"];

/**
 * 화면이 실제로 적재하는 산출물. `vite.config.ts` 의 `sheetsift-wasm` 별칭이
 * 지시하는 것과 **동일한 쌍**이다.
 */
export const ENTRY = ["sheetsift_wasm.js", "sheetsift_wasm_bg.wasm"];

/** 산출물이 부재하거나 낡았다는 진술. 정상이면 null 이다. */
export interface Stale {
  /** 영문으로 기재한다. 저장소가 공개다. */
  message: string;
}

/** 한 디렉터리 이하의 파일 시각 중 가장 최신. `target` 은 산출물이라 제외한다. */
function newestUnder(dir: string): { at: number; path: string } | null {
  let best: { at: number; path: string } | null = null;
  const walk = (at: string) => {
    for (const e of readdirSync(at, { withFileTypes: true })) {
      if (e.name === "target" || e.name === "node_modules" || e.name === "gen") continue;
      const p = join(at, e.name);
      if (e.isDirectory()) {
        walk(p);
        continue;
      }
      const t = statSync(p).mtimeMs;
      if (!best || t > best.at) best = { at: t, path: p };
    }
  };
  if (!existsSync(dir)) return null;
  walk(dir);
  return best;
}

/**
 * 산출물이 원본보다 낡았는가.
 *
 * 산출물은 **가장 낡은 것**을 본다(`.js` 와 `.wasm` 이 쌍이고, 한쪽만 갱신된
 * 것도 낡은 것이다). 원본은 가장 최신을 본다.
 */
export function stalePkg(at: string): Stale | null {
  /* 끝의 구분자 유무로 진술의 경로가 한 글자 밀리지 않게 정규화한다. */
  const root = resolve(at);
  const pkg = join(root, "web", "pkg");
  const run = "npm run pkg";
  /* **적재되는 둘만 본다.** 디렉터리 전체의 최고(最古)를 보면, 개칭 이전의
     산출물이 잔류하는 것으로 영구히 낡았다고 진술한다(실측). 쌍의 명칭은
     `vite.config.ts` 의 `sheetsift-wasm` 별칭이 지시하는 것과 동일하다. */
  const times: number[] = [];
  for (const n of ENTRY) {
    const at = statSync(join(pkg, n), { throwIfNoEntry: false })?.mtimeMs;
    if (at === undefined) {
      return { message: `web/pkg/${n} is missing. Compile the engine first: ${run}` };
    }
    times.push(at);
  }
  const built = Math.min(...times);

  const roots = readdirSync(join(root, "crates"), { withFileTypes: true })
    .filter((e) => e.isDirectory() && !OUTSIDE.includes(e.name))
    .map((e) => join(root, "crates", e.name));
  roots.push(join(root, "Cargo.toml"), join(root, "Cargo.lock"));

  let newest: { at: number; path: string } | null = null;
  for (const r of roots) {
    const found = statSync(r, { throwIfNoEntry: false })?.isDirectory()
      ? newestUnder(r)
      : existsSync(r)
        ? { at: statSync(r).mtimeMs, path: r }
        : null;
    if (found && (!newest || found.at > newest.at)) newest = found;
  }
  if (!newest || newest.at <= built) return null;

  const mins = Math.round((newest.at - built) / 60000);
  const when = (ms: number) => new Date(ms).toISOString().slice(0, 19).replace("T", " ");
  return {
    message:
      `web/pkg is ${mins} minute(s) older than the Rust sources, so the screen would ` +
      `talk to a stale engine.\n` +
      `  built:  ${when(built)}  web/pkg\n` +
      `  source: ${when(newest.at)}  ${newest.path.slice(root.length + 1)}\n` +
      `Recompile it: ${run}`,
  };
}

/**
 * vite 에 장착하는 관문. **경고가 아니라 정지한다** — 경고는 기동 로그 수십 행에
 * 섞여 판독되지 않고, 그러면 아무것도 하지 않은 것과 동일하다.
 */
export function freshPkg(root: string) {
  return {
    name: "sheetsift-fresh-pkg",
    /** 묘사 이전에 판정한다. 브라우저가 구 엔진을 적재하는 것 자체를 막는다. */
    buildStart() {
      const stale = stalePkg(root);
      if (stale) throw new Error(stale.message);
    },
  };
}
