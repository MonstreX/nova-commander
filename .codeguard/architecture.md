# Architecture - nova-commander

> Source of truth for module boundaries. The machine-checked part is enforced by the `core-no-ui`, `app-no-ui`
> and `unsafe-only-in-platform` forbid rules (see config.toml). Changes to this file are architectural decisions:
> record an ADR in `decisions/`.
>
> Status: **architecture v2 (ADR-0004, 2026-11-07)**, terminal part revised by ADR-0006, plugins by ADR-0016.
> Stages 2a, 3a-3e, 2b, 2c, 5, P1-P6, 6 and 2d are built (reviews R59, 2026-11-13, and R7, 2026-11-21: this
> file says what the program is); the order of work from here and its tracking are in `docs/roadmap.md` (it
> carries ADR-0012's order on).

## System context
nova-commander is an orthodox file manager in the style of FAR Manager: two file panels with FAR's look
(blue panels, double-line frames, pull-down menu, F-key bar) and an embedded terminal behind the panels.
It is its own console application: it owns its screen and window title; the terminal is a component inside it.
By default the panels fill the screen and the command line sits under them. As in FAR, every command line runs
as a new process in the panel's folder inside one long-lived console (the "user screen"), which Ctrl+O shows;
`cd`, a drive and `set` are handled by the program itself, everything else (`cls` too) by the command
interpreter (ADR-0006). Alt+Shift+T switches to a persistent guest
shell chosen in the settings (cmd, Windows PowerShell, pwsh, WSL bash, Git Bash, or one of the user's own) that receives every key. A panel shows any resource through a virtual file system: local disks now,
SFTP and archives through plugins. Plugins are native DLLs installed by copying a folder.
Single executable plus optional plugin folders, no services, no database. Windows is the only implemented
platform; all OS-specific code lives in `src/platform` so that Linux and macOS can be added there later.

## Stack
Rust (edition 2024, MSVC toolchain), Cargo workspace (ADR-0004 §8). Crates in use (ADR-0002, ADR-0003):
`ratatui` 0.30 with its bundled `crossterm` (rendering and input; always import it as `ratatui::crossterm`),
`portable-pty` (ConPTY), `vt100` (VT emulation), `serde` + `serde_json` (settings), `windows-sys` (raw Win32
bindings for `src/platform`, ADR-0005), `regex` (FAR's regular-expression masks, ADR-0020), `quick-xml` (Colorer's HRC grammars, ADR-0024), `unicode-width` (the view core's cells, ADR-0025); dev:
`tempfile`.
Plugins' own: `netway`'s `russh` (the `ring` backend), `russh-sftp` and `tokio` since S6.4a (ADR-0023);
the `zip` crate of ADR-0021 gone since S6.3d (every format through 7-Zip's library).
Loaded at run time, not linked: 7-Zip's `7z.dll` (LGPL, the unRAR restriction; found as
FAR's arclite finds it, shipped beside the plugin by a package, ADR-0022) through the SDK's `sevenzip`
binding.
Planned, each with its own ADR when added (plugins are loaded by `platform`, not `libloading`: ADR-0016): the
remote file systems' further protocol libraries, the plugin `plugins/netway`'s alone (ADR-0023:
`suppaftp`, `reqwest` with `quick-xml`, an S3 crate, `rustls`, `tokio`; FAR's NetBox's behaviour, under
permissive licences: the program is MIT and takes no GPL code), a
Windows resource compiler (icon), possibly `alacritty_terminal` instead of `vt100`, more `windows-sys` features
as `platform` grows.

## Portability (product requirement)
The program runs without installation: copying the folder is the installation. All settings live in
`settings\settings.json` next to the executable (flat dotted keys as in VS Code). Plugins live in
`plugins\<name>\` next to the executable. Never write to the registry, `%APPDATA%`, `%LOCALAPPDATA%` or the user
profile.

## Diagrams (architecture as code)
Read the diagram before estimating or implementing a task; update it in the same change when the
structure it shows changes.
| Diagram | File | Shows |
|---|---|---|
| C4 context (+ the single container) | `diagrams/c4-context.md` | user, host console, file system, shells, plugins, remote servers |

No ERD: there is no database or persistent data model (settings files are not modelled).

## Product reference
`docs/far-parity.md` lists which FAR Manager features, keys, menus and colours are taken, simplified, deferred
or dropped. UI work follows it; changing a row is the owner's decision. `docs/README.md` maps the other
documents (the roadmap, the inventory of long processes, the tech debt, the reviews).

## Modules
Binary crate at the repository root; top-level modules under `src/` in directory form (`src/<name>/mod.rs`).
Stage numbers refer to ADR-0012 (which supersedes ADR-0004 §9) and `docs/roadmap.md`.
| Module (path) | Status | Responsibility | May depend on | Must not depend on |
|---|---|---|---|---|
| `src/main.rs` | exists | entry point: load settings, set up the console, event loop, restore the console, save settings | all | - |
| `src/app` | exists | application state and behaviour: panels, command line, key map, menus, dialogs, viewer/editor screens, file-operation flows, terminal modes, link to the shell, settings conversion | `vfs`, `jobs`, `editor`, `terminal`, `settings`, `plugin_host`, `platform` | `ui` |
| `src/ui` | exists | rendering with ratatui: panels (the view modes' columns as `app::panel_modes` reads them from the settings, their widths and cells, the highlighting groups' colours laid over the panel's (the groups are `app::highlighting`), size/date formatting), terminal area, bars, menu, dialogs, viewer/editor, themes (ADR-0008: roles, theme files, the built-in FAR palette). Reads `app` state; only writes view metrics (rows per column, stripes, where each panel, its entries, the key bar, the command line and the menu in front are for the mouse, and the first line shown of a scrolled menu) | `app`, `vfs`, `editor`, `terminal` (read-only), `platform` (`files`: attributes, link kinds and targets; `locale`; `input`: held modifiers; `process`: console title, colours and flash) | - |
| `src/editor` | exists | text documents for the viewer, quick view and the editor; `view`: the view core of ADR-0015 (a file's blocks by byte offset handed in by the caller, the code page told from the first block through `vfs::text`, only the rows on screen laid out, the viewer's moves and search; the editor's piece table over the file read, its steps, undo, the line count and the streamed save's bytes, S2d; hex editing, ADR-0027); `syntax`: Colorer's grammars and painting (ADR-0024) | `vfs` | `ui`, `app`, `ratatui`, `crossterm` |
| `src/terminal` | exists | the user screen (one ConPTY that runs one runner process per command line, ADR-0006) and the runner itself (`--run` mode, no UI); the guest shell: shell profiles, ConPTY session, VT screen state, hidden `cd`, shell integration (OSC 9;9 / OSC 133); the mouse as the program inside asked for it (xterm's protocols) | `platform` | `ui`, `app`, `ratatui`, `crossterm` |
| `src/settings` | exists | portable settings file next to the executable: load, defaults, atomic save; the histories, the user menu's file, the viewer's and the editor's positions | `vfs`, `platform` (`files`: the name case rule; the line break) | `ui`, `app`, `ratatui`, `crossterm` |
| `src/platform` | exists since 2a (ADR-0005); split by subsystem in 3a, completed in stage 3 (3a-3e, ADR-0012) | the OS layer, one OS-neutral interface per subsystem: `files` (attribute model, file properties of Ctrl+A: times, owner, links, hard links and their setters, link kinds, directory details, name order and case rule, path rules, copy/move, folder change notifications, the Recycle Bin), `volumes` (volume roots, drive kinds, drive-menu actions, CD kinds, virtual disks, the storage device), `system` (computer, user, memory, power for the info panel, 3c), `process` (shells, the command interpreter and what it knows - its own commands, the program search, its variables, the folders it cannot run in, `cd /d` -, whether a file is a program, console), `elevation` (the helper process and its pipe, ADR-0007), `input` (the console's records, 3e), `clipboard`, `locale`, `libraries` (a library loaded by its path and its exported functions, 5a, ADR-0016). Implementation per OS chosen with `cfg`: `platform/windows/`, stubs for every other OS in `platform/other.rs`; the only place with Win32 `unsafe` | external crates only | everything else in the crate |
| `src/vfs` | exists since 3b step 1 (ADR-0013, `fs` dissolved into it) | `VfsPath` (a local OS path, or a provider's `/` path), the `Provider` trait (blocking, called from worker threads only) (`open_random`: a file read anywhere in it while others may write it, ADR-0015) and its registry, `Entry`, masks and sorting; `FileOpener` (a disk's file opened as a provider's folder for the search in archives, S6.7, implemented by `plugin_host`); `local`: the built-in provider's reading, the drive menu's drives, the info panel's disk and system (`read_info`, 3c), make folder, removing one object (file, empty folder, link, to the Recycle Bin), atomic save; `text`: FAR's word dividers and a file's code page told from its first block, for the view, Find and the line editors (ADR-0028); `test_tree` (tests only): a tree in memory with failures to inject. The interface grows with the steps of 3b that use it | `platform` | `ui`, `app`, `jobs`, `ratatui`, `crossterm` |
| `src/jobs` | exists since F5/F6 (ADR-0009): `transfer` (copy, move), `tasks` (the background task list with its per-volume queue, ADR-0010), `listing` (a panel's folder read on a worker thread, ADR-0013, 3b step 1), `delete` (FAR's `delete.cpp`, 3b step 3), `attributes` (Ctrl+A's "Set", FAR's `setattr.cpp`, ADR-0011), `folder_size` (F3's count of folder sizes), `folder_info` (Ctrl+Q's count of a folder, ADR-0015), `file_reader` (a file's blocks read by offset as a view asks for them, ADR-0015), `find` (Alt+F7's search, FAR's `findfile.cpp`, 2b step 7), `tree` (the folders of a disk for the tree panel, FAR's `treelist.cpp`, 2c), `tree_cache` (the trees' files the tree panel reads and the operations keep up, FAR's `tree3.cache`, 2c), `tree_nodes` and `tree_worker` (a tree as nodes with their times, read in the background by priority, ADR-0014), `work` (the worker thread, questions, pause and cancel every operation shares) | background operations on worker threads: listing, copy/move/delete between providers, progress, cancel, conflict questions | `vfs`, `platform` | `ui`, `app`, `ratatui`, `crossterm` |
| `src/plugin_host` | exists since 5a (ADR-0016) | finds `plugins\*\plugin.json`, loads DLLs (through `platform`), adapts the C ABI to `vfs` providers and commands; raw FFI calls only in `src/plugin_host/ffi`; gives each plugin its data folder `settings\Plugins\<name>` beside the logs (API 7, ADR-0023 addendum S6.4b) | `vfs`, `platform`, `nc-plugin-api` | `ui`, `app`, `ratatui`, `crossterm` |
| `crates/nc-plugin-api` | exists since 5a | `#[repr(C)]` types, function tables, API version; no dependencies | - | everything |
| `crates/nc-plugin-sdk` | exists since 5a | safe Rust traits for plugin authors, export macro, panic catching at the boundary; from P6 the safe wrappers of the system's calls FAR's plugins make (ADR-0016 addendum P6.0e); from S6.3a `sevenzip`, 7-Zip's library through its interfaces (ADR-0022) | `nc-plugin-api` | the program's crate |
| `plugins/<name>` | 5a: `probe` (the tests' plugin, not shipped); `tmppanel` since 5d; `samefolder`, `filecase` since P6.1; `commands` (FAR's farcmds) since P6.2a; `emenu` since P6.3; `compare` since P6.4; `proclist` since P6.5a; `network` since P6.6a; `archive` (FAR's arclite) since S6.1a; `netway` (FAR's NetBox, ADR-0023) since S6.4a, shipped since S6.4b; the editor's `align`, `autowrap`, `brackets`, `drawline`, `editcase` (FAR's editor plugins, API 10, ADR-0026) since S2d.7 | `cdylib` plugins, each with its `plugin.json`; no `unsafe` | `nc-plugin-sdk` | the program's crate, `ratatui`, `crossterm` |
| `xtask` | exists since 5a | `cargo run -p xtask -- package <folder>`: the portable folder (the program, its plugins, the licence and Far Manager's notice) | `serde_json` | the program's crate |

## Dependency rules
Machine-checked (forbid rules in config.toml, warn at mvp unless noted):
- `core-no-ui`: `src/{platform,fs,vfs,jobs,terminal,editor,settings,plugin_host}`, `crates/**` and `plugins/**`
  must not reference `crate::ui`, `crate::app`, `ratatui::` or `crossterm::` (`src/fs` is gone, dissolved into
  `vfs` by ADR-0013; the rule still names it).
- `app-no-ui`: `src/app/**` must not reference `crate::ui`.
- `unsafe-only-in-platform` (block): `unsafe` only in `src/platform`, `src/plugin_host/ffi`,
  `crates/nc-plugin-api`, `crates/nc-plugin-sdk`; every unsafe block has a `// SAFETY:` comment
  (`clippy::undocumented_unsafe_blocks`, block).
Reviewer-checked:
- No dependency cycles between top-level modules; `vfs` does not know `jobs`; nothing but `main` and `ui` (which
  reads its state, ADR-0003) depends on `app`.
- Plugins talk to the program only through `nc-plugin-api`; the host draws all plugin UI.
- No `cfg(windows)` / OS-specific code outside `src/platform` (and the platform-dependent tests that need it).
  A plugin crate (`plugins/<name>`) cannot reach `src/platform`: it may use the standard library's own
  OS extensions under `cfg` (`std::os::windows::ffi`, as TmpPanel's lists), nothing more.

## Critical areas
Changes here are always task class "critical" (spec first, review, harden):
- File operations that delete, overwrite, move or rename user files (`src/vfs/local/ops.rs`, `src/jobs` and later every
  writable `vfs` provider): data loss is the main risk of this product. Tests run against temporary directories;
  overwrite and delete need explicit confirmation flows; a cancelled operation never leaves a half-written target
  under the final name.
- Console lifecycle in `src/main.rs`: the console must be restored (raw mode off, main screen back, title
  restored) on every exit path, including panics.
- Saving files (`vfs::local::ops::write_atomic`, `save_streamed`, `write_bytes_in_place` - the viewer's byte editing,
  `app::editor_save_file`, `settings::save`, `settings::positions`): a failed save must keep the previous content; a
  settings or positions file that failed to parse is never overwritten.
- The plugin boundary (`src/plugin_host/ffi`, `crates/nc-plugin-api`, `crates/nc-plugin-sdk`): memory ownership,
  string lifetimes, API version checks, panics never cross the boundary.
- The elevated helper (`src/vfs/local/elevated.rs`, `elevated_setting.rs`, `elevated_session.rs`,
  `src/platform/windows/elevation.rs`, `--elevated` in `src/main.rs`, ADR-0007): a process with the administrator's rights acting on requests. It carries
  out only the fixed set of primitive calls; anything malformed ends it; automated tests never start it elevated.
- Commands run from the command line (`app` shell link, `terminal`): a command must never run in a directory
  other than the one the user sees; the guest shell is moved only while it waits at its prompt.

## Conventions an agent cannot infer from the code
- Windows is the only implemented platform. OS-specific code goes into `src/platform` behind `cfg`; do not add
  Linux/macOS implementations unless a human asks. From stage 3a `platform` has stubs for every other OS
  (`platform/other.rs` and `platform/other/` by subsystem: same interface, every call `Unsupported` or an empty value, or what `std` does the
  same everywhere: a link's target, "hidden" as a leading dot) and
  `cargo check --workspace --exclude netway --target x86_64-unknown-linux-gnu --all-targets` must pass
  (ADR-0012), the plugins and their SDK included (the SDK's `system` and `sevenzip` are the plugins'
  `platform`: Windows code there behind `cfg`, stubs or `std` elsewhere; a plugin Windows only by its nature
  is `#![cfg(windows)]` whole, empty elsewhere). `netway` compiles C (`ring`) and is built and tested
  natively in WSL Ubuntu instead (`cargo test --locked -p netway`, its target folder in WSL's `/tmp`, removed
  after; ADR-0023 addendum S6.4a). Neither is among the gate's pipelines yet: run them before each commit
  that touches OS-specific code, the SDK or a plugin. Code outside `platform`
  calls it by subsystem (`platform::files::...`, `platform::volumes::...`).
- The FAR look is a product requirement: box drawing, the 16-colour FAR palette, F-key bar at the bottom,
  `docs/far-parity.md` is the reference. Do not modernise the visual style without being asked.
- Tests never touch real user directories; use a temporary directory created by the test.
- Python helpers from `.codeguard/core` are run with `python -B`: a `__pycache__` there is a protected-path change.
