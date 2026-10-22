import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";
import tailwindcss from "@tailwindcss/vite";
import { fileURLToPath, URL } from "node:url";

export default defineConfig({
  plugins: [vue(), tailwindcss()],
  resolve: {
    alias: {
      "@": fileURLToPath(new URL("./src", import.meta.url)),
      // 컴파일한 wasm 겹. `docs/wasm.md` 의 2행이 여기에 산출한다 — 저장소에는
      // 수용하지 않으므로, 컴파일하지 않은 상태로 빌드하면 즉시 정지한다.
      "sheetsift-wasm": fileURLToPath(new URL("./pkg/sheetsift_wasm.js", import.meta.url)),
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
});
