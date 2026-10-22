import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";
import tailwindcss from "@tailwindcss/vite";
import { fileURLToPath, URL } from "node:url";

export default defineConfig({
  plugins: [vue(), tailwindcss()],
  resolve: {
    alias: {
      "@": fileURLToPath(new URL("./src", import.meta.url)),
      // 구운 wasm 겹. `docs/wasm.md` 의 두 줄이 여기에 내놓는다 — 저장소에는
      // 담지 않으므로, 굽지 않은 채로 빌드하면 바로 선다.
      "sheetsift-wasm": fileURLToPath(new URL("./pkg/sheetsift_web.js", import.meta.url)),
    },
  },
  // Pages 는 저장소 이름이 붙은 자리에 올라간다. 절대 경로로 구우면 그 자리에서
  // 404 가 되고, 배포는 멀쩡히 성공한다 — 상대 경로로 굽는다.
  base: "./",
  build: {
    outDir: "dist",
    emptyOutDir: true,
    target: "chrome120",
  },
});
