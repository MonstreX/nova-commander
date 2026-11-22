# ADR-0004: Architecture v2 - OS layer, virtual file system, background jobs, FAR look, DLL plugins

- Date: 2026-11-07
- Status: accepted (decisions agreed with the owner in chat on 2026-11-07; config.toml rules committed by the
  owner in ef3a65d); §9 "Order of work" superseded by ADR-0012 (2026-11-08); §1 and §7 amended (the addendum
  at the end, R7 review E11)

## Context
The first prototype (ADR-0003) showed the look and feel. Before it grows, the owner set six directions:
1. Prepare for other operating systems: a layer of OS abstractions.
2. A panel shows any resource, not only a disk: network, (S)FTP, files over SSH, an archive as a file system.
3. The terminal shows one line under the panels by default (its output is hidden behind the panels);
   Ctrl+O switches to a terminal mode, pressing it again returns to the hybrid mode.
4. The program is its own application, as FAR Manager is: its own window title; the terminal is a part of it.
5. Panels, menus, actions and default colours follow FAR Manager. Not everything is copied; some things are
   dropped or simplified.
6. A plugin system with a base set: a temporary panel and an SFTP client (FTP and stored passwords deferred).

## Decision

### 1. OS layer (`src/platform`)
Everything that depends on the operating system lives in `src/platform`: window title, drives and mount points,
opening a file with its associated program, shell discovery, process creation flags, loading dynamic libraries,
later trash and clipboard. The implementation is chosen at compile time (`platform/windows.rs` behind
`#[cfg(windows)]`), not through trait objects. Only Windows is implemented and tested now; Linux and macOS are
added later as new files in `platform` without changing the rest of the code. Code outside `platform` does not
assume `\`, drive letters or `cmd.exe`. `portable-pty` and `crossterm` already support all three systems.

### 2. Virtual file system (`src/vfs`)
A panel shows a *provider + path* instead of a local directory.
- `Provider` trait: `list`, `stat`, `open_read`, `open_write`, `mkdir`, `remove`, `rename`, plus capability flags
  (writable, renamable, executable files).
- `VfsPath` instead of `PathBuf`, e.g. `C:\work`, `sftp://user@host/home`, `zip:D:\a.zip!/dir`.
- Providers nest: an archive inside SFTP is read through the parent provider's stream (a remote archive is
  first downloaded to a temporary file when random access is needed).
- The local disk is the built-in provider `local`. Windows network shares (`\\server\share`) are local paths;
  browsing servers and shares can be a plugin later.

### 3. Background jobs (`src/jobs`)
Every provider call runs on a worker thread; the UI never waits. A panel shows "reading..." while a listing
loads; copy/move/delete show progress, Esc cancels, and questions ("overwrite?") come back to the UI as dialogs.
Copying between any two providers streams read -> write; inside one provider the fast path (rename, native copy)
is used. This removes the blocking copy of the prototype.

### 4. Terminal modes and shell integration
- Hybrid mode (default): panels fill the screen down to the command line. The command line is the "one terminal
  line"; the shell's output lies behind the panels. Ctrl+Up/Ctrl+Down shrink/grow the panels to show more
  terminal lines (FAR's panel height decrement), default 0.
- Ctrl+O: full-screen terminal. Every key except Ctrl+O goes to the shell (vim, htop, Tab completion, the
  shell's own history work). Ctrl+O again returns to the hybrid mode.
- In the hybrid mode the command line stays ours (model "A" of ADR-0003): arrows, Enter on a file and Ctrl+Enter
  belong to the panels. Shift+Enter runs the command in a separate console window, as in FAR; the prototype's
  "send as typed" on Shift+Enter is removed - shell integration tells when a program waits for input.
- Shell integration: when the program starts a shell it installs a prompt that emits OSC 9;9 (current directory)
  and OSC 133 (prompt start/end, command finished): `PROMPT` for cmd, a `prompt` function for PowerShell,
  `PROMPT_COMMAND` for bash. With it the program knows the shell's directory and when the shell waits for a
  command. Consequences: the `cd` prefix is sent only when the shell's directory differs from the panel's; after
  `cd` in the terminal mode the active panel follows the shell; prompt detection by text becomes the fallback.
- Risk: `vt100` may not handle full-screen programs well enough. A short spike at the start of that work decides;
  the fallback is `alacritty_terminal` (ADR-0002 already lists it).

### 5. Own application, FAR style
A console program that owns its screen, as FAR does (not a GUI window): it sets its own window title
(`<path> - Nova Commander`, updated on directory change and while a command runs), the executable carries an
icon and version information (`build.rs` resource), a double click opens its own console window. The terminal is
a component inside the program. If Windows Terminal is the default console on Windows 11, the window opens as a
Windows Terminal tab - the same as FAR.

### 6. FAR Manager look and behaviour
FAR Manager is open source under the BSD 3-Clause licence (github.com/FarGroup/FarManager). Its default palette
(`far/palette.cpp`), panel view modes (`far/config.cpp`), menus, key bar labels (`far/farlang.templ.m4`) and
default highlighting groups (`far/hilight.cpp`) are the reference. We reimplement them in Rust; no FAR code is
copied (if a fragment ever is, the BSD notice goes with it). The name and logo "Far" are not used.
Rule set by the owner: follow FAR as exactly as possible, adapted to the OS, including the mouse. Deliberate
deviations (Ctrl+O full terminal, shell choice in menus instead of F2) and deferred features are listed in
`docs/far-parity.md` (approved 2026-11-07).

### 7. Plugins: native DLLs with a C ABI
- Installation = copy a folder into `plugins\` next to the executable and restart; removal = delete the folder;
  disabling = `"plugins.<name>.enabled": false` in `settings.json`. No registry, no hot reload.
- Layout: `plugins\<name>\plugin.json` (name, plugin version, API version, library file name) +
  `plugins\<name>\<name>.dll`.
- Rust has no stable ABI, so the boundary is a C ABI: the DLL exports `nc_plugin_entry(host) -> plugin`; both
  sides exchange versioned function tables. A plugin with an incompatible API version or a broken manifest is
  not loaded and is listed with the reason; the program keeps working.
- Strings cross the boundary as UTF-8 (pointer, length); memory is freed by the side that allocated it.
- A plugin can register a VFS provider (prefix like `sftp:`, opening a file by extension or signature), items in
  the plugin menu (F11) and the drive menu (Alt+F1/F2), and dialogs. The host draws everything: a plugin
  describes a dialog (fields, buttons), the host renders it, so the look stays uniform and plugins do not depend
  on `ratatui`.
- Plugin calls run on worker threads. The SDK catches panics at every boundary call. A crash in native plugin
  code still terminates the program - the accepted cost of DLL plugins (alternatives considered: plugins as
  separate processes speaking JSON over pipes - isolation and no `unsafe`, but a protocol, process management and
  file data through pipes; WASM - sandboxed, but a heavy runtime).
- Base plugins ship the same way, as folders, so the loading mechanism is exercised from day one:
  `tmppanel` (temporary panel: a list of files from different places; the first plugin, it proves the API),
  `sftp` (key files and ssh-agent; passwords are asked for and not stored), later `archive` (zip, then every
  7-Zip format as FAR's arclite: ADR-0012 §7).
  FTP and stored passwords are deferred.
- Plugins are built per OS and architecture (`.dll` / `.so` / `.dylib`); the source does not change.

### 8. Repository layout: Cargo workspace
```
Cargo.toml                 workspace + the program (binary crate at the root, src/)
crates/nc-plugin-api       C ABI types and function tables, no dependencies
crates/nc-plugin-sdk       safe Rust wrapper and export macro for plugin authors
plugins/tmppanel           cdylib
plugins/sftp               cdylib
plugins/archive            cdylib (later)
```

### 9. Order of work
Each stage leaves a working program and a green gate.
1. Documents: this ADR, `architecture.md`, `rules.md`, `AGENTS.md`, C4 diagram, `docs/far-parity.md`.
2. FAR base, split by `docs/far-parity.md`:
   2a. screen, palette, highlighting, panels and view modes, sorting, selection, key bar with modifiers,
       function keys, mouse, hybrid/terminal modes with shell integration, window title and icon;
   2b. menu bar with all items, options dialogs, save setup, user menu (F2), shell choice, histories, folder
       shortcuts, attributes, apply command, compare folders;
   2c. info panel, quick view, tree panel, find folder;
   2d. viewer and editor with FAR keys (basic set first), code pages - deferred by the owner: after stage 6
       unless moved earlier; until then the prototype viewer/editor stay with the FAR palette.
3. `platform` layer and the workspace.
4. `vfs` + `jobs`: background listing and operations with progress and cancel.
5. Plugins: API, SDK, `plugin_host`, `tmppanel`.
6. `sftp`, then `archive`.

## Consequences
- `fs` is dissolved into `vfs` (listing, providers) and `jobs` (operations) in stage 4; until then it stays.
- `unsafe` is allowed in `src/platform`, `src/plugin_host/ffi`, `crates/nc-plugin-api` and
  `crates/nc-plugin-sdk` (forbid rule `unsafe-only-in-platform`, owner commit ef3a65d). Plugins contain no
  `unsafe`; the SDK macro generates the export.
- The gate runs clippy and tests with `--workspace`, so plugin crates are checked like the program.
- New external crates come with their own ADR when they are added: `libloading` (DLL loading, stage 5), an SSH
  library for `sftp` (stage 6, `russh` vs `ssh2` decided then), `zip` (archive plugin), a resource compiler for
  the icon (stage 2).
- The FAR look replaces the prototype's approximate colours and layout; tests that assert the old layout change
  together with the behaviour (stage 2 is a behaviour change, not test weakening).
- ADR-0003's "visible `cd` prefix for every command" and "no pass-through mode for full-screen programs" are
  superseded by section 4 once stage 2 is done.

## Addendum 2026-11-22 (review R7, E11): §1 and §7 as built
- §1: the implementation is a folder per system, `platform/windows/` and the stubs `platform/other/`
  (ADR-0005, ADR-0012), behind one OS-neutral interface per subsystem; dynamic libraries are loaded by
  `platform::libraries`, not `libloading` (ADR-0016). "Code outside `platform` does not assume `\`, drive
  letters or `cmd.exe`" holds for paths and drives; the shells' kinds and dialects, the code pages' Windows
  numbers and the stubs' blank dates do not yet (ADR-0017 addendum 2026-11-22, the `[S7]` items).
- §2: a provider's path is its provider and a `/` path inside it (ADR-0013's `Mounted`); an archive or a
  remote folder is a plugin's provider (ADR-0021, ADR-0023).
- §7: plugins are loaded and unloaded while the program runs (P6.0e, ADR-0016 addendum), not at restart only;
  the API is versioned with its structs' fingerprint (API 11, R7 A3). The base plugins are `tmppanel`, the
  plugins of FAR's set (P6), `archive` (7-Zip's library, ADR-0022) and `netway` in place of `sftp`: SFTP, SCP,
  FTP(S), WebDAV and S3 (ADR-0023), a session's password kept by DPAPI (ADR-0023 addendum S6.4b) - FTP and stored passwords
  are no longer deferred.
