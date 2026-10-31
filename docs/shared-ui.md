# The shared screen package

`packages/sheetsift-ui` is the table viewer's screen — the grid, the toolbar, the
value picker, print, the tooltip, the theme toggle and their CSS. This app's
`web/` uses it as an npm workspace, and goe-navi takes the same source through a
git dependency on this repository.

Before this package there were two copies: goe-navi's original and the one
`03643ca` brought into `web/`. Both kept changing, and a fix made on one side
never reached the other. Now there is one source.

**This is a record, not a rule.** What the package may and may not hold is stated
by `crates/shell/tests/shared_ui.rs`.

## What the package holds and what it is handed

The package holds no words, no colour values, no storage name and no data door.
The app hands those over once, before mounting, with `installSharedUi`:

```ts
import { installSharedUi } from "sheetsift-ui";

installSharedUi({
  storagePrefix: "sheetsift",          // keys become `sheetsift.theme`, `sheetsift.cols.…`
  appName: "SheetSift",                // the print header
  messages: { t, tn, formatNumber, formatDateTime },
  source: { limits, queryRows, columnValues, printRows, printEnabled },
  errors: { recognize, format },
  // shortcuts: { save: true, print: true, source: true, contextMenu: true },
});
```

| What | How |
|---|---|
| Words | The package calls `t(key)` with keys only. `SharedUiKey` lists every key it uses; the app's dictionary proves it has them all with `ui satisfies Record<SharedUiKey, string>` (`web/src/i18n/messages.ko.ts`) |
| Colours | `styles/tokens.css` names the tokens and carries the defaults. A product overrides the same names in its own CSS. No other file in the package writes a colour value, print included (`--c-print-*`) |
| Storage prefix | `storagePrefix`. Keys keep the shape `<prefix>.<name>`, so settings already saved under the old copy are read as before |
| Data | `source` — the same request shapes the core takes. This app wraps its open `Book` (`web/src/lib/workbook.ts`). `printRows` returns the rows to print in one call and the package prints exactly what it returns; the receiving app owns any cap on that count. `printEnabled` only decides whether the print button shows |
| Errors | `errors.recognize` pulls a code out of a thrown value (or returns `undefined` for anything else); `errors.format` turns it into words at render time, so a switch of language follows |
| Behaviour | Options, off by default. `shortcuts` blocks the webview's own Ctrl+S / Ctrl+P / Ctrl+U and the context menu. This app does not pass it |

Calling a package function before `installSharedUi` throws. A silent default would
let a product run on the other product's storage name.

### What the receiving app must also do

- **Tailwind.** The components still use Tailwind utilities (`text-fg-muted`,
  `bg-accent`, …). The app's Tailwind must scan the package (`@source`), and its
  `@theme inline` must map the colour utilities onto the `--c-*` tokens. This
  app does both in `web/src/styles/base.css`.
- **CSS.** Import `sheetsift-ui/styles/tokens.css`, `grid.css`, `tooltip.css`,
  `print.css` and `seg.css`. `base.css` — the page background, fonts and the
  app's own layout — is **not** shared; each product keeps its own.
- **vue** is a peer dependency. Two copies break reactivity.

## How the package is taken

Exports are the source files (`.vue`, `.ts`, `.css`); the receiver's vite
compiles them.

| Import | Points at |
|---|---|
| `sheetsift-ui` | `src/index.ts` — `installSharedUi` and the injected types |
| `sheetsift-ui/components/<Name>.vue` | `src/components/` |
| `sheetsift-ui/composables/<name>` | `src/composables/<name>.ts` |
| `sheetsift-ui/lib/<name>` | `src/lib/<name>.ts` |
| `sheetsift-ui/styles/<name>.css` | `src/styles/` |

A git dependency installs the **repository root**, so the root `package.json`
carries the same `exports` (with the `packages/sheetsift-ui/` prefix), a `files`
list that ships only the package, and the `vue` peer. The test above holds the two
export lists together.

Checked before this was set up, by installing this repository into an empty
project through `git+file://` (npm 12, `allow-git=root`):

- nothing ran at install time — the root has no `prepare`, and `workspaces` is
  not acted on for a dependency;
- what arrived was the package's `package.json` and `src`, plus `README.md`,
  `LICENSE` and the root `package.json` — no `crates`, no `web`;
- it is installed under the name the dependency is declared with
  (`"sheetsift-ui": "github:…#<tag>"` lands in `node_modules/sheetsift-ui`); the
  lock file records the root's own name (`sheetsift`).

## Choosing the source of each file

The two copies forked at susi `eae037b` — the last susi commit before `03643ca`
took the screen. `03643ca` differs from `eae037b` only in the style of comments
and in the three changes that commit names (value search moved to the core, the
blank token and list limit read from `limits()`, dragging to select allowed);
those count as this side's changes.

Each file was compared three ways: base = susi `eae037b`, S = this repository's
`web/src` at `7c47ade`, N = goe-navi's side (susi `eae037b..c260610^`, then
navi-core from `82a0e96`, which is the move of susi `c260610` and not a change of
its own).

- Changes made only on S are taken as they are.
- On N the screen files changed in few places. Each is a row below.
- Comment style follows this repository (`CLAUDE.md`).

| File | Item | Evidence | Taken | Why |
|---|---|---|---|---|
| `lib/grid.ts`, `ColumnFilter.vue`, `PrintView.vue` | The blank token became a sentinel (`\u{FFFF}`) shown through one function `valueText` | N: susi `65f4137`. S: `00c15f0`, `03643ca` | S's mechanism, N's single function | Both sides fixed the same bug. S already reads the token from the core (`limits().blankToken`) and the words from the dictionary (`filter.blank`); N hard-codes both. N's point — compare against the token in one place, not in each screen — is taken: `valueText` in `lib/grid.ts`, used by the value list and print |
| `ColumnFilter.vue` | Narrowing by the shown text, so typing the blank's label finds the blank entry | N: susi `65f4137` | Not taken | S hands the search to the core (`needle`, `03643ca`), which never sees the label. Doing it here would need the core to know the dictionary's words. Left as a known gap; no test on either side |
| `lib/colWidth.ts` | `codePointAt(0)!` → `codePointAt(0) ?? 0` | N: navi-core `2f97f9b` | N | Same meaning; goe-navi's wording check reads `!` as punctuation and would stop on the shared source. No test |
| `components/GridTable.vue` | Hard-coded tooltip wording (`순서 바꾸기` → `순서 변경`) | N: navi-core `2f97f9b` | Not applicable | S takes these words from the dictionary (`grid.head.tip`); the wording is each product's |
| `lib/shortcuts.ts` | Blocking the webview's shortcuts | base = N; absent on S | N, as an option, off | Generic for a webview product, but this app made no such decision. `installShortcutGuard` takes which keys to block; `installSharedUi` calls it only when `shortcuts` is given. The Rust side is `crates/webview` |
| `lib/app.ts` | `brand.json` through `virtual:brand` | N: navi-core `82a0e96` | Left on N | Product identity. The package takes `storagePrefix` and `appName` instead |
| `i18n/codes.ts` | `DATA_BRAND_MISMATCH`, `SHEET_TOO_WIDE`, `DATA_DIR_OPEN_FAILED`, removing `APP_VERSION_MISMATCH` | N: susi `e34d5d3`, `ffcd815`, navi-core `914492d` | Left on each side | Error codes are each app's own list; the package holds none |
| `styles/base.css` | `@source ".."` | N: navi-core `82a0e96` | Left on N | `base.css` is not shared. The need it answers — Tailwind scanning the screen's source — is now the receiving app's line (see above) |
| `styles/print.css` | Literal print colours | base = N = S | Moved to tokens | The package writes no colour values; `--c-print-*` carry the same values and `.dark` does not override them |
| `styles/base.css` → `styles/seg.css` | The segmented control (`.app-seg`) | S | Moved into the package | `ThemeToggle` is shared and uses it; this app's language toggle keeps using the same classes |

Files that exist only on S and came along because the shared parts use them:
`composables/useGrouping.ts` (thousands separator, chosen by the person),
`composables/useFault.ts` (now handed the error codec), `styles/tooltip.css`.
`lib/paint.ts` stays in `web/` — only this app's start-up uses it.

Left out on purpose (goe-navi's own): `lib/dialog.ts`, `files.ts`, `ipc.ts`,
`vault.ts`, `update.ts`, the admission screens and their CSS.

No goe-navi change in this range came with a test on the screen side, so none was
carried over.
