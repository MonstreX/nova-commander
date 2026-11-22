# ADR-0020: `regex` for FAR's regular-expression masks

- Date: 2026-11-14
- Status: accepted (roadmap step P3.3, review R39, which the owner put in the plan; the crate is the one
  dependency it needs)

## Context
FAR's file masks take a regular expression in Perl style, `/body/options` (`filemasks.cpp`, `RegExp.cpp`):
in the panels' Select, the filters, highlighting, the groups of file masks, Find file and the associations. The
program had none: such a mask matched nothing, silently (review R39). FAR's own engine backtracks (back
references, look-around); a mask is matched on the UI's thread for every entry the panels show and highlight,
so an expression that takes exponential time would hold the program.

## Decision
- Take the `regex` crate (MIT or Apache-2.0, the Rust project's), `default-features = false` with `std`,
  `unicode` and `perf`. It matches in linear time: no expression can hold the program. It has no back
  references and no look-around; an expression with them fails as one with errors does ("File-mask string
  contains errors"), never silently.
- FAR's options: `i` (no case), `s` (`.` takes a line end), `m` (`^` / `$` at lines), `x` (blanks and comments
  ignored); `o` changes nothing; any other letter is an error. Without `i` case counts, as in FAR. The expression
  is searched for anywhere in the name (`RegExp::Search`).
- The list is read as FAR's `filemasks::assign` reads it: expressions and the masks between them, `|` for the
  exclusions; one expression with errors makes the whole list fail.
- The same crate serves the later searches with regular expressions (Find file's text, the editor and the
  viewer: stage P4 and on), unless one of them needs FAR's back references: then its own ADR.

## Consequences
- `architecture.md` lists `regex` among the crates in use.
- `vfs::mask` holds a mask as wildcards or as a compiled expression; the panels' and the dialogs' callers are
  unchanged (`FileMask::parse_with` returns `None` for errors, as before for an empty list).
