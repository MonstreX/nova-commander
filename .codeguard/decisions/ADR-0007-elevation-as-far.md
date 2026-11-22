# ADR-0007: Administrator rights on demand, as FAR does (elevated helper process)

- Date: 2026-11-07
- Status: accepted (owner, 2026-11-08: a convenience worth having; when, by ADR-0012)
- Extends: ADR-0004 (file operations, critical area), ADR-0006 (`--run` mode as a precedent for a second mode
  of the same executable)

## Context
The program runs with the user's normal rights. Files and folders owned by Administrators or SYSTEM (anything
created by an elevated installer or tool, `C:\Program Files`, `E:\AI.Tools` on the owner's machine) grant users
`RX,W` only, without DELETE. Moving, renaming or deleting them fails with `os error 5` (access denied); `move` in
cmd fails the same way, Explorer asks for administrator permission. Creating symbolic links without Developer
Mode fails with `ERROR_PRIVILEGE_NOT_HELD` (1314), which FAR handles the same way.

FAR (`far/elevation.cpp`): when an operation fails with access denied (or a missing privilege), it asks

```
╔═══════════════ Access denied ═══════════════╗
║ You need to provide administrator permission ║
║ to move this object:                         ║
║ E:\AI.Tools\README.md                        ║
║                                              ║
║ [x] Do this for all current objects          ║
║ [ ] Do not ask again in the current session  ║
╟──────────────────────────────────────────────╢
║              { Ok }  [ Skip ]                ║
╚══════════════════════════════════════════════╝
```

(the Ok button carries the UAC shield). On Ok it starts a copy of itself with `ShellExecuteEx` verb `runas`
(`far.exe /service:elevation <pipe> <pid>`; Windows shows the UAC prompt), talks to it over a named pipe and has
it carry out the single failed call (create/remove directory, delete, copy, move, set attributes, hard/symbolic
link, recycle, owner, security, ...). The helper stays for the session, so the next object needs no new prompt.
Settings (Options > System settings): request administrator rights for modification (on) / for reading (off) /
use additional privileges (off).

## Decision

### 1. When
- Only after an operation failed with access denied (`ERROR_ACCESS_DENIED`) or a missing privilege
  (`ERROR_PRIVILEGE_NOT_HELD`), and the program itself is not elevated. Nothing is elevated in advance.
- The same operations as FAR, no more and no fewer: every call FAR can pass to its helper
  (`far/elevation.cpp`, `ELEVATION_COMMAND`, plus `FindFirstFile` in `platform.fs.cpp`). Each comes with the
  feature that makes the call here; features that exist now get it in the first step.

| FAR's call (question "to ... this object") | Request kind | FAR features that make it | Here |
|---|---|---|---|
| `MOVEFILE` (move) | modify | F6 move / rename, Shift+F6 | first step |
| `DELETEFILE`, `REMOVEDIRECTORY` (delete) | modify | F8 / Shift+Del / Alt+Del, a move across drives, wipe | first step |
| `COPYFILE` (copy) | modify | F5 copy, a move across drives | first step |
| `CREATEDIRECTORY` (create) | modify | F7, copy / move of folders, folder links (template) | first step |
| `CREATEFILE` (open) | modify or read by the access asked | editor save and open, viewer, quick view, copy of a single file, find in files | first step for writing (editor save); reading with the "for reading" setting |
| `REPLACEFILE` (replace) | modify | editor save with a backup | with the editor's backup option |
| `FindFirstFile` (list) | read | panel listing (Enter, `cd`, Ctrl+R, drive menu), copy / delete of trees, find file, folder size | with the "for reading" setting |
| `GETFILEATTRIBUTES` (get attributes) | read | everything that asks whether an object exists or is a folder | with the "for reading" setting |
| `SETFILEATTRIBUTES` (set attributes) | modify | Ctrl+A, copy (attributes of the copy), delete of read-only files | first step (copy, delete); Ctrl+A when it comes |
| `CREATEHARDLINK`, `CREATESYMBOLICLINK` (create link) | modify | Alt+F6, F7 with a link type, copy of links | first step for copy of links; Alt+F6 / F7 when they come |
| `MOVETORECYCLEBIN` (recycle) | modify | F8 / Del to the recycle bin | first step (the recycle bin is 3b) |
| `SETOWNER`, `GETFILESECURITY`, `SETFILESECURITY`, `RESETFILESECURITY` (process) | modify / read | Ctrl+A owner, copy with "Copy access rights" | with Ctrl+A and the copy dialog's access rights |
| `SETENCRYPTION` (encrypt / decrypt) | modify | Ctrl+A "Encrypted" | with Ctrl+A |
| `GETDISKFREESPACE` | read | free space in the panel title, Info panel, disk menu | with the "for reading" setting |
| `DETACHVIRTUALDISK` | modify | disk menu: Shift+Del on a virtual disk | with the disk menu's virtual disks |
| `SETVOLUMELABEL` | modify | Info panel / disk menu: volume label | when the label can be changed |

- What FAR never elevates stays so here: command lines and programs started from the panels (they run with the
  user's rights), settings (FAR keeps them in the profile; the portable settings file next to the executable
  reports a failed save, as now).
- "For modification" is on by default, "for reading" off (FAR's defaults): with reading off, a denied listing or
  open shows the error as now. Reading through the helper passes the data or a duplicated handle back (FAR
  duplicates the handle, `PROCESS_DUP_HANDLE`).
- Not a matter of rights: a read-only file is a question of the delete and copy flows (FAR's "The following
  file is read only", `MAskDeleteRO`, and the copy dialog's "Ask about read-only files"; deletion 3b, copy
  2b), not part of this ADR.

### 2. The question (FAR's dialog)
- Title "Access denied", "You need to provide administrator permission" / "to <move | delete | copy | create |
  ...> this object:", the object, "[x] Do this for all current objects" (default on: the rest of this operation
  goes through the helper without asking again), "[ ] Do not ask again in the current session", buttons Ok /
  Skip (Skip: that object fails as now and is reported; Esc cancels the operation).
- "Do not ask again" with Ok: later operations use the helper without asking; with Skip: never ask again this
  session.

### 3. The helper
- The same executable in a third mode, `nova-commander --elevated <pipe> <pid>` (like `--run`, no UI, no
  console needed), started with `ShellExecuteExW` verb `runas`. If the user refuses UAC, the object fails with
  the original error and the question is not asked again for this operation.
- One helper per session, started on first approval, ending when the program ends or the pipe closes.
- It executes only the fixed set of primitive requests of the table in section 1, on absolute paths, each
  answered with success (and data or a duplicated handle for reads) or the OS error code; a request joins the
  set with the feature that needs it. It loads no settings and no plugins, starts no programs, has no shell.
- The safety rules stay where they are (the engines in `jobs`, unprivileged side): no overwrite, no copy into
  itself, a failed copy removes only its own partial target. The helper is a different way to make the same primitive
  call, not a second implementation of the operation.

### 4. Security (critical area)
- The pipe has a random name (`\\.\pipe\nova-commander-elevation-<pid>-<random>`), one instance, a DACL that
  allows only Administrators (FAR's `create_named_pipe`); the program accepts only a client whose process id is
  the helper it started (`GetNamedPipeClientProcessId`), the helper talks only to a server whose process id is
  the one on its command line (`GetNamedPipeServerProcessId`), as FAR does.
- Requests are length-prefixed, size-limited, versioned; anything malformed ends the helper.
- OS calls (ShellExecuteExW, pipes, process ids, DeviceIoControl for junctions) live in `src/platform`
  (an `elevation` subsystem of the portable interface, ADR-0012; `src/platform/windows/elevation.rs`, a stub
  on other OSes); the request format and the helper's dispatcher are plain Rust in the local provider
  (`src/vfs/local/elevated.rs`: the helper makes the local provider's primitive calls), unit-tested without
  elevation: tests start the helper unelevated against a test pipe.

### 5. Settings
Options > System settings (with the FAR-style dialogs, 2b): "Request administrator rights: [x] for modification
[ ] for reading [ ] use additional privileges". Until then: modification on, the others off (FAR's defaults).

## Alternatives considered
- Run the whole program as administrator: the owner can do it now; loses drag and drop from Explorer and mapped
  network drives of the normal session, and every command line would run elevated. Not FAR.
- Windows' own elevated copy engine (`IFileOperation` through the COM elevation moniker, what Explorer uses):
  far less code, but its own dialogs and progress windows over the console, no control of conflicts and the
  program's safety rules. Not FAR.
- "Take ownership" (`takeown` / `icacls`) offered on access denied: changes permissions for good, which the user
  did not ask for. Not taken.

## Consequences
- New mode `--elevated` in `main.rs`; `src/platform/windows/elevation.rs`; `src/vfs/local/elevated.rs`; the
  engines in `jobs` (delete, copy and move) ask the question through their task and retry through the helper.
- windows-sys gains features for pipes and process functions (`Win32_System_Pipes`, `Win32_System_Threading`,
  `Win32_System_IO`); no new crate.
- `architecture.md`: the module rows (`vfs`, `platform`, `main`), a critical area for the helper and the pipe.
- Manual check: UAC cannot be automated; tests cover everything up to `ShellExecuteExW`.
- Order (ADR-0012): the helper and its question are made in 3d and offered at once by copy, move and delete
  ("Access denied"); deletion (3b step 3) and copy / move (3b step 4) leave the place for it in their error
  question. The first step covers the "first step" rows of the table (move, delete, recycle, copy, create
  folder, editor save, attributes and links set by copy and delete), the owner's case first; the rest comes
  with its features.
