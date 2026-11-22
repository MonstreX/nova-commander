# Long processes: the inventory of ADR-0019

Every process that can last: its window, its Esc, how a stop ends (at what is done, or back to before), the
system calls that may hang, and what it lacks against ADR-0019. Made on 2026-11-13 before the whole-program
review (ADR-0018) and corrected by its pass on processes (`docs/reviews/2026-11-13-after-stage-5.md`, R8-R30);
the gaps are closed by the steps of `docs/roadmap.md` (S19 done; P1, P2), each with a stand that makes it hang.
Brought up to date on 2026-11-22 by the review before stage 7 (`docs/reviews/2026-11-21-before-stage-7.md`,
F1-F11): the viewer and the editor of S2d, the archives and NetWay of S6.

Marks: **ok** - as ADR-0019 asks; **part** - some of it; **gap** - none of it. "Hangs" names the calls that may
not come back (a network share, a stuck device, a plugin). Calls `CancelSynchronousIo` cannot break - the WNet
functions, `LookupAccountSidW`, the shell (`SHFileOperationW`, `ShellExecuteExW`), the clipboard,
`LoadLibrary`, a plugin's code - need the Abandon path. Since P1.5 `CancelSynchronousIo` breaks the call of work
let go (`app::waiting`) and of a folder reading nobody waits for; one it cannot break is let go, and a disk with
4 threads hung on it past 3 s gets no more (R25, `jobs::disk_threads`). The keys, the window and Esc of every
wait are one mechanism (`app::held`, R54): work that can be asked to stop is asked, one that does not stop
within 3 s asks "It does not answer": Enter waits more, Esc abandons.

## The program's own

| Process | Window | Esc | A stop ends | Hangs | Mark, what is missing |
|---|---|---|---|---|---|
| Copy, move (F5/F6; ADR-0009/0010) | progress, Background / Pause / Cancel | FAR's question, then cancel | at what is done; the partial file removed | `CopyFileExW` stops within a chunk; opening a dead share's file; the totals walker and the walk list with the task's cancel (R15, 19-2) | **part**: hung I/O not broken; the target's check and the volumes now off the UI's thread (R19, 19-3) |
| Delete, Shift+Del, to the Recycle Bin (ADR-0012 2b) | progress as copy | as copy | at what is done | the shell's Recycle Bin call per item; a dead share; the walk lists with the task's cancel (R15, 19-2) | **part**: the question's looks at the disks (Recycle Bins, the tree's folder) and the task's volumes off the UI's thread (R11, R19, 19-3); the Recycle Bin call cannot be broken |
| Attributes on many items (ADR-0011) | a task as copy | as copy | at what is done | a dead share; `LookupAccountSidW` | **done**: Ctrl+A reads the selected items' properties through `wait_for` (R16) |
| Reading a folder for a panel (ADR-0013 §5) | the title's "Reading: N" | Esc -> Yes: what was read is shown | at what is done; a worker stuck past `READ_WAIT` left (D3) | `CreateFileW`, `GetFileInformationByHandleEx` on a share | **part**: a reading dropped has its call broken (`CancelSynchronousIo`) and a limit on the workers left on its disk (R25, P1.5); an automatic refresh is never waited for on the UI's thread (P1.6); no notice when the worker is left; a stopped re-read goes back to before, its selection whole (R23, P3.1) |
| Find file (Alt+F7) | its results window, the place being searched | Stop, Esc -> Yes | at what is done | listing and reading on a share | **part**: a read stuck on one file holds the stop until it returns |
| Find in a file's text (Alt+F7 "Containing") | the file and its percent | as above | at what is done | a read on a share | **part**: as above |
| F3 on a folder (its size) | "Counting..." | Esc breaks it | at what is done: finished folders keep their sizes | listing on a share | **part**: no "stopped at N" notice; quadratic loops (R33) |
| Quick view of a folder (Ctrl+Q) | the counts as they grow | another item stops it | the counts so far | listing on a share | **part**: no Esc of its own |
| Folder tree (Ctrl+T, Alt+F10; ADR-0014) | "Reading" with its count | closing the tree stops the worker | at what is read, kept for next time | listing on a share | **part**: Enter on the tree, in the tree window and the target tree ask their disk through `wait_for` (R22); the tree losing the keys still asks `is_dir` on the UI's thread, as FAR's `OnFocusChange` |
| Quick view of a huge file (Ctrl+Q; ADR-0015) | shows what it has, the rest as it comes | leaving it stops | at what is read | a read on a share | **part**: a read stuck on a share holds the worker |
| F3 viewer, F4 editor: opening a file (ADR-0015) | the window opens at once; with nothing to show past 300 ms "Reading the file... N s" in its body (R7 F9) | Esc, F10 close it at once, a read hung or failed too (R7 F1) | the reader dropped, never joined | `CreateFileW`, a block's read on a share | **ok**: only the blocks shown are read (`jobs::file_reader`), nothing whole, no size limit; the editor's stamp and identity on a thread of their own (R7 F5) |
| Editor: save (F2, Shift+F2, Shift+F10; ADR-0015 §4) | "Saving the file" with the percent's bar | Esc: the temporary file removed | written whole or not at all: a temporary file put in place by `ReplaceFileW` with FAR's flags, ACL, owner, attributes, creation time and streams kept; a file with several names written in place (R2); a file changed on disk refused (`REWRITTEN`) | a write on a share | **ok**: on a worker, streamed from the piece table; the questions' looks at the file (stamp, read-only) off the UI's thread (R7 F5) |
| Disk work behind menus (`app::waiting`): the drive menu's reading, entering a drive, F7, network names (one `WNetGetUniversalNameW` per selected item), connecting a network drive, the elevation question, the guest shell, history places' `exists`, shortcuts, the user menu, the user screen | past 300 ms: its own ("Waiting for the disk to mount...") or "Working..." | Esc stops the waiting | back to before: its end is never done to the app | `GetVolumeInformationW`, the WNet functions, `CreateDirectoryW` on a dead share | **ok** (R9, stage 19-1; P1.1, P1.5): past 300 ms a window (its own or "Working..."), Esc stops the waiting, the app stays as it was with a notice (a menu the work was asked from back after it), the hung call broken by `CancelSynchronousIo` or its thread let go |
| A command in the program's console | the command's own output | Ctrl+C reaches the command | the command's own | the command | **ok** (a command is the user's) |
| Opening by the system (`ShellExecuteExW`, `SEE_MASK_NOASYNC`): Enter on a document, Shift+Enter, the drive menu, the tree, a plugin's ShellOpen | past 300 ms: "Working..." | Esc stops the waiting | - | a share, a slow handler | **part** (R10, 19-3): on a thread of its own with COM, a window and Esc past 300 ms; the shell call itself cannot be broken |
| Quitting with tasks running | none | none | - | a task stuck on a dead share or a plugin: joined without a limit; the history writer joined | **part** (R12, 19-2): a job is waited for at most 2 s at the end, the history writer 5 s; tasks that do not stop 5 s after "Cancel all" bring "Wait / Quit anyway" |
| Quitting with viewers and editors open (S2d.6c) | each modified editor's "Save?" in turn | Cancel stops the quitting | - | an editor whose file hangs | **ok**: the questions skip an editor's pending keys (R7 F1) |
| The start | none | none | - | the last folder's `is_dir` on a dead share (R20); plugins one by one (R17) | **part**: the last folder waited for at most 2 s, then the start directory (R20); plugins (R17) still a gap |
| The shell list (Ctrl+O's shells, the options) | none | none | - | `is_file` in every PATH folder (R21) | **part**: looked for once a run (the first time at the start) |
| The status line's link target | - | - | - | `read_link` at every draw (R18) | **as FAR**: its status line reads a junction's target at every draw too (`panelmix.cpp:731`); the panel's disk has just answered its listing |
| Selection, sorting, totals, highlighting of a huge folder | none | none | - | none (memory): on the UI's thread | **part**: names put in the system's form once per sort, a flag per entry for "selected first" (R31); the totals kept until the entries or the selection change (R32, P3.2); F3's sizes and a plugin's reselection by sets (R33); an entry's highlighting once per draw, not per cell (R34); against FAR's speed with P3.6 |
| The clipboard | none | none | - | `GetClipboardData` with delayed rendering by a hung program (plausible); a million paths put at once (R29) | **done** (P1.5, P3.4): every clipboard call waited for at most 2 s on a thread of its own, then "The program holding the clipboard does not answer" and the thread let go (FAR waits for ever); what is put is made into the clipboard's blocks before it is opened, so it is held only to hand them over (R29). The screen waits for those 2 s at most: a paste's text is needed by the key that asks for it |
| Settings, history, user menu written | none | - | written whole or not at all (`ops::write_atomic`) | a write on a share; on the UI's thread at Shift+F9 and at exit | **ok** |
| Descriptions (`Descript.ion`) | none | none | - | read on the info panel's thread; written only by the editor; F4 on the info panel's description `symlink_metadata` on the UI's thread | **mostly ok** |
| Elevation (ADR-0007): starting the elevated helper | none | none | - | the pipe's accept has a deadline | **part**: no Esc while it waits |

## The viewer and the editor (S2d)

| Process | Window | Esc | A stop ends | Hangs | Mark, what is missing |
|---|---|---|---|---|---|
| Viewer: F7 / Shift+F7 search, text or bytes | "Searching for" with the percent's bar | Esc | the cursor where it was | a block's read on a share | **ok**: on a worker over the file's windows (`editor::view::search`) |
| Viewer: Alt+F8 by line, the line index | the busy bar while lines are counted | Esc | at the place it was | as above | **ok**: `jobs::line_index` on a worker |
| Viewer: the bytes typed saved (ADR-0027) | "Saving the file" | Esc | written in place or not at all | a write on a share | **ok**: on a worker (`vfs::local::ops::write_bytes_in_place`, R7 E6) |
| Editor: F7 search, its "All" list | "Searching for" with the busy bar and the time | Esc | the cursor where it was; the list so far | an unedited block read on a share | **ok**: on a worker over the document and the file held (R7 F6: never the file now at the path) |
| Editor: Ctrl+F7 replace, All | "Replacing" with the busy bar | Esc | nothing replaced | as above | **ok** (R7 F2, F3): the matches found on one worker pass, then replaced as one step over the document |
| Editor: Alt+F8, opening at a line, a plugin's far line | none: the cursor goes once the reader brings the blocks it needs | Esc closes the editor (R7 F1) | - | as above | **ok** (R7 F4): walked from the line counter's checkpoints, each block brought once, never the file's start again |
| Editor: Ctrl+C / Ctrl+X / Ctrl+Gray+ on a block not read yet | none | - | - | as above | **ok** (R7 F10): the copy waits for the blocks it needs, then is done |
| Colorer: the catalog and a scheme loaded (ADR-0024) | the text uncoloured until it comes | - | - | `quick-xml` over big HRC files | **ok**: on workers; painting on budgets of 3 ms a tick, outlines and pair matches through `try_read` and the pending retry (R7 B5, F7) |
| Colorer: the type read ahead for the file under the cursor | none | - | - | an open on a share, a placeholder (OneDrive) | **ok** (R7 B6, F8): local, not offline files only, counted in `jobs::disk_threads` and let go |
| Editor plugins: commands and key hooks (API 10, ADR-0026) | as a plugin's panel key | as a plugin's panel key | the plugin's own end | the plugin | **ok**: under the plugin's wait, its requests through `query()` |
| F3 / F4 on a plugin's or an archive's file: the copy out, the change put back | the copy's task window | the task's Cancel | the copy kept on failure | the plugin, a share | **ok** |

## Plugins (ADR-0016)

| Process | Window | Esc | A stop ends | Hangs | Mark, what is missing |
|---|---|---|---|---|---|
| A plugin started with the program (`new`, `info`, items) | none | none | - | the plugin: up to 10 s, the plugins side by side (one deadline for all) | **part** (R17, P2.2): started side by side on their own threads; the program waits for them at its start, as FAR, before its first screen |
| A panel key, a command (`tmp:`), a file opened (`open_file`), found files (`find_list`) | past 300 ms: the plugin's progress (`progress()`) with the percent's bar, or "The plugin is working..." with a busy bar and the time (D23; S2d review) | the plugin is asked to stop (`cancelled()`); 3 s later "It does not answer": Enter waits, Esc abandons (D14) | the plugin's own end, with what it did; abandoned: its late answer let be | the plugin | **ok** (P2.1, API 2) |
| F11 and drive menu commands, configurations (`Job::Run`) | as a panel key | as a panel key | as a panel key; a late panel closed unseen (R13) | the plugin | **ok** (R13, stage 19-1; P2.1) |
| A plugin's panel listed | the panel's "Reading" | Esc -> Yes as a folder | the waiting stops and the plugin hears it (`cancelled()`) | the plugin | **ok** (P2.1) |
| A plugin's windows (message, menu, input, dialog) | the window | the window's own Esc | the plugin's answer | - | **ok** |
| A message without buttons (busy) | the message | Esc closes it and asks the plugin's call to stop | the plugin's own end | the plugin | **ok** (P2.5) |
| Files put on a plugin's panel (F5 to it) | busy message | Esc asks the plugin to stop | TmpPanel: back to before (the list as it was) | the plugin | **ok** (P2.5) |
| F7 on a plugin's panel; a plugin's files copied out, written, removed (5c-2) | F7: past 300 ms "Working..."; copies: the task | F7: Esc stops it; copies: the task's Cancel | at what is done; the call in flight is told to stop (`cancelled()`) and its answer let be | the plugin | **ok** (R14, P2.2: the work's stop reaches the plugin's file calls, `vfs::stop`) |
| TmpPanel: folders put with their contents (`add_contents`) | "Sending files to temporary panel" (no count yet) | Esc asks it to stop | back to before: the list as it was | a huge tree, a share | **ok** (P2.5) |
| TmpPanel: a list file read, `tmp:<command` | busy message | Esc stops the command and all it started | nothing taken | a share; the command | **ok** (P2.5; R51: an empty file is looked at before it is read) |
| TmpPanel: its panel read again (`UpdateItems`, D22) | the panel's "Reading" | Esc -> Yes (waiting only) | as a plugin's panel | a share | **part**: as a plugin's panel |
| A plugin's configuration dialog | the dialog | its Esc | - | - | **ok** |
| Archives (`archive` on 7-Zip, ADR-0021/0022): open, list, extract, add, delete, test, SFX | as a plugin's panel key, the copy's task | the plugin asked to stop | 7-Zip's callbacks hear the stop; a stopped update discards the new archive, the old kept | 7-Zip, a share | **ok** |
| NetWay (ADR-0023): connect, list, transfers, synchronize, "keep up to date" | as a plugin's panel key, the copy's task | the plugin asked to stop | at what is done | the server, the network | **ok**: its `Runtime::wait` hears `cancelled()` every 50 ms |
| NetWay's transfers in the background (S6.8) | the program's copy window | Background / Pause / Cancel | as a copy | as above | **ok**: the volumes off the UI's thread, through the program's engine |
| Find in archives (S6.7) | Find's results window | Stop, Esc -> Yes | at what is done; a late panel closed unseen | the plugin | **ok**: `provider::wait` polls the stop |

## For the future (owner, 2026-11-13)
Plugins in a process of their own, so that one that hangs can be killed and started again (ADR-0019 §5).
