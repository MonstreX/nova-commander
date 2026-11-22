# ADR-0015: Files of any size are viewed and edited without reading them whole; Quick view counts in the background

- Date: 2026-11-11
- Status: accepted (owner, 2026-11-11)
- Details: docs/far-parity.md (2c step 3, Ctrl+Q; 10. Viewer; 11. Editor). Replaces the prototype viewer's and
  editor's way of reading a whole file (`editor::Document::load`, its size limit)
- Related: ADR-0014 (the same rule for the tree: slow whole-disk work in the background, nothing lost)

## Context
FAR's editor reads every line of a file before it shows it (`fileedit.cpp` `LoadFile` / `PushString`): a big file
takes a long time to open. Its viewer reads by blocks (`CachedRead`) and opens at once, but what walks the whole
file - line numbers, search, the end with wrapping - holds the keys. FAR's quick view (Ctrl+Q) is that viewer in
a panel, and on a folder it counts it (`GetDirInfo`) behind a modal progress window. The prototype viewer and
editor here read the whole file and refuse one over a limit. The owner: huge files are to be viewed and edited
with the most speed and comfort the system's means give; no strategy that reads a whole file for that.

## Decision
1. **A position is a byte offset.** Viewing reads only the blocks around what is on screen, through a cache of
   blocks read with positioned reads (not a memory map: on Windows a mapped file cannot be cut or grown by its
   writer, and a log being written must stay viewable). Opening, Home, End, a percentage are a block's work
   whatever the size; End finds the line starts backwards from the file's end.
2. **Whole-file work in the background.** The line index (line numbers, how many lines), search, the editor's
   save run on threads of their own with their progress shown; the keys work meanwhile and Esc stops them.
   Until the lines are counted a position shows as a percentage of the bytes.
3. **Only the screen is laid out.** Wrapping, hex and decoding are done for what is shown; the code page is told
   from the first block. A file that grows shows what was added.
4. **The editor edits a piece table.** The original stays on disk and is read as the viewer reads it; edits go
   to a buffer of their own; undo is the pieces' own operations. Opening is immediate and memory holds only the
   edits. Saving streams the pieces to a new file beside the original and puts it in its place; an original
   changed by another program while open is reported, never read again silently.
5. **Quick view (Ctrl+Q).** A file's contents are shown by the view core of items 1-3, built in stage 2c (step
   3b) so the viewer (F3, stage 2d) grows from it; its keys (F2 wrap, F4 hex, F7 search, F8 code page) come
   with 2d. A folder is counted on a thread of its own (`jobs::folder_info`): FAR's rows (folders, files, files
   size, allocated size, cluster size, files slack, MFT overhead) grow as it goes, marked `~` until the count
   ends; another item under the cursor stops it. The space each file takes comes from its folder's listing,
   not from opening the file.

## Where it lives (2c step 3b, owner's choices 2026-11-11)
- The core is `editor::view` (no UI, no threads): it is given blocks and names the one it lacks; a move that
  lacks one changes nothing until it comes. `jobs::file_reader` reads the asked blocks by offset on a
  thread of its own (owner: the screen never waits for the disk) through `vfs::Provider::open_random`.
- Quick view shows a file wrapped (FAR's default) with control characters as FAR's OEM glyphs (owner).

## Consequences
- The prototype `Document` stays until stage 2d replaces it; nothing new is built on its whole-file reading.
- Line numbers of a huge file are known only after the background count; until then the viewer shows
  percentages.
- Quick view's numbers of a big folder appear at once and grow; FAR's modal window and its Esc are gone, the
  numbers and their `~` kept.

## Addendum (owner, 2026-11-13): the line index and "go to" in the viewer and the editor
- **Go to (Alt+F8) takes a line, a percentage or an offset** in the viewer and in the editor alike. FAR's
  viewer takes only an offset or a percentage (`viewer.cpp` `GoTo`); a line there is ours, over FAR (owner).
- **The line index** (item 2's background work): a thread reads the file in order in large pieces (1-4 MB)
  and finds the breaks with a vectorised byte search (`memchr`), so the disk sets the pace; it keeps the start
  of every 1024th line only (a sparse index: under 1 MB for 100 million lines). A line N is its checkpoint plus
  at most 1023 lines counted in a block or two. UTF-16 breaks are looked for at even offsets; a lone `\r`
  breaks a line; a `\r\n` across two pieces is one break. A file that grows has its index extended from the end.
- **Until the index reaches a place** the status line shows a percentage; going to a line already indexed is
  immediate, one past it shows "Counting lines... N%" with Esc (ADR-0019) while the count goes on.
- **The editor's pieces each know their breaks** (a balanced tree of pieces with their line counts, as VS
  Code's piece tree): a line is found in O(log n) after edits, the original never read again for it.
- **Binary (hex) editing** in the editor comes later, after 2d's basic set (owner).
- Until 2d the prototype viewer and editor read the file off the screen's thread, behind a window with Esc
  (ADR-0019, review R8); their 16 MB limit stays.

## Addendum (owner, 2026-11-18): the viewer and the editor as FAR's originals
- The viewer and the editor follow FAR's sources exactly (keys, look, menus, settings), syntax
  highlighting included (FAR's Colorer), except for big files: they open at once - the viewer on the
  view core, the editor on its piece table - and never wait for a whole read into memory (this ADR).
- Syntax highlighting also in the viewer when it shows a text file, as in the editor (FAR's viewer has
  none; the owner's choice, documented in far-parity 10).
- S2d.1a: F3 is the viewer on the view core (`app::viewer` over `quick_view_text::FileText`); the
  prototype's `Document` stays only behind the editor until S2d.4.
- S2d.4a: the editor's document is a piece table over the file's bytes (`editor::view::pieces`): the
  original read by blocks as the screen needs them (the viewer's reader), the edits' bytes kept in the
  file's code page, so the view core's lines, search and go to work on the edited document as on a file;
  undo and redo give the pieces back; the save streams the pieces in order (the original's from the
  disk). The pieces are a list for now (a line lookup scans them); the balanced tree with the pieces'
  line counts (the addendum of 2026-11-13) comes when the editor's line numbers need it.
- S2d.4b: the editor (`editor::view::edit_view` over the piece table, `app::editor_text` with the
  viewer's reader): the cursor is a line's first byte and a column of the screen (FAR's cursor past a
  line's end); every step reads what it needs first and names a missing block (the reader brings it and
  its neighbours); keys typed ahead wait in order, an action (F2, Esc) waits for them. The save streams
  the document into a file beside the original and renames it over it, the file read again after it
  (undo not kept over a save yet, S2d.4c). The status line's line numbers are counted with checkpoints
  every 64 KB for a document up to 16 MB; past it `?` until the pieces count their lines.
