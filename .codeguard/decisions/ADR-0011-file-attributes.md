# ADR-0011: File attributes, times, owner and permissions (Ctrl+A)

- Date: 2026-11-08
- Status: accepted (owner, 2026-11-08: the scope, then this text as it is); steps
  re-ordered by ADR-0012; steps 1 and 2 made one (owner, 2026-11-08)
- Extends: ADR-0004 (`platform`, `jobs`), ADR-0009 (the transfer engine's questions), ADR-0010 (tasks)
- Related: ADR-0007 (elevation, proposed): changing an owner usually needs it

## Context
FAR's Ctrl+A (Files > File attributes, also F4 in the drive menu for a drive's root) opens "Attributes"
(`far/setattr.cpp`): what a file is and links to, its attributes, its four times and its owner, for one item
or for the selection, optionally for everything inside the selected folders. The program runs on Windows now
and is designed to run on Linux and macOS later (ADR-0004 §1), where the same key shows far2l's dialog:
permission bits, owner and group, times.

What FAR's dialog has (DlgW 76; texts from `farlang.templ.m4`):
- Header: "Change file attributes for" and the name (or "selected objects"); for a link its kind
  (Symlink / Junction / Volume mount point / AppExecLink / unknown reparse point) and target in an edit line;
  "hard links" with a count and a list of the names; a device's name.
- Attributes, check boxes that are three-state for a selection or folders (unchanged / on / off):
  Read only, Archive, Hidden, System, Compressed, Encrypted, Not indexed, Sparse, Temporary, Offline;
  "&More" opens the rest: Reparse point (only to remove), Virtual, Integrity stream, No scrub data, Pinned,
  Unpinned, Recall on open, Recall on data access, Strictly sequential, Device.
- Date/Time with a title in the user's order (`DD.MM.YYYY`, `hh:mm:ss`), a UTC box, four lines:
  write, creation, access, change; buttons Original / Current / Blank (blank = leave unchanged).
- "Owner:" edit line (`DOMAIN\name`).
- "Process subfolders" (for folders), then `{ Set } [ System properties ] [ Cancel ]`.
- Applying: owner, compression, encryption, sparse, the other attributes, then the times (`ESetFile*`); each
  failure asks Retry / Skip / Skip all / Cancel; "Setting file attributes for" shows progress over a tree.
- "System properties" opens Windows' own properties dialog (with its Security tab: the ACL).

## Decision
1. **One model in `platform`, per OS.** `FileProperties` read for a path: attributes (Windows bits), the four
   times (`write`, `creation`, `access`, `change`; any may be missing), owner (and group on Unix) as names,
   permission bits on Unix, link kind and target, hard link count. Setters each do one thing and return
   `io::Result`: attributes, compression, encryption, sparse, times, owner, mode. Windows now; the Unix
   implementation (`chmod`, `chown`, `utimensat`, birth time read-only) comes with the port and is described
   here so the model already fits it.
2. **FAR's dialog on the form engine (ADR-0009).** Same items, places, hotkeys and three-state boxes as
   `setattr.cpp`, "More" as FAR's second dialog, dates in the user's format, UTC, Original / Current /
   Blank. On Unix the attribute block is far2l's: `rwx` for user, group and others, setuid, setgid,
   sticky, owner and group lines; no "More".
3. **Applying runs as a task (ADR-0010)** with FAR's order of changes: progress ("Setting file
   attributes for"), FAR's questions Retry / Skip / Skip all / Cancel, it can go to the background. One item
   without "Process subfolders" is a task too, unlike FAR (owner, 2026-11-08: one way, and the screen never
   waits for a slow disk); it does not wait in the volume queue, it writes no file data. "Process
   subfolders" follows links to folders, as FAR's default `ScanJunction`, each place once (owner,
   2026-11-08).
4. **Permissions beyond the owner are not edited here.** ACLs are Windows' business: "System properties"
   opens the shell's properties dialog (`SHObjectProperties`), as FAR does. An owner the user may not set
   fails with the system's error; elevation is ADR-0007's.
5. **F4 in the drive menu** opens the dialog for the drive's root (FAR's `ShellSetFileAttributes`).

## Steps
Re-ordered by ADR-0012 (base subsystems first); steps 1 and 2 are one step, 3b step 5b (owner, 2026-11-08:
the model alone would have no caller, which the gate refuses as dead code):
1. The model in `platform`, reading and setting on Windows (with the Linux stubs of 3a).
2. The dialog for the current item or the selection, without "Process subfolders"; Ctrl+A,
   Files > File attributes, F4 in the drive menu.
3. Right after: "Process subfolders" as a background task with progress and Retry / Skip / Skip all / Cancel
   (3b step 5c).
4. The Unix model and far2l's dialog, with the port.

## Consequences
- The task list gets a second kind of task; the question dialogs of ADR-0009 are reused.
- `platform` grows by owner and security calls (`GetNamedSecurityInfoW`, `SetNamedSecurityInfoW`,
  `LookupAccountSidW`); no new crates.
- The Detailed view's attribute column and the status line keep reading attributes as now; they move to the
  shared model when it lands.
