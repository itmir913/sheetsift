import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";
import tailwindcss from "@tailwindcss/vite";
import { fileURLToPath, URL } from "node:url";

// **경계의 두 벌 중 하나를 선택한다.** 화면은 `sheetsift-book` 이라는 명칭으로
// 반입하고 어느 쪽인지 인지하지 않는다 — 인지하게 하면 화면이 두 벌이 되고, 두
// 벌은 한쪽만 수정된다.
//
// 창에서 구동할 때는 tauri 가 `--mode tauri` 를 부여한다
// (`crates/tauri/tauri.conf.json` 의 `beforeDevCommand`·`beforeBuildCommand`).
// 환경 변수가 아니라 모드인 사유는 **운영체제마다 부여하는 방법이 상이하지
// 않아서**다 — 명령 하나가 윈도와 리눅스에서 동일하게 작동한다.
export default defineConfig(({ mode }) => ({
  plugins: [vue(), tailwindcss()],
  resolve: {
    alias: {
      "@": fileURLToPath(new URL("./src", import.meta.url)),
      // 컴파일한 wasm 겹. `docs/wasm.md` 의 2행이 여기에 산출한다 — 저장소에는
      // 수용하지 않으므로, 컴파일하지 않은 상태로 빌드하면 즉시 정지한다.
      "sheetsift-wasm": fileURLToPath(new URL("./pkg/sheetsift_wasm.js", import.meta.url)),
      "sheetsift-book": fileURLToPath(
        new URL(`./src/lib/book.${mode === "tauri" ? "tauri" : "wasm"}.ts`, import.meta.url),
      ),
    },
  },
  // Pages 는 저장소 명칭이 부착된 위치에 배포된다. 절대 경로로 컴파일하면 그
  // 위치에서 404 가 되고, 배포는 정상적으로 성공한다 — 상대 경로로 컴파일한다.
  base: "./",
  build: {
    outDir: "dist",
    emptyOutDir: true,
    target: "chrome120",
  },
}));
