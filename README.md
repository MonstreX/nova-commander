# Nova Commander

A console file manager in the style of [FAR Manager](https://github.com/FarGroup/FarManager), written in Rust:
two panels, an embedded terminal, a built-in viewer and editor, native plugins.

> **Alpha.** Version 0.1.0-alpha.1 is the first public build. It is used daily by its author, but expect
> rough edges; please report what you find in the issues.

## Why

FAR Manager is the reference for keyboard-driven file management: its panels, keys, menus and dialogs are
what Nova Commander follows, closely enough that FAR's users feel at home from the first minute. Nova Commander is a
new implementation of that idea, not a port: it keeps FAR's way of working and rebuilds the inside around
what a file manager meets today - huge files, slow network shares, remote servers, modern shells - with a
portable core meant to run on more than one system.

## What it does differently

- **Never frozen by a slow disk.** Reading folders, copying, deleting, counting sizes, searching and every
  plugin call run on worker threads. A network share that stops answering or a stuck device does not hang
  the program: a wait shows what it is waiting for, with a progress or busy bar and its time, and Esc gives
  control back.
- **Files of any size, opened at once.** The viewer and the editor read only the part on the screen. A file
  of many gigabytes opens instantly, jumps to its end at once and is edited in place (a piece table over
  the file, saved streamed), where FAR's editor first reads the whole file into memory.
- **Syntax highlighting built in, in the editor and the viewer.** Colorer's grammars for hundreds of
  languages are part of the program, not a plugin, and colour the viewer's text too.
- **Hex editing** in the editor and in the viewer, which FAR does not have.
- **An optional code editing mode** for those used to today's code editors: Tab / Shift+Tab indent a
  block, Enter keeps the indent, brackets and quotes close themselves, Ctrl+/ comments lines by the file's
  language, Alt+Up / Alt+Down move lines, Ctrl+Space completes words. Off, every key is FAR's.
- **A real terminal behind the panels.** Command lines run in one long-lived console (ConPTY), so
  full-screen programs, colours and interactive tools work as in a terminal window; Alt+Shift+T switches to
  a guest shell - PowerShell, pwsh, WSL, Git Bash or your own - that receives every key.
- **Remote file systems and archives included.** The shipped plugins open archives as panels through
  7-Zip's library (FAR's ArcLite reimplemented) and SFTP, SCP, FTP(S), WebDAV and S3 servers (after FAR's
  NetBox); their copies can run in the background through the program's own copy engine.
- **Plugins in safe Rust.** Plugins are native libraries behind a small, versioned C ABI with a Rust SDK
  that keeps `unsafe` out of plugin code and catches a plugin's panic at the boundary; plugins load and
  unload while the program runs.
- **Wide characters done right.** CJK text and emoji are laid out by screen cells, in names and in text.
- **Portable.** One folder is the installation: settings (JSON), histories and logs live beside the
  program, nothing goes to the registry or the user profile.

Much of FAR is here as you know it: the panel modes, sorting, highlighting, the drive menu, the user menu,
file associations, folder shortcuts, histories, Find file, the tree panel, quick view, the info panel,
file attributes, links, elevation, themes.

## Requirements

- Windows 10 or 11, x64 - the platform built so far. Every system call sits behind one portable layer
  (`src/platform`), so other systems are added there; Linux is planned.
- [7-Zip](https://www.7-zip.org/) installed, for the archive plugin (its `7z.dll` is found as FAR's
  ArcLite finds it).
- The classic Windows console (conhost) is recommended: Windows Terminal takes some of FAR's keys for
  itself (F11, Alt+F4, Ctrl+Shift+digits).

## Installing

Unpack the release archive into a folder you can write to (for example `D:\Programs\NC`; not
`Program Files`, the settings are kept beside the program) and run `nova-commander.exe`.

To start it as FAR's own shortcut does - in the classic console, maximized, with a larger font - make a
shortcut to `nova-commander.exe`, open its Properties, set "Run" to "Maximized" and choose the font on
the Font tab.

## Building from source

- Rust (edition 2024) with the MSVC toolchain.
- `cargo build --release` builds the program; `cargo run -p xtask -- package <folder> --release` lays out
  the portable folder with the plugins, Colorer's grammars and the notices.
- `cargo test --workspace` runs the tests.

## Status

Alpha: the file manager, the viewer, the editor and the shipped plugins are complete enough for daily use.
Next come the program's own help (F1), the Linux port and whatever daily use brings up.

## License

MIT, see [`LICENSE`](LICENSE). Third-party components and their licences are listed in
[`THIRD-PARTY-NOTICES.md`](THIRD-PARTY-NOTICES.md). Nova Commander is not affiliated with FAR Manager;
FAR's behaviour is reimplemented, no FAR code is copied.
