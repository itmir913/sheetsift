import { createApp } from "vue";
import App from "./App.vue";
import { APP_NAME } from "./lib/app";
import "./styles/base.css";
import "./composables/useTheme"; // 최초 테마를 <html> 에 즉시 반영한다.

/* 문서 제목은 **기동 이후에 부여한다.** `index.html` 에 기재하면 명칭이 저장소
   두 곳에 존재하게 되고, 한쪽만 수정하면 탭 제목과 화면이 분기한다. */
document.title = APP_NAME;

createApp(App).mount("#app");
