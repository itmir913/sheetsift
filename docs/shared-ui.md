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
  `print.css`, `seg.css` and `picker.css` (the column picker modal; without it the
  `<dialog>` renders unstyled). `base.css` — the page background, fonts and the
  app's own layout — is **not** shared; each product keeps its own.
- **vue** is a peer dependency. Two copies break reactivity.

### Choosing columns (`ColumnPicker.vue`, `applyColumns`)

`GridToolbar` holds the modal, so every screen that uses the toolbar gets the
button. The receiving app passes `allColumns` (from `useColumnLayout`) and handles
one emit:

```ts
// GridToolbar emits: applyColumns [{ order, hidden, dropped }]
layout.applyColumns({ order, hidden });          // remembers, per screen
const kept = dropColumnState(dropped, { sorts, filterText, filterValues });
```

- `order` holds **every** column, hidden ones included; `columns` is `order` minus
  `hidden`. Toggling a chip changes `hidden` only, so a column returns to its place.
- `dropped` is the keys hidden by this apply. The app owns the sort and filter state,
  so it must clear those columns' rules itself — `dropColumnState` (pure, in
  `lib/grid.ts`) does it for all three consumers. A rule on a column nobody can see
  would silently filter rows, and a leftover sort would print its raw key.
- The last column can never be hidden. `load()` and `applyColumns()` enforce it (a
  stored `hidden` that covers everything is cleared), not the modal.
- **The `columns` array you pass `GridTable` must be a subarray of the layout's own
  `columns`** — filter it if you must, never reorder it. The table counts from the
  front until it meets a column that is not in `pinnedKeys`, because a pinned column
  has to be a contiguous run at the left edge (`position: sticky`). Break the
  precondition and pinning only ever *shrinks* — a non-pinned column in front yields
  zero pinned columns, and no column outside `pinnedKeys` is ever pinned instead. It
  fails quietly, and the package cannot warn: a filtered array is also what a
  legitimate column chip produces, so the warning would fire on correct use. Hold the
  precondition with a test in the receiving app.
- `GridTable` takes `pinnedKeys` (the keys of the pinned columns; `layout.pinnedKeys`), not
  a count: a count means "the first N of the array you were given", which differs once
  the app filters that array further than the layout does. Likewise it draws the range
  inputs only when `filterRanges` is passed — an app that does not track ranges would
  otherwise get inputs that never reach its query.
- `move(from, to)` takes column **keys** (what `GridTable` emits), never indexes: an
  index means a position in whatever array the table was given, and a receiving app
  may filter that array further than the layout does. It moves `from` to `to`'s place
  in `order`, leaving hidden columns where they are; the modal never calls it, it edits its own draft with `reorder`.
- The dictionary needs the `picker.*` keys and `tools.columns`; `picker.count` is a
  plural keyed by the number of shown columns, with `total` as a parameter.

### The `cell` slot hands over the row object itself

A receiving app may need a per-row judgement the package knows nothing about —
goe-navi evaluates a condition on each row and emphasises one cell of it. The
only thing the slot gives it to identify the row is the `row` value, so:

**The three `cell` slots hand over the very object that came in the response's
`rows` array. The package does not copy a row, rebuild it, spread it or map it.**

A receiving app may therefore use the row object as a key (a `WeakMap`) and look
up whatever it computed for that row. The three slots are the table body
(`GridTable.vue`), the row details modal (`GridTable.vue`) and print
(`PrintView.vue`).

This was **not** already true when it was asked for. `DataGrid` held the rows in a
plain `ref`, so Vue wrapped every row in a deep reactive proxy and the slot handed
over the proxy, not the response's object — `toRaw(row)` was the response's object
but `row === res.rows[i]` was false. A receiving app that keyed its results on the
rows it returned from `queryRows` would have looked up a different object and found
nothing. `rows` and `printRows` are `shallowRef` now, which is enough because a row
is never edited in place: a query replaces the whole array.

Why it is pinned down rather than left as it happens to work: the lookup fails
**silently**. No error, no warning — the emphasis simply stops arriving and the
table still looks right. A row index was asked for first and measured as
unworkable: `GridTable` and `PrintView` are not given `offset`, print always
queries from 0, and body and print share one slot, so the receiver could not tell
which query a number belonged to. The object answers both questions by itself,
because the body's rows and print's rows are different objects.

`GridRowIdentity.test.ts` holds this. It mounts the three slots and asserts the
slot's `row` **is** the element of the array the source returned. Each of the three
legs was checked against the defect: putting either `ref` back fails them.

### Row details (`GridTable.vue`)

`GridTable` holds this modal, not `GridToolbar` or `DataGrid`: some screens build
the toolbar and the table themselves, and a modal mounted anywhere else would do
nothing there. Press a row and every column of that row is listed vertically.

- `detailColumns` (from `useColumnLayout`) is **required**, and it carries hidden
  columns too — hiding a column removes it from the table, not from the row.
- Values are drawn through the **same `cell` slot** as the table, so a receiving
  app's chips and link buttons look the same in both places.
- Styles live in `grid.css` with the `rd-*` prefix. No new stylesheet, so no
  receiving app has to add an `@import` it could forget.
- Colours still come from the receiving app: the pair row carries `data-cvars`
  and the variables, so the whole row is tinted, and the app's own rule paints it
  (goe-navi's `header-tint.css`).
- The dictionary needs the six `detail.*` keys; `detail.position` takes `n` and
  `total`. `total` is the length of `rows` — the list the modal walks, which is
  what the table itself is showing.
- The body is not focusable. Row keys are handled by the `<dialog>`, so giving the
  body a tab stop only drew a focus ring across the whole list.

### The funnel popover (`ColumnFilter.vue`, `placePop`)

The popover is **fixed to the viewport**, and `GridTable` recomputes its place
whenever anything scrolls (capture phase, so the grid's own horizontal scroller
counts) or the window is resized. Placing it inside the scrolling box made it
drift away from its header on the first scroll. Closing it on scroll was
rejected: the person loses the selection they were in the middle of making.

**When the header itself scrolls out of view the popover sticks at the edge of
the visible area** rather than closing — same reason. `placePop` clamps the box
into the visible rect, sideways as well as vertically, and it is a pure function
in `lib/grid.ts` so the rule is pinned by tests rather than by looking at a
screen.

Height is **not one number for both kinds of column**: a number column carries
the range block (slider plus two inputs) above the value list, and giving it the
text column's height left two value rows on the owner's screen. `POP_MAX_H`
holds both numbers in one place, `POP_MIN_H` is the height asked for even where
there is no room (the popover scrolls itself past that), and the value list keeps
`POP_MIN_ROWS` rows through a `min-height` in `grid.css` that cites the same
reason.

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
