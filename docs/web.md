# 웹 껍데기를 굽고 띄우는 법

`crates/web` 이 바탕을 브라우저에 붙이는 겹이고, `web/` 이 그것을 쓰는 한 장
짜리 화면이다. 번들러도 npm 도 쓰지 않는다 — 화면이 모듈 JS 한 벌이라 그럴
까닭이 없다.

**이 글은 규칙이 아니라 기록이다.** 되는 것과 안 되는 것은 글이 아니라 시험이
말한다.

## 굽기

```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --locked

cargo build -p sheetsift-web --target wasm32-unknown-unknown --release
wasm-bindgen --target web --no-typescript --out-dir web/pkg \
  target/wasm32-unknown-unknown/release/sheetsift_web.wasm
```

`wasm-bindgen-cli` 의 판은 `wasm-bindgen` 크레이트와 **같아야** 한다. 어긋나면
도구가 그렇다고 말하고 선다 — 조용히 틀리지 않는다.

구운 것(`web/pkg/`)은 저장소에 담지 않는다. 담으면 소스와 구운 것이 갈라지고,
어느 쪽이 참인지 알 수 없어진다.

## 띄우기

`file://` 로는 안 된다. 모듈 JS 와 wasm 은 **같은 출처에서 받아야** 한다.

```bash
cd web && python3 -m http.server 8080
```

## 이 셋을 왜 `cargo test` 에 넣지 않았나

굽기는 검사가 아니다. 그리고 **돌리는 명령을 새로 만들지 않는다** — 밖에 두면
어디선가만 돌고 어디선가는 안 돌게 된다. 그래서 굽는 줄은 이 글에만 있다.

대신 **관문이 볼 수 있는 자리를 최대한 넓혀 두었다.**

`crates/web` 의 알맹이(`Viewer`)에는 `wasm_bindgen` 이 한 줄도 없다. 보통
Rust 라서 `cargo test --workspace` 가 데스크톱에서 그대로 본다 — 양식을 열고,
시트를 고르고, 정렬·필터를 걸고, 머리글을 못 찾았을 때 어떻게 되는지까지.

브라우저와 주고받는 겹(`bindings.rs`)은 글자를 받아 글자를 돌려주는 껍질뿐이고,
그것만 따로 본다.

```bash
cargo test -p sheetsift-web --target wasm32-unknown-unknown
```

**여기 로직이 들어오면 그만큼이 관문 밖으로 빠진다.** 얇게 두는 것이 요점이다.

## 브라우저로 돌려 보고 알게 된 것

돌려 보지 않았으면 몰랐을 것이 둘 있었다.

**`hidden` 이 안 듣는다.** 브라우저 기본 규칙은 `[hidden] { display: none }`
인데, 조작부에 `display: flex` 를 주면 그쪽이 이긴다. 감춘 줄 알았던 조작부가
**그대로 보인 채** 앞 시트의 검색어와 정렬 칩을 들고 있었다. 화면과 자료가
어긋나는데 아무 오류도 나지 않는다. `[hidden] { display: none !important }`
한 줄로 막았고, 까닭은 `web/style.css` 에 적어 두었다.

**시계가 없다.** 이것은 이 껍데기를 짓기 전에 바탕에서 먼저 났다 —
`docs/wasm.md` 에 있다.

둘 다 **글자로 보는 검사도 데스크톱 시험도 보지 못하는** 부류다.

## 지금 되는 것

진짜 양식(교육청 자료를 다듬은 수십 MB 짜리 xlsx)으로 돌려 보았다.

- 양식을 열어 시트 목록을 내는 데 몇 초
- 십만 행 넘는 시트 하나를 SQLite 로 바꾸는 데 일 초 안쪽
- 그 뒤로 정렬은 수십 밀리초, 모든 열을 훑는 전체 검색은 수백 밀리초

**숫자는 적지 않는다.** 기계와 파일이 달라지면 달라지고, 그때 이 글만 낡는다.
직접 재 보는 것이 빠르다 — 화면이 조회마다 걸린 시간을 말해 준다.

**자료는 브라우저 밖으로 나가지 않는다.** 올리는 곳이 없다. 파일을 읽는 것도
바꾸는 것도 조회하는 것도 전부 그 자리에서 일어난다.

## 아직 없는 것

- **값 선택 목록.** 바탕에는 있고(`grid::column_values`, 껍데기의 `values`),
  화면이 아직 안 쓴다. 지금 거는 것은 전체 검색 하나다.
- **열 고르기·고정·너비.** 열이 많은 시트는 가로로 길다.
- **`.sift` 로 담기.** 바탕에 형식이 있으나 껍데기가 부르지 않는다. 한 번
  바꾼 것을 다시 쓰려면 그 길이 있어야 한다.
- **아주 큰 파일.** wasm 은 32비트라 주소 공간이 묶인다. 어디서 막히는지는
  재어 보지 않았다.
