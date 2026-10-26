# Using sheetsift-core

For applications that embed the core rather than run the viewer. `core` knows
nothing about tauri and nothing about the file system: it takes bytes and returns
bytes — that is what lets desktop and web (wasm) share the same code. Writing
files is the caller's job.

## Crates

| Crate | Responsibility |
|---|---|
| `crates/core` | Sheet conversion (`sheet`), structure detection (`infer`), the stored format (`sift`), the query engine (`grid`), the password envelope (`vault`), the signed container (`seal`) |
| `crates/aead` | One layer of AES-256-GCM. Kept separate so it is compiled with optimizations even in debug builds |
| `crates/shell` | The platform-neutral kernel (`Viewer`). Binds the converted connection, column definitions and requests in one place |
| `crates/wasm` | The shell that attaches the core to the browser. `wasm_bindgen` exists only in this crate |
| `crates/desk` | The commands the window calls, plus **file reading and writing**. Ordinary Rust, so the full gate covers it |
| `crates/tauri` | The tauri app that attaches those commands to a window. Outside the workspace, so `ci:tauri` owns it |
| `web/` | The Vite + Vue viewer. Table, sorting, filtering and printing |

## No human-readable text, no keys

Errors return an `ErrorCode` and substitution values only; the wording is
attached by the application. The serialized names (`DATA_SEAL_INVALID`) are the
keys that interlock with the application's text dictionary, so renaming one
forces the application to follow.

**No keys live here.** The sealing symmetric key, the signing public key and the
magic are injected by the calling application. This repository is public, so any
key written here is a key everyone knows.

## Unused layers are not compiled in

`sift` and `infer` are **feature-gated**. Everything is enabled by default; an
application that wants less opts out.

```toml
sheetsift-core = { git = "…", rev = "…", default-features = false }
```

The point is not smaller binaries. **Error codes live in a single enum**, so
adding one code here forces every consuming application — including the ones
that do not use the layer — to write one more line of human-readable text that
can never appear on screen. Codes belonging to a disabled layer are absent from
the enum entirely.

Whether a disabled combination still compiles is **not** something
`cargo test --workspace` can tell you: the tests run with default features. One
path that would break such a combination — always-compiled code calling into a
gated layer — is blocked textually by `tests/features.rs`. The real check is the
build of the application that disables the layer.

## What the data decides

`sheet` knows no sheet name, no table name and no column name. It receives a
`TablePlan`; the caller's definition file composes it. **Code never branches on
the values in the data.** The single statement that extracts data to bytes is
`sheet::serialize` — an application that ships no such feature has to verify in
**its own** tests that it never calls it; that cannot be blocked from here.

## `.sift`

The on-disk format for an already-converted sheet. The payload is exactly the
SQLite database `sheet` produced, plus one record describing **where the data
came from** (name, size, hash) and the column information. A copy goes stale
when the original changes, so reopening it must be able to tell, right there,
whether it came from the same source.

**The password envelope is the default.** A sheet may contain people's names, and
if the choice is offered at save time nobody takes it. A plaintext path exists
separately and the caller has to ask for it deliberately.

The header layout and key derivation of `seal` and `vault` are pinned by
fixed-value tests. If one of those tests stops passing, it means already
distributed files will not open — do not edit the values, raise the container
version.

## The gate

```bash
npm run ci
```

That one command runs everything. It is split into pieces internally, and **the
list of pieces exists only in `package.json`** — copying it elsewhere would make
two copies, and the day a piece is added only one of them gets updated. The
GitHub Actions workflow likewise just calls a piece per job instead of restating
the checks. **No new commands get invented** — a command that lives outside the
gate ends up running in some places and not others.

`ci:gate` also carries a gate that **reads the source as text**
(`crates/core/tests/secrets.rs`). It checks that decrypted plaintext and keys are
held in self-zeroizing containers, that they are wiped on drop and redacted when
printed, and that the crypto crate's zeroization feature survives compilation.
A running test cannot see any of this — if the wiping line goes missing the
results are still correct and the build is still green.

**Failing to find what it guards is a failure, not a pass.** And a gate that
quietly stops checking anything is just as invisible, so the same file feeds it
deliberately broken source and asserts that it stops.

`ci:wasm` checks that the code works on the web by **running it there once**.
Compiling and working are different things — some functions panic the moment
they are called because there is no operating system underneath. The
`tests/wasm.rs` files begin with `#![cfg(target_arch = "wasm32")]`, so on a
desktop `cargo test --workspace` sees them as empty files. This piece needs clang
to compile SQLite's C sources, so in practice the Actions runner is the only
place it really runs.

Keep logic in ordinary Rust where `ci:gate` can reach it, and leave only the
web-only surface to `ci:wasm`.

## Further reading

- `docs/wasm.md` — what is and is not possible in the browser, and how to install
  what the wasm build needs
- `docs/web.md` — why the compilation step exists
- `docs/infer.md` — how far structure detection goes
- `docs/desktop.md` — what baking the desktop build requires
