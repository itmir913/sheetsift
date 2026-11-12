# Using sheetsift-core

For applications that embed the core rather than run the viewer. `core` knows
nothing about tauri and nothing about the file system: it takes bytes and returns
bytes — that is what lets desktop and web (wasm) share the same code. Writing
files is the caller's job.

## Crates

| Crate | Responsibility |
|---|---|
| `crates/core` | Sheet conversion (`sheet`), structure detection (`infer`), the stored format (`sift`), the query engine (`grid`), the password envelope (`vault`), the signed container (`seal`), schema versioning (`migrate`), the key holder (`keyring`) |
| `crates/aead` | One layer of AES-256-GCM. Kept separate so it is compiled with optimizations even in debug builds |
| `crates/shell` | The platform-neutral kernel (`Viewer`). Binds the converted connection, column definitions and requests in one place |
| `crates/wasm` | The shell that attaches the core to the browser. `wasm_bindgen` exists only in this crate |
| `crates/desk` | The commands the window calls, plus **file reading and writing**. Ordinary Rust, so the full gate covers it |
| `crates/tauri` | The tauri app that attaches those commands to a window. Outside the workspace, so `ci:tauri` owns it |
| `crates/webview` | Turns off the webview's own browser shortcuts (`harden`). Not used by the viewer here; an application that wants it takes it as a git dependency. Takes tauri, so it is outside the workspace too and `ci:webview` owns it |
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

`sift` and `infer` are **feature-gated**. They are enabled by default; an
application that wants less opts out.

`migrate` (schema versioning and the schema fingerprint) and `keyring` (the
holder for the sealing key material and trusted public keys, with no values in
it) are gated too, but **off by default**: the viewer in this repository uses
neither, and `migrate` adds an error code the viewer could never show. An
application that uses them opts in. The crate's own tests turn them on through a
dev-dependency on itself, so `ci:gate` still runs them.

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

## One judgement, one reader

What counts as a number, what counts as blank, and how a cell's text becomes a
number all live in `core::value` — once each, with the SQL mirrors beside them.
`infer`, `sheet` and `grid` call those; none of them has its own copy. Tests
compare each Rust predicate against its SQL mirror over generated inputs, so a
change to one that forgets the other stops the gate.

The rule is the query engine's, not `f64::parse`'s. `1,200` is a number;
`1,2,3`, `1e5`, `NaN` and `inf` are text. `as_real` / `as_int` only read what
the predicate accepts, and `as_int` refuses to round or saturate.

These are `pub`, not crate-private: an embedding engine that builds its own
SQL must be able to ask this crate what blank is, instead of writing
`x IS NULL OR CAST(x AS TEXT) = ''` beside it — that spelling trims nothing and
knows nothing of the data's blank marker, and it agrees only while the imported
values happen to arrive trimmed. `value::is_blank_sql`, `value::trim_sql`,
`value::trim`, `value::is_number`, `value::as_real`, `value::as_int` and
`value::fits_real` are the callable form of those rules;
`crates/core/tests/embedding.rs` lives outside the crate, so making any of them
private again stops the gate.

### A `real` column only holds what reads back as a number

SQLite writes an extreme `real` back as text in exponent form (`1.0e+17`), and
the predicate above calls that text. Storing such a value would mean the build
read the cell as a number while the query engine reads it as text — the same
cell, two answers, no error anywhere. So the build **refuses to store it**
rather than the predicate loosening to accept exponent notation:
`value::fits_real` is the boundary, and a refused cell is counted in
`Made::unread` like any other cell that could not be read. The boundary is
written narrower than today's SQLite (`1e-4` up to `1e15`), because the
rendering differs between SQLite versions and this crate does not pin the
version the embedding app links; narrower can only refuse too much, which is a
path with a statement on it. 요강 data never reaches those magnitudes — that is
why this is small, not why it was left open.

### `Made::unread` is a warning you have to pass on

`sheet::build` returns a `Made` per table. Its `unread` lists `(column, cells)`
for every number column holding text the predicate rejects: that cell becomes
`NULL`, so the row stays and only the number is gone. **Counting is not
warning** — if `unread` is not empty, say so to the person making the file, in
one line naming the columns and the cell counts. Nothing in `core` can do that:
it holds no human-readable text, by the rule above.

The viewer in this repository declares an all-text plan, so `unread` cannot be
non-empty there; `crates/shell` stops with `SheetTableFailed` if it ever is,
rather than letting it pass.

### One decomposition, two renderings

SQL stays SQL: a closure cannot become a `WHERE` clause, so "one function both
sides call" is not available for filtering. What is available is one
decomposition with two renderings. `grid::Match::compile(filters, columns, blank)`
reads the request once; `Match::where_sql` renders it as SQL and `Match::row`
applies it to one row's cells. `build_where` is a consumer of `Match`, so the SQL
path is a translation rather than a parallel implementation, and generated-input
tests compare the two renderings row for row.

Exporting a small per-condition predicate instead would leave the layer that
actually diverges written twice: how conditions combine (AND between filters, OR
between a filter's `keys`) and what counts as an error. So **every rejection
lives in `compile`** — unknown column, `min` above `max`, a range on a
non-number column, and `values` and `excluded` given together (an embedding app
was silently applying both). `Match::row` never fails: a bad request is a fact
about the request, judged once when it is decomposed, and a short row is read as
if the missing cells were blank.

`Match::compile` takes the caller's own column order as `(key, is_number)`, and
`row` takes cells in that order. A dict-encoded column's cell is its stored form
— trimmed text, blank stored as blank — because that is what the query compares.
Whether a column can be filtered at all is this crate's own notion, so that
rejection stays with the query side.

`where_sql` is public too, and the **caller supplies each column's SQL
expression** (`where_sql(&|i| ...)`, `i` being that same column order), because
the same conditions are also applied to a different table in an embedding app —
which had a third copy of this assembly, and that copy applied `values` and
`excluded` together where this one refuses. Only values the person typed are
bound as parameters; the expression a caller supplies must come from its own
definitions. Dict-encoded columns are this crate's storage shape, so the engine
renders those itself.

### Row order has its own comparison

`grid::compare_sorted(a, b, blank, numeric, dir)` is the Rust twin of the
`ORDER BY` keys. In SQL the direction attaches only to the last key, so blanks come last whatever
the direction, then — in a number column — values that are not numbers, and only
the last key follows the requested direction.

`compare_values` cannot stand in for it: that one orders the **value-selection
list**, where blanks come first.

Among values that are not numbers, the order is whatever SQLite's
`CAST(… AS REAL)` makes of them, because that is the query's own key and it has
no text tiebreak after it. `value::cast_real` mirrors that cast (leading-prefix
parse, `0` when nothing parses) and a differential test asks SQLite to confirm
it. That mirror is not a loosening of what counts as a number — `is_number`
still decides that — it only orders the values that are not.

A dict-encoded column passes `numeric: false` even when it holds numbers: the
query sorts it by dictionary id, and those ids are assigned in collation order.

### A filter says which control wrote it

A column's value list is built with **that column's own selection left out** —
otherwise picking one value shrinks the candidates to that one value and there is
no way back. Which filters are "its own" used to be inferred from the key, and
that inference breaks the moment two controls filter one column: a card above the
table narrows 대학, the table narrows with it, and the column's funnel still
lists every university with its full count. Select one of those and you get an
empty table with no error.

So the inference is gone and `Filter::origin` states it. `Origin::OwnPicker` is
the default, which is what every existing caller gets: the filter belongs to that
column's own popover and the value list leaves it out. `Origin::Elsewhere` is
anything else — a card, a global search, a second control — and the value list
**and the column's data span** apply it like any other column's filter. An app
that has two controls on one column marks the outer one; an app that has one
control writes nothing and nothing changes for it. The engine cannot tell two
controls apart by itself, and this is not specific to any embedding app: this
repository's own screen reaches the same situation the day it grows a second
control.

### The value-selection list's order

A number column's distinct values are listed **numerically**, not as text: a
list reading `1 · 10 · 100` cannot be scanned by a person against a table
ordered by value, and when the distinct values exceed `MAX_VALUES` a text sort
makes the cut drop genuinely different entries. An application that builds such
a list in Rust rather than through `grid::column_values` must order it with

```rust
grid::compare_values(a: &str, b: &str, blank: &str, numeric: bool) -> Ordering
```

Blanks first, then — for a number column — the values the predicate reads as
numbers in numeric order, then the rest in text order; equal numbers written
differently (`90`, `90.0`) are ordered by text so neither vanishes. A text
column gets blanks first and text order. `values_order_sql` is the same rule in
SQL, and a test sorts the same samples both ways and compares.

### Column names that shadow the rowid

Paging is ordered with a unique tail — the base table's `rowid` — because
without one, stitching pages by `offset` can repeat one row and drop another
while the count still matches. In SQLite an explicit column named `rowid`,
`_rowid_` or `oid` shadows the real one, so `sanitize` prefixes an underscore to
such a header, `sheet::build` rejects a `TablePlan` that names a column that
way, and `grid` stops if a column definition does. `sheet::shadows_rowid` is the
predicate.

### Error codes say which mistake it was

`grid` distinguishes four: `InvalidSortKey` (the sort named a column that is
absent or cannot be sorted), `InvalidFilterKey` (the filter or value list named
a column that is absent or cannot be filtered, or gave both a pick and an
exclusion), `InvalidRange` (a range on a column that is not a number, or a
minimum above the maximum) and `TableDefinitionInvalid` (the column definition
itself cannot be served — today only a column name that shadows the ordering
tail). They were one code until now, separated only by the English `detail`,
which no screen shows — so a person who typed a bad range was told the sort
column could not be found.

`TableDefinitionInvalid` is not the person's mistake and there is nothing for
them to change, so its wording has to differ from the other three.

**An embedding application must write a message for `INVALID_FILTER_KEY`,
`INVALID_RANGE` and `TABLE_DEFINITION_INVALID`.** `INVALID_SORT_KEY` keeps its meaning for the sort cases, so
an application that does nothing still shows its old message where it was right.

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
