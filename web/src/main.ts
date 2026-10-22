import { createApp } from "vue";
import App from "./App.vue";
import { APP_NAME } from "./lib/app";
import "./styles/base.css";
import "./composables/useTheme"; // 첫 테마를 <html> 에 곧바로 반영한다.

/* 문서 제목은 **켠 뒤에 붙인다.** `index.html` 에 적어 두면 이름이 저장소
   두 곳에 살게 되고, 한쪽만 고치면 탭 제목과 화면이 갈린다. */
document.title = APP_NAME;

createApp(App).mount("#app");
