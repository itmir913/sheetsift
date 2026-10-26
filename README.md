# sheetsift

**Read Excel files without opening Excel.**

Opening a spreadsheet just to look something up means waiting for a heavy
application to start, loading the whole workbook into memory, and risking an
accidental edit to a file you only wanted to read. sheetsift is the read-only
alternative: open a large `xlsx` or `csv` and get sorting, filtering and search
that stay fast on sheets that make Excel crawl.

- **Read-only.** The source file is never written to.
- **Fast on large sheets.** Sorting and filtering stay responsive where a
  spreadsheet application stalls.
- **Private.** In the browser the data never leaves it — there is nowhere to
  send it. The desktop build talks to no network at all.
- **Two builds, one core.** A desktop app (tauri) and a browser app (wasm) run
  the same code.

## Why it is fast

An `xlsx` is XML inside a zip, which cannot be queried directly at speed. The
sheet is converted once into SQLite; repeated strings are interned into a
dictionary, so sorting and filtering reduce to integer comparisons.

A converted sheet can be kept as a `.sift` file and reopened without converting
again. It is password-sealed by default, because a sheet may contain people's
names.

## Running it

In a window:

```bash
npm run dev:tauri
```

In a browser:

```bash
npm run dev:web
```

The UI is the same one. **Exactly one thing differs:** the window takes the path
returned by the operating system's file dialog and reads it directly, instead of
routing the bytes through the UI. Drag-and-drop is bytes on both platforms.

Pick a file and it is converted to SQLite right there, then queried by sorting,
value selection and search. How many header rows there are is specified by the
user in the controls.

The distributable builds are `npm run build:tauri` and `npm run build:web`.
`docs/desktop.md` lists what baking the desktop build requires, and
`docs/wasm.md` what the browser build requires (clang and `wasm-bindgen-cli`).

## Tests

```bash
npm run ci
```

That one command is the gate.

## Using it as a library

The viewer is built on `sheetsift-core`, which takes bytes and returns bytes and
knows nothing about tauri or the file system. Integrating it in another
application — the crate layout, the feature gates, the `.sift` format, the keys
the caller has to inject, and what the gate does and does not cover — is
documented separately in **[docs/core.md](docs/core.md)**.
