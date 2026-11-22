# FAR Manager parity - what nova-commander takes, changes or defers

Reference: FAR Manager 3 sources (BSD 3-Clause, `THIRD-PARTY-NOTICES.md`), in `far-source/` and at
github.com/FarGroup/FarManager; read them before any FAR behaviour. See ADR-0004 section 6.
Status: **approved by the owner on 2026-11-07**. Rule: follow FAR as exactly as possible, adapted to the OS
(OS-specific data comes from `src/platform`). Exact option lists, labels and key behaviour are taken from the FAR
sources at implementation time; this table fixes the scope and the deviations.

Decisions: **Take** - as in FAR · **Changed** - deliberate deviation (reason given) · **Later** - deferred, with
the stage or "deferred" when no stage is set yet.
The order of work and its tracking are in `docs/roadmap.md`. In the rows below a stage (2a, 3b, 5c-2 ...)
says where a feature was built (its commits in `git log`), a roadmap step (P3.5, S2d.1 ...) where it will
be. Sections 1-12 go by area; 13 is the plugins, stage 5 step by step.

## 1. Screen layout
| FAR | Decision | Stage |
|---|---|---|
| Two panels over the full height; command line; key bar at the bottom | Take | 2a |
| Top menu bar hidden until F9 (`ShowMenuBar = false`) | Take | 2a |
| Clock in the top-right corner | Take | 2a |
| Key bar labels change while Alt / Ctrl / Shift is held; Alt+Shift, Ctrl+Shift, Ctrl+Alt sets | Take. Risk: needs modifier-only key events (raw console input in `platform` if crossterm does not deliver them) | 2a |
| The cursor: FAR's `CursorSize1` (15% of the cell in insert mode) set as FAR sets it (`SetConsoleCursorInfo`), a line under the character, while the panels and their lines have it; the size the console had while a command or the guest shell fills the console (FAR's `SetInitialCursorType`) and on exit. A terminal that draws its own cursor shape (Windows Terminal's profile `cursorShape`) shows that one, for FAR too; overtype's block (`CursorSize3`, 99%) comes with the editor | Take (owner, 2026-11-10) | 2b |
| Ctrl+O: hide panels, show the user screen | Take (ADR-0006): both panels off, the user screen with the command line under it; again, the panels shown before come back (FAR's `m_StateBeforeHide`). A real shell is on Alt+Shift+T (the guest shell, not in FAR) | 2a |
| Ctrl+Up / Ctrl+Down: panel height decrement (shows terminal lines) | Take, default 0 = one line | 2a |
| A command started from the command line: the panels give way to its output while it runs and come back when it ends; keys go to the running program | Take (ADR-0006: the command is a process of its own, its end is its exit) | 2a |
| Ctrl+F1 / Ctrl+F2 left/right panel on/off, Ctrl+P inactive panel on/off, Ctrl+U swap | Take | 2a |
| Ctrl+Left/Right with panels: resize panels' split | Take | 2a |
| Window title `{path} - Far` | Take as `{path} - Nova Commander`; while a command runs: the command | 2a |
| A mark comes off as soon as its object is done (FAR's `ClearLastGetSelection`), during the operation, also when it runs in the background; what was skipped, failed or not reached stays marked; of several destinations only the last takes marks off. Differs from FAR: a folder with something skipped inside stays marked (FAR unmarks it), and deletion unmarks too (FAR's list just loses what is gone) | Take / Changed (owner) | 3b (done) |
| F3 on a folder (FAR's `CountDirSize`): the marked folders, or else the one under the cursor (on `..` the panel's folder), get their total size in the size column instead of `<Folder>`; the selection's size on the panel adds them; the panel is sorted again at the end; a later reading keeps the size with `~` (FAR). F3 on a file views it, whatever is marked. Links to folders are walked (FAR's `ScanJunction`), the place a link leads to once (FAR counts two links to one folder twice). A count of more than a moment shows FAR's window (the folder, files, bytes), Esc breaks it keeping what was finished, other keys do nothing (owner, 2026-11-08: as FAR, an exception to counting beside the work) | Take / Changed (owner) | 3b (done) |
| Started outside a console (Explorer, a shortcut) where Windows would open a Windows Terminal tab: the program opens in the classic console window instead, without tabs; run from a shell it stays in that shell's window, in a terminal's own profile it stays there (`start` from a classic console gets a new console and opens in the classic window as well) | Changed (owner) | 3b (done) |
| Multiple screens (F12 list, Ctrl+Tab, viewer/editor windows in background) | Take: the Screens list done (2b step 1); done (P4.2): its hotkeys 0-9 / A-Z (Alt with them too) bring a line to the front as FAR's `VMenu`, Ctrl+Tab / Ctrl+Shift+Tab go to the next / previous screen (FAR's `SwitchWindow`: the panels, a task's progress, a search's results, a command in the background), the list says how its tasks are while it is open. Differs: a task that has ended is no screen for Ctrl+Tab (its report is opened from the list, which removes it). Done (S2d.9a, ADR-0010 addendum): viewers and editors are windows as FAR's - behind the window in front, in the list as FAR's `WindowMenu` lines ("View" / "Edit", `*` when modified; ours after the panels, before the tasks, a long path losing its middle) with the cursor on the window in front, F12 and Ctrl+Tab in them, closing one brings back the window in front before it, modal ones (from Find file's results, the user menu's editor, a plugin's file's editor) without F12, Ctrl+Tab and "Screen", quitting asks about each with unsaved changes (FAR's `ExitAll`; ours the viewer's bytes typed too). Done (S2d.9b): F4 on a file an editor has asks FAR's "already loaded. How to open this file?" (Current / New instance / Reload / Cancel; from Find file's results New instance / Cancel) with Confirmations > "Reload edited file" (`confirmations.allowReedit`, on), which off brings that editor unasked unless it has unsaved changes; Alt+F11 in the viewer and the editor (the view and edit history; not in a modal one) | S2d.9 |

## 2. Default palette (16 console colours, `far/palette.cpp`) - Take, stage 2a
FAR's `F_YELLOW` / `F_WHITE` are the bright colours (indices 14 / 15), `LIGHTGRAY` is 7.
| Element | Colour (fg on bg) |
|---|---|
| Panel text, frame, title, scrollbar | light cyan on blue |
| Panel title of the active panel | black on cyan |
| Column titles, panel info text | yellow on blue |
| Selected (marked) files | yellow on blue |
| Cursor / cursor on a selected file | black on cyan / yellow on cyan |
| Total info / selected info on the frame | light cyan on blue / yellow on cyan |
| Pull-down menu text, frame, title | white on cyan; hotkey letter yellow on cyan |
| Menu selected item | white on black; hotkey yellow on black |
| Disabled menu item | dark grey on cyan |
| Top menu bar | black on cyan; selected white on black; hotkeys yellow |
| Dialog text, frame, buttons | black on light grey; hotkeys yellow on light grey |
| Dialog input field / focused button | black on cyan |
| Warning dialog (delete, errors) | white on red; focused button black on light grey |
| Key bar: number / label / background | light grey on black / black on cyan / light grey on black |
| Command line | console default colours |
| Viewer / editor text | light cyan on blue; status line black on cyan; selection black on cyan |
| Help | black on cyan; topics yellow; frame black on cyan |
Differs from FAR (simpler, ADR-0008): colours come from a theme, a JSON file with a palette, about 30 roles
(`panel.cursor`, `dialog.focused`; a role not given falls back to its parent) and `inherits`; FAR's ~150 flat entries
map onto the roles. The built-in `far` theme is this palette; `nc5`, `dn` and `mono` are FAR's own schemes from
`Addons/Colors/Interface`. Options > Themes lists them and the user's files (`settings/themes/*.json`); moving over
one shows it, Enter keeps it (`screen.theme`). Differs from FAR: the program only chooses a theme; FAR's
Options > Colors is not here, themes are made in a separate utility of the package (later).
The console's own 16 colours (FAR's `Classic palette`, `interf.cpp` `SetPalette`, on by default): a colour name is only
a number until the console says what it looks like, and Windows' default is the Campbell scheme. Like FAR the program
sets them at start (`OSC 4`), each theme with its reference (`far`, `mono`: FAR's `DefaultPalette`; `nc5`, `dn`: the
VGA colours of DOS, `docs/themes-reference.md`). Differs from FAR: the console gets its colours back on exit
(its table read at the start and set back with `OSC 4`, then `OSC 104` for Windows Terminal), moving over a theme shows its colours too, and the screen is drawn in the theme's exact colours (`#rrggbb`), as a Windows Terminal tab goes back to its own scheme whenever the keyboard layout changes, by any key and with no program of ours (owner's check, 2026-11-09). Off with `screen.consolePalette`.

## 3. File highlighting (default groups, `far/hilight.cpp`) - Take
| Group | Colour | Stage |
|---|---|---|
| Hidden or system attribute | cyan (cursor: dark grey) | 2a |
| Directory | white | 2a |
| `<exec>`: `*.exe,*.cmd,*.bat,*.com` + `%PATHEXT%` | light green | 2a |
| `<arc>`: FAR's full archive mask list | light magenta | 2a |
| `<temp>`: `*.bak,*.tmp` | brown (dark yellow) | 2a |
| Options > Files highlighting and sort groups (editor dialog) | In the theme utility (ADR-0008, owner 2026-11-11), not in the program | Later |

8b done: the groups are data (`files.highlighting`, FAR's `Highlight`; none kept: FAR's `SetHighlighting`
defaults above, hidden and system as two groups): the first groups, the upper and lower sort groups and the
last ones, each a filter's conditions (masks with the groups of file masks: `<exec>`, `<arc>`, `<temp>` come
from them, made again when they change), the colours of its four states (normal, selected, under the cursor,
selected under the cursor) for the name and for its mark, each one of the sixteen or transparent, the mark
and "Inherit", "Continue processing". An entry goes through them as FAR's `GetHiColor`: each group that takes
it lays its colours over those before it, its mark too unless inherited, and the next groups are looked at
only with "Continue processing"; the panel lays the result over its own colour of the entry's state (FAR's
`GetShowColor`), the whole row, and the mark stands before the name in its own colour (after the
selection's `√ ` of a column that has it) when the name column has room, in the status line too in the
panel's colour (FAR's `Highlight` panel option off: none of it). A group (or a filter) whose mask cannot be
made, such as one of a deleted group of file masks, takes nothing, as FAR's empty `filemasks`. Not taken:
FAR's legacy "black on black" for the palette's colour (a colour here is transparent instead). The editor
is not in the program but in the theme utility (ADR-0008); the sort groups are 8d.

8d done: the sort groups (FAR's `GetGroup`, `list_less`): Shift+F11 and "Use sort groups" in the Ctrl+F12
menu (after the separator, before "Show selected first"; `+` / `-` / Enter as the other options) turn them on
for the panel, saved as `panels.left.sortGroups` / `panels.right.sortGroups` (off, as FAR's). An entry's group is
the first of the upper then the lower sort groups of `files.highlighting` that takes it: the upper ones first,
in their order, then what none takes, then the lower ones; inside a group the mode sorts. Only by name, name only
and extension, after `..`, folders first and selected first; reversing the order keeps the groups' order. The
groups are read again whenever the panel sorts, and when the groups of file masks change.

## 4. Panels
| FAR | Decision | Stage |
|---|---|---|
| Double frame, path centred in the top border, sort-mode letter in the top-left corner | Take | 2a |
| Column titles, columns split by `│`; status line under `╟─╢`; total info and selected info on the bottom border; free space; scrollbar; `..` in root | Take, defaults as in FAR | 2a |
| FAR 3 labels in the size column: `Folder` / `Up` (`<Folder>` / `<  Up  >` when the column is wide enough); aligned extensions; `}` after a cut name | Take | 2a |
| Directories first; hidden/system shown, Ctrl+H toggles | Take | 2a |
| Links (`far/panelmix.cpp` `FormatStr_Size`, `flink.cpp`): a symbolic link or junction to a folder carries the folder attribute and is a folder (sorted, highlighted and entered as one; the path stays the link's path, `..` leads back to it). The size column and the status line show `Symlink` / `Junction`, the tag's name for other Microsoft reparse points (`CLOUD_1`, `APPEXECLINK`, ...) or `?Symlink?`, in `<>` when they fit; a link to a file has no size of its own, a deduplicated or cloud file keeps its size. Changed: a link (symbolic link, junction: a name-surrogate tag) has ` →` after its name in the panel (kept when the name is cut), and the status line spells it out: `name → target` and the whole label (`<Junction>`; FAR cuts it to the six-character size column); short of room, the date and time make way and the target keeps its end (`name → …\links\target`) (FAR can only put a highlighting mark before the name and shows the target in Ctrl+A, delete, Info and Quick view). Not taken: `DetailedJunction` (`Volume` for a volume mount point; off by default in FAR), names of third-party HSM tags | Take | 2a |
| Copy (F5) / move (F6) of links: as links (FAR's default with "Copy contents of symbolic links" off: a folder link through `CreateDirectoryExW` with the link as template, a file link as a symbolic link to the same target); trees are not followed into links. Delete (F8): the link only; the question names its target ("a link to"). "Copy contents of symbolic links" walks a link as what it leads to: the copy is a file or a folder of its own, a move takes only the link away and leaves what it leads to (FAR's `InsideReparsePoint`); a link back to a folder being copied is reported and left out (ADR-0009 §10, FAR has only `ScanTree`'s own guard), one into the folder written to is refused. Not yet: FAR turns the box on by itself for a target without reparse points (FAT) and across network drives | Take | 2a, checkbox 3b step 4b (done) |
| Names taken exactly (FAR's `nt_path`): copy, move, overwrite, F7 and saving never take `a ` for its sibling `a` or `d.` for `d` | Take | 3b step 4 (done) |
| Make links: Alt+F6 (hard link, junction, symbolic link, symbolic link file / directory), F7 with a link type and target | Take, with FAR-style dialogs. Alt+F6 done (3b step 5): FAR's dialog and defaults, nothing on a plugin panel, a hard link to a folder walks it as a copy (folders made, files linked, folder links made again as links), the bytes of a junction as `mklink /J` writes them, a symbolic link's target in FAR's form (`C:\x`); a junction to a drive's root (FAR mounts its volume) comes with F7; ours: a failure asks Retry / Skip / Skip all / Cancel (FAR: OK, the rest stops), an existing file is replaced by a hard link only through a temporary sibling | F7 with a link type 2b |
| Drive menu: the drives are read, and Del, Shift+Del, Ins, Shift+F6 and Enter look at the drive and act, on a worker; FAR's "Waiting for the disk to mount…" is up while Ins or Enter loads the media, or Ins connects the drive, the other work shows nothing, as FAR's; keys pressed meanwhile come after it, in order. Enter on the panel's own folder reads it again with the cursor and selection kept (FAR's `Update(UPDATE_KEEP_SELECTION)`). A drive is entered, from the menu or by `D:` on the command line, at the folder the active panel last had on it, however it left (FAR's per-drive `=D:` folder), or at its root when that folder is gone | Take | 3c (done) |
| Drive menu: a volume that arrives or leaves (a stick plugged in or pulled, a disc put in or taken out) reads the open menu again at the same line, as FAR's `update_devices` does Ctrl+R (one that comes while other work runs, after it); ours: a panel on that drive reads its folder again (FAR's panels do not listen), one on a drive that left shows the error | Take / Changed (ADR-0012) | 3c (done) |
| Drive menu: an optical drive is named by what it can do (CD-ROM, CD-RW, CD-RW/DVD, DVD-ROM, DVD-RW, DVD-RAM, BD-ROM, BD-R, BD-RE, HDDVD-ROM/-R/-RW/-RAM) with the type column and Ctrl+8 "Show CD drive parameters", else "CD-ROM", as FAR's `get_cdrom_type`: SCSI GET CONFIGURATION profiles, then MODE SENSE's capabilities page, then the words in the product name | Take | 3c (done) |
| Drive menu: virtual disks (FAR's `GetVHDInfo`, `remove_virtual`): with F9 "Detect virtual disks" and the type or path column, an attached VHD / VHDX is "virtual" (a mounted ISO is named as any CD drive, "DVD-ROM") and Ctrl+2 shows its file; Del asks "Virtual disk detaching" and detaches it, whatever the options; Shift+Del detaches it without asking, as FAR's `RemoveHotplugDrive`; a failure says "Cannot detach a virtual disk" with Retry / Cancel (ours: FAR's Shift+Del is silent when the detaching fails). Not yet: FAR's elevation when detaching needs an administrator, the DetachVHD confirmation switch (later) | Take / Changed (Shift+Del failure shown) | 3c (done) |
| Drive menu (Alt+F1 / Alt+F2): the volume label column is on by default (FAR's default `DriveMenuMode` leaves `DRIVE_SHOW_LABEL` off); Ctrl+3 and F9 switch it as in FAR | Changed (owner, 2026-11-09) | 3c |
| File attributes: Ctrl+A on a shown panel, Files > File attributes, F4 in the drive menu (`far/setattr.cpp`) | Take. Done (3b step 5b, ADR-0011 steps 1 and 2): FAR's "Attributes" dialog with its items at FAR's places (the name or "hard links (N)" with the names, Junction / Symlink / Volume and the target, another reparse point by its tag's name), the ten boxes and "More" with the other ten (a reparse point may only be taken off), three-state boxes for a selection, compressed and encrypted never both, the four times to a ten-millionth of a second in the user's date order with masked fields, UTC, Original / Current / Blank, `*` gives a time line to the others, a blank part keeps the original's, the owner (`DOMAIN\name` or a SID); Set changes in FAR's order (owner, compression, encryption, sparse, reparse point, attributes, times), each failure asks Retry / Skip / Skip all / Cancel and Skip goes on with the next change; the selection is taken off as each object is done (Ctrl+M brings it back); Set runs as a task with FAR's "Setting file attributes for" progress (3b step 5c), "Process subfolders" (when a folder is among the objects) turns the boxes to three states and a folder's times and owner to blank, what was changed by hand stays, and Set goes through everything inside, into links to folders once per place. Done (P4.3): a symbolic link's or a junction's target is edited there (FAR's `ModifyReparsePoint`: unquoted, environment variables expanded, a junction's relative target counted from the link's folder, the reparse point replaced in one call; a target differing only by case is no change), an App Alias ("App Alias:"), a WSL link ("Symlink LX:"), an NFS link and a container's link show their targets read only, System properties opens Windows' own properties window (one object only), the drive menu's F4 shows the drive's "Device:" (`\Device\HarddiskVolume3`) under its volume, a click on a time's label goes to its date and a double click puts now into its line, and a change refused for want of rights asks "Access denied" (with FAR's reasons: set attributes, set owner, encrypt / decrypt, open) and goes through the helper. Not yet: DFS targets (FAR's list from `NetDfsGetClientInfo`; no DFS namespace here to test against) | Unix with the port |
| Access denied: FAR offers to retry with administrator rights (`elevation.cpp`: "You need to provide administrator permission to move this object", Do this for all current objects) for the same operations as FAR | Take - ADR-0007. Done (3d steps 3-4): when an operation fails for want of rights (access denied, or a missing privilege for a symbolic link) and the program is not elevated, FAR's "Access denied" asks: Ok goes through a helper, Skip leaves the object to the usual error question, Esc stops; "Do this for all current objects" (on) and "Do not ask again in the current session". The helper is this program with `--elevated`, started with `runas` on the first Ok, one per session, ended with the program; a named pipe of a random name for Administrators only, each end checks the other's process, as FAR; ours besides: the helper's end of the pipe allows no impersonation, and it takes only full paths of files and folders (no `..`, no devices in any form), never a drive root. A prompt refused, or a helper that cannot start, leaves the object's own error and the rest of that operation is not asked about (FAR asks again). Copy, move, delete, the Recycle Bin, F7, links, a target's read-only attribute; Ctrl+A (P4.3: each change of Set and a link's new target, the helper making the whole change, since it passes no handles back). Not yet: the times and rights of a copy made through it, Append to a protected file, F7's boxes, reading through the helper and its settings (later); the editor's save (S2d.4) | Take - done for what exists | 3d |
| Read-only files: FAR asks before deleting one ("The file is read only" / "Do you wish to delete it?", `MAskDeleteRO`: Delete / All / Skip / Skip all / Cancel; Confirmations > "Delete read-only files", `confirmations.readOnly`) and before overwriting one ("Also ask on R/O files" in the copy dialog) | Take | delete 3b (done), copy 2b |
| Delete (`far/delete.cpp`): F8 to the Recycle Bin (FAR's default `DeleteToRecycleBin`, `system.deleteToRecycleBin`), Shift+F8 the file under the cursor whatever is selected, Shift+Del at once past the Recycle Bin; a held key deletes once (`IsRepeatedKey`); `..` and a root never. FAR's question first ("Do you wish to move the file / folder / objects to the Recycle Bin?" with Move, "Do you wish to delete ...?" with Delete in warning colours; one object by name, a link with "a link to" its target, several as a list of `delete.showSelected` names, 10, then "…and N more"; `confirmations.delete`); where the drive has no Recycle Bin (a local fixed drive whose bin is not turned off by its properties or by policy) or where the shell would not recycle the first object (FAR's `can_recycle` probe: an `IFileOperation` deletion aborted in its `PreDeleteItem`, e.g. a file bigger than the bin; review R1) the question says delete. Then "Delete folder" for a folder that is not empty (Delete / All / Skip / Cancel; at every level past the Recycle Bin; `confirmations.deleteFolder`), read-only files, "Cannot delete the file / folder" (Retry / Skip / Skip all / Cancel) and "Cannot move ... to the Recycle Bin" / "Try to delete it permanently?" (Delete / Skip / Skip all / Cancel), the system's error text last. The Recycle Bin is `SHFileOperationW` with undo and no UI, one call per selected object; links go as links. Names are taken exactly (`\\?\`, FAR's `nt_path`): `a ` is never its sibling `a`; the Recycle Bin cannot take such a name, so it gets "Try to delete it permanently?". The "Deleting" progress: the object and the count, the total bar with `delete.showTotal` (off, FAR's `DelShowTotal`). The hard-link question belongs to the wipe (Alt+Del), deferred with it. Changed (owner): the progress has Background / Pause / Cancel and the deletion is a task in F12 and the per-volume queue as F5/F6 (ADR-0010; FAR deletes in the foreground only); plain Del deletes as F8 when the command line is empty (FAR's Del only edits the command line). Changed (owner): the question about one file gives its size and last write time under the name (as Explorer and Total Commander); the count of `delete.showTotal` runs beside the deletion, not before it, and is on by default (FAR's is off); the question about a folder or several objects says how many files and folders they hold (below). Changed: an object already gone is no error, and a folder that keeps what the user skipped inside stays without one more "Cannot delete the folder" (FAR shows both). Done (P4.1): the names in the question in the highlight colour (FAR's `DelHighlightSelected`; one name only when it is not the one under the cursor), at most half the screen of them; FAR's switch for it (no dialog, on) is not kept: always on | Take, changed as noted | 3b (done) |
| Counting ahead of an operation (owner's rule): a count never holds the operation up; it runs beside it, the totals grow as it goes and a total still being counted shows `…` (`Files: 120 / 340…`), and instead of a percent of a total that keeps growing the total bar shows a block going to and fro, the task line says `Copy …`, the console title has no `{35%}` and the taskbar button is indeterminate. Copy and move (ADR-0009 §3) and delete do so; the rule holds for every count to come (the question's object counts, find; F3's folder sizes are FAR's window, by the owner's later choice) | Changed (owner) | 3b (done) |
| The question before deleting a folder or several objects also says how many files and folders they hold, the selected ones included, and their size as people read it ("20 000 files and 101 folders, 1,25 GB": Total Commander's counts, Explorer's size; links add no size); counted beside the open question (the rule above: "Counting…", then the numbers growing with `…`), Delete works before the count ends and the deletion's progress goes on with the same count (past the Recycle Bin: `Files: done / every object`). To the Recycle Bin the count is finished before the first object moves (a folder held open inside cannot be renamed, which the bin does: NTFS answers access denied) and the progress goes by the selected objects, each taken in one step | Changed (owner) | 3b (done) |
| The status row of tasks (ADR-0010) shows only while a task is in the background: the operation in front has its dialog, so starting one does not move the panels | Changed (owner) | 3b (done) |
| Copy (F5) / move (F6) / link (Alt+F6) "Use filter" (`far/copy.cpp` `m_UseFilter`): off at each dialog, disabled for a single file; Filter (enabled with the box) opens the filters menu of Copy's own marks with the active panel's file types, the dialog coming back after it. The operation leaves out what Copy's filter does not take (FAR's `FileInFilter`, `COPY_FILTERED`): a selected item stays where it is and selected, a folder is left out with all in it, a file inside is not copied; a move walks its folders instead of renaming them whole (FAR's `m_UseFilter \|\| !SameDisk`) and a moved folder that keeps what was left out stays, no error; the totals count only what goes (`GETDIRINFO_USEFILTER`); a junction or a symbolic link (Alt+F6) is made for the item whatever the filter says (FAR's `MkSymLink` before the copy), hard links walk through it; "Retry skipped" keeps the filter | Take | 2b step 8a-2b (done) |
| Copy (F5) / move (F6) progress time (`far/copy_progress.cpp` `UpdateTime`): "Time", "Remaining" and the speed go by the time the operation worked, the time it waited for the user left out (FAR's `WaitUserTime`: a question, "Operation has been interrupted"); ours besides: Pause stops the clock too. Done (P4.2). A task that ended cleanly leaves the Screens list after System settings' "Keep finished tasks, seconds" (`tasks.keepFinishedSeconds`, 30; ADR-0010, FAR has no tasks) | Take, ours as noted | P4.2 (done) |
| Copy (F5) / move (F6) keep (`far/copy.cpp`, ADR-0009 §9): "Preserve all timestamps" sets creation, last access, last write and change time; a folder made gets its times at the end of the operation, after what went into it (`m_CreatedFolders`); without the box a file keeps its last write time and a folder is new. "Access rights": Default (a new file gets what the target folder gives, a renamed one keeps its own), Copy (the source's DACL), Inherit (for what goes by a rename: only what the target folder passes on, `reset_security`). A new folder takes the read-only, hidden, system and archive attributes of its source. Changed: what the target cannot keep is one line per item in the final report and the copy stands (FAR asks Retry / Skip after each); the rights are written after the file is opened for its times, so rights that shut the user out still leave the times right. Through a provider the last write time is given when the provider can take it (`Provider::set_times`), all four with the box, and the read-only, hidden and system marks (P6.0d: a plugin's `set_times`, FAR's `PutFiles` items; tech-debt P6.0d for what differs) | Take, changed as noted | 3b step 4b (done) |
| Verify after copy (owner, from Total Commander / TeraCopy): a "Verify" check box in the F5/F6 dialog, off by default; each copied file is read back and compared with its source; a mismatch asks Retry / Skip / Cancel and the copy never stays under the real name; a move deletes the source only after the check passed. On a disk the copy is read past the file cache; alternate data streams are not compared | Changed (owner) | 3b step 4b (done) |
| Retry what was skipped (owner, from TeraCopy): the report of a copy or move that skipped objects after errors has "Retry skipped": a new task with the same settings for those objects only, each to the place it was going to; a move takes the folders it emptied away. Refusals and the user's own Skip are not retried | Changed (owner) | 3b step 4b (done) |
| A file held by another program: FAR's error names who holds it (`OperationFailed`: `IFileIsInUse` - the program, "opened / edited / played", Switch to / Close buttons; else Restart Manager - up to 5 processes, "…and N more", why they cannot be closed). Done (3d step 5): the error questions of copy, move and delete say "Object is being opened in:", up to five processes as `name (PID 7, application, [service])`, "and N more processes." and why, from the Restart Manager, asked on the worker. Not yet: `IFileIsInUse` with "edited / played" and its Switch to / Close buttons; FAR's fallback by `FILE_PROCESS_IDS_USING_FILE_INFORMATION` | Take - done but for `IFileIsInUse` | 3d |
| Ctrl+1 Brief, Ctrl+2 Medium (**default**), Ctrl+3 Full, Ctrl+4 Wide, Ctrl+5 Detailed (full screen: size, allocated size, write/creation/access time, attributes), Ctrl+0 Alternative full | Take | 2a |
| Ctrl+6 Descriptions, Ctrl+7 Long descriptions, Ctrl+8 File owners, Ctrl+9 File links | Later (deferred by the owner) | - |
| Options > File panel modes (edit modes and columns) | done (2b step 9) | 2b |
| Sort: Ctrl+F3 name, Ctrl+F4 extension, Ctrl+F5 write time, Ctrl+F6 size, Ctrl+F7 unsorted, Ctrl+F8 creation, Ctrl+F9 access; repeat = reverse; Ctrl+F12 sort menu (directories first, selected first, sort groups) | Take | 2a |
| Ctrl+F10 descriptions, Ctrl+F11 owner sort | Ctrl+F10 done (P6.0b): by the entries' descriptions, a plugin's `Z` (a disk's have none until the descriptions, 12); Ctrl+F11 later (with owners) | P6.0b / - |
| Ctrl+N long/short names | Later (deferred by the owner) | - |
| Ctrl+R re-read; automatic refresh on changes (`filesystemwatcher.cpp`, `FileList::UpdateIfChanged`): the panel's folder (not its subfolders) is watched for every kind of change; a change is shown at most once a second, cursor and selection kept, while the panels are on screen (a dialog, menu, viewer, editor or running command postpones it, as a hidden panel does); a folder that is gone takes the panel to the nearest existing one above (`SetCurPath`), Ctrl+R restarts a watch that ended. Settings `panels.autoUpdateLimit` ("Disable automatic panel update if object count exceeds", 0) and `panels.autoUpdateRemoteDrives` ("Network drives autorefresh", on); their dialog items come with 2b. Changed: a deleted watched folder is reported as a change (FAR stops watching and leaves the panel until a key) | Take | Ctrl+R 2a, refresh 3b (done), dialog 2b |
| Reading a folder: "Reading: N files" in the panel title, Esc asks whether to stop and keeps what was read (`filelist.cpp`) | Take; the folder is read on a worker thread (ADR-0013), the keys work on the shown folder meanwhile. Differs (P3.1, R23): a folder read again (Ctrl+R, a change, Ctrl+H, a filter) and stopped keeps the list it had, put through the hidden files' switch and the filter as they are now, its selection whole (FAR shows the part read) | 3b (done) |
| Ctrl+I file panel filter | Take - done (2b step 6b, below) | 2b |
| Ctrl+L Info panel: computer and user name, disk (label, file system, size, free, serial), memory, power status, folder description file (`DirInfo`, `ReadMe.*`, ...), plugin info | Take. Done (3c step 5): Ctrl+L turns the passive panel into FAR's "Information" panel and back (again on either side; a drive chosen for its side, Alt+F1/F2, brings the files back as FAR's `ChangeDisk`); Tab goes to it, where the file keys do nothing, the command line works in the other panel's folder, Ctrl+R reads it again and Ctrl+F12 opens "Show Info" (+ unfolds, - folds, Enter and * switch; `[-]` / `[+]` on each section's title); Ctrl+U takes it along. Lines as FAR: computer name and description, user name, description, access level, Elevated (Limited / Full); the disk of the other panel's folder titled "Fixed disk C:\ (NTFS)" with the SUBST folder, share or image file, Space total / available / used, volume label, serial number; memory Committable / Addressable / Physical and installed; AC power, battery charge, life and full time. Ours: a "Device" line (model and bus, owner 2026-11-09), the power section on by default (FAR's `ShowPowerStatus` is off; owner), read again every 3 s while shown (FAR reads on each redraw). The folder description file and its F3/F4: done (2c step 4). Its settings (P3.5): "Show CD drive parameters", the computer's and the user's name formats (FAR's `InfoComputerNameFormat`, `InfoUserNameFormat`), read again as they change; the panel type kept over a restart for every side (FAR's `Panel.*.Type`: files, tree, info, quick view); read again as the folder beside it changes (the other panel's list read again); F5 / F6 with it passive offer the active panel's folder (its `m_CurDir`). A plugin's lines (P6.0b): beside a plugin's panel the "Plugin" section with its `InfoLines` (a separator's title in its middle) instead of the description, "Show Info"'s "Plu&gin panel" item folding it (`infoPanel.plugin`) | Take / Changed (owner) | 3c (done; description file 2c step 4, done; plugin info P6.0b, done) |
| Ctrl+Q Quick view in the other panel (text, folder size and counts) | Take (ADR-0015, owner: files of any size never read whole; the folder counted in the background). Panel and folders done (2c step 3a); a file's contents with the view core done (2c step 3b); its F2 / F4 / F7 / F8 with the viewer (2d) | 2c |
| Ctrl+T Tree panel; Alt+F10 find folder | Tree panel: 2c step 1 (done); Alt+F10: 2c step 2 (done: 2a from the panels, 2b F10 / Alt+F10 / Shift+F10 in the copy, move and link dialogs) | 2c |

9a done: the modes are data (`panels.modes`, FAR's `ViewModes`; none kept: FAR's `InitialModes`, so nothing
looks different): Brief, Medium, Full, Wide, Detailed and Alternative (FAR's 6-9 come with descriptions,
owners and links, owner 2026-11-11), each with its columns and its status line's written as FAR writes them
(types `N,S,D,T`, widths `0,6,0,5`: 0 the type's default or a share of the rest, `40%` a part of what the
fixed ones leave, as FAR's `PrepareColumnWidths`), its name and FAR's six flags: full screen, align file
extensions, align folder extensions, folders in capitals, files in small letters, a name all in capitals in
small letters (the cases in the panel only, not in the status line). The types FAR reads, with their
modifiers: `N` (`M` the selection's mark, `D` it takes room only when selected, `R` long names keep their
end, `F` always to the right, `N` without the extension, `O` the last name only), `S` and `P` (`C` digit
groups, `E` no space before the unit, `F` "1.21 M", `T` units of 1000 in small letters), `D`, `T` (6 wide:
`a`/`p` on twelve hours, more than 8: parts of a second), `DM`, `DC`, `DA`, `DE` (`B` brief: day and month,
the year instead of the time when older than six months; `M` the month's short name in the user's
language), `A`, `X` (the extension, `R` to the right), `C0`..`C99` (a plugin's texts, 5c-3) and `Z` (a
plugin's descriptions, 5c-3; a disk's files have none yet). An unknown type is left out, as FAR's; `O`,
`LN`, `F`, `G` and plugins' `<...>` keep their width and are not drawn yet (tech-debt).

9b done: Options > File panel modes (FAR's `SetFilePanelModes`, "Edit panel modes"): the six modes in FAR's
order by their names (the mode's own when it has one) with their hotkeys, the user's own after a separator
(`panels.modes.custom`, a panel showing one saved as `custom-1`...), the cursor on the active panel's mode;
Enter, F4 or a hotkey opens the mode's dialog (FAR's 76 by 19: Name, Column types / widths, Status line column
types / widths, the six flags, OK, Reset, Cancel), Ins a new mode's (before the user's mode under the cursor,
else the first of them; Reset there gives the FAR mode under the cursor, as FAR's), Del takes a user's mode away
at once (a panel showing it shows the mode before it, the others keep theirs; after Ins too a panel keeps its
user's mode, where FAR's would show the one now at its number), Ctrl+Enter shows the mode under the cursor in
the active panel and Ctrl+Shift+Enter in the passive one (an info panel keeps it for its files panel), the list
staying; Reset puts FAR's own mode back (disabled for the user's). A saved user's mode that is not there is
Medium.

2c step 1a done: the tree panel (FAR's `TreeList`), on by default (FAR 3 turns it off completely by
default; owner 2026-11-11: on here, `panels.tree.turnOffCompletely` turns it off as FAR's). Ctrl+T (FAR's
`FilePanels::ProcessKey`: the tree on the active side, or on the passive one, becomes files; otherwise the
passive panel becomes the tree, an info panel there giving way) and Left / Right > "Tree panel". It shows
every folder of the disk of the other panel's folder, read on a worker thread (`jobs::tree`, links to
folders walked into as FAR's default "Scan symbolic links", never into a folder they are inside of) with
FAR's "Tree" message and its count after a moment; keys wait meanwhile, Esc asks "Do you really want to
cancel it?" and Yes keeps what was read. FAR's lines (`│ `, `├─`, `└─`, the root `\`), the other panel's
folder in the selected colour, the cursor in its colour with the focus and in brackets without it, the
separator and the cursor folder's path. The cursor follows the other panel's folder (FAR's `SyncDir`; a
folder not in the tree is read and put in with the folders above it, another disk is read whole). Keys on
the tree: the cursor keys, Home, End, PgUp, PgDn, Gray + / Gray - (the next or previous folder of the same
depth), Enter (the other panel goes there; a folder that is gone leaves the tree with what was in it),
Ctrl+R (read again), Tab, Ctrl+U (it goes with its side); `panels.tree.autoChangeFolder` (FAR's "Auto
change folder", off) makes each move an Enter; with a command typed Enter runs it. A view mode chosen for
its side (Ctrl+0..9 on it too, as on the info panel: FAR's `ChangePanelViewMode`), a drive chosen for it
or the info panel there gives the tree's place up; a hidden tree follows nothing until it shows (FAR's
`Update`), and closing it stops its reading. The
folders are in the order of their names level by level (FAR sorts the whole paths with its name order,
which comes to the same for the names that do not hold characters sorted before `\`).

2c step 1b done (reading reworked by ADR-0014, below): the trees' files (FAR's `tree3.cache`: `TreeList::ReadTreeFile`, `SaveTreeFile`,
`AddTreeName`, `DelTreeName`). One file per volume, named by its label, serial number, file system and,
for a network disk, its share, in `settings\TreeCache` (owner 2026-11-11; FAR 3 writes `tree3.far` in the
disk's root and caches only removable disks and CDs), FAR's lines (`\a\b`) in UTF-8. The tree is read from
its disk's file when there is one (FAR's `Update`) and the disk is read only without one; a whole reading
that was not stopped is saved unless the tree with its root has fewer folders than "Minimum number of
folders" (FAR's `MinTreeCount`, 4); Ctrl+R reads the disk again and saves it. A file that cannot be written
is taken away with FAR's "Cannot save folders tree to file". A folder read in for the other panel goes into
the file, with the folders above it. F7, F5, F6, Alt+F6 and F8 put what they did into the file of a disk
that has one, at their end on their worker (FAR does it as each folder is made or removed): the folders
made (a link alone, not what it leads to), a move's or a deletion's folders that are gone (of a folder left
half deleted, what is still there stays); a renamed folder keeps the folders in it (FAR's `RenTreeName`
keeps it alone until it is read in). After an operation or F7 the tree is read from its file again. Enter
on a folder that is gone takes it out of the file too. Options > "Tree settings" as FAR 3 builds it (no
`TREEFILE_PROJECT`): "Auto change folder" and "Minimum number of folders"; with
`panels.tree.turnOffCompletely` it and the Left / Right "Tree panel" are hidden (FAR's `LIF_HIDDEN`).

Changed (owner 2026-11-11, ADR-0014): the tree is not read behind FAR's modal "Tree" message. Ctrl+T shows
it at once from its disk's file (or the root alone) and its worker reads in the background, the separator
saying "Reading: N" meanwhile, the keys working all along: first the folder under the cursor when its
folders are not known yet, then the folders from the root to the other panel's folder (the cursor goes
there once it is in) and that folder's subtree, then the rest of the disk to its end. The file keeps each
folder with its last write time when it was read; a folder whose time is the same is not listed again, so
a disk read once shows at once and only what changed is listed (also what other programs did). What was
read is saved as the worker goes and when the tree closes, so a reading left halfway goes on from there.
Ctrl+R lists every folder again with the tree in use; after an operation or F7 the panels' folders and
the operation's target are listed first; Esc no longer stops anything (closing the tree does). A folder
that cannot be looked at for a moment (no rights, a device not ready) keeps what was known of it.

2c step 1c1 done: the operations on the tree (FAR's `TreeList::ProcessKey`, `GetSelName`) take the folder
under its cursor as the key is pressed: F5, F6 and Alt+F6 copy, move or link it to the other panel's
folder (FAR's dialogs naming it by its whole path, cut at the line's end; the other panel's folder in the
line, the folder's own path when the other panel is in it), Shift+F5 / Shift+F6 with the folder's own path
in the line (FAR's `CurrentOnly`); a target written without its folder goes beside it (FAR's `ShellCopy`
changes to the folder that holds it); F7 makes a folder in it, the cursor staying; F8 and Shift+Del ask to
delete it (to the Recycle Bin by its disk; held, once); F4 and Ctrl+A give its attributes, the root's too.
The menus' Files items do the same with the tree active. A hidden tree (Ctrl+O) takes no key, as FAR's. The
files panel under the tree keeps its selection. On the root F5, F6, Alt+F6,
Shift+F5, Shift+F6, F8 and Shift+Del do nothing (FAR would take the disk's root). The tree gets what was done
when the operation ends (its target and the panels' folders listed again, ADR-0014).

2c step 1c2 done: FAR's quick search on the tree (`FastFind`, `FindPartName`): Alt+letter opens the files'
"Search" box, the cursor goes to the first folder whose own name starts with the text (wildcards too), from
the cursor on, then from the top; Ctrl+Enter / Ctrl+Shift+Enter the next / previous; Enter closing it takes
the other panel there, and so does "Auto change folder" whatever key closed it. FAR's mouse on the tree
(`ProcessMouse`): a click on a line takes the keys and the cursor there, a double click or the right button is
Enter, as any move with "Auto change folder"; a button held over the frame above or the separator below
(pressed there or brought there) moves up or down as a key held (then Enter with "Auto change folder"); the
wheel scrolls the list and the cursor (by one with Alt); a folder dragged to the other panel asks to copy it
(Shift or the right button: move; the root is not dragged), the cursor going back to it when the drag's note
shows. Ctrl+Enter and Ctrl+F put the folder's whole path in the command line (quoted, a space after it),
Ctrl+Alt+Ins on the clipboard, Shift+Enter opens it in the system's file manager, Ctrl+\ takes the cursor and
the other panel to the root, Ctrl+Shift+0..9 / Alt+Shift+0..9 keep it as a folder shortcut, as the shortcut
menu's Ins does (RCtrl+0..9 takes the other panel to one, as FAR's). FAR's `OnFocusChange`: the tree losing
the keys on a folder that is gone lets it go (its folder listed again).

2c step 2a done: Alt+F10 and Commands > "Find folder", FAR's `FolderTree` (a modal `TreeList`,
`MODALTREE_ACTIVE`): the tree of the active panel's disk (the tree's folder when the tree is active) in a
window from the fifth column and third row to four from the screen's edges, with a shadow, " Find folder "
on its frame and its own key bar (Help, Rescan, Zoom, Quit); read as the tree panel's (ADR-0014: from the
disk's file at once, the rest in the background, "Reading" on the separator), the cursor on the active
panel's folder once it is in. The separator is a row higher than the panel's and "Search:" with its edit
field (to the window's 26th column) is under it, the folder's path on the last row. Typed text (Alt+letter
as the letter, FAR's) goes to the search line and takes the cursor to the first folder whose own name
starts with it from the cursor on, the line keeping the last text that found one (FAR's `m_LastName`,
kept when the line is cleared); the line's editing keys work there; Ctrl+Enter / Ctrl+Shift+Enter the
next / previous; the cursor keys and the wheel clear it and move as on the tree panel (Gray + / - move
and keep it); the window's keys are taken only as they are (Alt+F10, Shift+F5 and the like go to the
search line, as FAR's). Enter on a folder that is there closes the window
and the active panel goes there (with the tree or the info panel active, the files panel beside it: FAR's
`SetCurDir` of the tree and its Enter); one that is gone leaves the tree. F2 and Ctrl+R read the disk
again, F5 fills the screen (no shadow, no key bar) or not, Esc and F10 close it. The mouse: the key bar
first (its keys pressed as FAR's), a button on a line takes the cursor there, a double click is Enter, a
click outside the window is Esc (left) or Enter (right), the wheel scrolls. Closing a tree (the panel or
the window) does not wait for its file to be written (ADR-0014). Checked end to end on drive E: (owner,
2026-11-11). With `panels.tree.turnOffCompletely` there is no Find folder (FAR's
`TurnOffCompletely`).

2c step 2b done: the tree of the copy, move and link dialogs (FAR's `copy.cpp` `DM_CALLTREE`). F10 and
the "F10-Tree" button open Find folder on the active panel's folder (the tree's cursor when the tree is
active; `MODALTREE_ACTIVE`), Alt+F10 on the passive panel's (the active one's when it is hidden;
`MODALTREE_PASSIVE`), Shift+F10 on the first folder of the target line (the first of several
destinations when they are on; `MODALTREE_FREE`: the tree of that folder's disk, read and kept as any;
ours: a folder written without its own folder taken where the operation would put it, FAR reads it as it
is; a folder not made yet leaves the cursor on the root, as FAR's `SetDirPosition` failing). The window stands from a third of the screen to the eighth column from its right edge (FAR's
`SetCoords` of a dialog's tree), the dialog drawn under it. Enter on a folder that is there closes it and
the folder goes to the target line with a slash at its end, the line taking the focus; with "Process
multiple destinations" on it is added after a `;` (quoted when it has `,` or `;`). Esc and F10 give the
dialog back as it was. An empty line (Shift+F10), a panel off the disks or a disk that is not there opens
nothing. The keys are taken only as they are (Ctrl+F10 is not the tree); an open drop-down list keeps
them. With `panels.tree.turnOffCompletely` the button is hidden and the keys do nothing.

2c step 3a done: Ctrl+Q and the Left / Right menus' "Quick view", FAR's quick view panel (`qview.cpp`). Ctrl+Q
with the active panel shown turns the passive panel into "Quick view" and back (again on either side), the
menu's item on its side; the info panel, the tree, a view mode or a drive chosen for that side take its place
and it takes theirs; Ctrl+U takes it along; a hidden one follows nothing. It shows the item under the cursor of
the panel beside it (the panel's folder for `..`, the folder under a tree's cursor): its name on the last row,
a file's type on the separator in the selected-info colour (the system's name of it, found as FAR finds it;
ours: a Store application's type without a plain name gives its friendly name, Notepad's for `.txt`). A
folder: `Folder "path"` (cut keeping its root and end), the link it is (`Junction "..."`, `Symlink "..."`,
`Volume "..."`, `Reparse point NAME`, `Unknown reparse point`), then FAR's rows Folders, Files, Files size,
Allocated size (% of the bytes) and, on a disk, Cluster size, Files slack and MFT overhead (% of the
allocated) after a blank row. Ours (ADR-0015): the count runs on a thread of its own, the numbers growing
marked `~` in FAR's highlight colour (`panel.highlight`) until it ends, the keys working (FAR counts behind a
modal window, `~` after Esc); another item under the cursor stops it, the same item keeps it; Ctrl+R counts
again (FAR keeps what it counted). The space a file takes comes from its folder's listing. On the quick
view F3 and Num5 (`VK_CLEAR`, now decoded as FAR's `KEY_NUMPAD5`) view the other panel's file by its
associations or count its folder there, the quick view keeping the keys; Gray + / - move its cursor; the
file keys do nothing; the mouse takes the keys there. The menus' symbols (`!.!`, `!\`, ...) of a quick
view are its folder and file (FAR's `QuickView::GetCurDir` and `GetCurName`: the other panel's folder, a
tree's the folder under its cursor; the whole path of the item shown), so an association chosen from
"Select association" runs on the file shown.

2c step 3b done (ADR-0015, owner): a file's contents on the quick view, FAR's `Viewer` in its box from the
first row to the separator, in the viewer's colours. The view core (`editor::view`) never reads a file whole:
positions are byte offsets, blocks of 64 KiB are read by offset on a thread of their own (owner: the screen
never waits for the disk; the rows come as their blocks do) and a bounded number kept; only the rows on screen
are laid out. The code page is told from the first block (FAR's `GetFileFormat`: a byte order mark, UTF-8
that decodes, UTF-16 by its zero bytes, else the system's ANSI page; told again as a file grows while all of
it is a short first block of plain ASCII, as a log started empty); lines end at `\r\n`, `\n`, `\r` or
after FAR's longest line (10 000 characters); tabs go to stops of 8; control characters show as FAR's OEM
glyphs, 0 as a space (owner). Wrapped, as FAR's default (owner). On an active quick view FAR's viewer keys:
Up / Down a row (Down stops with the end on screen, FAR's `LastPage`), PgUp a screen less a row, PgDn the
last row shown to the top, Home / Ctrl+Home / Ctrl+PgUp the start, End / Ctrl+End / Ctrl+PgDn the last
screen (an empty last row after a final break, as FAR's), the wheel a row per line of the system's setting;
unwrapped (2d) Left / Right a column, Ctrl+Shift+Left the first. Ctrl+Left / Right and Ctrl+Up / Down stay
the panels'. A file open there may go on being written, renamed or deleted; one that grows while its end is
shown shows what was added (the length looked at every second), one that shrank starts again; one that
cannot be read says why. The quick view's own key bar (FAR's `MQViewF*`): a file's F2 "Unwrap" and F8 the
code page F8 would switch to (FAR's `NextCPname`: ANSI, OEM), a folder's F2 "UserMn" and Alt+F8 "Histry";
F2 / F4 / F7 / F8 themselves come with the viewer (2d).

2c step 4 done: the info panel's folder description (FAR's `InfoList::ShowDirDescription`). For a folder on a
disk the last section, "Description", shows the first file in the other panel's folder that the names of
Options > "Folder description files" find, in their order (FAR's default `DirInfo,File_Id.diz,Descript.ion,
ReadMe.*,Read.Me`; `,` or `;` between them, quotes keeping either in a name; wildcards as the system matches
them; folders passed over), looked for with the panel's other facts on its thread (again when the other panel
goes to another folder, on Ctrl+R, every few seconds, and when the names change). Shown: the file's name as
the section's title and its contents to the panel's bottom by the view core (2c step 3b: never read whole,
wrapped, the code page told from its first block); none: FAR's "Folder description file is absent". On the
active info panel the viewer's keys move it, the wheel too, F3 / Num5 view it, F4 edits it (with none a new
file of the first name without wildcards, `DirInfo`, made when saved); the mouse's left button on it is F3,
the right one F4. Ctrl+F12's "Show Info" has FAR's "Description" between Memory and Power status, kept with
the other sections (`infoPanel.description`); the names are `infoPanel.descriptionFiles`. The info panel's
own key bar (FAR's `MInfoF*`: F3 VieDiz, F4 EdtDiz, Ctrl+F12 Mode; with a file the viewer's F2 Unwrap and
F8 code page; ours: F4 keeps "EdtDiz" where FAR's viewer writes "Hex" over it though F4 edits).

## 5. Selection - Take, stage 2a
Known limitation: crossterm reports the keypad's `+ - *` like the main keyboard's, so the gray keys are
recognised by asking the key state while the press is handled (`platform::is_gray_key_down`). If the program is
busy and the key is already released, the character goes to the command line instead. A full fix reads console
input in `platform`.
Ins, Shift+arrows / PgUp / PgDn / Home / End; Gray `+` / `-` select / unselect group (mask dialog), Gray `*`
invert; Ctrl+M restore selection; Ctrl+Gray `+`/`-` by extension, Alt+Gray `+`/`-` by name; Shift+Gray `+`/`-`
all; folder selection rule (`SelectFolders`) as in FAR.

## 6. Mouse - Take, stage 3e
As in FAR: left click moves the cursor and activates the panel, right click shows Explorer's menu when EMenu is
loaded and Panel settings' "Right click selects files" is off (FAR's default; P6.3), else selects (`RightClickRule`), double
click = Enter, wheel scrolls panels/menus/viewer, click on the key bar presses that key, click on the top line opens
the menu bar, click on the column titles/borders as in FAR, drag and drop of files between panels (copy/move
following FAR's rules), mouse in menus and dialogs.
Done (3e step 2): on the panels - click, double click (Enter), the middle button (Enter), right button
selecting along the way, a button held over a panel's top or bottom, the wheel and the tilt wheel, the sort
letter (left: drive menu, right: sort menu); `interface.mouse` turns it off. Done (3e step 3): the top line
(its left corner is Ctrl+O, the rest opens the menu bar at the menu under the mouse), the key bar (a key is
pressed when the button is let go over it, the right button adds Alt), the command line (cursor, double click
selects the word, a click after it the line). Done (3e step 4): menus and lists (F9 with its menu bar, pop-up
menus, the drive menu, F12 and the other lists, the list under the command line) as `VMenu`: the item under
the mouse follows it, pressed and let go on an item is Enter, the middle button Enter, a click outside closes
(the middle button chooses), a button held on the box's edge moves on, the wheel moves by one. Done (3e step 5):
dialogs as `Dialog::ProcessMouse` - an edit line takes the cursor (double click: the word), a drop-down list
opens and its lines are chosen with the mouse, a check box or radio button is Space, a button is pressed when
let go on it, outside the dialog either button is Esc (Changed by the owner: FAR's right button there is Enter,
`Dialogs.MouseButton`, which would answer a question nobody read); the progress dialog's buttons too. Done (3e
step 6): files dragged between the panels as `Panel::ProcessMouseDrag` (Shift or the right button: move; the note
at the mouse; dropped, the F5 or F6 dialog, for the folder under the mouse when there is one). Done (3e step 7):
a program on the user screen or in the guest shell gets the mouse when it asked for it.
Done (P4.1): a menu or list longer than its box shows FAR's scroll bar on its right side (`menus.scrollbar`, FAR's
`ShowMenuScrollbar`, on): `▲` / `▼` held move by one, the field between them drags the selection; a list dialog
(F12, themes, shells) scrolls to its cursor. A dialog is dragged by any place that is no item (the right button
puts it back) and moved by Ctrl+F5 (the arrows, Ctrl+arrows by ten or five, Home / End / PgUp / PgDn to the
edges, Enter or Ctrl+F5 leaves it, Esc puts it back, FAR's marks in its corners). Differs: a moved dialog stays
inside the screen (FAR lets it go off but for a column).

## 7. Function keys (key bar labels from `far/farlang.templ.m4`)
| Key | Plain | Shift | Alt | Ctrl |
|---|---|---|---|---|
| F1 | Help - the list of keys (2b); FAR's full help (owner, 2026-11-14: after stage 7) - H1 | Add to archive - S6.2 | Left drive menu - Take | Left panel on/off - Take |
| F2 | User menu - done (2b step 5a) | Extract - S6.2 | Right drive menu - Take | Right panel on/off - Take |
| F3 | View - Take | Archive commands - S6.2 | External viewer - Take (2d) | Sort by name - Take |
| F4 | Edit - Take (a folder: its attributes, R7 review D8) | Edit new file - done (R7 review D1: FAR's "Editor" dialog, history `NewEdit`, code page; a folder not there asked about, made by the save; an empty name `New_File_<time>.txt`; on a plugin's panel a copy put to it once saved; the editor's Shift+F4 too) | External editor - Take (2d) | Sort by extension - Take |
| F5 | Copy - Take | Copy in place - Take | Print - Later (deferred) | Sort by write time - Take |
| F6 | Rename/move - Take | Rename in place - Take | Make link - Take (`platform`) | Sort by size - Take |
| F7 | Make folder - done (R7 review D4: FAR's "Make folder" dialog - history `NewFolder`, "Link type" none / directory junction / symbolic link with its "Target" (history `NewFolderLinkTarget`), "Process multiple names" kept as `system.multiMakeDir`; differs: a link that cannot be made leaves no empty folder behind) | - | Find file - done (2b step 7) | Unsorted - Take |
| F8 | Delete - Take | Delete current file - Take | Command history - done (2b step 3) | Sort by creation - Take |
| F9 | Menu bar - Take | Save setup - Take (2b) | Video mode - Later (deferred) | Sort by access - Take |
| F10 | Quit - Take | Last menu item - Take (2b) | Find folder - Take (2c) | Sort by descriptions - done (P6.0b) |
| F11 | Plugin commands - done (5a; its F4 hotkeys, F3 information, Shift+F9 configuration done in 5d; Shift+F1 the plugin's help with H1) | Sort groups - done (2b step 8d) | File view history - done (2b step 3) | Sort by owner - Later |
| F12 | Screens list - done (2b step 1) | Selected first - Take | Folders history - done (2b step 3) | Sort menu - Take |
Alt+Del (wipe), Ctrl+A (attributes), Ctrl+G (apply command), Ctrl+Z (describe): attributes - done (3b step 5b);
apply command - done (2b step 5b): Ctrl+G and Files > "Apply command" ask FAR's "Enter command to process
selected files" with the last command given, then run it once for each selected file (or the one under the
cursor), its special symbols speaking of that file, one after another as the user menu's lines; each file
leaves the selection once its line runs (one whose questions are cancelled stays selected), and the selection
before it is kept for Ctrl+M (FAR's `SaveSelection`). A run asked for while another goes on comes after it, with its own title, and a Cancel or a command sent to
the background stops only the run it belongs to.
Differs: the question keeps no history (FAR's `ApplyCmd`), and Esc does not stop between files (a command sent to
the background does); wipe and describe - Later (deferred).

File associations - done (2b step 5c), FAR's `filetype.cpp`: Commands > "File associations" is a list (each line
the description, `│`, the masks; "Ins, Del, F4, Ctrl+Up, Ctrl+Down" in its frame; Enter or F4 changes, Ins adds
before the cursor with all its boxes on, Del asks "Do you wish to delete association for" with the masks), and
"Edit file associations" has the masks (FAR's masks: `,` `;` quotes, exclusions after `|`), the description and six
commands with their check boxes - Enter, Ctrl+PgDn, F3, Alt+F3, F4, Alt+F4. A mask written wrong says "File-mask
string contains errors" and the dialog comes back. On a file the key looks for the associations whose masks take
its name and whose command for it is on: none - the key does what it does without them (Alt+F3 and Alt+F4 the
external viewer and editor when they have a command - F3 and F4 with "Use external viewer / editor" -, else the
program's own, as FAR's `FileList::ProcessKey`; Ctrl+PgDn a plugin that opens the file - an archive by its contents, S6.1a - else nothing, as in FAR); one - its command runs as a user menu line, its special symbols
speaking of the file; more - "Select association", each line its description or else its command. Enter's command
goes into the history as it runs, its symbols replaced and its questions answered. Saved as `files.associations`. Differs: `if exist` / `if defined` conditions do not take
lines out of "Select association" (cmd still checks them when the command runs); regular-expression masks
(P3.3, ADR-0020) have no `%RegexGroup`; the check boxes do not grey their edit lines; Shift+Enter (a new window) is as
before; the masks field keeps no history (FAR's `Masks`).

Choosing the shell is **not** on a function key: Commands menu > "Choose shell" and Options > Command line
settings > default shell (2b). F2 is FAR's user menu (owner, 2026-11-10: "as in FAR and the Norton Commander
family").

Done (2b step 5a): F2, FAR's user menu (`usermenu.cpp`). Its files are `UserMenu.ini` in the text form of FAR's
menu files (read in UTF-8, UTF-16 or the OEM code page, written in UTF-8; the owner, 2026-11-10: the program's
own name, no reference to FAR, and FAR's `FarMenu.ini` is not read): the local one in the active panel's folder,
else the main one, which is `settings\UserMenu.ini` beside the settings file (the program is portable: FAR's global and user menus are one
here). Title "Local menu" / "Main menu" and " » " with each submenu's label; bottom title "Ins, Del, F4, Alt+F4,
Ctrl+Up, Ctrl+Down"; the hotkey in a column of four before the label, `--` a separator with its label as title,
`►` after a submenu; labels with their special symbols replaced. Keys: Up/Down round the ends over separators,
Home/End, PgUp/PgDn, Enter or the hotkey (a letter, a digit, F1..F24) runs the item or opens its submenu, Right
opens, Left and Esc go back, Shift+F10 closes it all, Shift+F2 the main menu from a local one and the nearest local
one from the main, BS the menu of a folder above (local only), Space does nothing. Ins asks "Do you wish to insert
a new command or a new menu?" (Insert command / Insert menu) and opens "Edit user menu" (Hot key, Label, ten
Commands lines) or "Edit submenu label" before the cursor; F4 and Shift+F4 change the item; OK checks FAR's
"Invalid format for UserMenu Label" / "Invalid format for Hot Key" and the dialog comes back; leaving with
changed commands asks "Save changes?" (Yes / No / Cancel). Del asks "Do you wish to delete the menu item" /
"the submenu" with its label; Ctrl+Up/Down move it; Alt+F4 opens the file in the editor and the menu is read again
afterwards. Every change is saved at once; an empty menu takes its file away. Commands > "Edit user menu" asks
"Choose user menu type to edit" (Main / Local / Cancel). An item's commands run one after another as the command
line runs them (`cd`, a drive and `set` by the program, the rest on the user screen), each once the one before has
ended (a command sent to the background stops what is left); `REM` and `::` lines are left out; each line's
symbols are replaced as its turn comes, but the name under the cursor is the one the item was chosen on (FAR's
`subst_context`); its `!?title?init!` questions are asked in one dialog titled with the item's label (Cancel stops
what is left). Labels get their special symbols and then the environment's variables; separators show their label
as it is written; titles lose their `&` marks. Special symbols
(`MetaSymbols`): `!!`, `!`, `!~`, `!``, `!``~`, `!.!`, `!-!`, `!+!`, `!@!` / `!$!` with Q S F W, `!&` / `!&~` with
Q q, `!:`, `!\`, `!/`, `!=\`, `!=/`, `!?!`, `!?title?init!` (symbols in brackets in its title and text),
`!#`, `!^`, `![`, `!]`; the list files (no signature) are kept five minutes or until the program ends, as FAR's
`delayed_deleter` keeps them. A local menu that cannot be read is said. Differs: short (8.3) names and real paths are the
long and plain ones; `!?!` is empty (no descriptions yet); list files are UTF-8 unless W (FAR's default and A are
code pages); a submenu is shown alone, not over its parent; the question dialog keeps no history and no
`%UserVarN`; a read-only menu file is not offered to be overwritten; no F1 help.

Done (2b step 3b, the owner's): Git Bash where Git for Windows is (its folder from
the registry, `Software\GitForWindows`, else beside a `git.exe` along `PATH`; a login bash), and the user's own
shells - Command line settings > "Shells...": a list (Ins adds, Enter or F4 changes, Del removes after a
question) and a dialog of name, program, arguments (quotes group words) and type (cmd, PowerShell, bash of
MSYS or Cygwin, WSL), which says how the shell speaks: its prompt, its hidden `cd` (`cygpath`, `wslpath`), the
keys that empty its input. Saved as `terminal.userShells`; they come after the system's in the lists.

## 8. Command line and panel/terminal interaction - Take, stage 2a
- Typing goes to the command line; Enter runs it; Esc clears; `cd` / drive letters change the panel.
- As in FAR (ADR-0006), each command line runs as a process of its own in the active panel's folder, on one user
  screen kept for the program's life: a runner (`nova-commander --run`) prints `C:\dir>command` on a line of its
  own and starts it as FAR's `Execute` (3d step 2): `%VAR%` expanded; a line without `<>|&` outside quotes that is
  not a cmd command has its first word looked up as FAR's `FindObject` (the folder, then `PATH`, with the executable
  extensions) and started directly, without cmd, its parameters as typed; what the system cannot start goes to the
  shell (`ShellExecuteEx`: a document typed on the line opens with its association, a folder in Explorer, a URL in
  the browser); what the shell does not find, and every complex line, to `%COMSPEC% /S /C "command"` (FAR's default
  `ComspecArguments`), so cmd syntax works. What cannot be started: FAR's message "Cannot execute", the line, the
  system's text (or "Cannot invoke command interpreter"); a refused administrator's prompt says nothing. Ctrl+C
  stops the command, its exit ends it, the panels come back and are re-read. No process stays in a folder, so the
  panels can move or delete it. As FAR, a Windows GUI program (Notepad, a setup program, Enter on one in the panel)
  is not waited for. A program changing the console's code pages leaves them as they were (FAR's `RestoreCP`).
  Enter on a program in the panel starts the file itself: nothing in its name means anything (but cmd expands
  `%VAR%` in a batch file's name, as for FAR). The program carries out itself, and only shows on the user screen,
  `cd` / `chdir` / a drive (the panel moves), `set NAME=value` (FAR's rules: `set "NAME=value"`, `%VAR%` expanded, an
  empty value removes; later commands get it), `pushd [dir]`, `popd`, `clrd` (FAR's stack, shown in `FARDIRSTACK`),
  `title text` (the console's title, while no task shows its progress there) and `exit [code]` (the program ends
  with the code; with tasks running it asks first, FAR ends at once); `chcp <number>` the runner sets itself and
  it stays; `set` alone, `set NAME`, `set /a`, `set /p`, `chcp` alone and `cls` run in cmd. Enter on an empty line
  with the active panel hidden shows the prompt, as FAR scrolls the user screen.
  Differs from FAR: "Cannot execute" names the line as typed, FAR the program it found; a `.ps1` under the cursor
  still goes as a line, so `%VAR%` in its name is expanded; no retry with short names after a too long name
  (`ERROR_FILENAME_EXCED_RANGE`, FAR's `exec_with_short_names_fallback`). FAR's own commands (`far:config` and the
  like), `@` before a line, doskey aliases and `if exist` are not built in yet; a `set /p` answer and state set inside a command (doskey, `call x.bat` changing the environment)
  do not outlive it - the guest shell is for that.
  Commands get the program's own environment (the one it was started with, plus `set`), not the one stored in
  the registry; a program is looked up with `PATHEXT` in its own order (`.COM` before `.EXE`), as cmd and FAR.
  Differs from FAR (safety): what cmd would run (a complex line, one of its commands, a batch file, cmd itself named
  on the line) is not run in a
  folder that is not a plain drive path (`\\server\share`, `\\?\C:\...`, `\\.\C:\...`): cmd cannot stand there and
  would run it in the Windows folder (FAR lets that happen; `del *` would delete there); a program started directly
  runs there. A mapped drive letter works, and so does the guest shell with PowerShell. Nor is a line run when the
  panel's folder is gone (deleted elsewhere, a drive offline): the runner enters the folder itself and refuses,
  instead of starting in the home folder.
- Differs from FAR (Volkov Commander's Esc, owner's choice): Esc on an empty command line hides or shows the panels
  as Ctrl+O does.
- Alt+Shift+T (not in FAR, ADR-0006): the guest shell chosen in Commands > Choose shell (cmd, Windows PowerShell,
  pwsh, WSL, Git Bash, the user's own), started on first use and kept until the program quits; it gets every key. It fills the console
  but for the program's own top row (only the clock, as in FAR) and bottom row (in the key bar's colours:
  the shell, its PID, whether it waits for a command, the keys back to the panels).
  Alt+Shift+T, or Esc at its prompt with nothing typed, brings the panels back. Alt+Shift+T no longer starts quick
  search (Alt+T finds the same names).
- Shift+Enter: run the command in a separate console window (as FAR); with an empty line the program under the
  cursor, and another file or a folder opens with its Windows association (a folder in Explorer), through
  `ShellExecuteEx` as FAR (no command interpreter: `&` or `%VAR%` in a name stay part of the name).
- Ctrl+Alt+Enter (FAR): the command line, or the file under the cursor, with the administrator's rights (the `runas`
  verb; Windows asks), not waited for; on a folder as Enter. Commands the program carries out itself are not
  built-in here, as with Shift+Enter: they go to the system. Differs from FAR (safety): what cmd runs goes to the
  panel's folder first (`cd /d "folder" && line`): elevated it would start in the system folder, and `del *` would
  delete there; where it cannot go (a mapped drive the elevated session does not see) nothing runs.
- Ctrl+Enter / Ctrl+J insert file name (on "..": the panel's folder); Ctrl+Shift+Enter name from the passive panel;
  Ctrl+[ / Ctrl+] left/right panel folder; Ctrl+Shift+[ / ] active/passive panel folder; Ctrl+F full file name;
  Ctrl+Alt+F network (UNC) name; Ctrl+Alt+[ / ] and Alt+Shift+[ / ] the same folders as network names. Names
  with spaces or shell metacharacters are quoted (FAR's `QuotedSymbols` plus PowerShell's).
  The network names are asked for on a worker (3c step 2): a mapped drive whose server is unreachable holds the
  keys, not the screen, until the network's timeout.
- Ctrl+Left / Ctrl+Right move by words in a typed command (FAR's word dividers).
- Text selection and the clipboard in the command line and dialog inputs, as FAR's `Edit` (non-persistent blocks,
  Del removes blocks): Shift+Left/Right (in multi-stripe views the panel takes them; Alt+Shift+Left/Right then),
  Alt+Shift+Home/End (Shift+Home/End belong to the panel), Ctrl+Shift+Left/Right by words; typing or pasting
  replaces the block, Del/Backspace delete it; Ctrl+Ins / Shift+Ins copy (the whole line without a block) /
  paste (line breaks become spaces); Ctrl+V pastes; Ctrl+Backspace / Ctrl+Del delete a word, Ctrl+Shift+Backspace
  to the start, Ctrl+K to the end, Ctrl+Y the line. Beside a shown panel FAR's panel keys win: Shift+Del
  (delete files, 3d), Ctrl+A (attributes, 2b), Ctrl+X (history); with the panels hidden they edit the line (Shift+Del
  cuts, Ctrl+A selects all). Dialogs also have Ctrl+A, Ctrl+C, Ctrl+X. Ctrl+C copies the block, or the line
  without one, as in FAR (a running command gets Ctrl+C itself).
- Panel names to the clipboard (FAR's `CopyNames`): Ctrl+Ins with an empty command line and Ctrl+Shift+Ins the
  names, Alt+Shift+Ins full names, Ctrl+Alt+Ins network names - the selection in panel order or the file under the
  cursor, one per line, unquoted (FAR's default `QuotedName`); ".." stands for the panel's folder. Ctrl+Shift+C /
  Ctrl+Shift+X put the files for Explorer's Paste (copy / move).
  Not possible: a paste from the terminal (Windows Terminal's Ctrl+Shift+V or right click) reaches the program as
  typed keys on Windows (crossterm has no bracketed paste there), so a multi-line paste presses Enter between the
  lines, as in FAR.
  Characters the current keyboard layout lacks (`&`, `~`, `$` with a Russian layout on) come from the pseudo
  console as Alt codes (Alt down, numpad digits, Alt up carrying the character), and so does a code typed with
  Alt+numpad. crossterm reports only the Alt up, as a release of the character; a character released with no
  press before it and no Ctrl or Alt is typed (`keyboard::Releases`; held keys are forgotten when the window
  loses focus). FAR reads the Alt up record itself.
- Shortcuts follow keys, not characters, as in FAR: with a Russian layout Ctrl+Х is Ctrl+[ and Ctrl+А is Ctrl+F
  (`platform::key_for_char`). Alt+letter keeps the typed letter for quick search; a character typed with AltGr in
  any installed layout stays text.
- The guest shell and the panels share one folder: on entry, while the shell waits at its default prompt (cmd,
  PowerShell) in another folder, it gets a `cd` first (a busy shell is not touched), and the command line moves
  into its input line; on leaving, its input line comes back to the command line and a `cd` done in it moves the
  panel. The program's own `cd` lines (`cd/d "..."` in cmd, `Set-Location -LiteralPath '...'; [Environment]::CurrentDirectory = $PWD.ProviderPath #vc` in PowerShell)
  are not shown; PowerShell also keeps them out of its Up history (a PSReadLine `AddToHistoryHandler` set at
  start, chained to the one already set); cmd's Up history still has them (doskey cannot filter). bash and a
  custom prompt do not show the folder, so they are not moved. Known limits until shell integration (OSC 133): a
  cmd `PROMPT` of fixed text that looks like a path is taken for the folder; a running program's output line that
  looks like `C:\dir>` counts as the prompt. While the panels are shown, a guest shell waiting at its prompt waits in the Windows folder (a hidden `cd` on leaving; the next entry brings it back), so it holds none of the user's folders: Windows keeps a process's folder open, and the panels could not move or delete it. PowerShell's `Set-Location` does not move the process, so its hidden line sets `[Environment]::CurrentDirectory` too. A busy shell, and WSL (its prompt does not show the folder), stay where they are and may hold that folder.
- Ctrl+E / Ctrl+X history; with both panels hidden Up / Down too (FAR's `CommandLine::ProcessKey`).
- Done (2b step 3): the history menus as FAR's `History::ProcessMenu` - Alt+F8 commands, Alt+F12 folders, Alt+F11 files
  viewed and edited ("View: ", "Edit: "), also in the Commands menu. In the middle of the screen, at most `ScrY - 8`
  lines, oldest first under separators titled with their date, the newest selected, Up/Down round the ends. Ins locks a
  line (`√`: kept whatever its age, Del leaves it), Shift+Del takes out the unlocked line under the cursor, Del all
  unlocked ones (FAR's question "All records in the history will be deleted. Continue?" while Confirmations > "Clear
  history list" is on), Ctrl+R the folders and files no longer there, Ctrl+C / Ctrl+Ins copy the line. Alt+F8: Enter
  runs, Shift+Enter in a new window, Ctrl+Alt+Enter as administrator, Ctrl+Enter onto the line only; F3 shows the
  folder the command was given in. Alt+F12: Enter goes there in the active panel, Shift+Enter too without noting it,
  Ctrl+Shift+Enter in the passive panel, Ctrl+Enter onto the line. Alt+F11: Enter opens as the line was opened (Shift+Enter
  without noting it), F3 views, F4 edits, Ctrl+Enter onto the line; the other Enters close the menu and do nothing,
  as FAR's handlers. A folder or file no longer there is said and the
  menu stays; the disk is looked at beside the UI. Folders are noted as FAR's `ChangeDir` does - the one left and the
  one come to - and the active panel's at the end; files as the viewer or editor opens them. System > "Save folders
  history" and "Save view and edit history" as "Save commands history": off, the list is of this run only. Differs:
  Edit of a file gone does not offer to make it (FAR's "Create the file?"); no help; Ctrl+Alt+F filters the lines (P4.4, `[text]`
  in the title, Ctrl+Alt+L locks it); plugin folders come with stage 5.
  While typing (or Ctrl+Space) FAR's list stands above the line (`EditControl::AutoComplete`, its defaults: shown, not modal, nothing appended): commands from
  the history that begin as the line does, newest first, then under titled lines files of the panel's folder for the last word (quoted as needed), `%NAME%` variables, cmd's own
  commands and the programs along `PATH`. Typing, Backspace, Del and Left/Right stay in the line and the list follows; Up/Down (which go round), PgUp/PgDn, Home/End (which stop at the ends) and Ctrl+End, Ctrl+Space put the
  line on the command line (the empty first line is what was typed); Shift+Del takes a line out of the history (the next one takes its place); Esc and F10 close; Enter closes and runs; any other key closes and does its work. The disk is read on a thread of
  its own, so a slow folder does not hold the keys (FAR reads it in the key's handling). Differs from FAR: at most 1000 files are offered, the first by name; a typed name of a folder that is another copy of the drive-relative form (`C:sub\\`) offers nothing; the keys are not checked for being still queued (FAR skips the list then). Not yet: App Paths from the registry, 8.3 names, `\\\\?\\` devices, the mouse (3e), the options (`ShowList`, `ModalList`, `AppendCompletion`, sources; 2b), completion in dialogs' history (2b).
  The histories outlive the program, as FAR's: saved in `settings\history.json` as each line is given, off the UI thread; 1000 lines
  of each stay whatever their age, older than 90 days only those, locked ones always (`HistoryCount`, `HistoryLifetime`); a line given
  again moves to the end, whatever its case, and keeps its lock. Two copies keep each other's lines in the file, but see them only after
  a restart (FAR's database is live); what one takes out or unlocks stays so in the file. The count and the lifetime are not settings. Commands are kept as plain text, as in FAR's database.
- Ctrl+PgUp parent, Ctrl+PgDn enter, Ctrl+\ root; Alt+letter quick search. Ctrl+PgUp at a drive's or share's root opens
  the drive menu with Interface settings' "Use Ctrl+PgUp to change drive" (FAR's `PgUpChangeDisk`, on), else nothing
  (a share's root: the network plugin first, P6.6c);
  Ctrl+PgUp in the drive menu closes it then (P3.5). FAR's third value (`[?]`, 2: a network drive's root goes to
  its share in the network plugin first) since P6.6c; the settings' old `true` / `false` read as 1 / 0, any number
  kept within 0 to 2 (written as a number since, which a build before P6.6c does not read). Shift+F5 / Shift+F6 copy or move the file under the cursor alone, its own name in the line (P3.5). Panel
  settings' "Show scrollbar" (FAR's `ShowPanelScrollbar`, off): the files' and the tree's scroll bar on the right
  frame, its arrows held and its thumb dragged with the left button (P3.5). FAR's private `Panel.CtrlFRule` is the
  settings file's `panels.ctrlFRule` (off): Ctrl+F, Ctrl+Alt+F, Ctrl+Alt+[ / ], Alt+Shift+Ins and Ctrl+Alt+Ins take
  the mode's cases. A mode's negative column width is an empty column (FAR's `EmptyColumns`): kept in the stripes,
  not shown. A drive chosen in the drive menu that cannot be read asks "Cannot read the disk in drive X:" with
  Retry and Cancel, the letter editable, the panel left where it was (FAR's `ChooseDisk`).
- Ctrl+Shift+Tab while a command runs (FAR's `ConsoleDetachKey`): the command goes on in the background and the panels
  come back. Differs from FAR: a pseudo console cannot be handed over, so instead of a window of its own the command
  becomes a line of the Screens list (F12), program and arguments in columns, and a part of the status row above the
  panels (each part is shortened as little as the width allows): Enter on its line shows it and keys go to it,
  Ctrl+Shift+F12 goes back (Ctrl+Tab / Ctrl+Shift+Tab go to the next / previous screen, P4.2, as FAR's manager takes
  them before any window); when it has ended its output can be read once, any key closes it, and Del on the list removes it. Ours besides: Ctrl+Shift+F12,
  as Windows Terminal keeps Ctrl+Shift+Tab for its tabs. Quitting says that such commands will be ended.
- Folder shortcuts - done (2b step 4), FAR's `shortcuts.cpp`: ten lists of folders. Ctrl+Shift+0..9 (either
  Ctrl) adds the active panel's folder to the end of a list - ours besides: Alt+Shift+0..9 in any layout, as
  Windows Terminal keeps Ctrl+Shift+1..9 for its tabs (owner, 2026-11-10). The digit key is told by
  the key, not by the character it types (`!`, `"`, `№`), and only on the panels: a shell or a quick search
  already open gets the character as before; RCtrl+0..9 (the right Ctrl alone; with the left one
  Ctrl+0..9 stays the panel's modes) goes there, `%NAME%` replaced, when the list has one folder, else shows
  "Folder shortcuts (n)": its folders, each with a hotkey given as FAR's `VMENU_AUTOHIGHLIGHT` does; a shortcut
  with no folder is a separator (`--`) or, before the first line, the title. There Enter goes, Ins adds the
  panel's folder before the cursor, Del takes the line out, F4 changes its title and path ("Save this
  shortcut?" when the path is not there), Ctrl+Up / Ctrl+Down move it, RCtrl and the list's digit goes down,
  another digit shows that list. On the info panel the other panel goes; with the panel hidden, and in a
  dialog's edit line, the folder is put into the line. Commands > Folder shortcuts: the ten lines "RCtrl+n │"
  and each list's first folder (`<none>`, `►` for several); Enter goes, Ins adds to an empty list, Del / F4
  change a list of one, and any other list is opened for them with its lines as they are, the path not looked
  for; RCtrl+0..9 does nothing there. Saved as `panels.folderShortcuts`.
- Answering a program that waits for input: while a command runs every key goes to it (ADR-0006).

## 9. Menus (F9, `far/config.cpp`) - stage 2b unless noted
| Menu | Items |
|---|---|
| Left / Right | All FAR items. Brief...Detailed, Alternative full - Take; Descriptions, Long descriptions, File owners, File links, Show long names - Later (deferred); Info panel - done (2b step 1); Tree panel - done (2c step 1a); Quick view - done (2c step 3a, after Tree panel as FAR's); Sort modes, Panel on/off, Re-read, Change drive - Take |
| Files | View, Edit, Copy, Rename or move, Link, Make folder, Delete, File attributes, Select/Unselect group, Invert/Restore selection - Take; Apply command - done (2b step 5b); Add to archive, Extract, Archive commands - Later (archive plugin); Wipe, Describe files - Later (deferred) |
| Commands | History, File view history, Folders history - done (2b step 3); Swap panels, Panels on/off, Compare folders, Edit user menu - done (2b step 5a); File associations - done (2b step 5c); Folder shortcuts - done (2b step 4); **Choose shell** - added; File panel filter - done (2b step 6b); Find file - done (2b step 7, below); Find folder - done (2c step 2a, hidden with the tree turned off as FAR's); Plugin commands - done (5a); Screens list - done (2b step 1); Task list (Ctrl+W) - done (P6.5e, below; "Choose shell" now `sh&ell`); Video mode, Hotplug devices - Later (deferred) |
| Options | System, Panel, Interface, Dialog, Menu, Command line, AutoComplete, InfoPanel settings; Confirmations; File panel modes; Save setup - Take (File panel modes done, 2b step 9). Folder description files - done (2c step 4, after File panel modes as FAR's). Files highlighting and sort groups - in the theme utility (ADR-0008). Viewer and Editor settings, Code pages - 2d, with them (owner, 2026-11-09). Colors - replaced by Themes (choice only; themes are made in a separate utility, ADR-0008). Tree settings - done (2c step 1b; its T is AutoComplete settings' too, the first of them taking it, as FAR has it: owner, 2026-11-11). Plugins configuration - done (5d-3a, after Interface settings as FAR's). Plugins manager settings - later. Languages - Later (English UI first). File descriptions - Later (deferred) |
Options that only make sense on another OS or for deferred features are hidden, not shown disabled.

File panel filter - done (2b step 6b; the filter's dialog, Ins / F4 / F5, 2b step 8a-1), FAR's `filefilter.cpp`:
Ctrl+I or Commands > "File panel f&ilter" opens the "Filters menu" of the active panel (none on the info
panel): the user's filters, a separator, "&0. Folders" and "Panel file type" for each type of the panel's
files (`*.ext`, `*.gitignore` for a name starting with a dot, `*.` without one; also the types its filter hid
and those marked with no file here, in FAR's case-blind order), hotkeys 1..9 and A..Z, each line FAR's `MenuString` (the title in 21 columns, `│`, `+` / `-` attributes, `S`, `D` /
`R`, `H`, `│`, the mask) and the keys "+, -, Space, I, X, BS, Shift+BS, Ins, Del, F4, F5, Ctrl+Up, Ctrl+Down"
below. `+` / `-` include / exclude, `I` / `X` the same strong (the filters after them do not count), the key
again takes it off, Space `+` or off, BS off, each going down; Shift+BS takes every mark off; Del asks "Do you
wish to delete the filter" for a filter of the user's ("Only custom filter can be deleted" / "edited" for the
others); Ctrl+Up / Ctrl+Down move one. The marks are kept for the panel's own area however the menu closes,
and the panel is read again: the first filter taking a name decides, a strong one over those before it; a
name none takes is shown unless some filter includes; a folder none takes is shown unless a filter includes
folders. The panel shows `*` after its sort letter while a filter is on. Ctrl+U swaps the panels' marks with
the panels. Saved as `filters.list`, `filters.folders` and `filters.types` (a type marked nowhere is dropped).
Sizes as FAR writes them (upper case K, M, G... for 1024, lower case for 1000); dates as seconds (since 1970,
or before now when relative). Differs: the hard links condition takes nothing yet (the panel does not know
the number of links).
8a-1 done: FAR's "Filter" dialog (`FileFilterConfig`, 80 x 15): "Name:", "Mask:", "Size:" with `>=` / `<=`,
"Date/Time:" with the kind (write, creation, access, change), Relative (days and a time before now in place of
the dates; Current and UTC hidden, the fields cleared when it changes), the `<=` / `>=` dates and times in
the user's order with FAR's masks, Current (now, local or UTC), Blank, UTC (the dates shown turned into UTC
and back), "Attributes" with its "..." dialog (FAR's 21 three-state boxes: set, clear or either), "Has more
than one hardlink", OK / Reset / Cancel; each box enables its fields as FAR's `SetAutomation`. Ins in the
filters menu makes a new filter for files only before the line under the cursor, F4 changes the user's filter
under it, F5 copies the line's filter (the user's without its title and marks, "Folders", a file type's for
files of that type); OK puts it in place at once, the menu comes back with it under the cursor, and the panels
are read again when the menu closes. A size it cannot read gets FAR's "File size field is incorrectly filled",
a wrong mask "File-mask string contains errors", the dialog back after either. Reset: the mask `*`, no size,
no date, no hard links, the attributes used with only folders left out. A disabled edit line, masked line or
list field of any dialog is drawn in FAR's `Dialog.Edit.Disabled` (the theme role `dialog.edit.disabled`); a
masked line is as wide as its item (FAR's `X2 - X1 + 1`), its mask taking what may be typed.

Find file (Alt+F7, FAR's `findfile.cpp`) - 2b step 7, planned with the owner on 2026-11-10; 7a done:
Alt+F7 and Commands > "&Find file" (first, as FAR's) open FAR's 80 x 21 dialog with all its items, those
of later steps disabled; the masks (empty: `*`), the area, "Search for folders" and "Search in symbolic
links" are saved (`find.mask`, `find.area`, `find.folders`, `find.symbolicLinks`), the text, case, whole
words and not containing stay for the session (FAR's shared search parameters); a wrong mask gets FAR's
"File-mask string contains errors" and the dialog back; Drive opens the drive menu of the active panel and
the dialog comes back with its "From the root of" line for the new drive. The search runs on its own
thread as FAR's `ScanTree` with `FilesFirst` (a folder's files, then each folder followed at once into),
hidden files too; the areas as FAR's `DoPrepareFileList` (the fixed drives and RAM disks, with the network
ones or not, letters and volumes without one; the folders of PATH, not walked into; the root of the
panel's drive; the panel's folder, walked into or not; the selected folders, or the one under the cursor);
a link to a folder is walked into when asked, never into a folder it is inside of. "Containing" reads
each file in 64 KB blocks in FAR's "Default code page" (its byte order mark, else UTF-8 when its first 32
KB are UTF-8 beyond ASCII, else ANSI), "Whole words" with FAR's word dividers; a file that cannot be read
has no text, so "Not containing" takes it, as FAR's; with a text no folder is found. The results dialog
is the console less a column and a row on each side, the found objects under their folder's line
(`folder\`, a new one each time the folder changes, its case aside), each as FAR's "D,S,A" (date, size,
attributes, `│` after each) and the name, "Files: n, folders: m" on the double line under the list,
"Searching "text" in" and the place (the text cut to 10, the line cut in its middle), then "Search done.
Found files: n, folders: m" and Stop becoming Cancel; the first find moves the focus to Go to unless the
user moved it; Up/Down/PgUp/PgDn/Home/End move in the list, Left/Right/Tab among the buttons; F3, Alt+F3,
Ctrl+Shift+F3 view and F4, Alt+F4, Ctrl+Shift+F4 edit a found file while the search goes on, the viewer
with the found files for Gray + / Gray -, the dialog back when it closes; Go to (and Enter on it) shows the
object in its folder on the file panel (the other one under the info panel; not read again when it is
there), a folder's line the folder in the one above; Esc asks "Operation has been interrupted" with the
search paused (Yes stops it and the dialog stays with what was found); Stop drops what the search still
had; New search the dialog again. Panel: done in 5d-5 (section 13).
7b-1 done: Hex (Alt+H) shows FAR's `HH HH ...` line of 23 bytes in place of the text, the text turned into
its ANSI bytes and back (FAR's `ConvertHexString` for its own code pages), the code page, case, whole
words and fuzzy disabled, the hotkey going on to the line; the bytes are looked for as they are, across
blocks too, no byte order mark skipped. "Fuzzy search" folds the text and each file alike as FAR's
`fuzzy_searcher` does (`FoldStringW` with ligatures expanded, then composites decomposed, digits and
compatibility forms folded, the lone marks dropped: "Ĝŕžèģōŗż" is "Grzegorz"). "Search in alternate
streams" (saved, `find.alternateStreams`): after each object its named streams, `a.txt:notes` with the
stream's size, looked at as files; Go to shows the file, F3 / F4 open the stream. Advanced opens "Find
file advanced options": "Search only in the first" (FAR's size, `64K`; empty: whole files; written wrong:
"File size field is incorrectly filled", the dialog stays) and the results' "Column types" / "Column
widths" (FAR's codes S, P, D, T, DM, DC, DA, A with their modifiers let go, 0 for the default width; the
name last), saved as `find.searchInFirst`, `find.columnTypes`, `find.columnWidths`; the Find file dialog
comes back under it with the focus where it was. With a text the results show `{:3}%` of the file being
read after the place.
7b-2 done: "Using code page" is FAR's list (`FillCodePagesList` for Find file): "Automatic detection", "All
standard code pages", under "System" the ANSI and OEM code pages (FAR's real ones, the legacy ones on a UTF-8
system), under "Unicode" UTF-8 and UTF-16 little and big endian, under "Favorites" the code page menu's
favourites (S2d.6f), under "Other" every other code page the system has, by number (UTF-7 left out), each
line `1251 │ ANSI - Cyrillic` with the system's names or the user's (`1251 │*Mine`, S2d.6f); Space /
Ins mark a code page (the cursor going on), the second line becoming "Selected code pages" and the list's
bottom frame saying "Space Ins"; the marks are kept also when the dialog is cancelled. A code page chosen
reads each file in it as it is (no byte order mark skipped, a character cut by a 64 KB block's end kept for
the next, double-byte and longer code pages too); "All" reads each file in each marked code page, or with
none marked in FAR's five (ANSI, OEM, UTF-8, UTF-16 LE, BE); Text / Hex turn one into the other through the
code page chosen (ANSI for the automatic and "All"). "Automatic detection" tells a file's code page as
the viewer does (R7 review E4, ADR-0028, FAR's `GetFileCodepage`): its byte order mark, else UTF-16 by the
zero bytes of its first 32 KB (FAR's `IsTextUnicode`), else UTF-8 past ASCII, else ANSI. Saved as `find.codePage` (`auto`, `all` or the number)
and `find.codePages` (the marks). Every drop-down list now opens as FAR's combo box: up to 8 lines
(`CBoxMaxHeight`) scrolled to the cursor with FAR's scroll bar on its right side, over the field when there
is more room above, PgUp / PgDn by the lines shown, separators with titles.
On a plugin's panel (P6.0b, FAR's `PluginMode`): the drives' two areas grey (FAR's `LIF_GRAYED`: the
cursor stops, nothing chosen; one chosen gives way to the root), "Search in symbolic links" and "Search
in alternate streams" off and disabled (their choice for the disks kept); the drives, PATH and the root
are searched from the plugin's root (`DoPreparePluginList`), the current folder and the selected ones as
on a disk, through the plugin; Panel only with real names; F3 / F4 on a found file without one open a
copy, as on its panel.
**7a** the "Find file" dialog (masks and "Containing" text with their histories, P4.1,
Case sensitive, Whole words, Not containing, Search for folders, Search in symbolic links, the seven search
areas, Find / Drive / Cancel; Advanced is 7b) and the searcher on its own thread with FAR's results dialog (the found files under their folders,
"Searching in:", New search / Go to / View / Panel / Stop, Esc asking to stop; F3 / F4 / Alt+F3 / Alt+F4 on a
found file while the search goes on, the viewer getting the found files for Gray + / -; Go to puts the panel on
the file); Commands > "Find file". **7b** the rest of the search inside files: code pages (all, the default one
by detection, one chosen), hex, fuzzy, Advanced ("Search only in the first N bytes"), the percent of the file
being read, alternate streams. **7c** Changed (owner): a "Background" button sends the search to a task of its
own in F12 and the status row (several may run); choosing it in F12 brings its results dialog back; an ended
search stays in F12 until its results are opened or it is deleted with Del (FAR searches in the foreground
only). 7c done: "Background" (between Panel and Stop) or Ctrl+Shift+F12 (the key that sends a command to the
background) sends a search that goes on behind the panels, only while it searches (owner); Ctrl+Tab /
Ctrl+Shift+Tab (P4.2) send any search behind as they go to the next / previous screen;
several may go on. Each is a part of the status row (`Find 37` and the moving block, `Find 37 done`) and a
line of the Screens list (`Find 37` and the masks with the text: `*.rs "hello"`); Enter there brings its
results dialog back with all it found, the search going on in it; Del takes away one that is over and leaves
one that goes on; a search ending in the background flashes the taskbar button as a task does. F10 says "n
search(es) in the background will be stopped" and asks then even with Confirmations > Exit off. 8a-2a done:
"Use filter" (saved as `find.useFilter`) enables Filter, which opens the filters menu of Find file's own marks
with the active panel's file types (FAR's `EditFilters(FFT_FINDFILE)`), the dialog coming back after it; the
search then takes each object through FAR's `FileInFilterEx`: one an excluding filter takes is left out, a
folder with all in it; one no filter takes is left out while some filter includes (a folder still walked
into); the results' title says "(using filter)". "Search in archives": done in S6.7 (section 14);
"Panel" (the found files on a temporary panel): done in 5d-5 with the TmpPanel plugin (section 13).

Groups of file masks - done (2b step 6a), FAR's `Options::MaskGroupsSettings` and `filemasks::assign`: named
lists of masks; `<name>` in any mask (Select / Deselect, the associations, later the filters and the
highlighting) stands for the group's masks, everywhere it is written, a group met again for nothing; `%PATHEXT%`
in any case stands for the programs' extensions (`*.COM,*.EXE,...`); exclusions alone (`|<temp>`) are taken
from every name. The list ("Ins, Del, F4, F7, Ctrl+R" in its
frame) shows each name in ten columns, `│` and the masks, in the order of the names (case counts, as in FAR's
database); Ins adds and Enter or F4 changes ("&Name:", "A file &mask or several file masks:"; a name of
another group takes its place), Del asks "Do you wish to delete", F7 ("Filter", "&Mask:") shows only the groups
whose masks take that name (another F7 hides more), with the name as the title and "Total: n" below, until
Esc, F10 or Enter; after a change the cursor stays at its line; Ctrl+R
asks "Do you wish to restore default mask sets?" and puts FAR's `arc`, `temp` and `exec` back (the others
stay). With no group saved FAR's defaults are there, as its `InitConfig` puts them. Saved as
`files.maskGroups`. Differs: a group with no name is not kept (FAR keeps it and cannot change it then); after a
change F7's list is shown again as it was looked for (FAR shows all, its title and count left as they were).
Done (2b step 2): System, Panel, Interface, Dialog, Menu, Command line (the default shell), AutoComplete, InfoPanel,
Confirmations - the switches the program acts on (`app::options_pages`); the rest of FAR's are in `tech-debt.md`.
Done (P4.1): the dialogs' edit lines with FAR's histories (`Masks`, `SearchText`, `FilterName`, `FilterMasks`,
`MaskGroupsFindMask`, `FS_Name`, `FS_Path`, `Copy`, a user menu question's `UserVarN` or `$name$`): the `↓` after
the line, Ctrl+Down / Alt+Down or a click on it opens the lines newest first with a date before each day, Enter /
Tab put one in, Ins locks, Shift+Del takes one out, Del the unlocked ones (asking first), Ctrl+Ins copies; a button
closing the dialog adds the line (Esc does not), an empty `DIF_USELASTHISTORY` line starts with the last one; kept
in `history.json` with the other histories; Dialog settings' "History in dialog edit controls" (`dialogs.editHistory`).
A user menu line's `%UserVarN` and `%name` take its answers. Linked items as FAR's `LinkFlags` (F9's "Use shell
name" and "decimal fraction", Panel settings' update limit). "File already exists" > Rename asks the new name (the
free `name (2).ext` offered whole, "Remember choice"; a name that exists is asked about again, Cancel brings the
question back). A file name's `&` shows in the F5 / F6 prompt. The Ctrl+F12 menu has `VMenu2`'s margin; a user
submenu is drawn over the menus it came from; the delete question highlights the names and lists half the screen
at most.

Viewer and editor (stage 2d, S2d.1-S2d.8, done after stage 6 as the owner set on 2026-11-07): built on the view
core (ADR-0015), the prototype viewer and editor gone. The **basic** set marked below came first, the rest of
sections 10-11 after it.

## 10. Viewer (F3) - stage 2d
Basic: status line at the top, F2 wrap, F4 hex/text, F7 search + Shift+F7 next, F8 / Shift+F8 code page,
Alt+F8 go to (a line, a percentage or an offset: the line is ours over FAR, ADR-0015 addendum), F10/Esc
quit, mouse wheel.
S2d.1a done: F3 opens at once on the view core (ADR-0015; the prototype's whole-file reading and its
16 MB limit gone): FAR's status line `name│t│cp│size│Col n│pct%` (`FileViewer::ShowStatus`; the clock
at its right when shown), its key bar (`MViewF1`... for each modifier; F2 Wrap / Unwrap and F8 the code
page it would switch to, `UpdateViewKeyBar`), its keys of text mode (Up / Down, PgUp / PgDn and
Ctrl+Up / Ctrl+Down, Home / End and Ctrl+Home / Ctrl+End, Ctrl+PgUp / Ctrl+PgDn keeping the columns,
Left / Right, Ctrl+Left / Ctrl+Right by 20, Ctrl+Shift+Left), F2 wrap (from the line's start, column 0
wrapped), F8 ANSI / OEM (`F8CP`'s default order; ANSI after any other), F6 the editor, Esc / F3 / F10,
Gray + / -, the wheel (the system's lines, one with Alt; the tilt wheel's characters). Ours: highlighting
in the viewer too for text files (owner 2026-11-18, S2d.5).
S2d.1b done: F4 hex (`ShowHex`: `0000000000: ` the offset, the bytes in stripes of eight with `│`
between, two spaces, their characters; 16 a line, Alt+Left / Alt+Right by one and Ctrl+Alt+Left /
Ctrl+Alt+Right by sixteen to their multiples within the width, Left / Right its columns), dump (F2 in
hex: the characters only, a screen's width a row), Shift+F4's "View mode" (Text, Hex, Dump; dump chosen
makes F4 go back from hex to dump, `m_DumpTextMode`), Ctrl+Left / Ctrl+Right a character on in both, a
character cut by a row's end shown `›` for its bytes left; the status line's `t` / `h` / `d`, F2's and
F4's labels as FAR's `UpdateViewKeyBar`; the place kept between the modes (`AdjustFilePos`: text from
the row that shows the byte).
S2d.1c done: Shift+F8's "Code pages" menu (`codepages::SelectCodePage` with the viewer's flags):
"Automatic detection" (the code page told again from the file), "System" (ANSI, OEM), "Unicode" (UTF-8,
UTF-16 LE / BE), "Favorites", "Other" (the system's pages of characters up to two bytes), each line
`1251 │ ANSI - Cyrillic` (`*` for a name of the user's); Ins / Del a favourite, Ctrl+H hides "Other"
(the title ` *`), F4 "Rename the code page" (OK, Cancel, Reset; history `CodePageName`); the cursor on
the code page shown; favourites, names and Ctrl+H's mode kept with the settings (`codePages.*`, FAR's
`CodePages.Favorites`, `CodePages.Names`, `CPMenuMode`). The separators carry their titles.
S2d.2 done: Alt+F8's "Go to" (`GoToRowCol`: the edit with its `LineNumber` history, the last line in it,
"Hex value"; `GetRowCol`): an offset, `50%`, relative `+` / `-`, hex by `0x`, `$`, a trailing `h` or the
box (`m`: decimal), the column after ` .,;:` for lines not wrapped; text from the row that shows the
byte (`AdjustFilePos`). Ours (ADR-0015 addendum): a trailing `l` makes it a line (`1500l`), found by the
line index - started at the first such go to (a file is never read whole only for being viewed),
"Counting lines..." with Esc while it goes on; a line past the last shows the end; relative lines not
taken.
S2d.3 done: F7's "Search" (`GetSearchReplaceString` with the viewer's props: Text / Hex turned through
the code page shown, "Search for" with its `SearchText` history, "Case sensitive", "Whole words",
"Fuzzy search" (off with a regular expression), "Regular expressions", "Find previous" / "Find next" /
"Cancel"; kept for the session's viewers as FAR's shared parameters); Shift+F7 and Space go on forward,
Alt+F7 back (`DoSearchReplace`: from what was found, else from the screen's start or end); at the end
FAR's "End of the document reached / Continue from the beginning of the document?" (back: "Begin ... /
... from the end") with Yes and Cancel (FAR's default `SearchWrapStop`); "Could not find the string"
(the bytes); what is found selected in `text.selected` (FAR's `Viewer.Text.Selected`) and shown in
place when on the screen, else from its row a quarter of the screen from the top (`SelectText`, lines
not wrapped scrolled to it). Done in the background through windows read by offset with Esc
("Searching for "text"" in the waiting window; FAR's percentage not shown).
S2d.6e: Shift+F2 word wrap as FAR's `Viewer::ReadString` with `m_WordWrap` (a row ends after its last
blank or `WordDiv` character that fits, or at the width; its trailing blanks not shown, the next row's
leading ones skipped), wrapping a line not wrapped (`ProcessTypeWrapMode`); F2's label "Unwrap" /
"WWrap" / "Wrap" and Shift+F2's "WWrap" / "Wrap" as FAR's; F2 and Shift+F2 set FAR's `IsWrap` and
`Wrap` (`viewer.wrap`, `viewer.wordWrap`) so the next viewer opens with them; the wrapping kept per
file with "Save wrap mode" (`m_mode_wrap_words` too).
S2d.6f: bookmarks as FAR's in the viewer (`BMSavePos`: the first byte shown and the first column) and
the editor (`m_SavePos`: the cursor's line, column, first column shown and row on the screen):
Ctrl+Shift+0..9 or RCtrl+0..9 sets one (ours Alt+Shift+0..9 too, as for the folder shortcuts: Windows
Terminal keeps Ctrl+Shift+1..9), Ctrl+0..9 goes to it; kept with the file by "Save bookmarks" (FAR's
`SaveShortPos`, on). Differs: the editor's bookmarks follow their line by its bytes (text put in or
taken out before them moves them, as FAR's lines do; an undo does not move them back, FAR's neither);
FAR's session bookmarks (macros and plugins only) wait for luamacro.
R7 review D7: the viewer's Ctrl+C / Ctrl+Ins / Ctrl+Num0 copy what the search found, Ctrl+U lets it
go, Alt+BS / Ctrl+Z go back through the places keys left (FAR's `UndoData`, 65 536 of them), Num5
closes it (out of byte editing).
S2d.6f: characters laid out by the console's cells in the viewer, the quick view and the editor (FAR's
`char_width` with "Full-width aware rendering", ADR-0025): a wide one (CJK) takes two cells, a row that
cannot hold it whole wraps before it (a space where it would be cut), a combining one joins the character
before it (the cursor and the edits take them together). Differs: always so (FAR's option is off by
default; the program draws through VT, where the terminal gives the widths anyway). Going up past a line
longer than the look-back starts on a double-byte page's lead byte, not its trail.
S2d.6f: Ctrl+B shows or hides the key bar and Ctrl+Shift+B the status line, in the viewer and the
editor (FAR's `ShowKeyBar` / `ShowTitleBar`, each its own for the viewer and the editor, kept in the
settings as `viewer.showKeyBar` and the rest; the key bar only with more than two rows, the status
line with more than one, as FAR's); Ctrl+O shows the user screen until a key is pressed, that key doing
nothing else (FAR's `ShowBackground` and `WaitKey`).
Full FAR viewer keys and look: status line at the top; F2 wrap / Shift+F2 word wrap; F4 hex / text; F6 switch to the
editor; F7 search, Shift+F7 / Alt+F7 next / previous; F8 OEM/ANSI toggle, Shift+F8 code page menu; Alt+F8 go to;
F9 viewer settings; F10 / Esc quit; Ctrl+Shift+0..9 / RightCtrl+0..9 bookmarks; Ctrl+F10 position the panel on
the file; Gray +/- next/previous file; mouse. Print (Alt+F5) - Later (deferred); F11 plugins - S2d.7.

## 11. Editor (F4) - stage 2d
Basic: status line at the top, F2 save, Shift+F2 save as, F10/Esc quit with a question, F7 search, Ctrl+F7
replace, Shift+F7 next, Alt+F8 go to line, Shift+arrows blocks, Ctrl+C/V/X (and Ctrl+Ins/Shift+Ins/Shift+Del),
Ctrl+Z / Ctrl+Shift+Z undo/redo, Ctrl+Y delete line, Ctrl+A select all, F8 / Shift+F8 code page.
Ours over FAR (owner, 2026-11-22): "Code editing mode" in "Editor settings" and Alt+Shift+F9's local
copy (`editor.codeEditing`, off: every key FAR's). On: Tab with a block over several lines indents its
lines (a tab, or the tab size's spaces with "Expand tabs"), Shift+Tab outdents them (alone: the cursor's
line); Enter keeps the line's indent, an indent more after `{`, `(` or `[` (the closing bracket after the
cursor on a line of its own), the blanks after the cursor dropped; a closing bracket typed with only blanks
before it takes an indent back; Home goes to the first character that is not blank, again to the column
0; BS in the leading spaces goes back to the previous tab stop; Ctrl+/ comments the lines out or in with
the file type's line comment (Colorer's type: `//`, `#`, `--`, `;` ...; none known: nothing); Alt+Up /
Alt+Down move the lines (FAR's vertical block keeps Alt+Shift+arrows). Each one undo step. Also (owner,
2026-11-22): brackets and quotes closed as they are opened (before a blank, a closing bracket, `,` `;` or
the line's end; a quote not after a word's character) and typed over when the next character is the one
typed; `(` `[` `{` or a quote over a block wraps it, the block kept on its text; BS between an empty pair
takes both; Ctrl+Shift+D puts the lines again after them, the cursor on the copy; Ctrl+Space offers the
words of the lines around the cursor (64 KB each way, those read) that start as the one before it, the
most frequent first, in a menu under it; the bracket at the cursor and its pair and the word under the
cursor wherever the screen shows it whole are marked (`text.pair`, `text.occurrence`; on the rows shown
only); "Ruler column" marks a column down the screen (`text.ruler`, 0: none); "Clean line ends on save"
(on) takes the lines' ending blanks out and gives a last line its break before F2 / Shift+F10 (a document
up to 16 MB). FAR has no such mode: Tab types over a block, Shift+Tab does nothing, Alt+I / Alt+U shift a
block a column. Code folding and several cursors: deferred by the owner (2026-11-22).
Alt+F8 takes a line, a percentage or an offset (ADR-0015 addendum). Binary (hex) editing - Take, after the
basic set (owner, 2026-11-13); FAR has none, Volkov Commander's taken (ADR-0027). S2d.8a done: the
editor's Ctrl+F4 (ours; F4 stays FAR's quit; Alt+F4 at first, moved as it closes Windows Terminal's
window - ADR-0027 addendum) shows the document as VC's rows - an 8-digit offset (more
past 4 GB), 16 bytes in fours, their characters (a zero `.`) - over the same piece table, any size at
once; the cursor on a byte's half in the hex column, Tab the text column; a hex digit overwrites the
high half then the low one, a character its bytes in the code page, nothing put in or taken out (Del,
BS and Enter move or do nothing, nothing typed past the end); each byte typed one undo step; the bytes
typed and not saved in the block's colours; the status line's `Off pos/size│byte` in hex; F2 keeps
the hex mode on its byte; Ctrl+F4 again shows the text at the byte; the key bar's Ctrl+F4 "Hex" /
"Text". Not there: the blocks, plugins' steps, Ctrl+F7, Alt+F8, the mouse, the highlighting and line
numbers. S2d.8b done: in the viewer's hex mode F2 turns byte editing on (differs: FAR's F2 there
goes to dump - dump is Shift+F4's) - the cursor on the first row's first byte, the rows from a
multiple of a row's bytes, the same typing and Tab; the bytes typed shown in place and in the
block's colours, kept beside the file (the viewer never holds it); F2 again writes them into the
file in place on a worker (only those bytes, Esc stops it), the editing going on; Esc / F3 / F10
leave it, asking "The bytes have been modified. Save?" (Yes / No / Cancel) when bytes are typed;
F4, Shift+F4, F6, Ctrl / Alt with Left / Right do nothing while it is on; the key bar's F2 "Edit" /
"Save". A plugin's file viewed is its extracted copy: the bytes written stay in the copy, and F6 does
nothing there (FAR's `DisableEdit` of a plugin's file). F6 closes the viewer once the editor is open (a
question before it - already loaded, read-only - keeps it while asked). F6 keeps the place both ways, as
FAR's: a new editor shows from the viewer's first row with the cursor on it, in its code page
(`StartLine -2` with the viewer's `FilePos`; an editor already open stays where it is), and the viewer
from the editor shows from the editor's first row in its code page (`GetCurPos`), whatever "Save file
position" says.
S2d.4 done (a-b): F4 opens a file of any size at once on a piece table over its unread original
(ADR-0015 §4): FAR's status line `name│*│cp│Ln n/total│Col n│Ch n│code` (`FileEditor::ShowStatus`;
the lines counted up to 16 MB, `?` past it until the pieces count their lines), its key bar for each
modifier (`MEditF1`...), FAR's defaults: the cursor past a line's end ("Cursor beyond end of line"),
blocks not persistent, Del removing the block, tabs of 8; the moves (arrows, Home / End, PgUp / PgDn,
Ctrl+Home / Ctrl+End / Ctrl+PgUp / Ctrl+PgDn, Ctrl+Left / Ctrl+Right), with Shift the stream block,
typing (in place of the block; past the end after spaces), Enter (the file's own break), BS, Del
(joining lines), Ctrl+Y, Ctrl+A, Ctrl+U, Ctrl+C / Ctrl+Ins, Ctrl+X / Shift+Del, Ctrl+V / Shift+Ins (the
file's breaks), Ctrl+Z / Alt+BS, Ctrl+Shift+Z (undo steps as FAR's since the S2d review: characters typed
one after another one step - `AddUndoData` - a move or another edit starting the next, a replacement, a
character typed over and a block typed over one step each, "All"'s replacements one step - `undo_block`); F2 (streamed into a file beside the original, put in its
place, the bytes not edited taken from what was read - a file up to 64 MB kept whole as it is read, as
FAR keeps its file whole - so a file another program rewrote in place meanwhile is never mixed in; one
not read whole by then is not saved, with the reason, "Saving the file" with Esc; since the S2d review as `write_atomic`: the exact name, the old
file's attributes, ACL and streams, every name of a hard-linked file, no link replaced; FAR's questions
first: "already exists. Do you wish to overwrite it?" for Shift+F2 onto another file, "The file was
changed by an external program. Save?" Yes / Save as / Cancel, "is a read-only file. Do you wish to
overwrite it?" taking the attribute off), Shift+F2 "Save file as" (R7 review D5: FAR's dialog - the
name, the code page, "Add signature (BOM)" for the Unicode pages, "Change line breaks to"; the text
written converted, the editor then in the new code page; FAR's `AddUnicodeBOM` taken as on), Shift+F10, Esc / F10 / F4 with FAR's "File has
been modified. Save?" in warning colours (Yes / No / Cancel), F6 to the viewer (edited: the same question,
Yes saves then views, No views the file as saved; not in the user menu's editor or
a plugin's file's, as FAR's without `FFILEEDIT_ENABLEF6`), Alt+F8 "Go to" (a line, `+` /
`-`, a percentage of the lines, the column). Differs for now: undo does not reach back past a save (the
file is read again after it), lines are cut at 10 000 characters as the viewer's; Ctrl+F7 and F8 /
Shift+F8 are S2d.4d.
S2d.4c done: F7 / Shift+F7 / Alt+F7 in the editor (`Editor::DoSearchReplace`): the viewer's dialog (its
parameters shared with it for the session), from the cursor (on past what was found when going on),
the edited document looked through on a worker with Esc, FAR's question at an end; found: the cursor at
its start, the text marked until the next key (FAR's defaults "Select found" and "Cursor at the end"
off).
S2d.4d done: Ctrl+F7 "Replace" (`GetSearchReplaceString` in replace mode: "Search for", "Replace with"
with their `SearchText` / `ReplaceText` histories, case, whole words, regular expressions, "Replace
previous" / "Replace next"), each match marked with FAR's question "Replace "x" with "y"" - Replace,
All (the rest without asking), Skip, Cancel; F8 / Shift+F8 in the editor (the code page menu shared
with the viewer): the file read again in the page while it is unedited; an edited document keeps its
page until it is saved (its edits are bytes in it; FAR converts its text) - said so.
S2d.6e: F7's dialog as FAR's editor's (no Hex: the editor's parameters have none; the viewer's Hex kept
for the viewer), "Word" / "Selection" putting the word at the cursor (`FindWordInString`) or the
block's first line (else the cursor's line) into "Search for", and "All": every match (each search
past the last) in FAR's list "Occurrences: N, lines: M" low on the screen, ten items at most, the
line and the first character in columns, the match marked; Enter goes to it, Ctrl+Enter goes and keeps
the list, F5 zooms, Shift+F5 hides the columns. S2d.9b: F4 / Alt+F4 put the lines found (each once) in a
new editor window on a file not there yet, FAR's `SaveFoundItemsToNewEditor` - "<name><.ext>.Filtered.txt",
or "<name>.Filtered<.ext>" for FAR's `SearchAllUseAltFileNameFormat` default masks - in the editor's code
page, the cursor at the item chosen (its match marked with "Select found"); Alt+F4 takes every item while
the list has no filter; the lines get the new file's line break. Not yet: Gray + (with the bookmarks, S2d.6f), the menu's filter
and horizontal scrolling (Ctrl+Numpad5's aligned matches). S2d.6f: Ctrl+Up / Ctrl+Down scroll the
editor under the list, as FAR's.
S2d.6e: undo over a save as FAR's: the document moves onto the saved file with its undo and redo (the
text deleted before the save read from the old file while it is there), the "changed" mark gone when
undone or redone back to the saved state (`UndoSavePos`), the cursor, screen, code page, lock and own
settings kept. Differs: past 64 MB of such deleted text the steps are let go and the file read anew
(FAR keeps the whole file in memory, the program does not, ADR-0015).
S2d.6f: FAR's editor keys from `Editor::ProcessKeyInternal`. Screen moves: Ctrl+Up / Ctrl+Down scroll
the text a line keeping the cursor on it, Ctrl+N / Ctrl+E to the screen's first / last line, Ctrl+PgUp /
Ctrl+PgDn to the file's first / last line keeping the column. Line keys: Ctrl+BS / Ctrl+Del (Ctrl+T)
the word to the left / right (`IsWordDiv`: FAR's word dividers and NUL, the blanks before them taken
with them), Ctrl+Shift+BS the line to the left, Ctrl+K the line to its end, Ins's overtype with FAR's
big cursor, Ctrl+Q's next key put in as its character (the status line's `"` while it waits). Block
keys: Ctrl+D takes the block out, Ctrl+P copies it to the cursor, Ctrl+M moves it there, Alt+U / Alt+I
shift its lines (or the cursor's) a column left / right as one undo step (`BlockLeft` / `BlockRight`:
the leading blank out, a tab one column less of spaces; a space before a line not empty), Ctrl+Gray +
adds the block's text to the clipboard's (`Copy(TRUE)`). With persistent blocks the block stays where
it is when the cursor moves or the text is edited (it follows its text), and Shift+arrows from a block
left so begin a new one, as FAR's. Ctrl+C / Ctrl+Ins with no block mark the cursor's line with its
break and copy it (FAR's `KEY_CTRLC`); BS takes a block out with "Del removes blocks" (FAR's "BS like
Del", on).
S2d.6f: FAR's vertical blocks (`BTYPE_COLUMN`): Alt+Shift (or Alt) with the arrows, Home / End and PgUp /
PgDn mark one from the cursor (`ProcessVBlockMarking`, `AdjustVBlock`: past a line's end too, turning
over when the cursor passes its other side), Ctrl+Alt+Left / Right by words, Ctrl+Alt+PgUp / Home and
PgDn / End to the file's ends, Ctrl+Alt+Up / Down as Alt+Up / Down. Its columns are marked past the
lines' ends; Ctrl+C / Ctrl+Ins copy it as FAR's `VBlock2Text` (each line's columns padded with spaces,
a break after each) marked on the clipboard as FAR's, Borland's and Microsoft's vertical blocks
(`FAR_VerticalBlock_Unicode`, `Borland IDE Block Type`, `MSDEVColumnSelect`; any of them read back),
Ctrl+Gray + adds to it; Ctrl+V of a vertical block puts each line in at the cursor's column on the next
line down, lines added at the file's end (`VPaste`); Ctrl+D, Ctrl+X / Shift+Del and Del / BS with "Del
removes blocks" take it out (`DeleteVBlock`, the cursor to its top left without persistent blocks);
Ctrl+P / Ctrl+M copy / move it; Alt+U / Alt+I move the character beside it over to its other side
(`VBlockShift`: a tab beside it turns the line's tabs to spaces, the line's trailing blanks go); Enter on
its last line and Ctrl+Y on it change its lines as FAR's counts. Each is one undo step. Differs: any key
but a vertical block's move ends its marking (FAR goes on marking after keys that do not let the block
go, from a corner the text may have moved; here the next Alt+Shift move begins a new block, as Shift
does for the stream block); its lines are kept by their bytes (ADR-0015), so an edit joining them keeps
the block on the lines the text is on.
S2d.6f: the dialog lines' path keys in the editor's line (FAR's `Edit::ProcessInsPath`): Ctrl+[ / Ctrl+]
the left / right panel's folder, Ctrl+Shift+[ / ] the active / passive one's, Ctrl+Alt+[ / ] and
Alt+Shift+[ / ] their network names, Shift+Enter / Ctrl+Shift+Enter the name under the active /
passive panel's cursor (quoted with spaces), in place of a stream block that is not persistent;
Ctrl+F the file's full name (FAR's `FileEditor` `KEY_CTRLF`: a block not persistent taken out, a
vertical one too, any block let go). RCtrl+0..9 stay the bookmarks, as FAR's editor has them.
S2d.6f: the editor's mouse (FAR's `Editor::ProcessMouse`, `Edit::ProcessMouse`): the wheel scrolls as
Ctrl+Up / Ctrl+Down by the system's lines a notch (a line with Alt), the tilt wheel moves the cursor
left / right; the left or right button on the text puts the cursor on the row and column (a block not
persistent let go, past the last line on it), a double click marks the word there (FAR's
`FindWordInString`) and a click on the same place within the double-click time after it the line's
text; on the scroll bar the top arrow is Ctrl+Up, the bottom one Ctrl+Down, the rest goes to the line
as far down the file (lines not counted: nothing). Differs: an arrow scrolls once a press (FAR goes on
while the button is held).
S2d.7a: F11 in the editor is FAR's "Plugin commands" of the plugins' editor items (`PF_EDITOR`, ADR-0026:
the program's own menu as the panels' F11, sorted, hotkeys, F3 / F4); one chosen runs with the editor
up and reads and changes it through FAR's `EditorControl` commands (API 10: get / set a line, Enter,
Ctrl+Y, typed text, Del, the cursor and the screen, the block, one undo step, tabs as spaces, the
positions, the file's name), its edits and the keys typed meanwhile in their order.
Full FAR editor keys and look: status line at the top; F2 save, Shift+F2 save as, Shift+F10 save and quit, F10 / Esc
quit with a question; F6 switch to the viewer; F7 search, Ctrl+F7 replace, Shift+F7 / Alt+F7 next / previous;
F8 / Shift+F8 code page; Alt+F8 go to line; F9 editor settings; stream blocks (Shift+arrows) and vertical blocks
(Alt+Shift+arrows); Ctrl+C/Ctrl+Ins copy, Ctrl+V/Shift+Ins paste, Ctrl+X/Shift+Del cut, Ctrl+D delete block,
Ctrl+P copy block, Ctrl+M move block, Ctrl+U unmark, Alt+U/Alt+I shift block; Ctrl+Y delete line; Ctrl+Z /
Alt+Backspace undo, Ctrl+Shift+Z redo; Ctrl+Backspace / Ctrl+Del delete word; Ctrl+A select all; bookmarks;
mouse. Code pages (OEM, ANSI, UTF-16, others the OS knows) make non-UTF-8 files editable; until then they stay
read-only (ADR-0003).

## 11a. From FAR's distribution beyond its core (owner, 2026-11-13)
- **Syntax highlighting** (FAR: the Colorer plugin, in the editor only) - Take, stage 2d: in the editor and,
  over FAR, in the viewer on text files. It must keep ADR-0015 (only the screen laid out, never the whole
  file): a highlighter that goes line by line with its state kept at checkpoints, as the line index is.
  S2d.5a-d done (ADR-0024): Colorer's regular expressions, HRC library and text parser ported into
  `editor::syntax`; Colorer-schemes' base shipped as `hrc\`; the editor coloured. Differs from
  FarColorer: (since the S2d review the type is chosen as FarColorer's `chooseFileTypeCh`: the file's name
  and its first `firstlines` lines, `firstlinebytes` characters of them); a screen beyond what was parsed is coloured from 200 lines above it, exact once
  the states reach it (FarColorer leaves lines more than `backparse` 2000 beyond its parse uncoloured);
  the colours are the themes' `syntax.*` roles (FarColorer's HRD files and its colour settings are not
  taken, ADR-0008); regions are painted to the line's text, not past it. Shown coloured at once, as FAR
  (owner 2026-11-21, the S2d review): the catalog is read as the program starts and the type of the
  file under the panel's cursor ahead on a worker, a type read already colours in the tick the text
  comes in, and a viewer or editor just opened is not drawn until its text is coloured (1.5 s at most;
  FAR reads the type before it shows the window); a type's load builds only the schemes it adds or
  changes (Colorer rebuilds every scheme's search lists). FarColorer's menu is the
  editor's F11 "Colorer" (S2d.7e1 done: its full list with a type taken, the short one without, only
  "Configure" with the highlighting off; "List types" is its "Select syntax" - "Automatic detection",
  "Disable", the favourites, the groups titled, `&K` hotkeys, Ins / Del favourites, F4 "Define key",
  kept as `syntax.favorites` / `syntax.hotkeys`; "Update highlighting"; "Reload schema library" reads
  the catalog anew in the background, the highlighting back when it is read; "Configure" opens the
  program's "Editor settings" - FarColorer's own dialog's colour styles and paths are not taken,
  ADR-0008). S2d.7e2 done: "Match pair", "Select block", "Select pair" (Colorer's `searchPair`
  over the `def:PairStart` / `def:PairEnd` regions, the cursor's end counted in), "Select region"
  and "Region info" (FarColorer's `cursorRegion` from the line's regions compacted as
  `LineRegionsCompactSupport`; "Region name" with the region and its scheme); a pair is looked for
  through the whole document, in one too big for the kept states 2000 lines on (FarColorer's
  backparse). S2d.7e3 done: "Outliner list" and "Errors list" (Colorer's `Outliner` over the
  `def:Outlined` / `def:Error` regions, FarColorer's menu `Outliner [filter]` - the filter typed, BS,
  Tab's completion, Ctrl+Left / Ctrl+Right levels, Ctrl+Up / Ctrl+Down moving the editor, Enter,
  Ctrl+Enter typing the item, Esc putting the cursor back), "Find Function" (its word at the
  cursor, the item elsewhere first; "Nothing found" with OK for FarColorer's "Gotcha" button); the
  outline is kept and collected on after an edit from the last state kept, none in a document too
  big to keep states for (FarColorer parses the whole file). The switch in F9 (S2d.6).
  S2d.5e done: the viewer's text mode coloured as the editor (FAR's viewer has none, the owner's
  choice), each row of a wrapped line from its own cells; hex and dump not; another code page (F8,
  Shift+F8) parses again from the start.
  S2d.6a: Options > "Viewer settings" with FAR's "Tab size" and "Maximum line width" (its validators:
  1-512, else 8; 1 000-100 000) and the program's "Syntax highlighting"; FAR's other options of the
  dialog come with what they switch (external viewer, positions and bookmarks kept, arrows, '\0',
  scroll bar, persistent selection, dump detection, code pages), as Alt+Shift+F9's local copy.
  S2d.6b done: all of FAR's "Viewer settings" but "Save bookmarks" (with the bookmarks, S2d.6f), as
  FAR's: the external viewer on F3 or Alt+F3 (after the file's association), persistent selection,
  scrolling arrows (`Viewer.Arrows`), visible '\0' (`ZeroChar` `·`), the scroll bar (Ctrl+S too), the
  places kept between runs (FAR's `FilePositionCache`, 512 files; in `positions.json` beside the
  settings), dump detection (a NUL in the first 2 KB), autodetected and default code pages;
  Alt+Shift+F9 sets the open viewer alone. Differs: the default code page's list has ANSI, OEM, UTF-8
  and UTF-16 (FAR lists every code page the system has); the dialog is one column (FAR's two); "Save
  file code page" is not greyed out under "Save file position" (it is kept either way, as FAR's).
  S2d.6c-d: Options > "Editor settings" as FAR's `EditorConfig` with what it switches: the external
  editor (F4 / Alt+F4), expand tabs (new ones, or all of an edited line as `Edit::ReplaceTabs`),
  persistent blocks, Del removes blocks, auto indent, tab size, cursor beyond end of line, the search's
  "Select found" / "Cursor at the end", white space shown (`·`, `→`, `♪`, `◙`), the scroll bar, line
  numbers (Ctrl+F3, `Editor.LineNumbers`), "Save file position" (the cursor, the screen and the code
  page, `editor-positions.json`, 512 files), "Lock editing of read-only files" (lock mode, `-` on
  the status line, Ctrl+L lets it go or sets it; Ctrl+F7 does nothing in it), "Warn when opening
  read-only files" ("is a read-only file / Do you wish to edit it?"), the code pages, highlighting;
  Ctrl+F3 and Alt+Shift+F9 change the open editor alone (FAR's editor `EdOpt`). "Save bookmarks" comes
  with the bookmarks (S2d.6f); "Allow editing files opened for writing" is not taken: the program
  always opens a file sharing writes (Rust's default), so it is always on. Differs: a kept place is
  by the lines' bytes, not their numbers (a file past the line count's bound is never counted; a file
  changed since puts the cursor at the start of the line holding the byte); line numbers are not
  shown for a file past that bound.
- **sftp** - stage 6 takes the mechanics of NetBox (FAR's distribution, open on GitHub): its sessions and
  protocols. The program is MIT (owner, 2026-11-13) and NetBox is GPL: its behaviour is the model, none of
  its code is taken; the protocols come from Rust libraries under permissive licences. The plugin is
  **NetWay** (`plugins/netway`, prefix `netway:` and the protocols' own; owner 2026-11-17, ADR-0023), its
  steps S6.4a-S6.6d, background transfers S6.8 later; NetBox's sources read in `netbox-source/`.
- **FAR's own plugins** (`far-source/plugins`, owner 2026-11-13), each a plugin on the program's API as
  TmpPanel: tmppanel (done); samefolder and filecase (done, P6.1); farcmds as `commands` (done, P6.2); emenu (done, P6.3); compare (done, P6.4); arclite (as `archive`: browsing zip done, S6.1a; extraction done, S6.1b; nested archives, properties done, S6.1c; writing into an open archive done, S6.2a; deleting, folders and
moving out done, S6.2b; new archives done, S6.2c; testing and the menu's Extract / Test done, S6.2d; Shift+F2 / F3 on a disk's files done, S6.2e; the `arc:` commands done, S6.2f; 7-Zip's library and browsing its formats done, S6.3a; their extraction, test and the formats' configuration done, S6.3b; writing every format but zip done, S6.3c; zip through the library done, S6.3d; SFX, partitions and the open warnings done, S6.3e; Alt+F7's search in archives done, S6.7); proclist (the local panel, its text, the tree and friendly names done, P6.5a-b, P6.5d; another computer put off, P6.5c; Ctrl+W's task list done, P6.5e), network (browsing done, P6.6a; connecting, up from a share, favorites P6.6b-d); editcase (done, S2d.7b: "Change Case" in the editor's F11, FAR's five conversions on the block or the word at the cursor or the nearest one, its cyclic change by the first word, its `AddWordDiv`), brackets (done, S2d.7b: "Brackets matching" in the editor's F11 and in "Plugins configuration", FAR's single and double brackets, after the bracket, quotation marks as their own pair with "Forward" / "Backward", "Search" and "Select", its "Configure" dialog and settings, "Beep if matched" the system's default sound through the program's `EDITOR_BEEP`), align (done, S2d.7b: "Align block" in the editor's F11, FAR's dialog, the stream block reformatted at the right margin under its least indent, "Do not leave short words at line ends", "Justify right border", or only justified; differs: one undo step), autowrap (done, S2d.7c: "Auto wrap" in the editor's F11 with FAR's dialog and settings, a line typed past the right margin broken at its last blank before it, again while too long, only in its masks and not its excluded ones; the editor's keys heard after it does them, ADR-0026), drawline (done, S2d.7d: "Draw lines" in the editor's F11 starts FAR's key loop - Shift with the arrows draws single or double box lines joining those around, Ctrl+Shift a tab's width, F2 single / double, Esc or F10 ends it, F3-F12 go nowhere, the other keys to the editor; its title and key bar while it runs; the drawing one undo step; ADR-0026's key loop; differs: the keys it passes on are not heard by AutoWrap's hook);
  luamacro (put off by the owner, 2026-11-14: "maybe some day"); hlfviewer (when the program has its own help). Not taken:
  ftp (NetBox's place), multiarc and newarc (arclite's), macroview, helloworld.

## 12. Deferred by the owner (no stage yet)
File descriptions (`descript.ion`, Ctrl+6/7, Ctrl+Z, Ctrl+F10 sort), file owners and links views (Ctrl+8/9,
Ctrl+F11), 8.3 names (Ctrl+N), video mode (Alt+F9), hotplug devices, printing (Alt+F5), wipe
(Alt+Del), Lua macros, UI languages other than English.
Not planned: FAR plugin API compatibility.

## 13. Plugins (stage 5, ADR-0016)
Stage 5a done (ADR-0016): plugins are loaded from `plugins\<name>\` beside the program (`plugin.json`: name,
version, API version, library), each on a thread of its own; `"plugins.<name>.enabled": false` in the settings
keeps one off. One with a broken manifest, another API version or a library that does not start is not loaded:
FAR's "Error / Error loading plugin" with its folder and the reason, the program going on. F11 is FAR's "Plugin
commands" (every plugin's items sorted by their text, Enter runs one on its plugin's thread); a plugin's message
window has FAR's look and OK, the plugin waiting for the answer; a command that fails or panics shows the
plugin's reason under its title. `cargo run -p xtask -- package <folder>` lays out the portable folder; `package target/debug` lays the plugins out beside the debug program (run it after a plugin or the API changes).

P6.0e (API 3, owner): plugins load and unload while the program runs - FAR's `PluginsControl`
(`PCTL_FINDPLUGIN`, `PCTL_LOADPLUGIN`, `PCTL_UNLOADPLUGIN`, farcmds' `load:` / `unload:`) as the host's
`plugins_control`, by name, folder, manifest or library; ours over FAR: Options > Plugins lists the plugins'
folders in a menu as F11's, each by its name with its state at the right in brackets in one column
(`[  loaded  ]` as the text, `[ starting ]` as the hotkeys, `[ disabled ]` and `[not loaded]` grey; a new folder too; owner, 2026-11-15), the keys in the bottom frame, Enter or
Space loads or unloads one (kept for the next start), Ctrl+R starts it anew; Ctrl+R in a plugin's wait window, once it does not answer
Esc, leaves the hanging call where it is and starts the plugin anew (tech-debt P6.0e). A library runs from a
copy beside it, so it may be replaced while the program runs.

Stage 5b-1 done (ADR-0016 §6-9): a plugin's windows drawn by the program. FAR's `Message`: its lines centred (or
from the left), a separator, the plugin's buttons with `&` hotkeys, the warning colours; the plugin gets the
button pressed, Esc or a window gone another way being Esc. A message without buttons (FAR's
`FMSG_KEEPBACKGROUND` progress window) returns at once and stays while the plugin's call runs, giving way to its
next window (ours: Esc takes it away, the keys stay the program's). FAR's `Menu` (`VMenu2` in the middle of the
screen): separators, check marks, grey lines (`Menu.GrayText`, the theme's `menu.disabled`) never chosen, `&`
hotkeys choosing at once, automatic hotkeys when asked (`FMENU_AUTOHIGHLIGHT`), the plugin's break keys by name
("F4", "Shift+Del", "Ctrl+Up") closing it as Enter does and telling which. FAR's `InputBox`: the prompt over a
line starting with the plugin's text. Every window waits its turn behind the program's own. Since the audit's
batch 5 (`docs/reviews/2026-11-12-stage5-far-audit.md`): a message as wide as FAR's `MessageImpl` makes it, `\1` / `\2` lines as
separators with titles, a line too long a read-only line, the system's error text (`FMSG_ERRORTYPE`), FAR's
button sets; a menu's grayed lines (the cursor stops, Enter does not take them) apart from disabled ones,
hidden lines, round the ends only when asked, `&` shown as it is, automatic hotkeys from the end; an input line
as FAR's `GetString` (an empty answer none unless taken, a password, "OK" / "Cancel", `%NAME%` expanded);
an empty column title shown empty. P6.0c: an input line's history (FAR's `HistoryName`, its last line unless
`FIB_NOUSELASTHISTORY`); a menu at the plugin's column and row with at most its `MaxHeight` lines; path
completion (`FIB_EDITPATH`) with the dialogs'. A plugin writes
its log (`settings\Logs\<plugin>.log`, moved to `<plugin>.old.log` past 1 MiB) instead of printing.

Stage 5b-2 done (ADR-0016 §11, owner): a plugin's dialog as FAR's `DialogInit` / `DialogRun` with its
`DlgProc`. Its items in their order - text, an edit line or a password line (stars; never copied), a check box
(with `[?]`), radio buttons (groups), a drop-down list, separators, buttons (the default one; one that does not
close, `DIF_BTNNOCLOSE`), each disabled, hidden or on the row of the one before - laid out by the program as
FAR's dialogs are (a label above its line, the buttons that end the dialog centred under a separator, at least
40 columns, at most the screen). The plugin's handler hears Init, an item changed (keys and the mouse alike),
a button that does not close and the closing (a button or Esc), sees every value, changes texts, states,
enables, shows and focuses items; the program waits for it only on closing, which its answer may refuse (a
message of the plugin's comes over the dialog meanwhile, the dialog back after it). The plugin gets the button
and every item's last value; a dialog gone another way is Esc.

P6.0c (API 3): a list box in the dialog (`DI_LISTBOX`: its lines in a box under its title, Enter closing the
dialog with it unless `DIF_LISTNOCLOSE`, the mouse putting the cursor on a line), a masked line (`DI_FIXEDIT`
with FAR's `9 N # A H X`), an edit line's history (`DIF_HISTORY`, `DIF_USELASTHISTORY`, added to as a button
closes the dialog, FAR's `CloseDialog`), the plugin's own place and coordinates (`X1`, `Y1`, `X2`, `Y2`, a text
at -1 centred), a list's lines replaced by the handler (`DM_LISTSET`), a dialog over a dialog (E14), each
key's change told at once, the focus moving (`DN_KILLFOCUS`, `DN_GOTFOCUS`), the dialog drawn anew
(`DN_DRAWDIALOG`, `DN_RESIZECONSOLE`), and with `DIALOG_KEYS` each key and click heard by the handler first,
which may take it (`DN_CONTROLINPUT`; tech-debt P6.0c for what differs).

Stage 5b-3 done (ADR-0016 §12): the plugins' items in the drive menu (FAR's `DiskMenuStrings`), under the
drives after a separator, each with the hotkey the user gave it with F4 ("Assign plugin hot key": FAR's
`SetHotKeyDialog`; none by default - corrected by the audit, E9) as "&7  Temporary panel", the lines by name
or, with F9's "Sort plugins by hotkey", by hotkey. Differs from FAR (owner, 2026-11-22): FAR gives only its
first-start keys (`Options::SetDriveMenuHotkeys`: ProcList 0, TmpPanel 1, NetBox 2, Network 3) and leaves
every other item without the user's key blank ("   Temporary panel"); here every item without the user's
key gets one as the menu opens (not saved) - FAR's first-start key for its plugin when it is free, else the
first free digit, then letter no drive in the menu has, in the lines' order by name;
Ctrl+7 and F9's "Show plugins" show or hide them; the arrows pass the separator; Enter, the mouse or the
hotkey runs the item on its plugin's thread (a failure shown under its title) until 5c gives plugins panels.

Stage 5c-1 done (ADR-0016 §13, owner): a plugin's panel, as FAR's `OpenW` / `GetFindData` / `ClosePanel` but
by path (`<plugin>://<instance>/<path>`, a `vfs` provider whose calls run on the plugin's thread). A drive
menu item opens it in the menu's panel, an F11 command in the active one; it is read in the background as
any folder (Esc stops waiting for the plugin), its title is the plugin's (FAR's `PanelTitle`), its folders
are entered and left, ".." at its root leads back to the folder the panel came from (the cursor kept as
FAR's), a folder the plugin refuses keeps the panel where it is with the plugin's reason; the plugin hears
when no panel shows it any more (however the panel left it); the settings save the folder it came from.

Stage 5c-2a done (ADR-0016 §13): a plugin's panel's files through its plugin, as FAR's `GetFiles` /
`PutFiles` / `MakeDirectory` / `DeleteFiles` by path - quick view (and every reader) reads them by offset,
F5 / F6 copy and move out of it and into it (a file written new, then appended in the transfer's chunks),
F6 to a new name in its own folder renames there, F7 makes a folder in it, F8 removes from it (no Recycle
Bin: deleted for good, as the question says); the copy's target line is the plugin's `<Format>:` (FAR's
`copy.cpp`: left as it is it copies into the panel, edited it is an ordinary target; fixed by the audit, E1).
P6.0d (API 3): what goes in keeps its times and marks (`set_times`, FAR's `PutFiles` items; a folder's at the
operation's end), F7 makes a nested `a\b` as FAR's `ShellMakeDir` (the folders on the way that are there
taken, the last made, a leading `\` from the panel's root, the cursor on the first), and a plugin that
renames nothing is overwritten in place (tech-debt P6.0d).

Stage 5c-2b done (ADR-0016 §13, owner): a plugin's real names, as FAR's `PFLAGS_REALNAMES` - an entry may
carry its file on a disk (an absolute path); quick view, F3 / F4, Enter, Ctrl+Enter and F5 / F6 take that
file directly, never through the plugin, while F8 asks the plugin (FAR's TmpPanel takes the item off its
list and leaves the file); a name typed in F5 / F6 for real files is beside them on their disk (a rename
there, never a copy into the plugin with the file then removed); F3's count and quick view take a real
folder on its disk.

Stage 5c-3 done (ADR-0016 §13, owner): a plugin's own panel modes, as FAR's `PanelModesArray` - with the
title the plugin gives the modes of its panel at Ctrl+0..9 in the program's panel-mode strings (column
types and widths, the columns' titles, the status line's columns, full screen, aligned extensions, the
detailed status line, case conversion); a mode it gives replaces the program's at that key (FAR's
`PrepareViewSettings`: no status columns is the name, or the name, size, date and time; the program's
cases only with case conversion), elsewhere the program's stands. Ctrl+6..9 show its modes there (the
program has none of its own at these keys yet) and do nothing on a panel without one. Each entry may carry
texts for `C0`..`C99` and a description for `Z` (FAR's `CustomColumnData`, `Description`), cut to their
columns with no mark. The view the panel had before it entered the plugin comes back as it leaves it and is
the one saved meanwhile (FAR's `PopPlugin`).

Stage 5d-1 done (ADR-0016 §14, owner): the temporary panel, FAR's TmpPanel, shipped as the plugin
`tmppanel` - "Temporary panel" in F11's menu and the drive menu opens the current one of its ten common
panels ("Temporary panel [0]", kept while the program runs) in its own mode at Ctrl+4 (FAR's `StartPanelMode`;
`N,S` / `0,8`, the status line `NR,SC,D,T` / `0,8,0,5`). Its entries are files and folders from anywhere,
named by their whole paths (in the program's modes their names only, sorted by them: FAR's
`OPIF_SHOWNAMESONLY`), each with its real name; F5 / F6 into it show FAR's copy dialog with the target
"Temporary panel:" (its `Format`): left as it is the paths go to it (FAR's `PutFiles`; a move removes
nothing), edited it is an ordinary copy there (the tree's Shift+F5 / Shift+F6 never offer it, its Alt+F6
does nothing there, as FAR's `treelist.cpp`); "Sending files to temporary panel" while it takes them;
folders go without their contents by TmpPanel's default (`CopyContents = 0`; on: with them, asked: "Copy
folder contents?", since 5d-3b); F8 on it deletes the real files as on their disk (FAR's; the owner's choice over
5c-2b's "off the list") and their names drop off as it is read, as do those of files gone otherwise; Enter on a folder goes
to that folder on its disk.

Stage 5d-2 done (ADR-0016 §14-15): a plugin's panel's own keys, as FAR's `ProcessPanelInput` and `KeyBar` -
the panel names them with their labels; one pressed goes to the plugin before the program's keys (Enter only
with an empty command line, `FileList::ProcessKey`), with the entry under the cursor, the selection and the
other panel's folder, the screen never waiting; the answer reads a panel again, takes the other panel to a
folder or to a file's folder with the cursor on it, or shows another of the plugin's panels; the key bar
shows the labels for the modifiers held. The temporary panel's (`TmpClass.cpp`): F7 "Remove" (names off the
list, the files stay), Alt+Shift+F3 "Goto", Alt+Shift+F12 "Switch" ("Available temporary panels", "&n. Total
files: k") and Alt+Shift+0..9, Alt+Shift+F2 "SavLst" ("Save file list as", the other panel's
`panel<n>.temp`, a name typed without a folder in the one the temporary panel was opened from, UTF-8
with its BOM, CR LF); the switch goes by the panel it is pressed on and keeps its view.
Ctrl+PgUp (and Enter on "..") at a plugin's root with real names goes back where the panel came from, the
cursor on the entry's name there (FAR's `ChangeDir`, `GoToPanelFile`). A plugin's panel opened over
another closes it and comes with the view, sort, its order and "directories first" the panel had before the
first (FAR's `ChangePanel` with `CreateNew`), which come back with its folder as ".." or a disk's folder
leaves it (`PopPlugin`); a plugin may read and delete its real names itself (FAR's `UseInternalCommand`). Its settings, list files
and the `tmp:` prefix, and Alt+F7's "Panel" come in 5d-3..5d-5.

Stage 5d-3a done (ADR-0016 §14): a plugin's settings and configuration, as FAR's `PluginSettings`,
`ConfigureW` and `plugins.cpp` `Configure` / `ConfigureCurrent` - a plugin's values (text by key) come from
the program's settings (`plugins.<name>.settings.<key>`) and those it sets are saved with them;
Options > "Pl&ugins configuration" (after "Interface settings", as FAR's `OptionsMenu`) lists every plugin's
configurations as F11 lists its commands (sorted, wrapping, automatic hotkeys, "Shift+F1, F4, F3", F4 its
own hotkeys), Enter runs one and the menu comes back when it ends, closing only on Esc; F11's Shift+F9
configures the item's plugin (its first configuration) and F11 comes back, its Alt+Shift+F9 opens "Plugins
configuration" with F11 back at its line when that closes. A configuration that changed the settings has
the plugin's menu items read again and every plugin's panel shown read again (FAR's `ConfigureCurrent`,
`Update(UPDATE_KEEP_SELECTION)`). The temporary panel's options (`TmpCfg.cpp`: FAR's keys and defaults,
read at the start, at every open and before its dialog, written on OK) and its dialog "Temporary panel"
(FAR's labels; its two columns one after another; "Copy folder contents" with three states): "Add to Disks
menu" and "Add to Plugins menu" show its items, its mode at Ctrl+4 takes the column types and widths, the
status line's and full screen from them, SavLst's extension is the list files' first mask's (the other
options since 5d-3b; the list files, the prefix and search results with 5d-4 and 5d-5). Not yet: Shift+F1 and F3 in these menus (help and plugin
information), the edit lines' histories.

Stage 5d-3b done (ADR-0016 §14): the temporary panel's options in effect, read in FAR's sources
(`TmpClass.cpp`, `TmpMix.cpp`). With "Use common panel" (FAR's default) every temporary panel shows the one
current of the ten common lists, " [n]" in its title; Alt+Shift+F12 "Switch" and Alt+Shift+0..9 make another
current (the menu with the current one selected) and both panels are read again; without it each panel
opened has a list of its own, gone when it closes, no " [n]", no Switch, SavLst offering `panel.<ext>`.
"Safe panel mode": no real names, "(R) " before the title, Ctrl+PgUp to the entry's folder with the cursor on
it, Enter on a folder to that folder, F3 / F4 / F5 / F6 / F8 and Del / Shift+Del / Alt+Del doing nothing on an entry (FAR's TmpPanel has no
`GetFiles` / `DeleteFiles`). "Any elements in panel": entries whose files are gone stay, Enter on one puts its
text in the command line (its help's; deviation D10). "Copy folder contents": folders put with everything in
them, or asked once per put ("Warning", "Copy folder contents?", Yes / No, Esc as No). F3..F8 but F7 do
nothing on an entry whose file is gone (the plugin's answer; other keys it leaves to the program), F7 has a
quick view beside it show its file again. Names on the list compared as written (FAR's `RemoveDups`); with
"ListUTF8" off SavLst writes UTF-16 with its BOM. Deviations D10, D12, D13 in `docs/reviews/2026-11-12-stage5-far-audit.md`. The list
files, the `tmp:` prefix and Alt+F7's "Panel" come in 5d-4 / 5d-5.

Stage 5d-4 done (ADR-0016 §14): a plugin's command line prefixes and the files it opens, read in FAR's
sources (`plugins.cpp` `ProcessCommandLine` / `OpenFilePlugin`, `pathmix.cpp` `GetPluginPrefixPath`,
`filelist.cpp` `ProcessEnter`; TmpPanel's `OpenW`, `ReadFileLines`, `ShowMenuFromList`, `TmpMix.cpp`). The
command line: `prefix:text` (the prefix before the first `:`, never a drive's letter) goes, after the history
took it and echoed on the user screen, to the first plugin that has the prefix, before the system's own
commands. Enter on a file after its associations (a file that is no program) and Ctrl+PgDn after its own: the
plugins whose masks it has, in their order, until one takes it; the program's own way after them. A panel
opened from a file goes over the panel; ".." at its root comes back with the cursor on the file and the
selection as it was (FAR's `PushPrev`, `PopPrevData`). The temporary panel: `tmp:` with `+safe` `+any`
`+replace` `+menu` `+full` (and `-`), `+0`..`+9` (compared as written, kept until the options are read
again); `tmp:list` (from the active folder, then the plugin's folder, the program's and `%PATH%`; one not found
does nothing), `tmp:<command` (its output as the list); a list file (`*.temp`, not empty) on Enter or
Ctrl+PgDn. A list in its encoding (UTF-16 or UTF-8 by their marks, UTF-8 by its bytes, UTF-16 by its zeros,
else the OEM page), empty lines dropped, `%VAR%` expanded, `|label|` dropped, names from the active folder;
Replace (the default) empties the panel first. "Menu from file list": `|label|command` lines (`-` a
separator), titled by the file's name without its extension; Enter takes the active panel to a folder, opens a
file by the system, puts anything else in the command line; Shift+Enter opens it by the system. Deviations
D15-D18 in `docs/reviews/2026-11-12-stage5-far-audit.md`; a window a plugin's call opens no longer loses its answer (D14).

Stage 5d-5 done (ADR-0016 §14): Alt+F7's "Panel" (FAR's `findfile.cpp` `FD_BUTTON_PANEL`,
`OpenFindListPlugin`; TmpPanel's `SetFindList`, `FindSearchResultsPanel`). The button is on once something is
found and some plugin takes found files (`AnySetFindList`); it closes the results (the search stops) and hands
the found files - folders too when they were looked for, not by bytes - to the first plugin that takes them,
its panel in place of the active one's. The temporary panel empties the list first whatever Replace says; with
the common panel and "New panel for search results" the first empty common panel takes them, else the next in
turn, and it becomes the current one. Deviation D19. F3 in F11's menu and in "Plugins configuration": FAR's
"Plugin information" (title, description, author, version, module path, the plugin and the item, the prefixes;
read-only lines, OK back to the menu at its line; deviation D20). Shift+F1 (a plugin's help) waits for the
program's help.

P6.1 done: FAR's samefolder and filecase, shipped as the plugins `samefolder` and `filecase`. "Same
Folder" (F11; the drive menu when its configuration says, `Add2DisksMenu`): the passive panel goes to the
active one's folder, from a side's drive menu that side to the other's, the cursor on the same item made
the top line (`SameFolder.cpp`). "Case conversion" (F11): FAR's dialog at its places (`filecvt.cpp`: the
name's and the extension's five cases side by side, "Do not process mixed case files", "Process
subfolders", "Process folder names", "Current run only", the word delimiters with "Reset"), the selected
files or the one under the cursor renamed (`ProcessName.cpp`), the panel read again without its
selection. What differs: tech-debt P6.1.

P6.2a done: FAR's farcmds shipped as the plugin `commands` ("Command prefixes"; `OpenCmd.cpp`). `view:`
and `edit:` (`[line,column]`, variables in them) open a file; `goto:` takes the active panel to a path, its
cursor on the name made the top line (`<file`: the path on the file's first line, or a shortcut's target);
`whereis:` looks in the current folder, the program's, `PATH`, the system's folders and `App Paths`, with
`PATHEXT`; `clip:` puts a file's text in the clipboard (its byte order mark, UTF-8, UTF-16, else the code
page; at most "The size of read data"); `load:` / `unload:` load and unload a plugin; `link:` makes a hard
link, a junction, a symbolic link (`/s`) or mounts a drive, the destination's folders made, errors told with
`/msg`, the panels read again unless `/n`. The configuration is FAR's dialog at its places. A wrong command
shows its syntax. What differs: tech-debt P6.2a.

P6.2b done: `view:<command`, `edit:<command`, `clip:<command` and `run:file<command` run the command
through `%COMSPEC% /c` (in `|folder|` when given, the program found as `whereis:` finds it) with its
standard output and errors in files as asked (`*` both as one, `1`, `2`, `?` each its own; the
configuration's default), a window telling the output's size every 2 s, Esc ending it with all it
started; then the viewer or the editor on the output (deleted as it closes; empty ones as "View/edit empty
files" says), the clipboard, or `run:`'s file. What differs: tech-debt P6.2b.

P6.3 done: FAR's emenu shipped as the plugin `emenu` ("EMenu"): Explorer's context menu for the panel's
selected files and folders (the one under the cursor first; on `..` the folder itself; files of different
folders refused as FAR's), or for one of This PC's items (the file system's first), as a text menu in the
program (submenus filled as they open, Right or Enter into one, Left, BS or Esc back; the items' help or
verbs after them as configured; items their objects draw by their help or verb) or as Windows shows it (at
the mouse or the window's centre), or FAR's menu choosing one. From F11 (its main menu: "Context menu", "My
Computer"), Apps and Shift+Apps on a panel or a drive of the drive menu (the text menu unless the GUI one is
configured), a right click on an entry or a drive (the menus at the mouse, shown as the button is let go;
Panel settings' "Right click selects files" off, FAR's default; on an entry Ctrl or Alt with it selects), and the prefixes `rclk:`, `rclk_txt:`, `rclk_gui:`, `rclk_cmd:<verb>`, `rclk_item:<text>`
(files after them, `%variables%` expanded). "rename" is Shift+F6, "delete" F8 (Shift+Del, Alt+Del) when so
configured, a command run is followed by FAR's message and the selection taken off as configured; FAR's
configuration dialog at its places, Alt+Shift+F9 in the menus. The shell through the SDK's `system::Shell`
(a thread of its own with OLE, Esc ending a wait). What differs: tech-debt P6.3.

P6.4 done: FAR's compare shipped as the plugin `compare` ("Advanced compare", F11): the two file panels'
items - all, or the selected ones only - compared by name (case not told apart, folders with folders), by
time (two seconds' precision, a whole number of quarter hours up to 26 hours ignored as a time zone's), size
and contents (byte for byte, or ignoring the new line characters' differences, or white space), in
subfolders down to a depth, each panel's filter as it is; the items that differ or have no pair selected,
the same ones not, the others as they were; FAR's dialog at its places (subfolders and contents off for a
plugin's panel, "selected items only" off without a selection), its settings under FAR's names, its
messages. A window tells the files compared; Esc stops it, the selection then as it was. What differs:
tech-debt P6.4.

P6.5a done: FAR's proclist shipped as the plugin `proclist` ("Process list", F11, the drive menu, `plist:`)
for the local computer. The processes are read with `NtQuerySystemInformation(SystemProcessInformation)`,
which holds every one of FAR's 23 counters (FAR reads them from the performance registry), each second by a
thread of the plugin's, the panel then read again (`ACTL_SYNCHRO`; a plugin's own panel read at its asking
tells "Reading" in its title only after the moment a reading waited for would, so it never flashes); what only opening a process tells (its
path, command line, bitness, GDI and USER objects, owner) is read once, when it is first seen. FAR's ten
modes with its column types (`X<n>` and `XP`, `XI`, `XC`, `XT`, `XB`, `XG`, `XU` the plugin's columns, `ZP`,
`ZW`, `ZD`, `ZC` the description), its titles and widths, saved as `Mode<n>`; the start mode (1) and the sort
saved as the panel closes. Enter brings the process's window to the front when Alt+Tab would show it, Ctrl+R
reads the processes again, Ctrl+F puts the path into the command line, Shift+F1 / Shift+F2 lower / raise the
priority class (asked for more than one), F8, Del, Shift+Del and Alt+Del end the selected processes (Shift+F8
the one under the cursor only, FAR's `ReturnCurrentFile`) with FAR's questions (refused: "Retry with debug
privileges?"), Ctrl+F3..F11 sort (Ctrl+F5 and Ctrl+F9 nothing),
Ctrl+F12 is FAR's "Sort by" with each counter, per second too; the configuration dialog at FAR's places.
Where it differs from FAR, for the program's architecture:
- An entry is named by the process's name and number (`svchost.exe 1234`): the program selects and reads its
  entries by name, and two processes may share one. A column with friendly names comes with P6.5d.
- The sorts the program has not (a number, a counter, the owner, the threads) are the plugin's own order with
  the panel set to "unsorted" (FAR's slot is `SM_CHTIME`); the program reverses it for the descending order.
  Ctrl+F12 marks the current sort with the menu's check mark, not FAR's arrow.
- `O`, the owner (`domain\user:session`), is a column of the plugin's: the program draws no owner column for
  a plugin's entries. A column not drawn (width 0) stays empty, the next keeping their places.
- FAR's entries also carry the page file per second as their allocated size and the threads as their links
  (the program's `A` and `LN` columns); the SDK's entry has neither yet, so a user's mode with them shows 0
  there (the plugin's `XT` shows the threads).
- `_Total`, the counters' sums, is the plugin's own (FAR's comes from the registry), hidden as FAR's.
- A process whose number came back with another start is a new one (FAR takes it for the old).
- Ctrl+0..9 read the panel again in its new mode (FAR's `FE_CHANGEVIEWMODE`); its first listing, before the
  panel shows the plugin's mode, is read again at once.
- F6, Shift+F6, `plist:\\machine` (another computer's processes): P6.5c, put off by the owner (2026-11-15).

P6.5b done: a process's text as FAR's `GetFiles` writes it - its module and bitness, path, file version and
description, PID, parent (with its name), priority, threads, owner (domain, user, SID) and session, start and
uptime, command line, current folder and environment (read from its process parameters, a WOW64 process's
through its 32-bit PEB), GDI and USER objects, the 23 counters (FAR's `DumpNTCounters`), its window (title,
HWND, styles by FAR's names), its modules (with versions and descriptions when asked; `EnumProcessModulesEx`,
else the loader's list) and its handles (unnamed ones when asked), each part as the options say. F3 and Ctrl+Q
read it as the entry (made again at most each second), F5 writes `<name>.txt` into the other panel's folder at
once as FAR does (no dialog; the selection cleared once all are written, Esc writing none cut short),
Shift+F3 ("View...") asks the text's parts first (FAR's "View with Options...")
and views a temporary file the viewer deletes; writing many tells the progress, Esc stops it. Where it differs:
- F5's file is FAR's UTF-16 with its mark; the viewer's and the quick view's text is UTF-8, as it was made
  before stage 2d's viewer read UTF-16 (the text is the same, and the viewer tells either).
- The file is named by the entry (`svchost.exe 1234.txt`): FAR's `svchost.exe.txt` would be written over by
  each process of that name.
- The handles come from `SystemExtendedHandleInformation`, whose process numbers are whole (FAR's
  `SystemHandleInformation` keeps 16 bits of them). A file's name is asked on a thread of its own: not answered
  in 100 ms (a disk's file in a second), its I/O is broken with `CancelSynchronousIo` and the file's kind
  stands for it (`<pipe>`), the thread let go and the copy closed - FAR's `TerminateThread` is never used
  (ADR-0019); a file that hung is not asked again, and at most 16 such threads wait at once.
- The owner comes from the process's token (FAR asks WMI); the session is written for every process.
- A file F5 cannot write is told (FAR's `GetFiles` returns silently).

P6.5d done (owner, 2026-11-15; ours over FAR): the process list opens as a tree by parent, as Process
Explorer's, folded at first - a process under the one that started it while that one is listed and started no
later (a parent gone leaves its number to another process; the idle process is nobody's parent), each level by
its start, `+` before a folded branch and `-` before an unfolded one, each level indented by two. Enter on a
process with a branch folds or unfolds it (the owner's key), each panel its own branches; on another, as in a flat list, it is FAR's
(the window to the front). Ctrl+F12's "Process tree" brings the tree back; every other sort shows the list
flat ("Unsorted" is FAR's), and the tree is the sort saved and opened with until another is. Every mode's
name column is the friendly name (`XN`, a type of the plugin's), as Task Manager names the processes: a
service host by its service ("Service Host: <service>", several by their group, the command line's `-k`),
else its file's description, else its image's name, the idle process "System Idle Process"; the entry's own
name (`svchost.exe 1234`) is the status line's, the quick search's and the files'. The files' descriptions
are read by the refresh thread, a part each turn (read cold they take seconds): a name not yet described is
its image's for a moment. ".." stands in a first column of the plugin's own (the program's change for such a
mode, whose row was empty).

P6.5e done (FAR 3's own, `far/plist.cpp`): Ctrl+W, and Commands > "Task &list" (FAR's hotkey L, so
"Choose shell", the program's own, takes E: C is FAR's "Compare folders"), shows FAR's "Task list" in the middle of the screen over
whatever is in front: the windows Alt+Tab shows (visible, the last active popup of their root owner, no
tool window, not cloaked) in the system's order, each `PID │ title` with the numbers right-aligned to the
greatest one's digits, this program's process or console window checked; wrapping, `&` shown as it is,
automatic hotkeys, as many lines as the screen's rows less five (FAR's `ScrY - 4`), "Del F2 Ctrl+R" below. F2
shows each window's image (its full path, "???" when it cannot be asked) instead of its title and back,
the cursor kept (on the last line when fewer are left); Ctrl+R reads the windows again; Del asks FAR's warning "Kill task" ("Do you wish to kill
selected task?", the window's title, "You will lose any unsaved information in this program", Kill /
Cancel), Kill ends the process (`TerminateProcess` with `ERROR_PROCESS_ABORTED`) or tells "Cannot kill the
specified task" with the system's reason, and the list is read again either way; Enter brings the window
to the front (restored when minimized); Esc closes it and what it stood over comes back. Not over a
program running in the terminal - the guest shell, a command run from the command line, a detached one
(its keys, Ctrl+W a shell's own, are the program's) -, a plugin's dialog or a plugin's window waiting for
its answer or telling it is busy (the list's question would leave it unanswered), the waiting work's
window or itself (FAR's `Active`).

P6.6a done (FAR's Network plugin, `far-source/plugins/network`, as the plugin `network`): FAR's "Network
browser" in the Plugins menu and the configuration, "network" in the Disks menu (each with its
configuration's box), `net:` and `netg:`. The top lists the network's providers, each provider its
domains, a domain its computers, a computer its shares (`WNetOpenEnum`), with a server's hidden shares from
`NetShareEnum` (but `IPC$`) as the configuration says (never, as hidden, always) and printers only when
asked; each in FAR's names (a provider by its name, a share by its whole path `\\SERVER\share` or with
"Show full share name" off by its own), the panel's title FAR's " Net: <folder> ", modes 3 to 5 FAR's
("Network name", `*:` the disk a share is connected as, "Comment"), mode 3 the first and the mode chosen
with Ctrl+0..9 saved as the panel closes (`PanelMode`). Enter on a share leaves for its folder, Ctrl+PgDn
for the disk it is connected as when it is one (connected with the user's credentials first when it
cannot be read, else FAR's "Error" with the system's reason); a share is never listed as a folder. Enter
on a domain or computer enters it once it opens (a connection tried first; its listing kept for the
reading), else tells why; on a printer it is the program's; F3 does nothing (FAR's). `net:\\server\share`
(`%NAME%` expanded) opens the computer below the parents the network tells (else at the top) with the
share under the cursor, whatever its case (FAR's `SetCursorToShare`); a bare name is a computer's, a
domain's with `netg:` or `NavigateToDomains`; a name the network does not know opens the panel as its
menu's item does (FAR's). A panel opens where the last one closed, the first of the session at this
computer's workgroup (FAR's `GotoLocalNetwork`, `IsFirstRun`). Differs from FAR (ADR-0014): a listing is
waited for 10 seconds - FAR waits for a workgroup whose list is not available for minutes here (154 s),
the program's keys and the plugin's other calls with it -, then "does not answer" and goes on by itself,
its answer the next listing's (Ctrl+R); a resource still being listed answers so at once (asked once at a
time); a thread left so keeps the plugin's library loaded until the program ends (an unload or update in
Options > Plugins takes effect after a restart then); the first panel goes to the workgroup only when it
answers in 2 seconds, else stays at the top. FAR's box around "Hidden Shares" is a titled separator. Not
yet: "Show '..' in the providers list" off (the program always shows ".."; kept and saved); a computer's folder from the folders' history loses its
`\\` (the history keeps paths as text). The program: a folder a plugin gives its own panel keeps `\` in
its names and folds `..` (typed, and from another plugin, `\` parts names as `/`).
P6.6b done (FAR's Network `ProcessKey`, `MapNetworkDrive`, `AskMapDrive`, `AddConnectionExplicit`,
`AddConnectionWithLogon`, `DisconnectFromServer`, `DeleteFiles`/`CancelConnection`, `FileNames2Clipboard`):
on a computer's shares the key bar is FAR's (F4 "ManCon", F5 "Map", F6 "MapTo", Shift+F5 "TMap",
Shift+F6 "TMapTo", F8 "Unmap"; on a domain F4 "ManCon"). F5 connects the shares (the selection, else the
one under the cursor) as disks at the first free letters from C, F6 at a letter chosen from FAR's menu of
the free ones ("Permanent connection to" / "Temporary connection to", F6 in it turning the kind round);
Shift with them a connection not restored at logon. A share that cannot be read is connected with a name
and password asked for (FAR's dialog titled by the share: "User name" with its history, "User password",
the last ones given filled in); a plain name refused is tried again as the server's user (`SERVER\name`),
a session with other credentials ends every connection to that server first; a letter remembered for
another connection takes the next free one (F6: FAR's "The disk name has a remembered connection"); no
letter free: "No free disk letters available"; a refusal: "Cannot connect "<share>" to X:" with the
system's reason; a name and password not given leaves that share for the next. F8 (Del, Shift+Del,
Alt+Del; Shift+F8 the one under the cursor) disconnects each share connected as a disk: the letter asked
for when it has more (FAR's "Disconnect network drive" message, a button for each), FAR's question
"Do you want to disconnect from the drive X:?" with the share and "Reconnect at Logon" (changeable only
for a disk restored at logon, its state kept as FAR's `DisconnectMode`) when the program's
Confirmations say so; a disk in use tried again with the process's folder moved off it, files open
asked about (FAR's "Do you want to disconnect the device anyway?") and ended by force, a failure told.
F4 asks for the name and password and connects with them, then leaves for the share or enters the
domain or computer; Enter does so too once the network refuses the user's own credentials (a share on
FAR's codes: a wrong password, a failed logon, access denied, an invalid handle; a domain or computer:
the first three and a logon type not granted), nothing told when the name and password are not given. Ctrl+F puts the name under the cursor into the command line, Alt+Shift+Ins and
Ctrl+Alt+Ins the names onto the clipboard (each quoted when it has a space; ".." the folder's own name,
`.\` at the top). F5 and F6 clear the selection and read the panel again; F8 reads both panels again.
Not yet: `net:\\server` of a computer that refuses the user's credentials does not ask
for a name and password (FAR's `GotoComputer` does): its folder is empty. Differs from FAR: the connections ended for a conflicting
session are that server's only (FAR compares a prefix of the name, `\\SERVER2` ending with `\\SERVER`);
off a computer's shares F5, F6 and F8 do nothing as FAR's but the key bar keeps the program's labels for
them (FAR blanks them); F6 with no letter free tells "No free disk letters available" as F5 does (FAR's
menu closes without a word). The API is 4 (ADR-0016, addendum P6.6b; every plugin's manifest says
`"api": 4`, one built for 3 refused by its version): plugins read the program's Confirmations by FAR's names (`confirmation`,
FAR's `FSSF_CONFIRMATIONS`: "Delete", "DeleteFolder", "RO", "Drag", "Esc", "Exit", "HistoryClear",
"RemoveConnection"; "AllowReedit" too since S2d.9b). The SDK: a share connected as a disk with a name and password or the user's own,
remembered at logon or not; a connection ended; whether a disk is restored at logon; the disks there are.
P6.6c done (FAR's `FileList::ChangeDir("..")` at a root, `KnownIDs.Network` with `OPEN_FILEPANEL`, the
plugin's `SetOpenFromFilePanel`): with the network plugin there a share's root (`\\server\share`) shows ".."
(FAR's `NetRoot`); Enter on it, and Ctrl+PgUp there with "Use Ctrl+PgUp to change drive" on, open the
plugin at the computer with the share under the cursor; with the option's third state (`[?]`, 2) a
network drive's root goes to its share so. A share the plugin does not go up from (one the network does
not know, or not Microsoft's: FAR's "we don't support upwards browsing from NetWare shares"; the WSL
computer's Plan 9 shares here), or no network plugin: the drive menu (FAR's `ChangeDisk`). The program
calls the plugin by its prefix `net_up:` with the share's name, not echoed (as `rclk_apps:`, ADR-0016
addendum P6.3; no table changes); the plugin answers it as `net:` does once the share is one it goes up
from, else leaves it to the program. Fixed with it: a computer reached through its parents (`net:\\SERVER`
of Microsoft's provider) opened only after its workgroup answered (the SDK asked each parent's provider
first, 154 s for a workgroup here; now only for a resource that names none). Differs from FAR: the
command line's prompt on a computer reached through its parents is the plugin's path
(`Microsoft Windows Network\WORKGROUP\\\SERVER`; FAR's is the computer's name, its `CurDir`): the plugins'
API (4, final) names no folder of a panel's own.
P6.6d done (FAR's Network `NetFavorites.cpp`, `EditFavorites`, `RemoveItems`, `GetResourceParent`): the
top lists FAR's "Favorites" folder first, in it the computers a name and password were remembered for
(as servers, not asked: FAR's `FAVORITES_CHECK_RESOURCES`). The logon dialog's "Remember password" (off
at first) keeps the name and password once connected, for the share or computer connected to; a share
or computer that does not read is connected with the ones kept first (FAR's `AddConnectionFromFavorites`,
before the question; F4's explicit connection asks always, as FAR's). With "Browse up to favorites if
possible" (on) a computer kept there stands under the Favorites folder (`net:\\SERVER`, its parents
not asked), and ".." (Enter, Ctrl+PgUp) on one reached through its workgroup goes up there, the computer
under the cursor; a kept domain so too (FAR's `GetFavoritesParent`: any resource but a share); the first
panel of the session opens at the Favorites folder when this computer is kept there (FAR's
`GotoLocalNetwork` asks `GetResourceParent`). In the Favorites folder F8 is "Remove": FAR's "Remove from Favorites" question
(`Remove "\\SERVER" from Favorites`, `Remove N items from Favorites`) removes each computer with its
shares; Shift+F4 on one tells FAR's "This is a SERVER" (FAR's button text, here a line with "Ok"). Kept as
FAR keeps them, the name and password as text (owner), in one setting of the plugin's, `Favorites`, a
line for each (FAR's `Favorites\SERVER[\share]` keys with `UserName` and `Password`). Differs from
FAR: with "Browse up to favorites" off, ".." on a computer entered from the Favorites folder goes back
to the folder (where it came from; FAR asks the network for its parent: the workgroup, or the top when
the network is not scanned); a computer whose share alone is kept is asked for a name and password at
once (FAR first tries an empty name and password: its `Favorites\SERVER` key exists as the share's
parent).

## 14. Archives (stage 6, ADR-0021, ADR-0022; FAR's arclite, owner 2026-11-16: "as FAR wherever there is a doubt")

S6.1a done (arclite's `plugin.cpp` `AnalyseW`, `OpenW`, `Plugin::info`, `list`, `set_dir`; `options.cpp`;
`attr.cpp` `load_arc_attr`; ui.cpp `SettingsDialog`; 7-Zip's zip handler for what it lists), the plugin
`archive`, its name "Archive" where FAR says "ArcLite" (no FAR names users see), zip then through the `zip`
crate (7-Zip's library since S6.3d, below). A file opens as an archive by Enter when its name matches arclite's include masks (its default list:
`*.zip,*.rar,*.[7bgxl]z,...`, matched by the program's masks, API 5's `NcHost::match_masks`) and not the
exclude ones when active, by Ctrl+PgDn whatever its name (its contents tell; "Mask settings apply to
Ctrl-PgDn" off) - the panel over the file's, ".." at its root back to the file; a file it does not read
is the program's (Enter as without a plugin, Ctrl+PgDn nothing, as in FAR); by `arc:[-d] [-t:zip]
[-p:<password>] <archive>` (`%NAME%` expanded, relative to the panel's folder) and by the Plugins menu's
"Archive": "Open archive" and "Open (format menu)" on the file under the cursor, any name ("File is not
recognized as archive" when it is none). The panel: arclite's title `Archive:zip:<name>[:\folder]`, start
view mode 2 sorted by name ("Use own panel view mode"), the format `arc` in the copy dialog; a folder's
items with their size and packed size (FAR's `AllocationSize`, API 5's `NcEntry::allocated`: the program's
allocated-size column and sort), their times (NTFS's extra field, the Unix one, else the MS-DOS time
in the local zone as 7-Zip reads it) and attributes (7-Zip's `GetWinAttrib`, arclite's posix split); a
folder only implied by its items listed too, with the archive file's times; a folder one whatever its
names' case (arclite's `lstrcmpiW`), `.` on a path passed over and `..` back to the folder before; a
name without its closing `/` a folder by its attributes (7-Zip's `IsDir`); names UTF-8 by the flag,
Info-ZIP's Unicode path (when UTF-8) or from a Unix tool, else in the host's code page (7-Zip's
`GetCodePage`: OEM for FAT, NTFS and Unix, ANSI for the others; the comment ANSI), the configuration's
"OEM Codepage" / "ANSI Codepage" when set (saved only with "Save Codepages"), `\` and U+F05C separators
as `/`. The info panel: arclite's lines - Path, Comment, Compression
ratio, Data size, Packed size, File count, Directory count (sizes as arclite writes them: `1.50 KB = 1536`).
S6.1b done (arclite's `get_files`, `extract`, `Archive::extract`, ui.cpp's `ExtractDialog`,
`OverwriteDialog`, `PasswordDialog`, `retry_or_ignore_error`, `show_error_log`, `ExtractProgress`; FAR's
`PluginGetFiles`, `GetFile`): the plugin writes its files out itself (API 5's `NcPlugin::get`). F5 / F6 to
a disk show arclite's "Extract files" for the other panel's folder (the archive's name without its
extension, made whole from its folder, when that panel is hidden): the folder with its history, Ignore
errors, the existing file's answer (Ask, Overwrite, Skip, Rename, Append), "Delete files from archive"
(`[?]` and disabled: no archive is changed before S6.2, so F6 extracts as F5), "Delete archive after
extraction", a separate folder named after the archive (`[?]` by default), "Go to target directory"
(disabled: only for archives of a disk's panel at once), the password, "Save settings" (kept under
arclite's `extract_*` names, "Settings saved"); a typed folder unquoted, `%NAME%` expanded once changed.
Alt+F6 does the same into the archive's own folder. The extraction: the selected folders made with what is
in them, the files in the archive's order, names Windows refuses given arclite's look-alikes (`correct_name_mode`
0x12: `:` as `…`, `?` as `¿`, ...), an existing one asked about by "Overwrite file" (sizes and times,
"(newer)", "Remember choice"; Rename as `name.0`, Append; Overwrite in the archive's case when the file's
differs, `strict_case`), an encrypted one's password asked once ("Enter password"), each file's and folder's
attributes and times set; a failure asked about (the system's words and the path, or the data's: "Wrong
password", "CRC check failed", "Data error", ... with the file and the archive; Retry / Ignore / Ignore All /
Cancel), not asked with Ignore errors; a file that could not be made or written goes, one whose data failed
stays with what was written of it (arclite's `error_state`), Esc in the middle of a file leaves none of
it. Shift+F5 / Shift+F6 take the entry under the cursor alone by the same dialog, the selection kept (FAR's
`ReturnCurrentFile`). Its progress is arclite's
window (half a second in: `{N%} Extracting files...`, the archive, the file, the sizes at the speed, the
bar); Esc asks "Do you want to interrupt current operation?" when the program's Interrupt confirmation is
on. Ended with failures: "Operation log" - "Show log" opens them in the viewer. Done: the selection goes,
both panels are read again; the archive deleted when asked and nothing failed, its panel closed to its
folder. F3 / F4: the file written by the plugin into a folder of the program's, the viewer or the editor
on it. Differs from FAR (S6.1b): the progress has no lines of arclite's write cache (each piece is written
as it comes) and no B (low priority) / P (pause) keys (the window is the program's; Esc only); a failure
has no source file and line; the log in the viewer has no title of its own; a file's change time is not set
(the system's own as it is written); a wrong AES password is forgotten as a wrong ZipCrypto one is (7-Zip
keeps it, every next file failing); F5 into another plugin's panel is the program's copy through `read`;
names are written through the system's usual paths (arclite's `long_path_norm` writes `\\?\` ones): a
name ending in a dot or a space loses it, as Windows' paths drop them (a path past `MAX_PATH` is written
whole).
S6.1c done (arclite's `show_attr`, attr.cpp, ui.cpp `AttrDialog`; FAR's `ProcessEnter` with
`PluginMode`): Ctrl+A shows arclite's "Attributes" - an item's properties as 7-Zip's zip handler lists
them (Path, Folder, Size, Packed Size, Modified, Created, Accessed, Attributes in arclite's letters with a
Unix mode after them, Encrypted, Comment, CRC, Method with its cipher - `ZipCrypto`, `AES-256` -,
Characteristics, Host OS, Version, Volume Index, Offset) in read-only lines, a folder's on "..", the
archive's own (the info panel's lines) on ".." at its root; its "..." leaves Ctrl+A to the program.
Enter / Ctrl+PgDn on a file of an archive: written out by the plugin into a folder of the program's
(`GetFile` with `OPM_SILENT | OPM_EDIT`) and offered to the plugins - an archive inside an archive opens over
the outer one's panel, ".." at its root back with the cursor on it, the copy deleted once its panel is
gone (with the program too, once its plugins have ended); one no plugin takes Enter opens by the system (its copy kept for it), Ctrl+PgDn lets go. Differs
from FAR (S6.1c): the quick view and Find file's search in a file read an archive's file through the
plugin's `read` a piece at a time, not written out whole first (`OPM_QUICKVIEW`, `OPM_FIND`): a large file
shows at once (the owner's rule: huge files open instantly), an encrypted one only with its password
given before (owner's decision, 2026-11-16); 7-Zip's "Method" has no Deflate level and spells 7-Zip's method 93 "Zstd", and
"Characteristics" names only the extra fields and flags the plugin knows, by its own names and in the
header's order (Zip64, NTFS, UT, ux, UnicodePath, WzAES, Descriptor, UTF8: 7-Zip's sources are not at hand
to copy its names and order; S6.3 brings its library and its own words); an item's "Comment" is read as
the archive's (UTF-8 when it is, else the ANSI code page), not by the item's UTF-8 flag; the times (there
and in "Overwrite file") are
local by each date's own daylight saving, as the panels show them (`SystemTimeToTzSpecificLocalTime`), not
by today's offset as arclite's `format_file_time` (`FileTimeToLocalFileTime`): an hour apart across the
change; Enter on a file no
plugin takes opens its copy without waiting for the program and putting a changed copy back (FAR's
`wait_finish` and `PutFiles`: S6.2 writes zip).
S6.2a done (arclite's `put_files`, update.cpp `PrepareUpdate`, `Archive::update`, ui.cpp `UpdateDialog`;
FAR's `PutFilesW`): F5 / F6 into an archive's panel - the program's copy dialog with the target `arc:`,
then arclite's "Add files to archive" (Compression level, Method, Solid, Advanced parameters with their
history, Encrypt archive, Encrypt header, Show password, the password twice or shown, the existing file's
answer Ask / Overwrite / Skip, Delete files after archiving, Ignore errors, Archive files open for
writing, Use filter, "Save settings" under arclite's `update_*` names; the passwords checked as it does);
the sources walked into the panel's folder, a file the archive has (a file to a file, a folder to a
folder, whatever the case) asked about with "Overwrite file" without Rename and Append, a folder there
gone into; the archive written anew as `{GUID}.tmp` beside itself (its items copied as they are, a file
taking an item's place written there), arclite's "{N%} Updating archive..." window with Esc's question,
then moved over it with its creation time kept, the panel read again; the sources deleted after when
moved and nothing failed or was skipped; every failure Retry / Ignore / Ignore All / Cancel and the log.
F4 on a file of an archive, saved: the copy put back through the same dialog (FAR's `PutFiles` with
`OPM_EDIT`, arclite's Overwrite). Differed from FAR until S6.3d (7-Zip's library writes zip since, as
arclite; the S6.3d paragraph below lists what went): the methods were the ones the `zip` crate
writes - PPMD, Deflate, BZip2 (not LZMA, LZMA2, Deflate64), the dialog starting at Deflate whatever the
items have (as arclite's `load_update_props` for a zip; S6.2f set this right: the first item's method
was taken before); encryption is AES-256, or the strength `em=` asks (the crate writes
no ZipCrypto, 7-Zip's default for a zip); "Advanced parameters" takes only `em=`; "Use filter" is
disabled (FAR's custom filter menu is not the plugins'); "Archive files open for writing" starts on (FAR's
"Copy files opened for writing" is not the plugins' to read; its default); a source named with its
folders is matched in the archive's folder of that path (arclite matches it in the panel's folder and
writes a second item); a put-back whose panel moved meanwhile goes through the plugin's `write`, which an
archive refuses (the copy is kept and said where); an archive with several items of one name (a `.jar`
may have them) is not changed, its reason said (the crate keeps one item of each name; 7-Zip all); a
file that was to take an item's place and could not be read (Ignore) leaves the archive's item as it was
(7-Zip's update leaves both out: nothing of the archive is lost here); names
are written as UTF-8 with its flag (7-Zip uses the OEM code page when a name fits it); no "Scanning
directories..." window while the sources are walked nor "Deleting files..." while moved ones go, and
Esc is looked at between the archive's items copied, not inside one (7-Zip's stream inside it).
S6.2b done (arclite's `delete_files`, `create_dir`, delete.cpp `Archive::delete_files`; FAR's
`PluginDelete` with `DeleteFilesW`, F7's `MakeDirectoryW`): F8 (Del, Shift+Del, Shift+F8) on an archive's
panel asks arclite's "Do you want to delete selected files?" (OK, Cancel; the program asks nothing), then
writes the archive anew without the entries and all under them, its "{N%} Updating archive..." window
with the bytes at the speed and the bar; F7 asks arclite's "Enter directory name:", writes the folder
with the time now and goes to it (FAR's `GoToFile`); F6 out of an archive shows "Delete files from
archive" on, and once nothing failed deletes the entries extracted, an entry with a file the user skipped
under it kept whole (arclite's `extracted_indices`). Differs from FAR (S6.2b): a file and a folder of one
name in an archive's folder (`x` and `x/`) are refused by F8 and F6 with the reason (the program hands
the plugin names, not FAR's items: the other one could go); F7 refuses a name with "." or ".." in it and
one there already (7-Zip writes a second item); an archive with several items of one name is not changed
(as S6.2a). S6.2c done (arclite's `create_archive`, `put_files` of an archive not open, ui.cpp
`UpdateDialog` with `new_arc`, update.cpp `Archive::create`; FAR's `PluginPutFilesToNew`): the Plugins
menu's "Create archive" and Shift+F1 on a disk's panel (the selected files, or the one under the cursor,
offered to the plugins that open files in their order; the first that makes a panel of no archive takes
them) show arclite's "Create archive": the profile with Save ("Input profile name") and Delete ("Do you want
to delete selected profile?"), choosing the profile the answers match; "Archive path" (its history
`arclite.arc_path`, the one file's name or its folder's with ".zip") with Evaluate and "Append extension";
"Select archiver"; the level, method, Solid, the advanced parameters, the encryption and its password;
"Create self-extracting archive" and "SFX options", "Split into volumes" and "Volume size"; "Delete files
after archiving", "Ignore errors", "Archive files open for writing", "Use filter"; OK, Cancel, "Save
settings" (arclite's `update_*` of a new archive). The path is made whole as `eval_arc_path` makes it
(quotes gone, `%NAME%` expanded, a folder's given the default name and extension, the extension added when
asked); a file there asks "Target file already exists. Do you want to overwrite it?" (Yes, No); the folder
is made, the archive written with "{N%} Creating archive...", the sources moved deleted when nothing failed
or was skipped, the log offered; the cursor goes to the archive in the active panel (and, from the menu,
the passive one) showing its folder; after Shift+F1 the selection goes. The menu's "Create archive" takes
neither the kept encryption, the move nor the advanced parameters (arclite's `create_archive`); Shift+F1
takes them. Differs from FAR (S6.2c): the archiver is zip only (S6.3 brings 7z and the others' "Other:");
"Create self-extracting archive" is 7z's (disabled, as arclite does for a zip); "Split into volumes" is
disabled (the zip library writes no split archive); a kept method the plugin does not write starts as
Deflate (arclite's empty one is LZMA); the archive is written beside its place and renamed over it (a file
there stays until the new one is whole, a panel showing it read again; arclite writes over it at once);
"Delete files after archiving" never deletes the new archive itself nor a folder it is in (what else is in
that folder goes; arclite deletes them with the other sources); a profile's name with no text or a `\` in
it is refused (arclite's are the settings' keys); a profile deleted or renamed has its values emptied,
its password too (arclite deletes its key); Evaluate's path is cut into lines
of the window's width; Shift+F1 is a disk's panel's only (FAR: also a plugin's with real names) and offered
to the plugins that have file masks (FAR asks every plugin with `AnalyseW` or `OpenFilePlugin`); the
menu's "Extract archive(s)", "Test archive(s)" come with S6.2d, "Convert archive into SFX" with S6.3.
S6.2d done (arclite's `bulk_extract`, `extract(arc_list)`, `bulk_test`, `test_files`, test.cpp's
`ArchiveTester`; FAR's `ProcessHostFile` and `KEY_SHIFTF2` on a plugin's panel): the Plugins menu's
"Extract archive(s)" and "Test archive(s)" take the active panel's selected files (the one under the
cursor when none is; folders passed over) of a disk's panel or a plugin's of real names. Extract shows
arclite's Extract dialog with the passive panel's folder when it shows a disk's, the kept answers, "Delete
files from archive" not offered and "Delete archive" offered; each archive's root goes into the folder,
into a folder named after the archive when asked or (asked neither way) when its root has more than one
entry, the archive deleted after when asked and it added no failure; a file that is no archive ("File is
not recognized as archive") and every failure go into the log offered at the end, none: the active panel
is read again with its selection gone; "Go to target directory" takes the cursor there (onto the one entry
of a lone archive). Test reads every file through, nothing written, the CRC checked, with "{N%} Testing
archive..." (the file, the bytes at the speed, the bar), the password asked once for an encrypted file; a
failure ends that archive's test and goes into the log named by its archive; none: the panel read again
and "Archive test complete". Shift+F3 on an archive's panel opened from its file ("Archive commands",
FAR's `ProcessHostFileW` with the selected entries or the one under the cursor) shows arclite's menu "Test
archive files", tests them, says "Archive test complete" and unselects them (arclite clears their
`PPIF_SELECTED`); a failure is shown as arclite's `error_dlg` (the archive, "Errors:", the words, the file,
the archive), the selection kept; Esc in the menu does nothing. Shift+F2 on an archive's panel is F5.
Differs from FAR (S6.2d): a test's failures are the zip library's, in arclite's tester's words (CRC,
data, method, each told apart for an encrypted file; anything else "Unknown extraction error"; 7-Zip's
open warnings and "Headers Error" are not told); Esc or Cancel at the password stops Shift+F3's test
silently and keeps the selection (arclite's `test_files` loses the stop in its `Error::Append` and says
"Archive test complete", the entries unselected); an entry the archive no longer has (a panel not read
again) fails the test with its name rather than being passed over; Shift+F2 copies out of a plugin's panel opened from a file
or one whose plugin writes its files out (FAR asks the plugin for its `HostFile`, which the ABI does not
carry); the menu has no "Convert archive into SFX" until S6.3 (arclite always lists it). S6.2e done
(FAR's `PluginHostGetFiles`, `ProcessHostFile` with `ProcessOneHostFile`; arclite's `get_files` with
`OPM_TOPLEVEL` and its `batch_options`, `ProcessHostFileW` with `OPM_TOPLEVEL`): Shift+F2 and Shift+F3 on
a disk's panel take the selected files (the one under the cursor when none is), the selection saved
first, one after another; each is offered to the plugins that open files in their order with
`OFP_EXTRACT` / `OFP_COMMANDS` (arclite checks its masks as for Enter; a folder is offered to none), and
the first that opens it has its panel read at its root and closed after, never shown. Shift+F2 gets the
root into the other panel's folder (the first file's name without its extension when the other panel
shows no files and nothing is selected, made whole from the panel's folder): arclite's Extract dialog for
a plugin's first file, the next ones silent with its answers ("separate directory" asked neither way
gives an archive of several entries a folder of its own), "Go to target directory" taking the active
panel there, "Delete archive" deleting it. Shift+F3 shows arclite's "Test archive files" for each and
"Archive test complete". A file done is unselected, one no plugin opens is passed over, one not done (a
stop, Esc in the menu, a failure) ends the run; both panels are read again at its end. Differs from FAR
(S6.2e): only a disk's panel (FAR: Shift+F2 also over a plugin's real names, Shift+F3 over a plugin's
panel opened otherwise than from a file); the other panel showing a plugin's folder gives the file's name
as for a hidden one (FAR hands the plugin's folder text on); the run stops when the panel has gone to
another folder meanwhile (FAR runs it in one modal call), or when another press is waited for (the
keys are the user's again under a plugin's dialog); a plugin that writes no files out ends Shift+F2's
run; the folder typed in one plugin's dialog is not handed on to the next plugin of the run (FAR's
`GetFiles` gives its `DestPath` back; the ABI's `get` does not: within one plugin its kept answers carry
it); a file whose path is not Unicode is passed over. S6.2f done (arclite's cmdline.cpp, `cmdline_update`,
`cmdline_extract`, `extract_items`, `delete_items`, `cmdline_test`): `arc:` with a first word of `c`, `u`,
`x`, `e`, `d` or `t` (whatever its case) runs that command with arclite's parameters (`-name` or `/name`,
`:value`; `--` ends them for `x`, `e`, `d`), names without a folder taken from the panel's folder, both
panels read again after it; no dialog of theirs. `c` makes a zip of the files and the files `@list`
names (UTF-16 or UTF-8 by their marks, else ANSI; a name a line), their shared folder taken off, from the
profile `-pr` or arclite's defaults with `-l`, `-m`, `-s`, `-p` (encrypted), `-eh`, `-mf`, `-ie`,
`-adv` over them, the cursor on it where a panel shows its folder; `u` adds them to the archive with
the method and level arclite's `load_update_props` gives a zip (Deflate, 7) and its encryption unless
given, `-o` (overwrite,
skip; else asked), and makes the archive when it is not there; their failures asked about, no log
(arclite's `cmdline_update` shows none). `x` extracts every archive's root into the folder named last
(arclite's defaults: overwrite, no folder of its own unless `-sd`; `-da` deletes the archive), the log
offered. `e` extracts the items whose path (`\` between parts) is named, whatever its case, or which
the masks take, each into `-out` (else the panel's folder) by its name, a folder with what is in it;
`d` deletes them and all under them, no question. `t` tests the archives. A command not understood shows
arclite's syntax (in place of its help topic "Prefix"). Differs from FAR (S6.2f): only zip is written
(a new archive whose format - `-t`, the profile's, else its extension's, arclite's 7z for another or
none - is not zip, `-sfx`, `-v` or a profile's volumes, and a method the zip library does not write -
LZMA, LZMA2, Deflate64 - are refused with the reason; `-m:copy` stores; `-t` of a name no format has is
refused the same way, where arclite shows the syntax); `-eh` is read and has no effect (a zip has no
header to encrypt); a name typed in `e` / `d` without masks' signs (`*?[,;|`) is compared as a path only
(the same answer as arclite's masks without a call to the program per entry); `u` of an encrypted archive encrypts what it adds only with `-p` (arclite takes
the archive's password, empty when it was not asked). With S6.2f API 5's tables are final: a change to
`NcHost` or `NcPlugin` from here is API 6 (ADR-0016). The configuration: arclite's
dialog with "Handle archive creation", "Handle archive commands", "Use own panel view mode", the code pages,
the include and exclude masks with "Active" (their lines enabled with it), "Mask settings apply to
Ctrl-PgDn", "Default masks", saved under arclite's names. The program: Ctrl+PgDn on a file no plugin takes
does nothing (FAR's `ProcessEnter`; it ran or opened the file as Enter). S6.3a done (ADR-0022; arclite's
`ArcAPI::load`, open.cpp `Archive::detect` and `Archive::open`, `ArchiveOpener`, archive.cpp `make_index`,
attr.cpp `load_arc_attr` and `get_attr_list`): 7-Zip's library found as arclite finds it (the plugin's
folder's `*.dll` that are 7-Zip's, else `7z.dll` of the folder `Software\7-Zip`'s `Path` names, the
user's then the machine's, else on the search path; `Formats\*.format` beside it), its formats read from
it (one per class id, the newer library's); a file opened by every format in arclite's order (the
signatures its first megabyte holds at their places, a zip local header after the start passed over
when the file's end record names another volume; before them the formats of its extension; after them
every other but those opened by extension only; UDF before ISO, RAR before Split, DMG before HFS), at a
signature's place through a stream from there (an SFX's archive), a `kPreArc` format again with data
after it, an archive naming its main file opened inside it (`Dmg→HFS`); Enter, Ctrl+PgDn, `arc:` and
"Open archive" take the first way, "Open (format menu)" lists every way and asks; the panel's title names
the formats (`Archive:7z:a.7z`); the items by 7-Zip's path (the archive's name less its extension, the
inner one put on, for an item without one: `a.tgz` gives `a.tar`), sizes, attributes (a Unix mode's half
dropped) and times (the archive file's when one is not told); "Reading archive..." half a second in
(the volume, files and bytes read of their totals, two bars) with Esc's question; encrypted headers ask
arclite's password (one that opens nothing asked again, arclite's loop; Cancel: nothing opens, the press
goes nowhere else); the info panel's lines are
arclite's (the path, each archive property with a value in arclite's words and forms, the compression
ratio and the sizes when every file tells its own, the counts); Ctrl+A lists every item property the
format has, named and written as arclite does. Differs from FAR (S6.3a): a zip at the file's own level is
still read and written by the `zip` crate (S6.3c moves it to the library), and every zip opens so when no
library is found (arclite opens nothing); a password given once serves every format tried in one pass
(arclite asks again per format); an archive inside another (a chain's) tells the library no name, size
nor times of its own (arclite gives its item's), its items without times take the disk file's, its
"Compression ratio" is to the disk file's size (arclite's to its item's), and a
chain goes at most 8 deep (arclite has no limit); an item's raw properties (NT security, reparse data)
are not listed; DMG / UDF / ISO / RAR / Split / HFS are told by their names (arclite by their class ids:
one library gives both). S6.3b done (arclite's `ArchiveExtractor`, `ArchiveTester`, `SettingsDialog`,
`MultiSelectDialog`, `FormatLibraryInfoDialog`, `analyse_open`'s formats, `Patch7zCP`): an archive of
another format than zip extracted through the library in one pass (a solid block read once) - F5 / F6
/ Alt+F6 by arclite's Extract dialog, F3 / F4, Shift+F2, the menu's Extract, `arc:x e` - each file asked
about as it comes (overwrite, skip, rename, append), made, written as its data comes, its attributes and
times set; each item's outcome in arclite's words ("Unsupported compression method", "Data error",
"CRC check failed" and their encrypted archive's "Wrong password or ...", "Unavailable data",
"Unexpected end of data", "Is not archive", "Headers Error", "Wrong password", "Unknown extraction
error"), an item the library passes over in a solid block told by its path from the root as arclite's
`GetStream` names it, a data or CRC error forgetting the password; the password asked when the library
asks it; an "anti" item deleting its file; tested (Shift+F3, the menu's Test, `arc:t`) in test mode with
the library's progress, the first failure in the tester's words; the quick view and Find file read a
file through what the library gave of it, kept (in memory up to 16 MB, past it in a temporary file),
extracted again to at least twice as far when a read goes past it (FAR extracts the file to a
temporary folder once). The
configuration: "Edit" beside each masks line (arclite's list of check boxes in columns shaped to the
screen), "Generate masks" (every format's extensions), "Enable / Disable archive formats" with "Active"
and "Edit" (7-Zip's formats offered), "Format settings apply to Ctrl-PgDn" (the lists filter Enter,
Shift+F2 / Shift+F3 and, with it, Ctrl+PgDn; none left: the file is not the plugin's), "7z library info"
(each library's path, version, its Formats and Masks; "7z library not found"); the OEM / ANSI code
pages given to the library as arclite gives them (its kernel32 imports of `GetOEMCP`, `GetACP`,
`MultiByteToWideChar`, `WideCharToMultiByte` answered by the plugin's own, put back before the library
goes). Differs from FAR (S6.3b): the extraction writes each piece as it comes (arclite's write cache of
up to 100 MB not kept: its own lines on the progress window not there); a result 7-Zip reports only
through `IArchiveExtractCallbackMessage2` is not seen (arclite has none either); the quick view and
Find file decode a file read through a few times its size at most (the library reads an item from its
start only; the first page comes without the whole file extracted), a read is not stopped by Esc, and
they ask no password (encrypted data read without the archive's password fails: "a password is
needed"; FAR's quick view extracts through GetFiles, which asks). S6.3c done (arclite's
`Archive::update`, `Archive::create`, `delete_files`, `create_dir`, `ArchiveUpdater`,
`set_properties`, `load_update_props`, `MultiVolumeUpdateStream`, `UpdateDialog`'s formats,
`cmdline_update`): every format 7-Zip's library writes but zip (7z, tar, gzip, bzip2, xz, wim, ...)
written through `UpdateItems` - F5 / Shift+F1 / F4's put-back into an archive of one of them by "Add
files to archive" (the level, its method for 7z, Solid, the header's encryption, encryption for 7z,
the items following the format as arclite's `set_control_state` keeps them; its method, solid and
encryption read from the archive as `load_update_props` reads them), F8, F7, F6 out with deletion,
`arc:u d`; a gzip, bzip2 or xz taking only its one file again ("Can't change single file archive
format"); an archive inside another, or of a format not written, "Archive modification not
supported". New archives: "Create archive"'s "Select archiver" with 7z, zip and "Other:" (every other
format written, by its library then name; a format of one file not for several files or a folder, SWFc
only for an uncompressed Flash file), the path's extension following the format and the volumes
(`change_extension`), each format's level kept (`levels`), arclite's default format (7z) and kept one
(`update_arc_format_name`); "Split into volumes" with its size (`name.001`, `.002`, ... written at their
place, all deleted when the writing fails); `arc:c` of any written format (`-t` by 7-Zip's name, else
the first format of the archive's extension, else the profile's, 7z), `-m` of arclite's methods, `-s`,
`-eh`, `-v`. The parameters given to the library as arclite's `set_properties` folds them (`0=` the
method for 7z and zip, `x=` the level - none for a bzip2 stored -, `s=`, `he=`, the advanced
parameters' shorthands written out, an `x=N` among them taking the level's place, digit names
renumbered); none for a deletion or a new folder (arclite sets none). Each file read as the library asks
(a failure asked about: ignored, the library leaves the file out), the progress the library's, Esc
stopping it, an item the library fails told as "Unknown update error"; what is before an archive after a
stub kept. Differs from FAR (S6.3c): a level a format does not take is put back to the one it writes as it is chosen
(arclite's list greys it out); the `Codecs\*.codec` modules' methods are not offered and arclite's
codec settings and `qs_by_default` (its `.farconfig` XML) are not read (its defaults: the levels as
they are, no `qs`); the advanced parameters' value that starts as a method given again takes the
method's place as arclite does. S6.3d done (ADR-0022 §1): zip read, extracted, tested and written
by 7-Zip's library as every other format, the `zip` crate gone; void with it, of the zip's earlier
steps' differences: the crate's methods (a zip offers arclite's LZMA, LZMA2, PPMD, Deflate, Deflate64,
its encryption 7-Zip's: ZipCrypto by default, AES by `em=`), an archive with one name twice refused (7-Zip
keeps every item), a file that could not be read keeping its item (7-Zip's update leaves both out, as
arclite's), the names written as UTF-8 with its flag (7-Zip's choice, the OEM code page when a name fits),
"Split into volumes" disabled and `-v` refused for a zip, a test's failures the crate's, the zip's own
properties and info lines (now 7-Zip's zip handler's, as arclite shows them), a zip opened without
7-Zip's library (nothing opens then, as in arclite: "7z library not found" in its configuration).
S6.3e done: self-extracting archives as arclite makes them - "Create archive"'s "Create self-extracting
archive" for a 7z (the path's extension `.7z.exe`, the volumes off then) and "SFX options" (arclite's "SFX
Options": the profile to apply among those making one, the module among the `*.sfx` found with the library -
the plugin's folder's, else those beside the 7-Zip found after it - by arclite's descriptions, "Replace icon"
with its path, "Replace version" with its six texts, "Install configuration" for an installer's module with
its seven, the profile the answers match chosen, "Select SFX module" / "Icon path is invalid"); OK refuses
a module without every codec for PPMD or encryption in arclite's words; the module copied, its icons and
version information replaced through the system's resource update (arclite's rsrc.cpp: the icon file's
images first in place of its first icon, all renumbered; the version information written anew in its first
one's language), the install configuration after it, the 7z written after the module with its places
counted from the module's end; the SFX options kept under arclite's `sfx.*` (and each profile's); `arc:c
-sfx[:module]` (7z.sfx by default); the menu's "Convert archive into SFX" ("{N%} Converting into SFX...",
`name.7z.exe` the module then the 7z, its times kept, the 7z deleted; anything but a pure 7z "Archive cannot
be converted into SFX archive"). A disk image of 7-Zip's MBR or GPT shows its partitions at its root as
`N.mbr.<fs>` / `N.gpt.<fs>` by the file system its boot sector tells (FAT, exFAT, NTFS, ext2 / ext3 / ext,
else an APFS, HFS or ext4 name's ending); Enter or Ctrl+PgDn on one opens it by that file system's format
(`MBR→NTFS`, the title `Archive:MBR→NTFS:disk.img:N`), ".." at its root back to the disk with the cursor on
it. A test tells the archive's open errors and warnings first (7-Zip's error and warning flags in arclite's
words, its texts, data after the archive's end a warning, an archive longer than its file an error), its
failure after them; warnings alone fail it ("Errors:" / "Warnings:" in its message and log). Differs from
FAR (S6.3e): a partition opens on a panel over its disk's (arclite shows it as the disk panel's folder `N`;
the keys, title and ".." the same); "Convert archive into SFX" tells a pure 7z by its signature at the
file's start (arclite opens it as 7z, asking an encrypted header's password); a self-extracting 7z is never
split (arclite leaves "Split into volumes" checked but disabled, and its writing would split it); `arc:c
-sfx` of a module not found says "Select SFX module" (arclite fails its internal check); F6 out of a gzip,
bzip2 or xz extracts, then its deletion says what F8's does ("Can't change single file archive format";
arclite's deletion fails in 7-Zip's words).
S6.7 done: Alt+F7's "Search in archives" (FAR's `ArchiveSearch`, `FindFilesOptions::SearchInArchives`):
the box is kept for the session (not saved, as FAR's `LastFindFileOptions`), off and disabled on a
plugin's panel without real names (FAR reads it there too, so the next search starts without it). Each file
the filter lets through, whatever its name, is offered after it is looked at to the plugins whose masks take
it (FAR's `OpenFilePlugin` with `OFP_SEARCH` and `OPM_FIND`, API 6's `OPEN_SEARCH`), on the search's thread;
the archive plugin checks arclite's masks and formats as for Enter and opens it without any window. An
archive opened is walked from its root into all its folders whatever the area (`FINDAREA_FROM_CURRENT`),
then closed; its objects are found by the mask, the text read through the plugin, under FAR's lines
`D:\dir\a.zip:\` and `D:\dir\a.zip:\docs\`; no archive inside an archive (its files have no real names).
Go to on one: the panel to the archive's file, the archive opened over it by the plugin that found it as
Enter opens it, its folder entered with the cursor on the object (".." back as from Enter). F3 / F4: the
archive opened again without any window, the file copied out, viewed or edited; the viewer's Gray + / -
go through the disk's found files only (FAR's `ViewList`). Panel: the archive's file for each object found
in it, no attributes (TmpPanel takes each name once). Differs from FAR (S6.7): the search shows no plugin's
window at all - an archive of encrypted headers is passed by, of several ways the first taken (FAR may ask
from its search's thread); F4's copy edited is never put back (FAR's editor then saves only as another
name, `SetSaveToSaveAs`): the program's editor has no "Save as", so the copy is kept and a message says where;
F3 / F4 on what was found in an archive open the program's viewer and editor, not the file's associations
(FAR's `ProcessLocalFileTypes` on the copy); TmpPanel opens no list for the search (FAR's would ask its
"Menu from file list" from the search and with "Replace" empty the common panel); a file the search is
inside of already (a list naming itself) is not opened again (FAR's would recurse without end).
Differs from FAR: `arc:`'s `-r` (recursive panel) and `-x` (delete on close) are not
understood (the program's panels have neither); the info panel has no 7-Zip archive properties beyond the
comment (a zip's "64-bit", "Characteristics" come with S6.3's library); an entry's CRC is not carried (FAR's
`CRC32` and `OPIF_USECRC32` only tell a changed entry when the panel is read again).

## 15. Remote file systems (stage 6, ADR-0023; FAR's NetBox as the plugin NetWay)
NetBox's behaviour, read in `netbox-source/` (its code is GPL: none taken), under the program's name NetWay.
- **The sessions panel** (S6.4b, NetBox's session list): F11's and the Disks menu's "NetWay" open
  "NetWay: Stored sessions [/folder]": the session folders and the sessions, "Press Shift-F4 to create new
  session item" in a folder of none, every mode the names under "Session name". F4 edits the session under
  the cursor ("Edit session"; with none stored it adds one), Shift+F4 adds one ("Add session", then "Save
  session as" with `user@host` in the panel's folder), Shift+F5 "Duplicate session", Shift+F6 "Rename
  session" (a name with folders moves it there; a change of case alone renames), Ctrl+F the session's whole
  name (a folder's path with `/`) into the command line, F7 "Make folder", F8 "Do you wish to delete
  selected session(s)" (a folder with all its sessions); the panel goes to the session's folder with the
  cursor on it. Key bar: NewSes, Duplicate, Rename, and NetBox's blanked keys. The session dialog's Session
  tab: the file protocol (SFTP with "Allow SCP fallback", SCP, FTP, WebDAV, Amazon S3; "Insecure" for
  plain FTP and WebDAV), the encryption (FTP, WebDAV, S3's two), the host and the port (switched with the
  protocol as NetBox's), the user and the password (S3's access keys), the private key file; OK, Connect,
  Cancel, and NetBox's question before a password is stored.
  Differs from NetBox (documented, ADR-0023 addendum S6.4b): a session is a file in the plugin's data
  folder (`settings\Plugins\netway\Sessions`), not WinSCP's registry or XML; its password is kept by DPAPI
  for the user, not WinSCP's reversible obfuscation (the question before storing it says so; off Windows no
  password is kept until the port's keyring); a folder made by F7 is a folder on the disk and stays empty
  (NetBox's goes when the panel leaves it); a name cannot have `<>:"\|?*` (a file's name); the hint line
  does nothing on Enter. Not yet: the other tabs (S6.5g, S6.6); put off
  (ADR-0023 §6): the key file's "..." browse, "Display Public Key", PuTTYgen's "Generate Key".
- **Export and import** (S6.4c, NetBox's `GetFilesEx` / `PutFilesEx` on the session list): F5 ("Export"
  on the key bar; F6 the same, its label blank) asks "Export session "db" to:" (or "Export N session(s)
  to:") with the other panel's folder and copies each session there as `Work-Prod-db.netway` (its whole
  name, `/` as `-`), a folder with all its sessions, a file there asked over ("File ... already exists.
  Overwrite?"). F5 / F6 from a disk's panel into the sessions panel, after the program's copy dialog, ask
  "Import session(s) from selected file(s)?" and take each session file in under its name in the panel's
  folder; none: "File "..." does not contain any session data.". F3 views a session's file.
  Differs from NetBox (ADR-0023 addendum S6.4c): sessions go out as their own files, not NetBox's XML,
  their passwords readable by their user on this computer alone; Cancel on the question over a file stops
  the export (NetBox's goes on); a session of the name is asked over on import (NetBox's overwrites);
  NetBox's and WinSCP's XML or INI files are not imported.
- **Prefixes** (S6.4c, NetBox's `CommandPrefixes` and WinSCP's `ParseUrl`): `netway:`, `sftp:`, `scp:`,
  `ftp:`, `ftps:`, `webdav:`, `s3:`. Nothing after one opens the sessions panel; a stored session's name
  (without its folders or whole, a host's folder after `/`) or an address
  (`sftp://user:password@host:port/folder/file`, WinSCP's protocols with their ports and encryption)
  names a session to connect to. Differs from WinSCP: a stored session is named by its whole name only (not
  by a start of an address: `db` is not `dbserver`).
- **Connecting and browsing SFTP** (S6.5a, NetBox's `Connect`, WinSCP's `TSecureShell` and
  `TSFTPFileSystem::ReadDirectory`): Enter on a session, the session dialog's Connect (an edited session
  saved first, a new one not saved), a prefix's session or address, Enter or Ctrl+PgDn on a `*.netway` file
  of a disk connect. "Connecting to host...", "Authenticating...", "Starting the session..." in a window
  titled by the session, Esc stopping it; WinSCP's "Continue connecting to an unknown server and add its
  host key to a cache?" (Yes / No / Cancel: kept in the plugin's `known_hosts`, this time, refused) and
  "WARNING - POTENTIAL SECURITY BREACH!" (Update / Skip / Cancel); "&Username:" when the session has none;
  the private key ("&Passphrase for key '...':" when it is encrypted) or else the agent (Pageant), then the
  session's password or "&Password:" with the server's questions (keyboard-interactive; one hidden question
  answered by the password), three tries. The panel: titled "host:/folder", opened at the session's folder
  or the user's home folder, ".." at its root back to the sessions with the cursor on the session; the
  items' size, time, hidden (a leading dot), read-only (no one may write), links (to a folder entered as
  one); the detailed mode (Ctrl+5) NetBox's "N,S,DM,O,G,R" with "Owner", "Group", "Rights"; Ctrl+R reads
  it again. Differs from WinSCP / NetBox (ADR-0023 addendum S6.5a): a changed host key has no "Add" (the
  new key replaces the old). SCP, FTP, WebDAV and S3 sessions connect since S6.5f-S6.6c (below).
- **Copying** (S6.5b, NetBox's `GetFilesRemote` / `UploadFiles`, `TCopyDialog`, `ShowOperationProgress`,
  `CancelConfiguration`; WinSCP's `ConfirmFileOverwrite`, `SFTPConfirmOverwrite`, `FileOperationLoopQuery`,
  `MaskFileName`, `ValidLocalFileName`): F5 / F6 of the server's files and folders ask "Copy" / "Move" -
  "Co&py "name" to:" or "Co&py N items to:", the other panel's folder (history "Copy"), "Transfer
  settings" in three lines, "Preset", "New and updated file(s) only", "Use same settings next time",
  "Transfer settings...", Ok / Cancel; a disk's folder not there: "Directory '...' doesn't exist.
  Create?". A disk's files into the panel ask the program's copy dialog, then NetBox's with the server's
  folder (history "WinscpRemoteDirectory"). A name after the folder masks the first level's names
  (`*.bak`, `new.txt`). A file there: "Overwrite local file 'name'?" (or remote) with both sizes and times,
  Yes / No / Cancel / Append / New name / Newer only / No to All / Yes to All; Append over a shorter file
  asks to append or resume (No to All resumes all). A failure: "Error transferring file '...'." with
  Abort / Retry / Skip / Skip all. Times kept; a moved file removed after it, a moved folder after its
  items. The progress window ("Copying" / "Moving", "Calculating directory size" first): the file, the
  target, the file's bar, "Elapsed:" / "Left:", "Transferred:" / "CPS:", the whole bar; Esc asks "Cancel
  current operation?". F3 views and F4 edits a copy made with no dialog or question; the edited copy goes
  back at each save while the editor stays ("Upload after every save"), else when the editor is left.
  Differs from NetBox (ADR-0023 addendum S6.5b): no "Never ask me again" check box; the queue comes with
  S6.8.
- **The session dialog's tabs** (S6.5g, NetBox's `TSessionDialog`): a tab bar on top ("[Session]
  Environment  Directories  SFTP  SCP/Shell", the protocol's tabs only), Ctrl+PgDn / Ctrl+PgUp or a click
  on a name. Environment: "End-of-line characters (if not known)" LF / CR/LF, "Server timezone offset"
  hours and minutes (SCP), " Daylight saving time " (Unix, Windows, Preserve) with the Unix / Windows
  presets, " Recycle bin " ("Preserve deleted remote files to recycle bin", "Preserve overwritten remote
  files to bin (SFTP only)", "Remote recycle bin:"). Directories: "Remember last used directory", "Remote
  directory:" (history "WinscpRemoteDirectory"), "Resolve symbolic links". SFTP: "SFTP server:" (Default
  or a command). SCP/Shell: "Shell:", "Listing command:", "Ignore LS warnings", "Try to get full
  timestamp", "Clear national variables" (SCP only). Each applied as WinSCP applies it. Differs from NetBox
  (ADR-0023 addendum S6.5g): the tab bar is a line of names, not buttons; the caption stays; the charset,
  the directory cache, SFTP's version / real path / packet sizes / POSIX rename and SCP's return variable,
  user groups, aliases and SCP1 compatibility are left out; the DST modes are kept, not applied.
- **WebDAV** (S6.6b, WinSCP's `TWebDAVFileSystem`): WebDAV sessions and `webdav://`, `dav://`, `davs://`,
  `http://`, `https://` addresses connect (HTTPS's unknown certificate asked as FTPS's); the panel, copying
  both ways, F7, F8, rename, Shift+F5 (the server's COPY) work as over SFTP; no rights or times are kept
  (WebDAV has none); Basic authentication, Digest when the server asks (S6.6f); the WebDAV tab after
  Authentication's place ("Enable compression": listings and whole downloads gzipped; "Tolerate
  non-encoded special characters in filenames"; "TLS client certificate:"). Differs from NetBox (ADR-0023
  addenda S6.6b, S6.6f): no resumed uploads; the tolerance kept but disabled (names always taken as they
  come).
- **S3** (S6.6c, WinSCP's `TS3FileSystem`): S3 sessions and `s3://` (HTTPS), `s3plain://` addresses connect,
  the access key ID and secret asked when missing ("A&ccess key ID:", "Secret access &key:"); the root shows
  the buckets, a bucket's keys are folders by their `/`s; copying both ways (a download resumed by a range),
  F7 (a bucket at the root, else an empty `dir/` object), F8, rename of a file, Shift+F5 (CopyObject) work
  as over SFTP; no rights or times kept; a bucket in another region found by its answer. Differs from
  NetBox (ADR-0023 addendum S6.6c): Duplicate of a folder copies through the client (NetBox refuses). The
  S3 tab (S6.6f) before Connection: "Default region:" (typed, its own history: NetBox's list of regions is a
  combo box the program's dialogs lack), "URL style:" (Virtual Host, Path), "Requester pays"; files over
  5 MiB go up in parts (WinSCP's multipart upload).
- **FTP and FTPS** (S6.6a, WinSCP's `TFTPFileSystem`): FTP sessions and `ftp://`, `ftps://`, `ftpes://`
  addresses connect (no host key; an FTPS certificate that does not verify asked with WinSCP's "The
  server's certificate is not known..." and its fingerprints, Yes / No / Cancel); the panel, copying both
  ways (Append and resume through APPE / REST), F7, F8, rename, Ctrl+A (rights by `SITE CHMOD`, times by
  MFMT) work as over SFTP; "Passive mode" on the Connection tab. Differs from NetBox (ADR-0023 addendum
  S6.6a): no local proxy command or tunnel for FTP, no Duplicate or links, the certificate's fingerprints and
  failure only.
- **The FTP(S) and TLS/SSL tabs** (S6.6e, NetBox's `TSessionDialog`): FTP(S) after Directories (FTP
  sessions) - " Protocol options ": "Use MLSD command for directory listing", "Support for listing of
  hidden files:", "Force IP address for passive mode connections:", "Use HOST command to select host on the
  server" (Auto / Off / On), "Account:", "Display all file revisions on VMS servers", "Passive mode",
  "Allow empty password", "Dup FF in commands", "UnDup FF from PWD", "TLS client certificate:" (FTPS),
  "Post login commands:" in three lines; TLS/SSL after Proxy (FTPS, WebDAV over HTTPS, S3) - "TLS options",
  "Session ID re-use" (FTPS). FTP through a proxy checks "Passive mode" with NetBox's "Passive mode must be
  enabled when FTP connection through proxy is selected.". Each applied as WinSCP's FileZilla engine
  applies it; the client certificate (PEM) also for WebDAV and S3. Differs from NetBox (ADR-0023 addendum
  S6.6e): VMS's revisions and the FF doubling are kept but disabled (no VMS listing read; UTF-8 has no
  0xFF); no "..." browse; the TLS versions stay hidden as in NetBox; a client certificate is PEM only.
- **Bookmarks, edit history, Generate URL, cleanup** (S6.6d, NetBox's `OpenDirectory`, `OpenDirectoryDialog`,
  `EditHistory`, `TGenerateUrlDialog`, `TCleanupDialog`): "NetBox commands" gains "Edit history
  Alt-Shift-F11", "Add bookmark", "Open directory  Alt-Shift-F12", "Generate URL", "Cleanup Configuration".
  Open directory lists the session's last folders (its path history, 40 at most) and, after a separator, its
  bookmarks sorted ("Manage bookmarks" when one was just added), "Del - remove, Ctrl-Enter - command line,
  Ctrl-Ins - clipboard" (F8 and Ctrl+C too; Del on the history clears it); Enter goes there. Edit history
  lists the files edited in the session, Enter or F4 edits one again (unless it is gone). Generate URL shows
  the session's address with "Include username / password / host key / remote directory", "Save
  extension" (http(s):// for WebDAV) and "Copy to clipboard". Cleanup Configuration removes the checked parts
  ("Configuration", "Stored sessions", "Caches": the known host keys and certificates), OK only with one
  checked. Differs from NetBox (ADR-0023 addendum S6.6d): the bookmarks are a file of the plugin's data;
  Ctrl+Enter sets the command line (NetBox inserts); Generate URL has no Script tab ("Create session",
  "Include raw settings" disabled: they do nothing in NetBox either); no INI or random seed file to clean.
- **Synchronized browsing, Compare directories** (S6.6g, NetBox's `ToggleSynchronizeBrowsing`,
  `SetDirectoryEx`, `SynchronizeBrowsing`, `CompareDirectories`): Ctrl+Alt+B or "Synchronize browsing
  Ctrl-Alt-B" (checked while on) says "Synchronized browsing enabled." / "disabled."; while on, the
  server's panel entering a folder or going up takes the disk's panel along (into the same subfolders, up as
  many levels), asks "Cannot open corresponding directory in the opposite panel. Do you want to try to
  create directory '...'?" for a missing one, and turns itself off with NetBox's message when it cannot
  follow; against anything but a disk's panel: "Synchronized browsing is supported only against regular
  panel.". "Compare directories" selects, in both panels, the files only on one side or modified at
  another time and says "Differences found: N local files, M remote files" (or "No differences found.").
  Differs from NetBox (ADR-0023 addendum S6.6g): no "Never ask me again" on the browsing message; comparing
  by size is not offered (NetBox's `CompareBySize` setting, off by default, is not in its dialogs).
- **Synchronize** (S6.6h, NetBox's `FullSynchronize`, `TFullSynchronizeDialog`, `TSynchronizeChecklistDialog`):
  "NetBox commands" > "Synchronize" against the disk's panel: "Local directory:", "Remote directory:",
  " Direction/Target directory " (Both, Remote, Local; from the server's panel the disk by default),
  " Mode " (Synchronize files, Mirror files, Synchronize timestamps), " Synchronize options " (Delete files,
  Existing files only, Preview changes, Selected files only), " Compare criteria " (Modification time, File
  size / Same size only), "Use same options next time" (with NetBox's question to keep a direction changed);
  NetBox's rules between them. The folders' trees are compared as WinSCP's `SynchronizeCollect` does;
  "No differences found.", or with "Preview changes" the "Synchronization checklist" (a line per item with
  its sides and its arrow, Space / Ins toggle, "+" / "-" all, " N of M "); the checked items are done in the
  copying's window ("Synchronizing"), without overwrite questions. Differs from NetBox (ADR-0023 addendum
  S6.6h): the checklist is a menu of checked lines (no column header, no maximize, no custom commands); the
  dialog has no transfer settings group (the default transfer settings are used); "Keep Remote Directory Up
  To Date" comes with S6.6j. Since the S6.6 review (ADR-0023 addendum): times compared at the server's
  precision (`ls -l`'s minutes or days), names without case for any letter, a deletion listed unchecked
  without "Delete files", "Selected files only" from either panel; a name from a server never leaves the
  disk's folder, "Replace invalid characters" off or on.
- **Location Profiles** (S6.6i, NetBox's `TLocationProfilesDialog`): "NetBox commands" > "Location
  Profiles": "Session profiles" / "Shared profiles", the bookmarks' names with their "Local directory:" and
  "Remote directory:"; "Add" asks "Bookmark name:" for the panels' folders now, "Remove" asks "Do you wish to
  remove bookmark '...'", "Rename" asks "New name:", "Open" takes both panels to the bookmark's folders, OK
  keeps the changes. Open directory's bookmarks are the session's profiles too. Differs from NetBox
  (ADR-0023 addendum S6.6i): the two lists are radio buttons, not tab buttons; Add names the panels'
  folders (NetBox takes the lines of the bookmark under the cursor, empty without one).
- **Keep Remote Directory Up To Date** (S6.6j, NetBox's `Synchronize`, `TSynchronizeDialog`): "Watch for
  changes in the local directory ...", "... and automatically reflect them on the remote directory",
  " Upload options " (Delete files, Existing files only, Update subdirectories, Selected files only,
  Synchronize on start), "Use same options next time", Start; then the disk's folder is watched and each
  change sent to the server (compared to it, newer files and new ones up, deletions with "Delete files") in
  the copying's window until Esc ("Cancel current operation?"). Differs from NetBox (ADR-0023 addendum
  S6.6j): the dialog does not stay open with Start / Stop and its counters (the copying's window shows the
  work instead); the folder is read again each second (no change notifications); no transfer settings group.
- **Transfer on background** (S6.8, NetBox's `TCopyDialog`'s `NB_TRANSFER_QUEUE`, WinSCP's `Queue`):
  NetBox's copy dialog has "Transfer on background (add to queue)" after "New and updated file(s) only"
  (kept with "Use same settings next time"); checked, the copy (F5 / F6 down, or up after the program's
  dialog) is the program's own background task - its progress dialog with Background, its overwrite
  questions, F12's list - its engine reading and writing through NetWay on a connection of the panel's
  own. Differs from NetBox (ADR-0023 addenda S6.8a-b, the owner's choice): no NetBox queue list
  (Alt+Shift+Q) nor its "No confirmations" (the program's questions); NetBox's transfer settings (resume,
  the text mode, rights, the file mask, "New and updated") do not reach the background copy; SCP and S3
  files go up whole at their end (kept on the disk meanwhile); "Transfer on background by default" of the
  configuration is not there (the dialog's box, kept with its settings, does it).
- **Commands** (S6.5j, NetBox's `CommandsMenu`, `ApplyCommand`, `ShowInformation`): Alt+Shift+W "NetBox
  commands" (attributes, link, Apply command, Server and protocol information, the session log disabled,
  Home directory, Configure); Ctrl+G "Apply command" ("Enter command to process selected files", the
  patterns' hints, Remote / Local command, "Apply to directories", "Execute recursively", "Show results in
  console", "Copy results to clipboard"; "No files selected" with none); the information with "Copy to
  Clipboard" / "Close". Differs from NetBox (ADR-0023 addendum S6.5j): results in the viewer; no local
  command yet; the menu's other items with their steps.
- **SSH, KEX, Authentication** (S6.5i, NetBox's `TSessionDialog`): SSH - " SSH protocol options " ("Enable
  compression"), " Encryption options " ("Encryption cipher selection policy:" AES, ChaCha20, AES-GCM, 3DES,
  "-- warn below here --", DES, Blowfish, Arcfour with Up / Down); KEX - " Key exchange algorithm options "
  ("Algorithm selection policy:" its 13 lines with Up / Down), " Options controlling key re-exchange " ("Max
  minutes before rekey", "Max data before rekey"); Authentication - "Bypass authentication entirely
  (SSH-2)", " Authentication options " ("Attempt authentication using Pageant", "Attempt
  'keyboard-interactive' authentication (SSH-2)", "Respond with password to the first prompt"). Applied as
  WinSCP. Differs from NetBox (ADR-0023 addendum S6.5i): what `russh` has not is listed but not offered, no
  warning below "-- warn below here --"; single-DES, GSSAPI, agent forwarding, the detached certificate and
  the Bugs tab left out.
- **Connection, Proxy, Tunnel** (S6.5h, NetBox's `TSessionDialog`): Connection - " Timeouts " ("Server
  response timeout", "seconds (0 - no timeout)"), " Keepalives " (Off, "Sending of null SSH packets",
  "Executing dummy protocol commands", "Seconds between keepalives"), " Internet protocol version " (Auto,
  IPv4, IPv6); Proxy - "Proxy type" None / SOCKS4 / SOCKS5 / HTTP / Telnet / Local, the host, port, user,
  password, " Proxy settings " ("Telnet command" or "Local proxy command", "Consider proxying local host
  connections", "Do DNS name lookup at proxy end" No / Auto / Yes); Tunnel - "Connect through SSH tunnel",
  " Host to setup tunnel on " (host, port, user, password, private key). Applied as WinSCP; "Opening
  tunnel..." before the session. Differs from NetBox (ADR-0023 addendum S6.5h): no local tunnel port (the
  tunnel's channel carries the session), keepalives as OpenSSH's requests for both kinds, no "Optimize
  connection buffer size"; the timeout bounds reaching the host.
- **SCP** (S6.5f, WinSCP's `TSCPFileSystem`): an SCP session or `scp://` address connects with no SFTP;
  an SFTP session with "Allow SCP fallback" falls back to SCP when the server has no SFTP, else says
  "Cannot initialize SFTP protocol. Is the host running an SFTP server?". The panel, the copying both ways,
  F7, F8, Shift+F5 / F6, rename, Ctrl+A, Alt+F6, Ctrl+Alt+F (`scp://`) work as over SFTP through the
  server's `ls -la`, `scp -f` / `scp -t`, `rm`, `rmdir`, `mkdir`, `mv`, `ln`, `cp`, `chmod`, `chown`,
  `chgrp`; the overwrite question has no Append and no file resumes (SCP writes none from an offset).
  "Duplicate" goes through the server's `cp -p -r -f` over SFTP too when the server runs commands.
  Differs from NetBox (ADR-0023 addendum S6.5f): each command on a channel of its own, not one shell read
  through markers; the time of an uploaded file set by `touch` after it (WinSCP's `T` record before it).
- **Endurance** (S6.5e, NetBox's `EnduranceConfigurationDialog`; WinSCP's `AllowResume`,
  `SFTPConfirmResume`, `SFTPSource` / `SFTPSink` with `PartialExt`, `QueryReopen`): a file from 100 KB on
  (the configuration's "Enable transfer resume/transfer to temporary filename for" all files, files above
  N KB or none) is written as `name.filepart` and put in place when whole (a file it replaces keeps its
  rights on the server); stopped, the partial file stays; the next transfer asks "Do you want to resume
  file transfer? Target directory contains partially transferred file 'name'." (Yes / No / Cancel), one
  bigger than the source "...which is bigger than a source file. The file will be deleted." (Ok / Abort).
  Never in the text mode, over a link, for the viewer's and the editor's copies. A session the server
  dropped: before the panel's next operation WinSCP's "Lost connection." counted down ("Reconnecting in
  N s...", Esc stops it; "Reconnect after" off: "&Reconnect" / "&Abort"), then the login again with the
  user and password that logged in, at the panel's folder; at most "Maximum number of retries" in a row.
  Differs from NetBox (ADR-0023 addendum S6.5e): reading the panel reconnects with no window (the
  program's panels read without windows: a question there is refused, "Lost connection." said); a
  transfer that loses its session mid-way asks its failure (Abort, then the next operation reconnects)
  instead of reconnecting under the same file; "Wait for reply dialog timeout" is kept only (the
  program's own wait for a host that does not answer is ADR-0019's). FTP too since S6.6k: a control
  connection idle 10 s is asked by a NOOP before the operation, and one that no longer answers is lost
  (WinSCP learns it from the operation's failure).
- **Operations on the server** (S6.5d, NetBox's `MakeDirectoryEx`, `DeleteFilesEx`, `TransferFiles`,
  `RenameFile`, `FileProperties`, `RemoteCreateLink`, `InsertFileNameOnCommandLine`,
  `CopyFullFileNamesToClipboard`; WinSCP's `DeleteFiles`, `CreateDirectory`, `MoveFiles`, `CopyFiles`,
  `ChangeFilesProperties`, `CreateLink`): F7 "Make folder" ("New folder name:", " Attributes ", "Set
  permissions" with Owner / Group / Others R W X, Set UID / Set GID / Sticky bit, Octal, None / Default /
  All, "Use same settings next time"); F8 "Do you wish to delete the file name" / "... N items" (a folder
  with its items, a link as itself); Shift+F5 "Duplicate" and Shift+F6 "Move" ("Duplicate file 'name' to
  remote directory:", the folder with the name or `*.*`, history "WinscpMoveTo", the name a mask);
  Alt+Shift+F6 "Rename"; Ctrl+A "Attributes" ("Change file attributes for", a link's "Link to:", Owner and
  Group by their numbers with the names beside, " Permissions " with [?] for several files, "Add X to
  directories", "Set attributes recursively"; OK only once something changed); Alt+F6 "Add link" / "Edit
  link" ("Link file:", "Point link to:", "Symbolic link"); Ctrl+Alt+F the file's `sftp://` address into
  the command line; Alt+Shift+Ins the selected files' addresses onto the clipboard. The key bar's
  Duplicate, MoveTo, Rename; Shift+F1-F3 blank. Each with NetBox's window ("Deleting", "Moving",
  "Copying", "Setting properties") and Esc, a failure asked with WinSCP's line ("Error deleting file
  '...'.", "Error renaming file '...' to '...'.", "Cannot change properties of file '...'.", "Cannot
  create link '...'."). Differs from NetBox (ADR-0023 addendum S6.5d): "Duplicate" copies through SFTP
  when the server runs no commands (else by `cp`, S6.5f); the command line gets the address
  (FAR's `FCTL_INSERTCMDLINE` inserts it: the program's answer sets the line); no Recycle Bin of the
  server's (the session's option comes with the SSH tabs, S6.5g).
- **Transfer settings** (S6.5c, NetBox's `TCopyParamsContainer`, `TCopyDialog`'s presets,
  `ConfigureEx`; WinSCP's `TCopyParamType`, `TFileMasks`): the copy dialog's line shows WinSCP's
  `GetInfoStr` ("Transfer type: Binary; ..."); "Preset" offers Default, Text files (ASCII), Binary all, No
  preserve time and Custom (the dialog); "Transfer settings..." opens "Transfer settings": Transfer mode
  (Text, Binary, Automatic with its masks), Filename modification (No change, Upper, First upper, Lower,
  Lower 8.3, "Replace \:*"?"), Download options ("Preserve read-only"), Upload options ("Set permissions"
  with Owner / Group / Others R W X, Octal, "Add X to directories", "Ignore permission errors", "Clear
  'Archive' attribute"), Common options ("Preserve timestamp", "Calculate total size"), Other ("File mask
  (example: *|.svn/):", "Speed (KB/s):", "Use temporary directory for downloads" - the queue's, S6.8);
  what a direction cannot use disabled; wrong masks, speeds and octal rights said as WinSCP's. "Use same
  settings next time" keeps them as the defaults. Options > Plugins configuration > NetWay: NetBox's menu
  "Transfer settings" ("NetWay - Transfer settings", the defaults) and "Editor/viewer settings" ("Upload
  after every save", the download's and the upload's settings or dialog). Differs from NetBox (ADR-0023
  addendum S6.5c): a preset is taken only when the list was changed (NetBox's sets "Default" back when the
  list is only passed by); the speed is an edit line with a history (no editable combo); no "Allow more
  than one edited file".
