# Compiling and running the web shell

`crates/shell` is the platform-neutral kernel, `crates/wasm` is the shell that
attaches it to the browser, and `web/` is the UI that uses that shell. Built with
Vite + Vue, with goe-navi's viewer ported over as-is.

For a while `web/` held a one-page harness placed there to check that the
boundary worked. It disappeared when the UI arrived — what the harness guarded
(which command is called in what shape) is now the job of `src/lib/book.wasm.ts`
alone.

Rendering a table is done by `GridTable` and `GridToolbar`, those two only.
Building a table per screen makes them diverge — column search present on one,
absent on another. That is what actually happened in goe-navi.

**This is a record, not a rule.** What is and is not possible is stated by the
tests, not by this document.

## Compiling

```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --locked

npm run pkg
```

The `wasm-bindgen-cli` version must be **identical** to the `wasm-bindgen` crate.
On a mismatch the tool says so and stops — nothing fails silently.

The compiled output (`web/pkg/`) is not kept in the repository. Keeping it would
let the source and the build diverge, with no way to tell which one is canonical.

## Running

`file://` will not work. The module JS and the wasm **have to come from the same
origin.**

The compiled wasm has to be at `web/pkg`. That location is known to the
`sheetsift-wasm` alias in `web/vite.config.ts`.

```bash
npm run dev:web
```

**Compiling wasm needs clang** (`docs/wasm.md`). Where it exists, the command
above produces `web/pkg` and lets you work with the UI locally — rerun
`npm run pkg` only when the kernel changes; Vite picks up UI files immediately.

Where clang is absent, change the UI, push, and **work with the deployed build.**
Cumbersome, but better than guessing at changes you cannot compile.

## Where it is deployed

The UI is deployed to GitHub Pages — the `pages` job in `ci.yml` compiles and
deploys it. **It runs only after every piece of the gate has passed** (`needs`).
Dropping one from that line would deploy a revision whose tests fail, and the
deployment would still succeed, so whether one is missing is read textually by
`ci:gate` (`crates/shell/tests/gate.rs`).

Since the compiled output (`web/pkg`) is not in the repository, that job produces
it. Vite then produces `web/dist`, and **that folder is what gets deployed** —
not `web/` itself, which contains the sources and `node_modules`.

There is a line that checks, before deploying, that the kernel is really there.
It also checks that the wasm made it into the bundle — if it did not, **the UI
renders and nothing at all happens, and the deployment looks successful.**

`npm run build` runs `vue-tsc` first. That is the only place where the UI is
compared against the shapes written in `web/src/pkg.d.ts`, so a mismatch stops
the deployment.

## Why compiling is not in the gate

Compiling is not a check. And **no new command gets invented** — so the compile
line exists only in this document, and what the gate includes is the UI's type
check (`ci:web`). Whether the bundle is actually produced is checked by the
deploying job; if it stops there, nothing is deployed.

Instead, **the gate's reach was widened as far as possible.**

The kernel in `crates/shell` (`Viewer`) contains not one line of `wasm_bindgen`.
Being ordinary Rust, `cargo test --workspace` covers it on the desktop as-is —
opening a sheet, selecting a worksheet, applying sorting and filtering, and what
happens when no header is found.

The layer that talks to the browser (`crates/wasm`) is a shell that takes strings
and returns strings, and is checked separately on its own. **The split exists so
that dependency is isolated in one crate** — the desktop shell does not drag in
`wasm-bindgen`.

```bash
cargo test -p sheetsift-wasm --target wasm32-unknown-unknown
```

**Any logic that leaks in here leaves the gate's reach by exactly that much.**
Keeping it a thin film is the point.

And `tests/wasm.rs` is **an entirely empty file** on the desktop. Change the
shape of a public function without touching it and `ci:gate` is still green. That
happened once — `npm run ci` runs `ci:wasm` along with the rest.

## What running it in a browser revealed

Two things would not have been known without running it.

**`hidden` does not apply.** The browser's default rule is
`[hidden] { display: none }`, but giving the toolbar `display: flex` wins over
it. A toolbar believed to be hidden was **sitting there in plain view**, holding
the previous sheet's search terms and sort chips. The UI and the data disagree
and no error occurs. The current UI removes it with Vue's `v-if` rather than the
`hidden` attribute, so this path is gone — what remains is the record that
**trusting something to be hidden is the most dangerous part.**

**The value-selection panel loses its own button.** Re-rendering the whole header
on every value selection removed from the DOM the button an open panel was
anchored to, so the panel lost its position and closed. One checkbox, and the
panel vanishes. Since only that column's display changes, only that is updated
now.

**A scroll the browser performed arrives later than the click.** Operating a
column outside the viewport makes the browser scroll that button into view, and
that scroll event arrives **after** the click. With "close the panel on scroll"
in place, the panel just opened closed on the spot — it reads as nothing having
happened, and no error occurs. It now follows instead of closing.

**Clearing one value while narrowed excludes values nobody touched.** The port
gave value selection an "all" state. Clearing one checkbox there should exclude
just that one from "every value currently passing", but it was excluding from
**what the narrowing happens to show.** Narrow to `Dae` so that only Daegu and
Daejeon are visible, clear Daejeon, and the filter becomes `region = Daegu`,
excluding Gwangju, Busan, Seoul and Incheon, which nobody touched.

1,200 rows become 198 rows instead of 1,012. And the table renders fine, the
number looks plausible, and the console is silent. **Nothing in the system knows
that the person never excluded those four.** It now keeps a separate unnarrowed
list and fills from that (the exclusion-list switch in `ColumnFilter.vue`).

This one, too, would not have been known without running it. The type check
passes, the source-reading checks pass, the core's tests pass — the core only
filtered exactly as instructed.

**There is no clock.** This one surfaced in the core before this shell was built
— it is in `docs/wasm.md`.

All of these are the kind **neither a source-reading check nor a desktop test
finds.** That is why it gets run.

## What works today

Run against real sheets (tens of MB of xlsx, an education office's data cleaned
up).

- A few seconds to open a workbook and produce the sheet list
- Under a second to convert one sheet of over a hundred thousand rows to SQLite
- After that, tens of milliseconds to sort, and hundreds of milliseconds for a
  full search scanning every column

**No numbers are written down.** They differ with the machine and the file, and
at that moment only this document goes stale. Measuring it yourself is quicker —
the UI shows the time each query took.

**The data never leaves the browser.** There is nowhere to send it. Reading the
file, converting it and querying it all happen right there.

## The boundary is one set with the window's commands

The UI is a port of goe-navi's structure. That side calls Tauri commands and this
side calls wasm methods, but the command names and the request field names were
laid out as one set, so **the only change was the call target.** That place is
`web/src/lib/book.*.ts` — the browser variant and the window variant; the UI
imports under the name `sheetsift-book` and does not know which it got. Details
are in `docs/desktop.md`.

| `crates/tauri` (window) | `crates/wasm` (browser) |
|---|---|
| `list_screens` | `listScreens()` |
| `query_rows { request }` | `queryRows(request)` |
| `column_values { request }` | `columnValues(request)` |
| — | `limits()` |

A request is passed as **a single bundle.** Back when three strings were taken
positionally, swapping positions raised no error, so a filter ended up in the
column-name position and "no such value" was shown. Whether such a position
creeps back in is read textually by the tests
(`crates/wasm/tests/boundary.rs`).

A request **carries which table it refers to** (`screenId`). If the layer
remembered what was open, a late response to a previous request would be rendered
as belonging to the new table after a table switch.

**The list is served in full, once.** Every table's column definitions arrive
together, so the UI does not collect columns by opening tables one at a time.
Identifying columns is done entirely when the workbook is opened, and only the
conversion into a SQLite table is done on that table's first query — converting
everything up front would delay the first screen because of sheets nobody is
going to look at.

**There are fields the list cannot answer.** goe-navi's `list_screens` also
serves the decimal places of numeric fields (`decimals`) and the data version
(`dataVersion`), and both belong to that app. A viewing layer presents the
workbook's values without altering them, so there is nowhere to decide the
decimal places, and it does not know when the data is from. `decimals` is **sent
as `null`** — sending 0 would silently round the workbook's decimals, and that
would not be visible anywhere on screen. `dataVersion` does not exist at all.

The ported UI **does not round** when `decimals` is `null`. It renders exactly
the value present in the workbook — the same statement as the promise that a
viewing layer does not alter values. goe-navi's `cellText` was set to stop on
`null`, but that is the rule of an app that knows its decimal places, so it was
not ported here.

## The header is specified by the person

`sheetsift` is **general-purpose.** It has to take an arbitrary workbook, so
whether the header is one row, two rows or n rows, and which row it starts at,
cannot be settled in advance. `goe-navi` is **the special case that pinned**
header position and screen kind, which is why the definition file decides
everything there.

The core attempts an inference and, when it fails, **returns that it failed**
(`headerFound: false`). What cannot be hit is not returned as a hit. The decision
is made by the person in the toolbar — specifying the start row and the row count,
as in `header rows 1–2`.

Even when the inference succeeds, the core **states that it is an inference**
(`headerGuessed`). **The UI does not render it** — marking even correct guesses
would put a warning on every sheet until nobody reads any of them. Instead it is
stated once, in a dismissible notice, only when detection failed and the UI
assumed the first row. That core field is currently unconsumed, but it has to be
there the day rendering the inference is decided on.

### The specification is carried by the request

Same reason as `screenId`. If the layer remembered it, a late response arriving
just after the specification changed would be **mistaken for the new column
layout** and displayed. No error occurs.

```
listScreens  { headers: { "<sheet>": { at, rows } } }
queryRows    { screenId, header: { at, rows }, … }
columnValues { screenId, header: { at, rows }, … }
```

`goe-navi`'s `list_screens` takes no argument. The general-purpose side is the
one holding an extra field, so that app simply does not carry it.

### How the inference actually fails

Workbooks whose columns differ in depth do exist. The admissions-results workbook
has one level in its left 18 columns (`grade`, `class`, `university name` …) and
two levels only on the right (`elective`, `standard score`, `grade band` under
`Korean`). The inference judges a row to be a group row **when the upper row is
sparser than the lower one**, and this workbook is the opposite (24 cells above,
14 below). So it tries a single-level reading, sees the cells left empty by
merging as nameless columns, and declines.

Declining is right. Opened as single-level in that state, `grade band` would be
duplicated across six columns, and there would be no telling which column sorting
or filtering applies to.

### Names are composed

At two levels or more, names are composed as `group · name` (`Korean · grade
band`). Partly because the table's header is a single row, and partly because
**without composing, the names are not unique** — `grade band` appears six times
in that workbook.

SQL keys are derived from this composed name too, not from the original text.
With a two-level header the lower row is blank in some columns (`grade` exists
only on the upper row), so the raw keys would be `_`, `_2`, `_3`, and a query
error would not say which column it was about.

A column with no name at all is given **Excel's column letter** (`column U`).
That is the only expression by which a person can find that column in the
original.

## How printing is checked

`window.print()` is blocking, so it cannot be called automatically. To judge only
how the page renders, **inject the print medium's styles into the screen** —
extract just the `@media print` rules from `document.styleSheets`, insert them as
an ordinary `<style>`, and clone `.print-root` to observe. Title, metadata,
column proportions and type sizes are all verifiable that way.

**Page breaks and repeating header rows are not judged that way.** Emulate the
print medium in the browser's dev tools, or print to PDF. The window's (WebView2)
print dialog is checked once by a person — there is no means of driving it
automatically.

## Not there yet

- **One sheet is one screen.** In goe-navi the definition file decided what to
  render it as (table, chart, search), and a viewing layer has no such file. So
  `kind` is always `"grid"`. The field was kept — it is the one place that grows
  the day a definition file is introduced.
- **Persisting the header specification.** It is lost when the workbook is
  swapped. Whether the same workbook has to be re-specified every time will be
  judged by using it.
- **Saving as `.sift`.** The format exists in the core but the shell does not call
  it. Reusing a conversion requires that path.
- **Extremely large files.** wasm is 32-bit, so the address space is limited.
  Where it gives out has not been measured. The window has a ceiling in place
  (`MAX_BYTES` in `crates/desk`) — it occupies roughly 14× the bytes read.
  Switching the per-cell string containers to a single arena would cut that ratio
  to less than half, but that is a change for a different reason.
