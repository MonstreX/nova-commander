# ADR-0025: `unicode-width` for the view core's cells

- Date: 2026-11-19
- Status: accepted (roadmap step S2d.6f, the tech-debt item of wide characters; the crate is already in the
  program, as `ratatui`'s and `vt100`'s, the same version)

## Context
The view core (`editor::view::lines`, ADR-0015) lays a line out as cells, one a character. The screen is drawn
by `ratatui`, which gives a character as wide as two columns (CJK, full-width forms) two cells and a combining
one none: a row of such characters ran past the box's edge, the cursor and the marks stood off the text, and a
combining character moved the rest of the row back a cell. FAR lays characters out by the console's cells
(`char_width::get`) when "Full-width aware rendering" is on.

## Decision
- Take the `unicode-width` crate (MIT or Apache-2.0, the version `ratatui` uses) as a direct dependency of the
  program. The view core asks it how wide each character it shows is, so its cells are the cells `ratatui`
  draws.
- A character two cells wide takes two cells: its own and a tail cell of the same byte (`WIDE_TAIL`), which the
  row's text leaves out (a row that starts on it or ends before it shows a space there). A character no cells
  wide (a combining one) joins the cell before it: it has no cell of its own, the row's text puts it after that
  cell's character, and the cursor and the edits take the two together. At a line's start it takes a cell.
- The width is always the console's, as FAR's with "Full-width aware rendering" on (its default is off; the
  program draws through the terminal's VT, where the widths are the terminal's anyway).

## Consequences
- `architecture.md` lists `unicode-width` among the crates in use.
- `Line` gains the combining characters it joins to cells (`joined`); the cursor never stands on a tail cell
  after a move left or right, and one put on it by Up / Down is shown on the character's first cell.
