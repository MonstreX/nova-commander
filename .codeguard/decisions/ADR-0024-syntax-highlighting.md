# ADR-0024: syntax highlighting as FAR's Colorer, its engine ported into the program

- Date: 2026-11-19
- Status: accepted (owner, 2026-11-19: Colorer's HRC grammars as in FAR, in the program's core, the colours
  from the theme's roles, on by default in the editor and the viewer with a switch)

## Context
FAR highlights syntax through its Colorer plugin (`github.com/colorer/FarColorer` over
`github.com/colorer/Colorer-library`, read in `farcolorer-source/` and `colorer-source/`, kept out of git as
`far-source/` is), with the HRC grammars of `github.com/colorer/Colorer-schemes` (read in
`colorer-schemes/`): some 300 languages, chosen by a file's name and first line. Stage 2d takes it in the
editor and, over FAR, in the viewer on text files (owner, 2026-11-18, far-parity 11a). The library and
FarColorer are MIT, as the program is; the schemes are LGPL 2.1. The editor and the viewer lay out only the
screen and never read a huge file whole (ADR-0015).

## Decision
1. **Colorer's engine in Rust, in the core** (`editor::syntax`, used by the editor and the viewer alike; not
   a plugin, owner): its regular expressions (`cregexp`: `\y` / `\Y` back references into the scheme's
   start, `~`, `\m` / `\M`, `?#n` / `?~n` look-behinds, named brackets), the HRC library (`HrcLibraryImpl`:
   the catalog, prototypes, types, schemes, regions and their parents, `inherit` with virtual schemes,
   keywords, blocks, `regexp` and `block` with their `region` / `region0-9` / `regionN` names, entities) and
   the text parser (`TextParserImpl`). Its code is ported, MIT both sides, with Colorer's copyright kept in
   the ported files' heads; the behaviour (which scheme, which region, where a block ends) is Colorer's,
   checked against its tests and its grammars' own samples.
2. **The schemes are data beside the program**, not in its binary: the package ships Colorer-schemes'
   `base` (the catalog, `hrc/`; the `hrd/` colour files are not taken, §3) unpacked in `hrc\` with the LGPL's
   text; the user may change or add schemes there as FarColorer's users do. A missing folder means no
   highlighting, never an error.
3. **The colours are the theme's roles** (ADR-0008), not HRD files: the regions of `def:` (Comment, String,
   Keyword, Number, Symbol, Directive, Error, ...) map onto roles `syntax.*` of the themes, and a region
   without its own role takes its parent's as HRD does; the built-in FAR palette gives them Colorer's
   console defaults (`hrd/console/default.hrd`) on the editor's blue.
4. **Line by line with the state at checkpoints**: the parser's state (the scheme stack) is kept every N
   lines as the line index keeps offsets; a screen is coloured from the nearest checkpoint above it. Where
   none is near (a huge file opened at its end or a far `Go to`), the parse starts `backparse` lines above
   the screen (FarColorer's parameter, 2000 by default) from the file type's base scheme, as Colorer itself
   does for such lines; the checkpoints fill on in the background (ADR-0014) and the colours are redrawn
   when the exact ones come. An edit drops the checkpoints below it (Colorer's `modifyEvent`).
5. **On by default** in the editor and the viewer's text mode, a switch in their settings (S2d.6, F9), and
   FarColorer's menu to choose the file type for the file open (its "File type" list by groups).
6. **Dependency**: `quick-xml` (MIT) for the HRC files in the program's crate (already the workspace's for
   NetWay's WebDAV).

## Steps (roadmap S2d.5)
- S2d.5a: `cregexp` ported with its tests.
- S2d.5b: the HRC library: the catalog and the schemes loaded, file types chosen by name and first line.
- S2d.5c: the text parser: a line's regions from a state, the state after it; checkpoints.
- S2d.5d: the editor highlighted, the theme's roles, edits re-parsing from their line, the file type menu.
- S2d.5e: the viewer's text mode highlighted (back-parse for huge files), the package's `hrc\`.

## Addendum 2026-11-21 (S2d review B9): 200 lines back, not 2000
A screen far past the kept states is parsed from `highlight::BACK` = 200 lines above it, not FarColorer's
2000: on every draw of such a screen (a document past the states' limit, or before the states reach it)
those lines are parsed on the UI thread, and 2000 made a debug build's paint lag; the colours are exact
once the states reach the screen, and "Match pair" still looks 2000 lines away (`BACK_PARSE`).

## Addendum 2026-11-22 (review R7, E11): the checkpoints fill on the UI thread, on a budget
§4's "the checkpoints fill on in the background" is built otherwise: the states past the last one are filled
on the UI thread between key presses, a budget of 3 ms a tick (`app::editor_syntax::FILL_BUDGET`, through
`try_read` of the loaded library, so a scheme loading on its worker never holds the screen), for documents
up to 16 MB (`highlight::FILL_LIMIT`; past it the screen is parsed from `BACK` lines above), and the screen
is redrawn when they reach it. The catalog's and the schemes' loading, the file type's read-ahead (R7 B6) are on workers. A
worker of its own for the parse would need the document's text off the UI thread (its piece table, ADR-0015
§4); the budget keeps a keystroke's lag within a tick, so it stays until a measure says otherwise.

## Consequences
- Some 5 000 lines of C++ become Rust in `editor::syntax`; the grammars stay Colorer's and can be updated
  by copying its `base` again.
- The colours follow the program's themes; HRD files and FarColorer's colour settings are not taken (a
  documented difference: one place for colours, ADR-0008).
