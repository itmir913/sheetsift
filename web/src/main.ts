import { createApp } from "vue";
import App from "./App.vue";
import "./styles/base.css";
import "./composables/useTheme"; // 최초 테마를 <html> 에 즉시 반영한다.

/* 문서 제목은 **여기서 부여하지 않는다.** 탐색기와 공유 카드는 기동 이전의
   문서를 판독하므로, 켠 뒤에 부여하면 공백으로 수집된다. `index.html` 이
   정본이고, 그것이 화면의 명칭과 분기하지 않는지는
   `crates/shell/tests/screen.rs` 가 대조한다. */

createApp(App).mount("#app");
