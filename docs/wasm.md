# 웹에서 무엇이 되고 무엇이 안 되는가

이 바탕은 웹(GitHub Pages)을 1차 목표로 한다. 그래서 `core` 가 tauri 도 파일
시스템도 모르고 바이트만 주고받는다. 그 설계가 실제로 통하는지를 한 번 돌려
보고 적어 둔 글이다.

**이 글은 규칙이 아니라 기록이다.** 크레이트 판이 오르면 달라질 수 있다.
무엇이 되는지는 글이 아니라 시험이 말한다 — `crates/core/tests/wasm.rs` 다.

```bash
cargo test -p sheetsift-core --target wasm32-unknown-unknown
```

## 돌려 보는 것이 따로 필요하다

wasm 은 그냥 실행할 수 없다. 구운 것을 노드나 브라우저가 불러 주어야 한다.
`.cargo/config.toml` 이 그 도구를 적어 두었고, 없으면 받는다.

```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --locked
```

판은 `wasm-bindgen` 크레이트와 **같아야** 한다. 어긋나면 도구가 그렇다고 말하고
선다 — 조용히 틀리지 않는다.

### C 컴파일러가 하나 더 있어야 한다

위 둘로는 모자란다. `rusqlite` 를 `bundled` 로 받으므로 **SQLite 의 C 소스를
함께 컴파일한다.** 데스크톱에서는 Visual Studio 의 `cl.exe` 가 그 일을 해서
보이지 않지만, wasm 에서는 `cc-rs` 가 **`clang` 만** 찾는다. MSVC 가 그 타깃을
내지 못하기 때문이다. 없으면 이렇게 선다:

```
Compiling sqlite-wasm-rs
warning: Compiler family detection failed due to error:
  ToolNotFound: failed to find tool "clang": program not found
```

**MinGW gcc 로는 안 된다.** 플래그 문제가 아니다 — 하나의 gcc 는 하나의 타깃을
내고(`gcc -dumpmachine` 이 말해 준다), GCC 본가에는 WebAssembly 백엔드가 없어
`--target=` 이라는 플래그조차 모른다. **wasm 을 내는 C 백엔드는 실질적으로
LLVM 하나**라, wasm 을 굽는 도구는 이름이 뭐든 속이 clang 이다(LLVM 배포본,
emscripten, wasi-sdk, `zig cc`, Visual Studio 의 clang 구성요소). CLion 이 담아
둔 것은 `clangd` 와 `clang-tidy` 뿐이고 **둘 다 `.o` 를 못 낸다.**

깔지 않고 가는 길이 있다. 아래 "관문이 둘" 을 본다.

## 관문이 둘

데스크톱 관문(`cargo test --workspace`)은 커밋 전에 이 컴퓨터에서 돈다. 그리고
**액션이 그 둘을 다 돌린다**(`.github/workflows/ci.yml`) — 데스크톱 줄은 잡
하나로, wasm 줄은 다른 잡으로. 러너 이미지에는 clang 이 있고 이 컴퓨터에는
없으니, wasm 관문이 실제로 도는 자리는 지금 거기뿐이다. 액션은 위에 적힌 두
줄을 그대로 돌리고, 데스크톱 clippy 가 볼 수 없는 껍질(`bindings.rs`)도 그
타깃으로 한 번 본다.

**관문을 파일 둘로 나누지 않았다.** 나누면 한쪽만 고쳐지고, 나가는 것과 평소
보는 것이 다른 줄을 지나게 된다.

`wasm-bindgen-cli` 의 판은 거기서도 손으로 적지 않는다 — `Cargo.lock` 에서
읽는다. 적어 두면 크레이트를 올리는 날 그 파일만 낡는다.

**그 액션이 지워지거나 줄 하나가 빠지면 데스크톱 관문이 선다**
(`crates/web/tests/boundary.rs`). 돌릴 자리가 하나뿐인 검사는 그 자리가
없어지는 것으로 조용히 사라진다.

### 아무 clang 이나 되는 것이 아니다

**Visual Studio 의 clang 으로는 안 된다.** Installer 의 "C++ Clang compiler for
Windows" 를 깔면 `…\VC\Tools\Llvm\x64\bin\clang.exe` 가 생기는데, 그것은
**WebAssembly 백엔드 없이 구워진 clang** 이다. 깔고 돌려 보면 이렇게 선다:

```
error: unable to create target:
  'No available targets are compatible with triple "wasm32-unknown-unknown"'
```

`clang --print-targets` 가 미리 말해 준다 — 거기 `wasm32` 가 없으면 그 clang
으로는 못 굽는다. VS 가 담은 것은 AArch64·ARM·X86 뿐이다.

**깔기 전에 그 줄로 먼저 본다.** 깔고 나서야 알면 기가 단위를 헛 받는다.

그러니 받을 것은 **모든 타깃이 들어 있는 clang** 이다. LLVM 공식 배포본
(`winget install LLVM.LLVM`)이나, 설치 프로그램 없는 압축본 하나로 끝나는
`zig`(속의 clang 을 `zig cc` 로 부른다)·`wasi-sdk` 가 그런 것이다. zig 쪽은
`clang` 이라는 이름으로 그것을 부르는 배치 파일을 하나 두고
`CC_wasm32_unknown_unknown` 에 그 파일의 전체 경로를 넣는다 — `cc-rs` 는
`clang.exe` 를 찾으므로 PATH 에 배치 파일을 두는 것으로는 안 된다. 되는지는
아직 확인하지 않았다.

어느 쪽이든 깔았으면 그 타깃이 실제로 들어 있는지 `--print-targets` 로 본 다음
시험을 돌린다.

**브라우저 없이 된다.** 노드에서 돈다. 브라우저에서 돌리려면
`wasm_bindgen_test_configure!(run_in_browser)` 와 드라이버가 더 필요한데, 지금
보려는 것(SQLite 가 열리는가, 조회가 도는가, 봉투가 열리는가)은 브라우저와
노드가 같은 wasm 을 쓰므로 노드로 갈음한다. **화면이 붙는 날 다시 볼 자리다** —
파일을 고르고 받는 일은 브라우저에만 있다.

## 되는 것

- **SQLite.** 손댄 것이 없다. `rusqlite 0.40` 은 `bundled` 로 두어도 wasm 에서는
  `sqlite-wasm-rs` 를 저절로 거친다. 메모리 DB 가 열리고 표가 만들어진다.
- **조회 엔진.** 정렬·필터·값 목록이 돈다. 시계만 갈았다(아래).
- **비밀번호 봉투.** 봉하고, 열고, 틀린 비밀번호를 거절하는 것까지 돈다.
  Argon2 의 작업 메모리도 잡힌다 — wasm 의 메모리는 자라야 하는 것이라
  데스크톱에서 아무 일 없던 것이 여기서 막힐 수 있는데, 막히지 않았다.
- **양식에서 DB 까지 한 바퀴.** xlsx 바이트를 받아 SQLite 로 바꾸고, 조회하고,
  바이트로 꺼내는 데까지 돈다. 이 바탕이 웹에서 하려는 일 전부가 그 한 바퀴다.
  `calamine`·`csv`·`encoding_rs` 가 거기 들어 있다.

## 갈아야 했던 것

### 시계

`std::time::Instant::now()` 는 wasm 에 **없다.** 그런데 컴파일은 멀쩡히 되고
부르는 순간 패닉한다 — `time not implemented on this platform`. 조회가 걸린
시간을 재는 자리가 그것이었다.

이 부류가 가장 고약하다. **글자로 보는 검사도, 데스크톱 시험도 보지 못한다.**
웹에서 한 번 돌려 보고서야 드러났다. 이 글이 있는 까닭이고, 그 시험을 남겨 둔
까닭이다.

`web-time` 으로 갈았다. 데스크톱에서는 그대로 `std` 고, wasm 에서는 브라우저의
`performance.now()` 다.

### 난수

`getrandom` 은 wasm 에서 **아무 자리도 고르지 않고 컴파일을 세운다.** 조용히
0 을 내놓는 쪽보다 낫다 — 그랬다면 봉인 nonce 가 겹쳤을 것이다.

브라우저의 `crypto.getRandomValues` 를 쓰라고 적어 준다. 판이 둘 섞여 들어오고
(`rand` 가 0.3, 다른 상자가 0.4), 0.3 은 기능만으로 모자라 `--cfg` 까지 받는다.
자세한 것은 `crates/core/Cargo.toml` 과 `.cargo/config.toml` 의 주석에 있다.

## 아직 안 본 것

- **브라우저에서의 실제 동작.** 위에 적은 까닭으로 노드로 갈음했다.
- **큰 파일.** wasm 은 32비트라 주소 공간이 4GiB 로 묶인다. 데스크톱에서 열리던
  크기가 여기서 안 열리는 날이 올 수 있다. 재어 보지 않았다.
- **봉인 그릇(`seal`)의 서명 확인.** 조회와 봉투는 보았고 이쪽은 안 보았다.
