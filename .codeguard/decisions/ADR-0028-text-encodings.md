# ADR-0028: one place for what a file's text is told by

- Date: 2026-11-22
- Status: accepted (R7 review E4; owner 2026-11-21: the review's fixes accepted, "the rest decided as needed")

## Context
Two code-page stacks grew apart. The view core (`editor::view::code_page`: `Encoding`, `detect`, ADR-0015 §3)
tells a file's code page by FAR's `GetFileCodepage` with its UTF-16 heuristics. Alt+F7's search
(`jobs::find::text`: `CodePage`, `Decoder`, `code_page`) had its own copy without them, so "Automatic
detection" read UTF-16 without a byte order mark as ANSI where FAR and our viewer find it. FAR's default
word dividers (`WordDiv0`) were written three times (the view, Find, the line editors). `jobs` cannot use
`editor` (architecture.md), so neither could take the other's.

## Decision
- `vfs::text` holds what the layers above share of a file's text: `WORD_DIVIDERS` and `detect` (the byte order
  mark, UTF-16 by its zero bytes, UTF-8 past ASCII; `whole`: a character cut by the end is no UTF-8).
  `vfs` is the lowest layer `editor`, `jobs` and `app` all use and is under the gate's `core-no-ui` check, so
  no new top-level module (and no change to the protected gate config) is needed.
- The view's `detect` and Find's `code_page` both call it; Find's "Automatic detection" now tells UTF-16
  without a mark as FAR does.
- The two decoders stay: the view decodes a character where the screen needs it (`Encoding::decode`), Find
  turns whole 64 KB blocks into characters (`Decoder`). Their merge, and the system's code-page tables for the
  port (`platform::locale` on Windows), wait for S7's code-page work (tech-debt `[S7]`).

## Consequences
- One rule tells a file's code page in the viewer, the editor, quick view and Find.
- A change to the word dividers (FAR's `Editor.WordDiv` setting, not built) has one place to start from.
