# What is and is not possible on the web

This foundation targets the web (GitHub Pages) first. That is why `core` knows
nothing about tauri and nothing about the file system, and exchanges only bytes.
This document records what happened when that design was actually run once.

**This is a record, not a rule.** It may differ once crate versions move. What is
possible is stated by the tests, not by this document — `crates/core/tests/wasm.rs`.

```bash
cargo test -p sheetsift-core --target wasm32-unknown-unknown
```

## A separate runner is required

wasm cannot run on its own; something — node or a browser — has to load what was
compiled. `.cargo/config.toml` names that tool; install it if it is missing.

```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --locked
```

The version must be **identical** to the `wasm-bindgen` crate. On a mismatch the
tool says so and stops — nothing fails silently.

### One more C compiler is required

Those two are not enough. `rusqlite` is taken with `bundled`, so **SQLite's C
sources are compiled along with it.** On the desktop Visual Studio's `cl.exe`
does that job and never surfaces, but on wasm `cc-rs` looks for **`clang` only**,
because MSVC cannot produce that target. Without it, this is where it stops:

```
Compiling sqlite-wasm-rs
warning: Compiler family detection failed due to error:
  ToolNotFound: failed to find tool "clang": program not found
```

**MinGW gcc will not do.** This is not a matter of flags — one gcc produces one
target (`gcc -dumpmachine` states it), and upstream GCC has no WebAssembly
backend, so it does not even recognize a `--target=` flag. **The only C backend
that effectively produces wasm is LLVM**, so whatever a wasm-capable C toolchain
is called, clang is what is inside it (the LLVM distribution, emscripten,
wasi-sdk, `zig cc`, Visual Studio's clang component). What CLion bundles is
`clangd` and `clang-tidy`, and **neither of them produces a `.o`.**

Without the install, `ci:wasm` stops right there. See "The gate is one command"
below.

## The gate is one command

What a person types is `npm run ci`, nothing else. One piece inside it
(`ci:wasm`) runs the two lines above plus clippy on the shell crate — desktop
clippy cannot see `bindings.rs`. The pieces live in `package.json`, and the
workflow just calls a piece per job instead of restating the checks.

The runner image has clang. If the working environment does not, `npm run ci`
stops at that piece — the installation above resolves it. **It is deliberately
not built to be skipped** — green after a skip reads as having run something that
never ran.

The `wasm-bindgen-cli` version is not written by hand there either — it is read
out of `Cargo.lock`. Writing it down would leave that one file stale the day the
crate moves.

**If that workflow is deleted or one of the pieces goes missing, `ci:gate` stops**
(`crates/shell/tests/gate.rs`). A check that runs in exactly one place dies
silently when that place does.

### Not just any clang will do

**Visual Studio's clang will not do.** Installing the Installer's "C++ Clang
compiler for Windows" produces `…\VC\Tools\Llvm\x64\bin\clang.exe`, and that is
**a clang compiled without the WebAssembly backend.** Install it, run it, and
this is where it stops:

```
error: unable to create target:
  'No available targets are compatible with triple "wasm32-unknown-unknown"'
```

`clang --print-targets` states this in advance — if `wasm32` is absent there,
that clang cannot compile for it. What VS bundles is AArch64, ARM and X86 only.

**Check with that line before installing.** Finding out afterwards means
gigabytes downloaded for nothing.

So what you want is **a clang that includes every target**: the official LLVM
distribution (`winget install LLVM.LLVM`), or one of the single-archive,
no-installer options — `zig` (which calls its internal clang as `zig cc`) or
`wasi-sdk`. For the zig route, place one batch file that invokes it under the
name `clang` and put that file's full path in `CC_wasm32_unknown_unknown` —
`cc-rs` looks for `clang.exe`, so putting a batch file on PATH is not enough.
Whether it works has not been verified yet.

Either way, once installed, confirm with `--print-targets` that the target is
really there before running the tests.

**No browser is needed.** It works under node. Running in a browser would
additionally need `wasm_bindgen_test_configure!(run_in_browser)` and a driver,
but what is being verified right now (does SQLite open, does querying work, does
the envelope open) uses the same wasm in both, so node stands in. **It is worth
revisiting the day the UI is attached** — selecting and receiving a file exists
only in the browser.

## What works

- **SQLite.** Nothing was done to it. `rusqlite 0.40`, even configured with
  `bundled`, routes through `sqlite-wasm-rs` automatically on wasm. An in-memory
  DB opens and tables are created.
- **The query engine.** Sorting, filtering and value lists work. Only the clock
  was replaced (below).
- **The password envelope.** Sealing, opening, and rejecting a different
  password all work. Argon2's working memory is obtained as well — wasm memory
  has to grow, so something that was fine on the desktop can be blocked here,
  and it was not.
- **The signed container (`seal`).** What was sealed opens, and both a container
  signed with another key and one with a single byte corrupted are rejected.
  A desktop pass cannot state anything about this, because `curve25519-dalek`
  picks a different implementation per platform (32-bit vs 64-bit) and makes
  that choice silently at compile time.
- **One full round trip from sheet to DB.** Taking xlsx bytes, converting to
  SQLite, querying, and extracting back to bytes all work. That round trip is
  the entirety of what this foundation does on the web. `calamine`, `csv` and
  `encoding_rs` are part of it.

## What had to be replaced

### The clock

`std::time::Instant::now()` is **absent** on wasm. Yet it compiles fine and
panics the moment it is called — `time not implemented on this platform`. The
place that measured query duration was exactly that.

This category is the hardest. **Neither a source-reading check nor a desktop test
finds it.** It surfaced only after running once on the web. That is why this
document exists, and why that test is kept.

It was replaced with `web-time`: plain `std` on the desktop, the browser's
`performance.now()` on wasm.

### Randomness

On wasm, `getrandom` **picks no source and stops the compilation.** That is
better than silently producing zeros — had it done so, sealing nonces would have
repeated.

It is told to use the browser's `crypto.getRandomValues`. Two versions are mixed
in (`rand` at 0.3, another crate at 0.4), and 0.3 needs more than a feature — it
takes a `--cfg` too. Details are in the comments in `crates/core/Cargo.toml` and
`.cargo/config.toml`.

## Not verified yet

- **Actual behavior in a browser.** Node stands in, for the reason above.

## Large files — what gives out first

**Memory.** And when that limit is reached, no error is returned: **the module
aborts.**

Measured once under node (a 5-column csv with long text in a remarks column).
The numbers can differ with environment and versions, so **read them as a trend,
not as a configuration.**

| csv | wasm memory | result |
|---|---|---|
| 4MiB | 21MiB | opens, queries |
| 37MiB | 181MiB | opens, queries |
| 73MiB | 360MiB | opens, queries |
| 148MiB | 732MiB | opens, queries |
| 296MiB | 1466MiB | opens, queries |
| ~600MiB | — | **abort** (`RuntimeError: unreachable`) |

### 1. Memory grows to roughly 5× the original

The parsed strings, the row structure and SQLite's table are all resident at
once. So the point where the 4GiB address space is reached is **a csv of roughly
800MiB to 1GiB** — not a 4GiB file.

### 2. An abort is not an error

What comes back when an allocation fails is not an `Err` but `unreachable`. The
container's `ErrorCode` never arrives and **the module itself stops.** On screen
it presents as a response that never arrives — no message, no code.

Which stage aborted is not stated by the abort. Distinguishing parsing from
conversion would require measuring per stage.

### 3. Memory is not given back

wasm memory **does not shrink.** Closing the sheet keeps the peak of that moment,
so opening files in sequence hits the limit on **the accumulated total** rather
than on any single file. The rising starting memory per row in the table above is
that effect.

### What the desktop resolves

The path-opening side (`crates/desk`) is 64-bit, so there is no address space
constraint, and it does not route the bytes through the UI, so there is only one
copy loaded. **None of the three items above applies there.** Its own limits
have not been measured.

### What was deliberately not built

**No line pre-blocks a file by size.** What counts as too large is known by the
caller, not by this layer, and pinning a number here would block files that are
perfectly openable in a different environment. That said, **an abort that never
arrives as an error** is the shape this repository rejects, so the day blocking
is decided on, the container gains one error code.
