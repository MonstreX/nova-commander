# ADR-0012: Base subsystems first - the order of work revised

- Date: 2026-11-08
- Status: accepted (owner, 2026-11-08: Linux stubs - yes, `vfs` now - yes, the order - yes; deletion as below;
  after a review of the plan the same day: the Linux check with tests, ADR-0013 for `vfs`, ADR-0007 decided
  before 3b ends, the deletion settings)
- Supersedes: ADR-0004 §9 (order of work). Amends: ADR-0011 (its steps), ADR-0009 (step 3 moves into 3b)
- Related: ADR-0007 (elevation, proposed), ADR-0010 (tasks)

## Context
Stages 1 and 2a are done, and parts of 2b went ahead of the plan (drive menu, F5/F6 with tasks). Each new
dialog has been built on whatever base was there, and the base is weak where the OS is involved:
- `platform` (about 2,900 lines, Windows only) has an interface shaped after Windows: `Attributes(u32)` are
  Windows bits read by `fs` and `ui`, `Entry.reparse_tag` and the `TAG_*` constants are Windows notions,
  `fs::drives` is built on drive letters. Nothing is exported for another OS, so the program does not even
  compile elsewhere.
- Windows knowledge sits outside `platform`: `cfg(windows)` in `app/keyboard.rs` and
  `app/cmdline_insert.rs`; the shells `cmd/powershell/pwsh/wsl.exe` in `terminal/shell.rs`; `%COMSPEC% /S /C`
  in the runner; the `PATHEXT` look-up in `terminal/gui_program.rs`; `powershell.exe -File` in
  `app/user_screen_link.rs`.
- Files: listing runs on the UI thread (a slow share freezes the screen); panels are not refreshed when the
  disk changes (far-parity says "Take, 2a"); delete is a synchronous `remove_dir_all` with no recycle bin, no
  progress, no Retry / Skip, no read-only question; links cannot be made; there is no `vfs`, so everything
  new is written on `fs`, which is to be dissolved later.
- Drives: no reaction to devices arriving or leaving; network calls and drive actions run on the UI thread.
- Terminal: `portable-pty` and `vt100` are portable, the rest (runner, Ctrl+C, a new console, GUI programs,
  the shell list) is Windows.
- No mouse; no Cargo workspace and no library loading (plugins need both).

Building more dialogs (Ctrl+A, menus, options) on this base means writing them twice. The owner decided to
build the base subsystems first, platform-dependent ones above all, and the UI on top of them afterwards.

## Decision
1. **New order of work.** Stage labels keep their meaning where they still exist, so `docs/far-parity.md`
   and the ADRs stay readable:

   | Order | Stage | Content | Status |
   |---|---|---|---|
   | 1 | 1 | documents | done |
   | 2 | 2a | screen, panels, keys, terminal | done, except the mouse (3e) and automatic refresh (3b) |
   | 3 | **3a** | portable `platform` interface, Linux stubs | next |
   | 4 | **3b** | files: `vfs` + `jobs` (the former stage 4) | |
   | 5 | **3c** | drives and volumes | |
   | 6 | **3d** | terminal and processes | |
   | 7 | **3e** | console input and mouse, clipboard, locale, workspace and libraries | |
   | 8 | 2b | menus, options, dialogs on the new base (Ctrl+A, Alt+F6, Find file, filter, ...) | partly done |
   | 9 | 2c | info panel, quick view, tree panel | |
   | 10 | 5 | plugins | |
   | 11 | 6 | `sftp`, then `archive`: zip, then every format of 7-Zip | |
   | 12 | 2d | viewer and editor (deferred by the owner) | |

   Stage 4 no longer exists: where earlier ADRs and documents say "stage 4" for `vfs` and `jobs`, read 3b;
   its UI features (Find file, the panel filter) are 2b. Each step leaves a working program and a green gate.

2. **3a: one interface, per OS behind it.**
   - `platform` is split by subsystem: files, volumes, processes and console, input, clipboard, locale,
     libraries. Every public type is OS-neutral: a portable attribute model instead of Windows bits (Windows
     bits stay reachable for Windows-only columns and dialogs), a link kind instead of a reparse tag, a volume
     root instead of a drive letter (a letter is one kind of root). What the user sees does not change: on
     Windows drives keep their letters as in FAR (the menu's hotkeys, `C:`, `C:\`); other systems show
     their volumes as is customary there (mount points, `/`, `/media/...`), owner 2026-11-08.
   - Path rules come from `platform`: what a root is (today `parent().is_none()`), the separator, UNC and
     drive roots, the end slash, the long-path form a raw OS call needs (`std` adds `\\?\` itself, the
     Win32 calls in `platform` do not yet). `VfsPath` itself is 3b.
   - Whether names differ by case comes from `platform`: masks, sorting, `is_same_dir`, quick search and the
     `to_lowercase` comparisons in `app`, `fs` and `terminal` assume they do not, as on Windows.
   - "Hidden" is part of the attribute model, worked out per OS (an attribute on Windows, a leading dot on
     Unix).
   - No `cfg(windows)` outside `platform` (tests that need Windows excepted). The shell list, the command
     interpreter and how a command line is passed to it come from `platform`.
   - **Stubs for every other OS** (`cfg(not(windows))`, so macOS compiles too): the same interface, every
     call answering `io::ErrorKind::Unsupported` (or an empty value where the interface has no error). They are
     not an implementation (that comes with the port), only a check that the interface stays portable:
     `cargo check --target x86_64-unknown-linux-gnu --all-targets` passes after every step from 3a on, tests
     included; a test that needs Windows is marked `#[cfg(windows)]`. The target is added once with
     `rustup target add x86_64-unknown-linux-gnu`. The gate does not run the check yet (its configuration is
     the owner's); until the owner adds it, it is run by hand before each commit and named in the report.

3. **3b: files on `vfs` and `jobs`**, in steps, each with its own commit and green gate. It starts with
   **ADR-0013** for `vfs` (ADR-0004 §2 is a sketch): the form of `VfsPath`, a blocking or an asynchronous
   provider, errors and capabilities, how the transfer engine works across providers and keeps the fast
   path (`CopyFileExW`) between local ones. `fs` is dissolved into `vfs` and `jobs` along the way.
   1. `vfs` and the `local` provider; the panels on `VfsPath`; listing on a worker thread.
   2. Change notifications refresh the panels.
   3. Deletion (below).
   4. Copy and move through `vfs`; the rest of ADR-0009 (access rights, timestamps, contents of symbolic
      links).
   5. Making links (hard link, junction, symbolic link) in `platform` with Alt+F6; the file properties
      model of ADR-0011 together with its "Attributes" dialog (owner, 2026-11-08: one step, the model alone
      would be dead code).

   **Deletion** as FAR's `far/delete.cpp`, run as a task (ADR-0010):
   - keys on a shown panel: F8 - to the recycle bin (FAR's default `DeleteToRecycleBin`), Shift+F8 - the file
     under the cursor, Shift+Del - delete at once, past the recycle bin (FAR's `delete_type::remove`), Del -
     as F8 when the command line is empty (the owner's addition: FAR's plain Del only edits the command
     line), Alt+Del (wipe) stays deferred;
   - FAR's questions: "Do you wish to move ... to the Recycle Bin?" / "Do you wish to delete ...?", a folder
     that is not empty, a read-only file (Delete / All / Skip / Skip all / Cancel), a file with hard links,
     errors with Retry / Skip / Skip all / Cancel;
   - FAR's "Deleting" progress, with Background / Pause / Cancel as F5/F6 have, a task in F12 Screens and in
     the per-volume queue. FAR deletes in the foreground only: going to the background is ours;
   - the recycle bin through `platform`: `SHFileOperationW` with `FOF_ALLOWUNDO` as FAR on Windows; the
     freedesktop Trash and the Finder's Trash with the port;
   - FAR's settings as keys of the settings file with FAR's defaults: delete to the Recycle Bin (System), the
     confirmations Delete, Delete non-empty folders and read-only files (Confirmations); the dialogs that
     change them are 2b.

4. **3c: drives and volumes.** The menu's model on volume roots (a mount point on Unix), letters kept on
   Windows as in FAR; a volume's identity (the per-volume task queue's `volume_of`; `st_dev` on Unix) and
   free space in the volumes interface; devices arriving and leaving refresh the menu and the panels;
   network names and the drive actions (Del, Shift+Del, Ins) leave the UI thread; CD types and VHD disks
   (`docs/tech-debt.md`). **What the info panel (Ctrl+L, 2c) shows of the system comes from `platform`
   here** (owner 2026-11-08: as FAR, or more where useful): per volume - label, file system, serial number,
   total / free / available to the user, the network name or SUBST folder, the device; of the system -
   computer and user name, memory, power. One call that reads it all off the UI thread; the panel itself
   is 2c.

   Amended by the owner, 2026-11-09: 3c goes in five steps, each with its own commit - (1) the menu's
   model on volume roots and a volume's identity for the queue (the volume serial number on Windows, so
   a SUBST drive, a network drive and its UNC path are one volume); (2) the drive actions and the
   remaining disk checks off the UI thread; (3) devices arriving and leaving; (4) CD types and VHD
   disks; (5) the system's data **together with the info panel (Ctrl+L)**, which moves here from 2c:
   the data alone would be dead code (as ADR-0011's model came with its dialog). The panel's folder
   description file stays 2c, its plugin info stage 5.

5. **3d: terminal and processes.** The runner, Ctrl+C to the command, Shift+Enter in a new console, GUI
   programs that are not waited for, opening by association, the shell profiles, whether a file is a program
   (`PATHEXT` on Windows, the execute bit on Unix: highlighting and Enter): all through `platform`.
   **Elevation**: the owner decides ADR-0007 before 3b ends; if accepted, its helper process is made here and
   offered at once by copy, move and delete ("Access denied").

   Amended by the owner, 2026-11-09: 3d goes in six steps, each with its own commit - (1) the command
   interpreter's knowledge (`PATHEXT` search, its own commands, `%VAR%`, `cd /d`, the folders it cannot run
   in) and whether a file is a program move from `terminal` into `platform::process`, the guest shell's
   folder is looked at off the UI thread; (2) starting as FAR's `Execute`: a program found is started
   directly (`CreateProcess`, no command interpreter, so network folders work), a document typed on the
   line opens with its association, the rest goes to the interpreter; FAR's own `pushd`, `popd`, `chcp`,
   `title`, `exit`; the code page restored after a command; "Cannot execute"; Ctrl+Alt+Enter runs as
   administrator; (3) ADR-0007's elevated helper (critical); (4) its "Access denied" question in copy, move,
   delete and F7; (5) a file held by another program named in the error (Restart Manager, `IFileIsInUse`);
   (6) Ctrl+Shift+Tab (FAR's `ConsoleDetachKey`) sends a running command to the background as a screen of
   F12 (ADR-0010): FAR gives the program its console window, which a pseudo console cannot do.
   Steps (3) and (4) are one commit: the helper alone would be dead code, its first user is the question
   (as ADR-0011's model came with its dialog).

6. **3e: the rest of the base.** Console input read in `platform` (gray keys, held modifiers) and the mouse
   as far-parity §6; the clipboard and locale behind the new interface; the Cargo workspace.
   Changed by the owner on 2026-11-09: loading libraries moves to the first step of stage 5 (it has no
   user before the plugin host, and code with no user does not pass the gate); what needs COM (the shell's
   answer about a Recycle Bin, long paths into it, `IFileIsInUse`) leaves 3e for one step with its own
   ADR.

7. **6: archives as FAR's arclite.** Changed by the owner on 2026-11-10: the `archive` plugin is not zip
   only. Its first step is zip (the `zip` crate, ADR-0004); its second reads and writes every format 7-Zip
   knows (7z, rar, tar, gz, xz, bz2, cab, iso, ...) through the 7-Zip library, as FAR's arclite does: open an
   archive as a folder, extract, add, delete, test, archive commands (Shift+F1 / Shift+F2 / Shift+F3). How
   the library is found or shipped, and its licence (LGPL with the unRAR restriction), get their own ADR
   when the step begins.

## Consequences
- ADR-0011's steps become: the model with the "Attributes" dialog, Ctrl+A, Files > File attributes and F4 in
  the drive menu in 3b; "Process subfolders" as a task right after; Unix with the port.
- `architecture.md` (module statuses) and `docs/far-parity.md` (stage column, deletion keys) follow this
  order.
- Until 2b is resumed, new UI is only what a base step needs to be usable (deletion's dialogs, the progress).
- `docs/tech-debt.md` items carry the step that takes them (3a-3e or 2b).
- 3a changes no behaviour; it touches `fs`, `ui` and `app` widely, so the tests stay as they are and the
  real program is driven once (e2e) before its commit.
