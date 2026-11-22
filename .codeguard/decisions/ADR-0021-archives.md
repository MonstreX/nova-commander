# ADR-0021: archives as FAR's arclite, the `zip` crate and API 5

- Date: 2026-11-16
- Status: accepted (roadmap S6.1-S6.2; ADR-0012 §7 and ADR-0004 §7 ask for this ADR as the step begins;
  owner 2026-11-16: "archives as FAR wherever there is a doubt"); decision 2 (zip by the `zip` crate) and
  the addenda's crate-made differences superseded by ADR-0022 §1 since S6.3d: zip is read and written by
  7-Zip's library as every format, the crate gone (R7 review E11). API 5 and the rest stand.

## Context
Stage 6 opens archives as panels, as FAR's arclite does (`far-source/plugins/arclite`): Enter on a file
matching its masks, Ctrl+PgDn on any file it recognizes, `arc:` in the command line, its Extract dialog for
F5 / F6 / Alt+F6, its Add dialog for F5 into an archive and Shift+F1, Shift+F2 and Shift+F3 on a disk's
archives. ADR-0012 §7 makes zip the first format (the `zip` crate) and every 7-Zip format the second (S6.3,
its library and licence in their own ADR). The plugin API (ADR-0016) is version 4 and its tables are final;
arclite calls two things of FAR that API 4 does not have:

- FAR's `FSF.ProcessName(PN_CMPNAMELIST)`: arclite's `AnalyseW` matches a file against its include and
  exclude masks itself, for Enter only by default ("Use masks for Ctrl+PgDn" off): Ctrl+PgDn on any file is
  checked by its contents. API 4's `PLACE_FILE_MASKS` makes the program match the masks for both keys.
- FAR's `GetFilesW`: F5 / F6 on a plugin's panel hands the selected items and the other panel's folder to
  the plugin, which shows its own dialog and writes the files (`OPM_VIEW`, `OPM_EDIT`, `OPM_QUICKVIEW`,
  `OPM_FIND` without the dialog, into the program's temporary folder). API 4 copies out through `read`, by
  the program's Copy dialog, one file at a time: neither arclite's Extract dialog nor one pass over a solid
  7z block is possible.

## Decision
1. **The plugin.** `plugins/archive` (ADR-0004 §8), the program's own name "Archive" where FAR says
   "ArcLite" (no FAR names users see); arclite's prefix `arc`, its messages, dialogs, options and defaults
   otherwise (`options.cpp`: the include masks list, the Enter / Ctrl+PgDn rules, start view mode 2, sort
   by name). Where the program's mechanisms and arclite differ, arclite's behaviour is taken unless the
   difference is written down in far-parity with its reason.
2. **zip by the `zip` crate** (MIT), with its pure-Rust codecs only (Deflate, Deflate64, BZip2, LZMA, AES
   and ZipCrypto, as the crate's features offer them without a C compiler); a method the build cannot read
   is that file's error, as 7-Zip's "Unsupported method". Its dependencies are the plugin's alone: the
   program does not link them. S6.3 (7-Zip's formats through its library, as arclite) has its own ADR.
3. **API 5, one release over S6.1-S6.2** (as API 3 was over P6.0): the version is 5 from S6.1a, the tables
   may grow until S6.2's last step, then they are final. Every plugin's manifest says `"api": 5`.
   - `NcHost::match_masks(masks, name)`: FAR's `ProcessName(PN_CMPNAMELIST)` through the program's masks
     (the same parser as the panels': `|` exclusions, regular expressions, ADR-0020).
   - `NcEntry::allocated`: FAR's `AllocationSize` (an archive's packed size), the program's allocated-size
     column and sort; `SIZE_NONE` when not told.
   - `NcPlugin::get` (S6.1b): FAR's `GetFilesW` - the entries, the destination folder, moving or not, and
     FAR's operation mode (`OPM_VIEW`, `OPM_EDIT`, `OPM_QUICKVIEW`, `OPM_FIND`, `OPM_SILENT`,
     `OPM_TOPLEVEL`); a panel that sets `PANEL_GIVES_FILES` is copied out by it, F3 / F4 / quick view /
     Find file on its files too (P2.4's temporary folder), the others through `read` as before.
   - S6.2 adds what writing needs (FAR's `DeleteFilesW` over a selection at once, `ProcessHostFileW`, the
     handling of Shift+F1 / Shift+F2 / Shift+F3); they are recorded in an addendum here when they are made.
4. **Steps.** S6.1a: the panel (opening by masks and contents, `arc:`, F11, configuration, the listing, title,
   info lines, ".." back to the file; F3 / F4 through `read`). S6.1b: extraction (`get`, the Extract,
   Overwrite, Password and error log windows, Alt+F6, Ctrl+A's properties). S6.2: writing zip. S6.3:
   7-Zip's formats. S6.7: search in archives.

## Consequences
- `architecture.md` lists `zip` among the plugins' crates; `plugins/archive` among the plugins.
- ADR-0016 notes API 5 here; the test manifests and the e2e stands say `"api": 5`.
- far-parity's arclite line and the Shift+F1-F3 rows follow the steps.
- A file inside an archive is still read by `read` for F3 until S6.1b moves it to `get` (tech-debt S6.1).

## Addendum (S6.1b, 2026-11-16): `get` as made
- `NcPlugin::get(state, instance, path, *const NcGet)`: the request in a struct (clippy's limit of seven
  arguments, which the gate keeps): `names` of the panel's folder, `destination` (FAR's `DestPath`),
  `moving`, `mode` (`GET_SILENT`, `GET_FIND`, `GET_VIEW`, `GET_EDIT`, `GET_TOP_LEVEL`, `GET_QUICK_VIEW`:
  FAR's `OPM_*` values) and `folder` (FAR's current folder, for a relative folder typed in the plugin's
  dialog). 0 done, `ERROR_CANCELLED` stopped, else the plugin's reason (FAR's 1, -1, 0).
- `PANEL_GIVES_FILES`: F5 / F6 to a disk (not into another plugin's panel: the program's copy through
  `read` stays, FAR's `PluginToPluginFiles` goes through a temporary folder) hand the selection and the
  other panel's folder to the plugin, which shows its dialog; F3 / F4 have it write the file into a new
  folder of the program's (`GET_SILENT | GET_VIEW` / `GET_EDIT`), the viewer or editor on its first file.
  Each is a press the program waits for as a panel key is (`KeyWait`), the keys held, Esc asking it to
  stop; its end is `Event::Got`: done takes the selection, both panels read again.
- `NcHost::resume`: arclite's progress asks "Do you want to interrupt current operation?" after Esc
  (with the program's "Interrupt" confirmation on); No goes on, and `cancelled` answers 0 until the next
  Esc (the stop is per call and stays without it).
- The step split: S6.1b extraction (F5 / F6 / Alt+F6 by arclite's dialogs, F3 / F4); S6.1c the quick
  view and Find file through `get`, an archive inside an archive, Ctrl+A's properties.

## Addendum (S6.1c, 2026-11-16)
- Enter / Ctrl+PgDn on a file of a panel that gives its files: `get` with `GET_SILENT | GET_EDIT` into a
  new folder of the program's (FAR's `ProcessEnter` with `PluginMode`), the copy offered to the plugins
  of its masks; a panel opened over it keeps the folder in its stack (`HostFile::temp`) and deletes it as
  the panel goes (FAR's `delayed_delete`: tried on a thread for a while, the plugin closing its panel after).
- The quick view and Find file keep reading a plugin's file through `read` (no `GET_QUICK_VIEW` /
  `GET_FIND` calls): a piece at a time, a large file at once (the owner's rule for huge files, ADR-0014's
  spirit); the modes stay in the ABI for plugins that want them. The owner accepted this in place of the
  split's "through `get`" (2026-11-16).
- The copies' folders also go with the program (after its plugins have ended) and with every wait for
  them that ends unanswered.
- `ITEM_READ_ONLY` (FAR's `DIF_READONLY`) for arclite's "Attributes" dialog: an edit line selected and
  scrolled, never changed.

## Addendum (S6.2a, 2026-11-16): writing into an open archive
- The step split: S6.2a writing into an archive open on a panel (F5 / F6 into it, F4's copy put back);
  S6.2b deleting (FAR's `DeleteFilesW` over the selection at once) and folders (`MakeDirectoryW`), F6 out
  of an archive with "Delete files from archive"; S6.2c new archives (Shift+F1, the menu's Create), the
  disk's archives (Shift+F2, Shift+F3), `arc:`'s commands, API 5's tables final.
- An archive's panel takes paths (`PANEL_TAKES_PATHS`): F5 / F6 into it keep the program's copy dialog
  with the target `arc:` (FAR's `ShellCopy` to a plugin's panel), then `put` (`NcPut`, ADR-0016's
  addendum) runs arclite's `put_files`: its "Add files to archive" dialog, `PrepareUpdate`'s walk and
  questions, the archive written anew as `{GUID}.tmp` beside itself and moved over it, the panel read
  again. F4's edited copy goes back through `put` with `GET_EDIT` when the panel showing its folder takes
  paths (arclite: the same dialog, Overwrite chosen), else through `write` as before.
- Writing by the `zip` crate: the old items copied as they are (`raw_copy_file`), a file taking an item's
  place written there, the others after; Store, Deflate, PPMd and BZip2 (the crate writes neither LZMA
  nor Deflate64), AES for encryption (the crate writes no ZipCrypto), 7-Zip's NTFS times field, the
  archive's comment and creation time kept, what is before the zip (a self-extracting archive's
  program) copied first (arclite's `copy_prologue`). An archive with several items of one name is not
  written anew (the crate keeps one item of each name): the update is refused with its reason. Every
  panel of the plugin on that archive is read again after it.

## Addendum (S6.2b, 2026-11-16): deleting and folders
- An archive's panel deletes and makes folders itself (`PANEL_DELETES`, `PANEL_MAKES_FOLDERS`, ADR-0016's
  addendum): F8 runs arclite's `delete_files` (its question "Do you want to delete selected files?", the
  entries' items and all under them left out of the archive written anew, arclite's deleting progress),
  F7 its `create_dir` ("Enter directory name:", the folder written with the time now). Both write as an
  update does (`update_write` with the plan's removed items, or one folder added) and put the new file in
  place the same way.
- F6 out of an archive: "Delete files from archive" on (arclite's `move ? triTrue`), disabled for an
  archive the plugin cannot change; once nothing failed, the entries extracted are deleted with all under
  them, an entry with a file the user skipped somewhere under it kept whole (arclite's
  `extracted_indices`: the selected ones less a skipped item and every folder above it).
- The program names the entries it hands a plugin only by their names (arclite takes the panel's item
  itself, its `UserData`): a file and a folder of one name in an archive's folder (the zip's `x` and `x/`)
  cannot be told apart, so F8 and F6 refuse such a name with their reason rather than delete the other.
  F7 refuses a name with `.` or `..` in it and one already there (7-Zip would write a second item).

## Addendum (S6.2c, 2026-11-16): new archives
- A new zip is made as arclite's `create_archive` (the Plugins menu's "Create archive") and its `put_files`
  into a panel of no archive yet (Shift+F1, ADR-0016's `open_new`) make it: the "Create archive" dialog
  (`UpdateDialog` with `new_arc`) with its profiles, the path made whole by its `eval_arc_path`, then
  `Archive::create` through the same walk and writer as an update (`update::prepare_in` with nothing there,
  `update_write::write_new`): written beside its place and put there as an update's is (`swap_in`: every
  panel of that file closed and read again), so a file there that the user agreed to overwrite stays
  until the new one is whole (arclite writes over it at once). The sources moved never take the new
  archive or a folder it is in with them (arclite's `DeleteSrcFiles` would).
- Kept as arclite keeps them: the `update_*` settings of a new archive and the profiles (`profiles`, one
  name a line, each value under `profiles\<name>\`).
- The split: S6.2c new archives; S6.2d "Extract archive(s)" and "Test archive(s)" of the menu, Shift+F2
  (`OFP_EXTRACT`), Shift+F3 (`ProcessHostFile`: Test); S6.2e the `arc:` commands and API 5's tables final.

## Addendum (S6.2d, 2026-11-16): testing, the menu's Extract / Test
- A test is arclite's `Archive::test` (`testing`): the files under the entries asked for read through in
  the archive's order with nothing written, the zip library checking each one's CRC at its end; the first
  failure ends it in arclite's words (`SetOperationResult`: the file's path from the folder tested, the
  archive's path), Esc stops it; the password asked once. A failure about an archive names it first
  (arclite's `Error::objects`), "Errors:" before its lines, as its `error_dlg` and log write them.
- The Plugins menu's "Extract archive(s)" and "Test archive(s)" are arclite's `bulk_extract` and
  `bulk_test` in the plugin (`bulk`): each archive opened by itself, failures into one log; the commands
  live in `menu_commands` (the plugin's lib kept under its size limit).
- Shift+F3 on an archive's panel is FAR's `ProcessHostFile` through ADR-0016's `process_host_file`: the
  plugin shows arclite's menu and its own windows, then tells the program whether to unselect the
  entries (its `PPIF_SELECTED` cleared) or that it changed something (TRUE). Its failures are its own
  windows, as arclite's `FAR_ERROR_HANDLER` shows them. Differs: a stop (Esc, Cancel at the password)
  says nothing and unselects nothing, where arclite's `test_files` loses it in `Error::Append` and says
  the test passed; a name the archive no longer has fails the test.
- The split: S6.2e Shift+F2 / Shift+F3 on a disk's files (FAR's `OpenPluginForFile` with `OFP_EXTRACT` /
  `OFP_COMMANDS`, the root got with `OPM_TOPLEVEL`); S6.2f the `arc:` commands and API 5's tables final.

## Addendum (S6.2e, 2026-11-16): a disk's archives by keys
- The program offers each file (ADR-0016 addendum S6.2e); the plugin answers `OFP_EXTRACT` and
  `OFP_COMMANDS` as arclite's `analyse_open` does (its masks checked, only Ctrl+PgDn may pass them by).
- `get` with `GET_TOP_LEVEL` is arclite's `extract` of a disk's archives at once: its Extract dialog for
  the plugin's first, the answers kept (`extraction::Batch`, arclite's `static batch_options`, with the
  folder given as FAR's `DestPath` handed on) for the next ones got silently, each archive of several
  entries in a folder of its own when "separate directory" is asked neither way; "Go to target
  directory" sets the active panel's folder (and cursor) through the host; "Delete archive" deletes the
  file without closing a panel (arclite's `close_panel` of a panel never shown closes none).

## Addendum (S6.2f, 2026-11-16): `arc:`'s commands
- Read by `command_line` as arclite's cmdline.cpp reads them (its tokens, parameters, `E_BAD_FORMAT` as
  `BadFormat`, the syntax shown for it), run by `command_run` with the parts the panels use: `bulk`'s
  `extract_each` / `test_all` for `x` / `t`, `extract_noting` per folder for `e`,
  `deleting::delete_extracted` and `swap_in` for `d`, `creating::written` for `c`,
  `updating::write_with` / `after_with` (no log) for `u`. `u` takes the archive's own options as
  arclite's `load_update_props` does for a zip (level 7, Deflate: its look at the first item never runs,
  `GetNumberOfItems`'s `S_OK` being 0, and 7-Zip's zip handler names no method of the archive); the "Add
  files to archive" dialog of S6.2a starts at Deflate the same way (it took the first item's method).
- Every name of a command is made whole as FAR's `get_absolute_path` does (`std::path::absolute`:
  `GetFullPathName`, `..` and `/` folded), so `swap_in` finds an open panel of the archive however it was
  named and no source's name keeps a `..`.
- With it the split of S6.2 ends; S6.3 brings 7-Zip's formats (its own ADR).
