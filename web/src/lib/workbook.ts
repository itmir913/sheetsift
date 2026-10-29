/**
 * 이 앱이 양식을 개방하고, 공용 화면(`sheetsift-ui`)이 자료를 수령하는 문.
 *
 * 표·조작부·인쇄는 공용 패키지의 것이고, 그 패키지는 자료를 어디서 수령하는지
 * 인지하지 않는다. 여기서 개방한 `Book` 을 [`bookSource`] 로 포장해 주입한다
 * (`shared-ui.ts`).
 */
import { shallowRef } from "vue";
import { Book, limits } from "sheetsift-book";
import type { GridSource } from "sheetsift-ui";
import {
  setBlankMark,
  setCellDecimals,
  type ColumnValuesResult,
  type HeaderSpec,
  type QueryResponse,
  type ScreenCatalog,
} from "sheetsift-ui/lib/grid";

/* ── 개방한 양식 ──────────────────────────────────────────────── */

/**
 * 현재 개방된 양식.
 *
 * **단일 위치에 배치한다.** goe-navi 는 자료가 앱 내부에 존재해 임의 시점에
 * 호출 가능했으나, 여기서는 사람이 파일을 선택한 이후에야 바탕이 구성된다.
 * 화면마다 책을 보유하게 하면 일부 화면은 수령하고 일부 화면은 수령하지 못하는
 * 식으로 분기한다.
 */
// `shallowRef` 다. `ref` 는 내부를 순회하며 포장하는데, 포장한 것은 더 이상
// `Book` 이 아니다 — 경계를 은폐한 위치가 함께 제거된다.
const open = shallowRef<Book | null>(null);

/** 개방한 양식이 아직 부재하는데 호출한 위치. **묵시적으로 공백 페이지를 반환하지 않는다.** */
function book(): Book {
  const b = open.value;
  if (!b) throw new Error("no workbook is open");
  return b;
}

/** 양식 바이트를 수용해 개방한다. 기존에 개방한 것은 폐기한다. */
export async function openBook(bytes: Uint8Array, name: string): Promise<ScreenCatalog> {
  return received(await Book.open(bytes, name));
}

/**
 * **경로로 개방한다.** 선택 대화상자가 반환한 것이 그것이다.
 *
 * 바이트로 개방하는 쪽과 동일한 위치로 귀착시킨다 — 개방 이후의 처리가 두 벌이
 * 되면 일방만 수정된다.
 */
export async function openBookAt(path: string): Promise<ScreenCatalog> {
  return received(await Book.openAt(path));
}

async function received(b: Book): Promise<ScreenCatalog> {
  open.value = b;
  const catalog = await b.listScreens<ScreenCatalog>({});
  setBlankMark(catalog.blank);
  setCellDecimals(catalog.decimals);
  return catalog;
}

/**
 * 화면 목록. 시트별 헤더 지정을 지참한다.
 *
 * 기재하지 않은 시트는 바탕이 추정한 것을 적용한다. 사용자가 한 시트의 지정을
 * 변경하면 그 시트를 기재해 재요청하고, 열 정의 전체를 교체한다.
 */
export function listScreens(
  headers: Record<string, HeaderSpec> = {},
): Promise<ScreenCatalog> {
  return book().listScreens<ScreenCatalog>({ headers });
}

/**
 * 공용 화면이 호출하는 문. **개방한 양식을 호출 시점에 참조한다** — 주입하는 시점에는
 * 아직 개방된 것이 부재하다.
 */
export const bookSource: GridSource = {
  limits,
  queryRows: (req) => book().queryRows<QueryResponse>(req),
  columnValues: (req) => book().columnValues<ColumnValuesResult>(req),
};
