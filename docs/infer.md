# How far structure detection goes

Opening an arbitrary xlsx without hand-writing a definition file means deciding,
right where the file arrives, where the header is and which columns are numeric.
`core::infer` is that first stage, and this document records what happened when
it was **checked against real sheets.**

**This is a record, not a rule.** What is and is not possible is stated by the
tests, not by this document — the test bundle in `crates/core/src/infer.rs`.

## There is exactly one thing it guards

**An inference is never returned as if it were certain.**

Pick the wrong header row and one row of data becomes the column names. Judge a
numeric column to be text and sorting becomes 1 · 10 · 2. Both are silent — the
view opens, the values appear, only the order differs. That is why
`Kind::Unknown` and `Header::Unknown` exist; whether the caller asks a person or
uses a default is the caller's business.

## Results of the check

Checked against goe-navi's sheets. They are an education office's originals
cleaned up by hand, so headers are tidied into a single row and the structure
differs per sheet.

**Every data sheet's header was located exactly.** A one-row header was caught on
the first row.

**Most column kinds were determined, and the ones that were not stayed
"unknown".** Unknown appeared in exactly one category — columns where numbers and
text are **genuinely** mixed (a score column with "undecided" in it). That is not
a wrong answer but the right one: picking either side there fails silently.

### What is not possible

**A sheet containing only prose is judged to be a table.** A guidance sheet (like
`00 read me`) has prose written in two cells row after row, which is
structurally indistinguishable from a table. A header is found and returned there
— and one of them was even judged to have a two-row header.

**It is right that text alone cannot decide this.** Which sheets are data sheets
is known by the caller (goe-navi writes it in its definition file). Injecting
sheet names or numbers into this module would be **mixing data into code**, so it
is not done.

## How the choice is made

It scans from the top and takes **the first row that satisfies the conditions.**
The most likely row is not chosen by score — when scores differ, nobody can
explain why that row was selected. The scan-from-the-top rule exposes why even a
wrong answer was wrong.

The conditions are as follows.

- There is a non-blank cell, and **no number whose kind matches the data below
  it.** A header holds column names, so its kind differs from that column's data
  — above a numeric column, text is written. Conversely, a data row matches what
  is below it. Choosing the first data row as the header is the worst possible
  answer, so numbers are treated with suspicion without being rejected outright:
  sheets that use years, rounds or grades as column names (`school · 2023 · 2024`)
  do exist, and rejecting those would turn a row of data below into the column
  names.

  A column whose kind could not be determined (mixed) is not used as evidence. If
  a number written there counted as proof of data, one mixed column would veto
  the header.
- **There is data below it.** The last row is not a header.
- **It is as wide as the data below it.** A guidance row with only "This table
  shows …" in a single top cell is common, and being text-only it too reads as a
  header. Accepting it would make the guidance sentence a column name and leave
  every other column nameless.

  The reason exact equality is not required is the opposite accident: sheets with
  a body cell protruding to the right of the header are also common, and
  requiring equality would find no header at all in such a perfectly normal
  table.
- **No nameless column is produced.** A blank in the first row would make that
  column's name an empty string. Usually that means the top row of a two-row
  header was caught alone, and if the lower row could not be read along with it,
  declining to choose is the right outcome.

A two-row header is identified by its **sparse-then-dense shape**: the upper row
holds group names and is therefore intermittent (one `admission summary` spans
several columns) while the lower row is dense. If both are dense, the second row
is data.

## What cannot be decided here

**Meaning cannot be decided.** Which column is a person's name and which is a
score, and which columns pay off when dictionary-compressed, cannot be decided
from text alone. This module goes as far as structure.
