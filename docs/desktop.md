# The window build

The same UI as the web, attached to a desktop window. This document records how
it is put together and **why each piece sits where it does.**

## Why there are two crates

| Crate | What lives there |
|---|---|
| `crates/desk` | What the commands the window calls actually do. File reading and writing. **Ordinary Rust** |
| `crates/tauri` | The layer that attaches those commands to a window. The only place that takes `tauri` |

`crates/tauri` is **outside the workspace** (`exclude` in the root
`Cargo.toml`). `tauri` pulls in the operating system's webview, so keeping it in
the workspace would make `ci:gate` on a Linux runner require webkit's
development files — that piece does not run the desktop at all, yet would demand
them, and one piece would break because of another's circumstances.

So the piece was separated. `ci:tauri` owns that crate and runs on Windows.
**The kernel sits in `crates/desk` for the same reason** — everything there is
judged by `cargo test --workspace`.

## How to start the window

```bash
npm run dev:tauri
```

That command moves into `crates/tauri` and calls tauri's CLI, and
`beforeDevCommand` starts the UI with `--mode tauri`. That mode picks the window
variant of the two boundaries (below).

**The port is pinned** (`--strictPort`). `devUrl` in `tauri.conf.json` is pinned
to 5173, but Vite silently moves to another port when that one is taken. The
window still loads 5173, so **an entirely different app appears in the window and
no error occurs anywhere** — which is what actually happened. Pinning makes it
stop right there when the port is taken.

**That command's working directory is `crates`** — not the directory holding
`tauri.conf.json`. tauri treats that file's **parent** as the app root. Hence
`npm --prefix ../web`, while `frontendDist` in the same file is resolved relative
to the file itself and reads `../../web/dist`. That the two differ is not our
doing.

## Two boundaries, one UI

The UI imports under the name `sheetsift-book` and **does not know which side it
is running on.**

| Running in | Boundary | What it imports |
|---|---|---|
| Browser | `web/src/lib/book.wasm.ts` | The wasm shell |
| Window | `web/src/lib/book.tauri.ts` | The tauri commands |

The choice is made by Vite's mode (`web/vite.config.ts`). Not an environment
variable, because **modes are not set differently per operating system** — one
command works identically on Windows and Linux.

Whether the two **export the same thing** is checked textually by
`crates/shell/tests/screen.rs`. If a command ends up existing on only one side,
the UI breaks in that variant only, while type checking passes because it looks
at whichever side it was pointed at.

## Exactly one thing differs: where the file is opened

| Window | Browser |
|---|---|
| `open_workbook { path }` | — |
| `open_bytes { bytes, name }` | `new Book(bytes, name)` |
| `list_screens { request }` | `listScreens(request)` |
| `query_rows { request }` | `queryRows(request)` |
| `column_values { request }` | `columnValues(request)` |
| `limits` | `limits()` |

The path-opening side exists because of **size**. Receiving bytes means the UI
loads the whole file once, which leaves exactly the constraint the desktop is
meant to remove (wasm's 4GiB) in place. The file dialog passes only a path.

Drag-and-drop is bytes on both sides. Turning that into a path in the window
would mean subscribing to the window's own events, which adds code that knows
which side the UI is running on. That is why `dragDropEnabled` is set to false —
it lets the webview handle the HTML drop as-is.

## Errors

What the window returns is either a core error or a shell error, and **their
serialized shape is identical** (`code`, `params`, `detail`). The UI does not
distinguish the two and attaches wording from a single dictionary.

The reason a "could not read the file" case was not added to the core enum is
that it would force the web shell — which will never read a file — to write that
line of wording too. Whether the two sides duplicate a code is checked by
`crates/desk/tests/desk.rs`, comparing the two files.

**Full paths are never put in substitution values.** Those values are shown on
screen and get captured and shared, and they contain the user's name.

## No test starts a window

Binding tauri's mock runner (`tauri::test`) into a test makes **the test
executable abort at load time on Windows** (`STATUS_ENTRYPOINT_NOT_FOUND`). Not
a single test runs, and the reason has nothing to do with the code. Putting a
test that cannot run into the gate makes that piece permanently red.

Instead, **the wiring is read as text** (`crates/tauri/tests/boundary.rs`) — is
every declared command attached, does every name the UI calls actually exist.
Both are the kind of mismatch that raises no error and only breaks in the window.
What the commands do is exercised by the tests in `crates/desk`.

## Baking

```bash
npm run build:tauri
```

The workflow's `bake` job does the same thing after the gate passes and keeps the
artifacts. **Baking is not a check** — combining them hides which side broke, and
the same command cannot be carried to two operating systems.

Windows is currently the only place it is baked. Whether it bakes on a Mac is not
asked.

## Not there yet

- **`.sift` export.** The means of writing exists (`file::write` in
  `crates/desk`, writing under a temporary name and swapping after completion)
  and the format exists in `core::sift`, but the kernel has no path that produces
  the sealed bytes, so there is no command yet.
- **Window menu and title.** The window does not show the name of the open file.
- **Measurements for large files.** How far the path-opening side goes has not
  been measured.
