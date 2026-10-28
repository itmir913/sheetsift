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

`publish.yml`'s `bake` job calls **that same script** after the gate passes, and
keeps what came out. It differs in its arguments: the script takes the bundles
from `tauri.conf.json`, while the job names them, and names the target too
(below). The tool's version is
pinned in `package.json` so the two cannot drift — a version written into the
workflow as well would be a second copy, and the local entrance would stay on
whatever it said before.

**Baking is not a check** — combining them hides which side broke, and the same
command cannot be carried to two operating systems.

Two operating systems, as two legs of one job: Windows and macOS. They are legs
rather than jobs because a second job would have to be named in
`crates/shell/tests/gate.rs` to be kept out of the gate, and a leg does not.

What each leg produces, and why:

| Leg | Command | Out |
|---|---|---|
| Windows | `--bundles nsis` | the installer, and the bare executable left in `target/release` on the way past — the no-install edition |
| macOS | `--target universal-apple-darwin --bundles dmg` | one dmg for Apple silicon and Intel |

`tauri.conf.json` says `targets: "all"`, which is what you get baking on your own
machine. The flag on the command line overrides it, so **what goes out is named,
not whatever `all` happens to mean** on that runner.

The Windows pieces are shipped as zips, each named with the version, and the
executable inside the no-install zip is renamed to `sheetsift.exe`. **The bare
executable is called `sheetsift-tauri.exe` in `target/release`** — Cargo names it
after the crate, and `productName` reaches only the bundle.

**The `.app` is never zipped.** It is a directory holding symbolic links, and an
archive flattens them; the app then does not start, and nothing in the workflow
would have said so. The dmg carries it whole.

## Releasing

```bash
# Raise the version in crates/tauri/tauri.conf.json, commit it, let the gate
# pass, and then tag that commit:
git tag 0.1.0
git push origin 0.1.0

# Nothing has happened yet. Release it:
gh workflow run publish --ref 0.1.0
```

**Releasing takes two things, and neither does anything alone.** The workflow has
to be **run by hand**, and it has to be run **on a tag**.

- **A pushed tag starts nothing.** `publish.yml` has no `push` trigger at all,
  so a tag can be pushed to mark a revision without publishing it.
  `crates/shell/tests/gate.rs` stops the gate if another trigger appears there.
- **A branch is refused, not skipped.** The `tag-only` job fails the run on
  anything that is not a tag, and the gate waits for it, so nothing downstream
  starts. It fails rather than skipping: a run that quietly did nothing is a run
  you believe published something.

The two fail in opposite directions, which is why both are required. If a tag
deployed by itself, a tag could never mean "this is the revision" without also
meaning "and it is now public". If a by-hand run deployed from anywhere, the
published thing would have no name.

Tags carry no `v`; they are plain versions. The gate is the same file `ci.yml`
calls (`gate.yml`); after it, `pages` (the screen) and `bake` (the app) run side
by side, and then `ship` attaches what came out to a **draft** release. Draft, because a green bake does not say the app
opens — someone looks before it is public.

`ship` stops if the tag and the version in `tauri.conf.json` are not the same
string. The version is what every file name is built out of, so a tag that
disagrees would attach one version's files to another version's release, and
nothing else would ever say so.

**One setting lives outside this repository, and it stopped the first release.**
The `github-pages` environment carries a "deployment branches and tags" rule.
It held `main` alone, so on a tag the `pages` job failed **before a single step
ran** — no step was red, no log said why, and nothing in the code could have
said so. The rule now also holds the tag pattern `*.*.*`:

```bash
gh api repos/<owner>/<repo>/environments/github-pages/deployment-branch-policies \
  --jq '.branch_policies[] | "\(.type)\t\(.name)"'
```

That pattern is the environment's own glob syntax, not the workflow's — `*`
only, no `+`. The `main` entry is left in place; nothing deploys from a branch
any more, so it is unused rather than wrong.

**There is no way to exercise the bake without releasing.** `ci:tauri` compiles
the crate on every push, but nothing bundles it until a tag is released, so a
break in the bundling config — the icons, `tauri.conf.json`, a bundler argument
— first appears in the run that publishes. That is the price of nothing going
out by accident. After changing any of those, expect to spend a tag on finding
out; the release is a draft, so a bad one is deleted rather than seen.

## The Mac build is unsigned

There is no Apple Developer account, so the app carries no signature and no
notarization. macOS tells whoever downloads it that the app is **damaged** and
refuses to open it. That is the missing signature, not a broken download, and one
command clears it:

```bash
xattr -cr /Applications/sheetsift.app
```

That line lives in `.github/release-notes.md` as well as here — the person who
needs it is reading the release page, not this repository.

**Nobody has opened the Mac build.** The author has no Mac. A green leg says it
compiled and bundled; it does not say it starts, and that check is still waiting
for a person with the hardware.

## Not there yet

- **`.sift` export.** The means of writing exists (`file::write` in
  `crates/desk`, writing under a temporary name and swapping after completion)
  and the format exists in `core::sift`, but the kernel has no path that produces
  the sealed bytes, so there is no command yet.
- **Window menu and title.** The window does not show the name of the open file.
- **Measurements for large files.** How far the path-opening side goes has not
  been measured.
