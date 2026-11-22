# ADR-0016: Stage 5 - plugins: the loader in `platform`, a Rust SDK, FAR's TmpPanel whole

- Date: 2026-11-12
- Status: accepted (owner, 2026-11-12)
- Details: ADR-0004 §7-8 (DLL plugins with a C ABI, the workspace), ADR-0012 §6 (loading libraries is the first
  step of stage 5), ADR-0013 (the provider interface the plugin ABI mirrors), `docs/far-parity.md`
- Amends: ADR-0004's consequence "`libloading` (DLL loading, stage 5)"

## Context
Stage 5 brings DLL plugins: `crates/nc-plugin-api` (C types and function tables), `crates/nc-plugin-sdk` (a safe
Rust trait and the export macro), `src/plugin_host` (finding, loading, adapting) and the first plugin,
`tmppanel`. ADR-0004 named the `libloading` crate for loading; ADR-0012 put loading libraries into `platform`
(3e) and then moved it to stage 5's first step.

## Decision
1. **Libraries are loaded by `platform`** (owner): `LoadLibraryExW` / `GetProcAddress` / `FreeLibrary` through
   `windows-sys` already in use, behind an OS-neutral `platform::libraries` interface (other systems: `dlopen`
   when they are ported; the stub says unsupported). No new crate.
2. **Only a Rust SDK in this stage** (owner): the ABI stays C (`#[repr(C)]`, `extern "C"`, UTF-8 strings as
   pointer and length, memory freed by the side that allocated it), but no C header is kept until one is
   needed.
3. **`tmppanel` is FAR's TmpPanel whole** (owner): items from different places, adding and removing, list files
   (`.tmp`) saved and opened, several temporary panels, its menu and settings, Alt+F7's "Panel" button.
4. **Steps**, each a commit with a green gate:
   - 5a: the workspace, `nc-plugin-api`, `nc-plugin-sdk`, `platform::libraries`, `plugin_host` (manifests,
     loading, the API version, a broken plugin reported with its reason, `plugins.<name>.enabled`), F11's
     plugin commands with the host's message window, and a packaging task (`xtask`) that lays out a portable
     folder (the program, `plugins\<name>\plugin.json` and its library) for end-to-end checks;
   - 5b: commands and dialogs: the drive menu's items, dialogs described by the plugin and drawn by the host;
   - 5c: a plugin's panel as a `vfs` provider (the ABI mirrors `Provider`), its title and columns;
   - 5d: `tmppanel` as in 3 (its steps in §14);
   - 5e: the rest of FAR's: plugin info on the info panel (opening a file by a plugin - for stage 6's
     archives - and Options > Plugins configuration moved to 5d, §14).
5. A test plugin (`plugins/probe`, not shipped) exercises the loader in tests; it is built with the workspace
   (a `cdylib` and `rlib`, a dev-dependency of the program, so its library exists before the tests run).

## 5b: what a plugin shows (owner, 2026-11-12)
6. **Dialogs are described in rows and laid out by the host** (owner): a plugin lists its items in order (text,
   edit lines, check boxes, radio buttons, drop-down lists, separators, buttons); the host places them as FAR's
   dialogs are (a label above its line, the buttons centred at the bottom) and draws them with its own form.
   Coordinates given by the plugin may come later, if needed.
7. **A dialog has events from the start** (owner): while it is up, the plugin's handler hears a button pressed,
   an item changed and the dialog closing, on the plugin's thread; its answer changes items (text, checked,
   enabled, focus) or keeps the dialog open. The program never waits for the handler: keys go on, the answer is
   applied when it comes (closing waits for it).
8. **A plugin writes a log, not stdout** (owner): the host gives every plugin a log function
   (`settings\Logs\<plugin>.log`); the SDK documents that `println!` draws over the screen. Catching the
   process's stdout is not done: the program draws through it too.
9. 5b goes in three commits: 5b-1 a message with the plugin's buttons (FAR's `Message`: warning colours, lines
   centred or left, no buttons = a window kept while the plugin works), a menu (FAR's `Menu`: separators, check
   marks, disabled items, automatic hotkeys, break keys), an input line (FAR's `InputBox`) and the log; 5b-2 the
   dialogs of 6-7; 5b-3 the drive menu's items (running the plugin until 5c gives it a panel to open).
10. The ABI is not frozen before the first release: `API_VERSION` stays 1 while the tables grow in 5b-5e, and
    a function's signature may change too (5b-3's `item_count` / `item` / `run_item`); this repository's
    plugins are built with the program, one built against an older table is not told apart (tech-debt).
    5d-3a grew `NcHost` (`setting`, `set_setting`) with `API_VERSION` still 1: a plugin built for the newer
    table would call past the end of an older host's (its size check catches only a smaller table it is
    handed, not an older version number). `API_VERSION` must be raised before plugins ship apart from the
    program (tech-debt).
    **Addendum (P2, 2026-11-13): `API_VERSION` is 2**, one release with every change ADR-0019 asks of the
    ABI: the host's `progress(done, total, text)` and `cancelled()` (§5 there), the panel's start sort and
    order (FAR's `StartSortMode`, `StartSortOrder`). Both sides check the version and their tables' exact
    sizes, so any later change takes a new version: dialogs' list boxes and masked lines, input history,
    the plugin's own find, times and attributes of files put in, the items asked again as menus open - API
    3, when wanted. The host's functions answer only the plugin's own thread, but for `progress` and
    `cancelled`, which a plugin's worker thread may call (review R45).
    **Addendum (R7 review A3, 2026-11-22): `API_VERSION` is 11**, the tables' only change: both carry the
    fingerprint of every ABI struct's size and alignment (`nc_plugin_api::ABI`, `NcHost::abi`,
    `NcPlugin::abi`), checked by each side after the version and the size, so a struct changed under the
    same version is refused; a test pins its value, so a change of a struct fails it until the version is
    raised. A function's signature is not seen by it: such a change raises the version by hand, as before.
    **Addendum (P6, owner 2026-11-14): `API_VERSION` is 3**, one release (roadmap P6.0a-d) with what FAR's
    first plugins (samefolder, filecase, farcmds, emenu, compare, proclist, network) call of FAR's API - the
    panels' state, items and changes (`PanelControl`), the viewer and the editor - and every item put off to
    API 3 above (dialogs' list boxes, masked lines and history, `InputBox` history, the plugin's own find, a
    panel's info lines and custom sort, the items asked as menus open, times and attributes of files put in).
    The version is 3 from the first of these steps; the tables may still grow until the last, as plugins
    ship with the program. P6.0d added `NcPlugin::set_times` (`NcTimes`: four times, the hidden, system and
    read-only marks when given), called for each file a copy writes into a plugin's panel and for each
    folder it made at the end; the last step is P6.0e, the rest of what the six plugins call (user screen,
    `ACTL_*`, `PluginsControl`, the file filter), after which the tables are final.
    **Addendum (P6.0e, owner 2026-11-14):** the host's `synchro` (any thread; the plugin's `synchro` called
    back on its thread, FAR's `ACTL_SYNCHRO`), `post_keys`, `screen` (its size and the console's window),
    `filter` (the panel's filter asked about entries in batches), `clipboard`, `show_help`; `DO_RUN_COMMAND`
    for the user screen (the program's terminal; plugins never draw on the console); a panel state's column
    types and widths; an entry's change time and its own attribute bits. What FAR's plugins do through Win32
    (emenu's shell menu, proclist's processes, network's shares) comes as safe wrappers in `nc-plugin-sdk`,
    which may hold `unsafe`, with the plugin that needs them: the ABI does not grow for it, the program holds
    no plugin's own code, plugins stay without `unsafe` (owner's choice over host functions and over
    `unsafe` in plugins). A plugin loaded and unloaded while the program runs (FAR's `PluginsControl`,
    farcmds' `load:` / `unload:`) comes in P6.0e too (owner).
    **Addendum (P6.0e second part, owner 2026-11-14):** plugins load and unload while the program runs. A
    library is loaded from a copy beside it (`.<stem>.nc-<pid>-<n>.<ext>`, removed when its thread ends,
    left-over copies removed at the next load), so the installed file may be replaced. Unloading lets the
    plugin go: its provider goes, its panels go back to the folders they came from, its windows, waits,
    requests and keys are forgotten, its call is asked to stop, the host's functions answer nothing to it,
    and its thread frees the library once its call returns. One whose call never ends is left where it hangs
    (thread and copy to the program's end; Windows has no safe way to stop it) and a new instance starts from
    a new copy: Ctrl+R in the wait's window once it does not answer Esc, or in Options > Plugins (the
    program's own list: Enter or Space loads or unloads, kept for the next start). `NcHost::plugins_control`
    (`PLUGINS_FIND`, `PLUGINS_LOAD`, `PLUGINS_UNLOAD`; by name, folder, manifest or library) is FAR's
    `PluginsControl`. A start is told later, never waited for by the screen. With this the tables of API 3
    are final: a change from here on is API 4.
    **Addendum (P6.3, 2026-11-15):** the program's calls FAR makes of a known plugin by its id go to a
    prefix: Apps and Shift+Apps (a panel; a drive of the drive menu with its root after the prefix) are
    `rclk_apps:` to the plugin that has it (FAR's `KnownIDs.Emenu` with `OPEN_FILEPANEL`, `OPEN_*DISKMENU`),
    a right click `rclk_mouse:X Y` (the screen's cell first, FAR's mouse position), not echoed; no table
    changes. A separator item with a text in a placed dialog shows it as its title (at
    its `X1` when given, in the middle for -1), as FAR's `DIF_SEPARATOR` with a text.
    **Addendum (P6.6b, owner 2026-11-15): API 4.** FAR's Network plugin asks before it disconnects a drive
    only when FAR's Confirmations say so (`GetSetting(FSSF_CONFIRMATIONS, "RemoveConnection")`); API 3 has
    no way to read them. `NcHost::confirmation` (appended to the host's table) answers by FAR's names
    (`Delete`, `DeleteFolder`, `RO`, `Drag`, `Esc`, `Exit`, `HistoryClear`, `RemoveConnection`, any case):
    1 the program asks, 0 it does not, -1 a name it does not know. The table changed, so the version is 4
    (owner: a new version over an append inside API 3, so that a plugin built for 3 is refused by its
    version with a clear reason, not by its table's size); every plugin's manifest says `"api": 4`. The
    tables of API 4 are final in turn: a change from here on is API 5.
    **Addendum (S6.1a, 2026-11-16): API 5** for the archives, one release over S6.1-S6.2 (ADR-0021 §3):
    `NcHost::match_masks` (FAR's `ProcessName` with `PN_CMPNAMELIST`) and `NcEntry::allocated` (FAR's
    `AllocationSize`) first; `NcPlugin::get` (FAR's `GetFilesW`) in S6.1b; the tables final at S6.2's end.
    **Addendum (S6.1b, 2026-11-16):** `NcPlugin::get` with its request `NcGet` (names, destination,
    moving, `GET_*` = FAR's `OPM_*`, the program's folder), the panel flag `PANEL_GIVES_FILES`, and
    `NcHost::resume` (a stop taken back: a plugin's own "Interrupt?" answered No); ADR-0021's addendum.
    **Addendum (S6.1c, 2026-11-16):** the dialog item flag `ITEM_READ_ONLY` (FAR's `DIF_READONLY`).
    **Addendum (S6.2a, 2026-11-16):** `NcPlugin::put` takes its request in a struct `NcPut` (the sources,
    moving, `GET_*` as FAR's `OPM_*` of `PutFilesW`: `GET_EDIT` the editor's copy put back after F4, the
    folder they came from as FAR's `SrcPath`) and answers `ERROR_CANCELLED` for the user's stop; the
    SDK's `Plugin::put` takes a `PutRequest`. TmpPanel takes the sources as before.
    **Addendum (S6.2b, 2026-11-16):** `NcPlugin::delete_files` (FAR's `DeleteFilesW`: the names, `GET_*`
    as `OPM_*`) and `NcPlugin::make_folder` (FAR's `MakeDirectoryW`: the name asked for in, the name
    made out), with the panel flags `PANEL_DELETES` and `PANEL_MAKES_FOLDERS`: F8 (Del, Shift+Del,
    Shift+F8) and F7 on such a panel go to the plugin, which asks itself; the program shows no question
    and waits for the call as for a panel key (`Event::Got`, `Event::Made`, the cursor on the folder
    made). The SDK: `Plugin::delete_files`, `Plugin::make_folder`, `PanelFlags::deletes`,
    `PanelFlags::makes_folders`.
    **Addendum (S6.2c, 2026-11-16):** `NcPlugin::open_new` (FAR's `OpenFilePlugin` with no file,
    `OFP_CREATE`: `AnalyseW` without a name, then `OpenW` with `OPEN_ANALYSE`): a panel of the plugin's with
    nothing in it yet, or none. Shift+F1 on a disk's panel offers the selected files (the one under the
    cursor when none is) to the plugins that open files, in their order, as Enter offers a file (FAR's
    default confirmations: no menu); the first that makes such a panel gets them through `put` (its folder
    `/`, copied, `OPM` 0, the panel's folder as FAR's `SrcPath`) on its own thread in the same job, the panel
    closed after (FAR's `PluginPutFilesToNew` with its temporary panel); the end comes as `Event::Created`,
    the selection gone once done and the panel read again (FAR's `ClearSelection`,
    `Update(UPDATE_KEEP_SELECTION)`). The SDK: `Plugin::open_new`; a dialog handler's
    `Controls::value` reads back the states and texts it set in the same call (FAR's `DM_GETCHECK`,
    `DM_GETTEXT` read the dialog as it is now; before, a profile written into the items was not seen by
    the check after it).
    **Addendum (S6.2d, 2026-11-16):** `NcPlugin::process_host_file` (FAR's `ProcessHostFileW`): Shift+F3
    on a plugin's panel opened from a file hands it the selected entries (the one under the cursor when
    none is) of the panel's folder, mode 0; the plugin asks what to do itself and answers what it did in
    `*done`: `HOST_FILE_CHANGED` (FAR's TRUE: the selection goes, both panels are read again) or
    `HOST_FILE_UNSELECT` (the entries unselected: FAR's plugin clearing their `PPIF_SELECTED`, then its
    `PluginClearSelection`); waited for as a panel key is, its end `Event::Processed`. The SDK:
    `Plugin::process_host_file` returning `HostFileDone` (by default nothing). Shift+F2 on such a panel,
    or one whose plugin writes its files out, is F5 (FAR: the plugin names its `HostFile`; the ABI does
    not carry it). A disk's files (`OFP_EXTRACT`, `OFP_COMMANDS` with `OPM_TOPLEVEL`) come with S6.2e.
    **Addendum (S6.2e, 2026-11-16):** `NcPlugin::open_file`'s `alternative` names why the file is offered:
    `OPEN_ENTER`, `OPEN_PGDN`, `OPEN_EXTRACT` (Shift+F2, FAR's `OFP_EXTRACT`), `OPEN_COMMANDS` (Shift+F3,
    `OFP_COMMANDS`); the SDK's `Plugin::open_file` takes `OpenHow`. Shift+F2 / Shift+F3 on a disk's panel
    offer its files one after another as FAR's `PluginHostGetFiles` / `ProcessOneHostFile` do: per file
    and plugin one job on the plugin's thread (`Job::DiskFile`: `open_file`, `list` of `/`, `get` with
    `GET_TOP_LEVEL` - `GET_SILENT` after the plugin's first file - or `process_host_file` with
    `GET_TOP_LEVEL`, `close_panel`), the panel never shown; not taken: `Event::Key` passing it on (the next
    plugin, then the next file); else `Event::DiskFile` (not opened: the next file; done: the file
    unselected, the next one; not done: the run ends). Waited for as a panel key is; the run goes on while
    its panel stays on its folder and no other press is waited for. The table's layout is unchanged:
    an API 5 plugin built before S6.2e reads `OPEN_EXTRACT` / `OPEN_COMMANDS` as Ctrl+PgDn (API 5 is
    not final until S6.2f; the shipped plugins are rebuilt with it).
    **Addendum (S6.2f, 2026-11-16):** API 5's tables are final: `NcHost`, `NcPlugin` and the structs and
    constants they use stay as they are under `API_VERSION` 5; a change from here (a call, a field, the
    meaning of a code) is API 6. `arc:`'s commands need no new call: `open_command` runs them and answers
    with the panels read again. Left as it is: a plugin's own calls on a panel's entries hand names, not
    FAR's items (`UserData`), so a file and a folder of one name in one folder cannot be told apart
    (ADR-0021 addendum S6.2b).
    **Addendum (S6.7, 2026-11-17): API 6.** Alt+F7's "Search in archives" (FAR's `ArchiveSearch`:
    `OpenFilePlugin` with `OFP_SEARCH` and `OPM_FIND`) takes a new reason of `open_file`, `OPEN_SEARCH`
    (4; the SDK's `OpenHow::Search`): the file opened without any window - no progress, password, menu or
    error - or passed. A new meaning of a code, so `API_VERSION` is 6 (an API 5 plugin would take it as
    Enter and show its windows on the search's thread); the tables' layout is unchanged and every manifest
    says `"api": 6`. The host asks it on the plugin's thread as a job of its own (`Job::SearchOpen`) that
    the search's thread waits for, stopped with the search (the plugin's `cancelled()`); the panel opened
    is walked through the plugin's provider and closed (`Job::Close`) once the search leaves it. `vfs`
    has the opener's interface (`FileOpener`, `OpenedFile`: closed when it goes) so `jobs` needs no host,
    and a place inside a container file (`provider:D:\a.zip!/dir`) names what was found there after the
    panel is closed; Go to opens the archive again as Enter does, F3 / F4 open it again with
    `OPEN_SEARCH` for the copy. API 6's tables are final in turn: a change from here is API 7.
    **Addendum (S6.4b, 2026-11-17): API 7.** A plugin's data folder (ADR-0023 §3: NetWay's sessions):
    `NcHost::data_folder`, the last field of the host's table (the SDK's `Host::data_folder`), names
    `settings\Plugins\<name>` beside the logs' `settings\Logs`, for what the user's work with the plugin
    keeps; its own folder (`plugins\<name>`) is replaced by an update of the plugin. The host does not
    make it: the plugin makes it when it writes there. A new field of the table, so `API_VERSION` is 7
    and every manifest says `"api": 7`; API 7's tables are final in turn.
    **Addendum (S6.5c, 2026-11-17): API 8.** A dialog's box, FAR's `DI_SINGLEBOX` (`ITEM_BOX`, the SDK's
    `DialogItem::single_box`): a single frame at its place in a dialog given a size, its title in its top's
    middle; one row high a line across with the title (NetBox's transfer settings). `PANEL_PUTS_ON_SAVE`
    (the SDK's `PanelFlags::puts_on_save`): a panel that takes paths gets the editor's copy of its file at
    each save (`put` with `GET_EDIT`) while the editor stays, FAR's NetBox's upload at `EE_SAVE`; the
    program counts the copy as made then, so leaving the editor puts it back only when saved since. The
    program has no editor events for plugins: the copy and its origin are the program's, the put is the
    plugin's news of the save. `API_VERSION` 8, every manifest `"api": 8`; API 8's tables are final in turn.
    **Addendum (P6.6c, 2026-11-15):** FAR's call of its Network plugin from a share's root (`KnownIDs.Network`
    with `OPEN_FILEPANEL` and the share's name) goes to the prefix `net_up:` as the P6.3 calls do: to the
    plugin that has it, not echoed; the plugin's "not mine" (`pass`) is FAR's failed open, the program's
    drive menu then. The listing shows ".." at a share's root while a plugin has that prefix. No table
    changes.

11. **5b-2's dialog** (owner, 2026-11-12): the items are text, an edit line (plain or a password), a check box
    (with FAR's third state `[?]`), a radio button (starting a group or not), a drop-down list, a separator and
    a button (the default one; one that does not close the dialog, FAR's `DIF_BTNNOCLOSE`); each may be
    disabled, hidden or stand on the same row as the one before. The host lays them out as FAR's dialogs are (a
    label above its line, each check box and radio button on a row of its own, the buttons at the end in one
    centred row under a separator; as wide as the content, at least 40 columns, at most the screen). The
    handler hears Init, Changed (a text, a check, a radio button, a list's choice), Button (one that does not
    close) and Close (a button or Esc; its answer may keep the dialog open), sees every item's value and may
    change texts, states, enable, show or focus items; it may show the plugin's other windows meanwhile (a
    message over the dialog). The program waits for the handler only on Close. The plugin gets the button
    pressed (or Esc) and every item's value. An in-dialog list box, masked input and edit lines' history are
    later (tech-debt).
    **Addendum (P6.0c, API 3):** the list box, the masked line, the edit lines' history, the plugin's own
    place and coordinates, a list's lines changed, the focus moving and the dialog drawn told, each key's
    change told at once. A dialog that asks (`DIALOG_KEYS`) has its handler hear each key and click first and
    take it or let it go: the dialog then waits for that answer too, the screen never (the keys after it held
    in order, Esc past 300 ms taking the dialog away, as a panel's keys under ADR-0019).

12. **5b-3's drive menu items** (owner, 2026-11-12: FAR's default hotkeys): a plugin's items are a second
    place of the same three functions as its F11 commands - `NcPlugin.item_count` / `item` / `run_item` take
    `PLACE_COMMANDS` or `PLACE_DISKS` (a place a plugin does not know has no items; 5e's configuration items
    are a third place), the SDK's `Plugin::disks` / `open_disk`. They stand under the drives after a
    separator, with FAR's default hotkeys 1..9, 0 by the plugins' order (none past the tenth) - a wrong
    claim of mine: FAR has none, the user gives each with F4 (§15, E9) -, by name or by hotkey as the
    menu's "Sort plugins by hotkey" says; Ctrl+7 shows or hides them. An item chosen runs on
    the plugin's thread as a command does until 5c, where it opens the plugin's panel.

13. **5c: a plugin's panel** (owner, 2026-11-12):
    - **By path, as `vfs`** (owner): a panel of a plugin is a `VfsPath::Mounted` - the plugin's provider id,
      `Base::Session(<the panel's instance>)` and a `/`-separated path. The plugin answers whole-path calls
      that mirror `vfs::Provider` (list, stat, read, write, make folder, rename, remove), not FAR's
      `SetDirectory` / `GetFindData` state; both panels, background tasks and the folder history work on it
      as on any provider. Calls still run on the plugin's own thread, one at a time: the host's provider
      forwards each to it and waits (workers only, never the UI thread). An item of the drive menu or an F11
      command may open a panel (FAR's `OpenW` returning a handle): the plugin names its instance and root;
      the panel closes (the plugin hears it) when it leaves the plugin's paths.
    - **Real names** (owner, FAR's `PFLAGS_REALNAMES`): an entry may carry a local path; viewing, editing,
      running, copying and moving it act on that file directly, never through the plugin. Deleting asks the
      plugin (TmpPanel only takes an item off its list) - changed by §14: F8 deletes the real files, as FAR's.
    - **The whole provider set in 5c** (owner): open, list with the panel's title, folders in and out, close,
      read (F3, copying out), write (copying in: TmpPanel's "add"), make folder, rename, remove; the probe
      exercises each; 5d is TmpPanel itself.
    - **The plugin's panel modes** (owner, FAR's `PanelModesArray`): a plugin may give its own modes
      (Ctrl+0..9: the column types and widths in the program's panel-mode strings, the columns' titles), each
      entry its texts for `C0`..`C9` and a description; without modes a plugin's panel shows the program's.
      Done in 5c-3: the modes come with the title (`NcPanelInfo.modes`, `NcPanelMode` with `MODE_*` flags;
      the SDK's `Plugin::panel_modes`, `PanelMode`), the texts with each entry (`NcEntry.columns`,
      `description`; the SDK's `FileEntry.columns`, `description`), `C0`..`C99` read as FAR's; a mode given
      replaces the program's at its key as FAR's `PrepareViewSettings` does; Ctrl+6..9 show a plugin's modes
      there (`ViewMode::Provider`); the view the panel had comes back as it leaves the plugin (FAR's
      `PopPlugin`) and is the one saved meanwhile.
    - Commits: 5c-1 the ABI and the host's provider, a plugin's panel opened, listed, entered, left and
      closed, with its title; 5c-2a the file operations (one entry, read, write, make folder, rename,
      remove); 5c-2b real names; 5c-3 the panel modes and `C0`..`C9`.

14. **5d: the temporary panel** (owner, 2026-11-12), FAR's TmpPanel as its help and defaults have it (`N,S` /
    `0,8`, the status line `NR,SC,D,T` / `0,8,0,5`, lists `*.temp`, the prefix `tmp`):
    - **F8 deletes the real files, as FAR's** (owner): on entries with real names the program deletes the
      files themselves as on their disk (its question, the Recycle Bin by the settings); the panel then drops
      the names of files gone. F7 takes names off the list without touching the files.
    - **A plugin declares its panel's keys** (owner): with its look a panel names its keys and their key bar
      labels (TmpPanel's F7 "Remove", Alt+Shift+F2 "SavLst", Alt+Shift+F3 "Goto", Alt+Shift+F12 and
      Alt+Shift+0..9 "Switch", Ctrl+PgUp); a key pressed goes to the plugin's thread with the item under the
      cursor and the selection, the screen never waits, and the answer (read the panel again, show a path in
      this or the other panel, open another instance) is applied when it comes. Not FAR's synchronous
      `ProcessPanelInput`.
    - **A plugin's settings and Options > Plugins configuration in 5d** (owner): the host keeps a plugin's
      values (key and value) in the program's settings under `plugins.<name>`; F11's Shift+F9 and Options >
      Plugins configuration open a plugin's configuration, TmpPanel's being its own dialog (5b-2).
      5d-3a done: the ABI's `NcHost::setting` / `set_setting` (FAR's `PluginSettings` Get / Set, text
      values) and `PLACE_CONFIG` (FAR's `PluginConfig`; `run_item` there is `ConfigureW`, a non-empty
      answer "changed"); the host keeps each plugin's values on its thread and the program keeps each one
      set as `plugins.<name>.settings.<key>`, saved with the settings; a configuration that changed them
      has the plugin's items read again and every plugin's panel shown read again (`ConfigureCurrent`).
      The menu comes back when the plugin's call ends, after the windows it showed (our calls are not
      synchronous as FAR's); after the configuration's own failure it does not. F11's Shift+F9 and
      Alt+Shift+F9 as FAR's `CommandsMenu`.
      5d-3b done: TmpPanel's stored options in effect as FAR's (`TmpClass.cpp`): one global current common
      panel (one instance `common`) or lists of the panels' own (`local<n>`, gone on `close_panel`); safe
      mode, any elements, folders' contents, duplicates as written, SavLst in UTF-16. The key answer grew
      (API_VERSION still 1, §10's tech-debt): `KEY_ACTIVE_FOLDER`, `KEY_ACTIVE_FILE`, `KEY_COMMAND_LINE`,
      `KEY_PASS` (the program does the key as its own, never sending it again) and `KEY_REREAD_QUICK_VIEW`;
      `Event::Key` carries the press's serial: the keys after a panel key wait for its answer in order (FAR's
      `ProcessPanelInput` is synchronous), the mouse too; a window the plugin opens in that call takes them
      at once; Esc once the plugin is slow (300 ms) stops the wait (D14). Deviations D10, D12, D13, D14 in
      the audit.
    - **Opening a file by a plugin and command line prefixes in 5d** (owner): FAR's `OpenFilePlugin` by a
      plugin's masks (Enter, Ctrl+PgDn on a file) and `CommandPrefix` (`tmp:...` in the command line) are the
      program's general mechanisms, used by stage 6's archives later.
      5d-4 done: the ABI's `PLACE_PREFIXES` and `PLACE_FILE_MASKS` (items that are no menu: a prefix or a
      mask list each, read again after a configuration), `NcPlugin::open_command` (FAR's `OPEN_COMMANDLINE`)
      and `NcPlugin::open_file` (`AnalyseW` + `OPEN_ANALYSE`; `KEY_PASS` not its file, no flags
      `PANEL_STOP`), both answering a `NcKeyAnswer` (`KEY_OPEN` its panel, `KEY_ACTIVE_FOLDER`,
      `KEY_COMMAND_LINE`, the new `KEY_SHELL_OPEN`), and `NcHost::decode` (the system's code pages, the OEM
      one a list is read in). The program waits for these answers as for a panel key's (D14); a panel opened
      from a file goes over the file's (`Entered::host`, FAR's `PrevDataList`). The plugin host's jobs moved to
      `plugin_host/worker.rs`. TmpPanel: `command`, `listfile`, `menu_list`, `run`. Deviations D15-D18 in the
      audit. API_VERSION still 1 (§10's tech-debt).
      5d-5 done: `PLACE_FIND_LIST` (one item: the plugin takes found files, FAR's `SetFindListW`) and
      `NcPlugin::find_list` (`OPEN_FINDLIST` + `SetFindList`: `KEY_OPEN` its panel, `KEY_PASS` the next
      plugin's); Alt+F7's "Panel" waits for the answer as a command's. TmpPanel's `find_list`
      (`FindSearchResultsPanel`). Deviation D19.
    - Copying or moving into a panel that takes paths (TmpPanel's) hands the plugin the sources' paths (FAR's
      `PutFiles`, after the program's copy dialog with the target `<Format>:` - corrected by §15; moving
      never removes the sources): it adds references, folders without their contents by TmpPanel's default
      (`CopyContents = 0`; its "Copy folder contents?" with the setting, 5d-3). A panel may name the mode it opens
      in (FAR's `StartPanelMode`; TmpPanel's own mode is at Ctrl+4). Enter on a folder with a real name goes to
      that folder on its disk.
    - Commits: 5d-1 the plugin with the panel's paths (opened from the drive menu and F11, entries named by
      their full paths, put, F8, Enter on folders, the start mode, the common panel's ten instances); 5d-2
      the panel's keys (Remove, Goto, Ctrl+PgUp, Switch, SavLst); 5d-3 the settings and the configuration
      (safe mode, any elements, its modes, the menus' items); 5d-4 the prefixes and opening a file (list
      files, `+safe` `+any` `+replace` `+menu` `+full` `+0..9`, a menu from a list); 5d-5 Alt+F7's "Panel"
      button (FAR's `SetFindList`, a new panel for search results).

15. **Stage 5 audited against FAR's sources** (owner, 2026-11-12): 5a's F11 menu to 5d-1 were written without
    reading FAR's sources; `docs/reviews/2026-11-12-stage5-far-audit.md` lists each difference with its file and line in
    `far-source/` (FarGroup/FarManager, cloned into the project, git-ignored). The owner: «делаем все как FAR,
    учитывая наши технические особенности архитектуры» - every difference is fixed to FAR's behaviour except
    where the architecture needs it: D1 (a plugin's panel as a `vfs` provider, the program's dialogs and
    transfer engine), D3 / D4 (the screen never waits for a plugin), D5 (dialogs laid out by the program), D6
    (Ctrl+6..9 a plugin's modes only, until the program's own 6-9). D2 (real names handled by the program
    only where the plugin does not do it itself), D7 (Esc at "Copy folder contents?" is No, with its setting in 5d-3) and D8 ("Sending
    files" on every put) follow FAR. §12's "FAR's default hotkeys" for the drive menu rested on a wrong claim:
    FAR has none (E9). Order: the temporary panel (E1, E3-E5, D7, D8), its keys with F7 "Remove" (E2, 5d-2's
    mechanism), plugin panels (E6-E8, D2), the menus (E9, E10), the windows' API (E11-E15); then 5d-2's rest.
    From now on every FAR behaviour is read in the sources first and cited. The windows' API grew as FAR's
    (E11-E13, E15): `MESSAGE_ERROR`, `MESSAGE_BUTTONS_*` and `\1` / `\2` lines; `MENU_ITEM_GRAYED`,
    `MENU_ITEM_HIDDEN`, `MENU_WRAP`, `MENU_SHOW_AMPERSAND`, `MENU_REVERSE_AUTO_HOTKEYS`; `INPUT_*` (FAR's
    `FIB_*` values); a null column title the program's, an empty one shown (the SDK's
    `PanelMode::titles` became `Vec<Option<String>>`).

## Addendum 2026-11-22 (A1): the program's name, Nova Commander
The program is Nova Commander (owner 2026-11-22; Voyager Commander was its working name). Its ABI's names
follow: the crates `nc-plugin-api` / `nc-plugin-sdk`, the types `NcHost`, `NcPlugin`, `NcStr` ..., the
library's export `nc_plugin_entry`. The layout and the fingerprint (`ABI`) are as they were, so API stays
11; a plugin built before the rename exports `vc_plugin_entry` and is not loaded ("no entry point"). None
was published. NetWay's DPAPI entropy keeps the old name, so the passwords stored under it open still.

## Consequences
- `unsafe` stays where the forbid rule allows it: `src/platform`, `src/plugin_host/ffi`, the two crates.
- The gate's `--workspace` checks the crates and plugins with the program.
- Plugins built for one OS and architecture; the packaging task builds them with the program.
