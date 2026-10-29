import { createApp } from "vue";
import App from "./App.vue";
import { installSharedUi } from "sheetsift-ui";
import { sharedUi } from "./lib/shared-ui";
import "./styles/base.css";

/* 문서 제목은 **여기서 부여하지 않는다.** 탐색기와 공유 카드는 기동 이전의
   문서를 판독하므로, 켠 뒤에 부여하면 공백으로 수집된다. `index.html` 이
   정본이고, 그것이 화면의 명칭과 분기하지 않는지는
   `crates/shell/tests/screen.rs` 가 대조한다. */

/* 공용 화면에 주입한다. **장착 이전이어야 한다** — 최초 테마를 <html> 에 즉시
   반영하고, 주입하지 않은 채로 묘사된 부품이 정지하지 않게 한다. */
installSharedUi(sharedUi);

createApp(App).mount("#app");
