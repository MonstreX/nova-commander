# ADR-0017: Stage 7 - the Linux port

- Date: 2026-11-12
- Status: accepted as a planned stage (owner, 2026-11-12); its open questions are decided when it starts
- Details: ADR-0004 §1 (portability), ADR-0005 (`platform` early), ADR-0012 (the portable `platform` interface,
  the Linux stubs, the Linux check in the gate), ADR-0007 (elevation), ADR-0008 (theme paths), ADR-0009 (the
  copy engine's Linux path), ADR-0011 (attributes: far2l's dialog on Linux), ADR-0016 (plugins); the `[port]`
  items of `docs/tech-debt.md`

## Context
Windows is the only implemented platform. Since stage 3a every OS-specific call lives in `src/platform` behind
an OS-neutral interface, the other systems have stubs, and the Linux build is checked after every step (by
hand: the check is not among the gate's pipelines, `docs/tech-debt.md`; the plugins and their SDK included);
code outside `platform` is the same on every system. In November 2026 that is about 9 400 lines of Windows code
in `platform` against about 91 000 lines that do not depend on the system. The owner asked how long a Linux
version would take once the Windows version works, and to put it in the plan.

## Decision
1. **Linux is stage 7, after a working Windows version** (after stages 6 and 2d), on the main branch: a second
   implementation of `platform` (`src/platform/linux`, replacing the stubs where Linux is meant), not a branch
   of its own, so it never falls behind the Windows work. macOS is not part of it.
2. Until then new OS-specific code keeps going into `platform` (a plugin's: the SDK's `system` / `sevenzip`)
   behind OS-neutral interfaces, and the Linux build is checked with every step (`cargo check --workspace
   --target x86_64-unknown-linux-gnu --all-targets`): the port stays a matter of writing implementations, not
   of moving code.
3. **Steps** (each a commit with a green gate and end-to-end checks in WSL Ubuntu and a real terminal; the
   estimate is 8-12 steps of the present size, to be made exact when the stage is planned):
   - 7a: files and folders - reading, metadata, permissions and owners (Ctrl+A as far2l's dialog, ADR-0011),
     links, watching folders (inotify), the locale and UTF-8 names; the program starts and shows panels;
   - 7b: the keyboard and the mouse in Linux terminals - crossterm's Unix input, the kitty keyboard protocol
     where the terminal has it, what is done with FAR's keys a terminal cannot send (open question 1);
   - 7c: the embedded terminal and processes - a pseudo terminal (`openpty`), the user's shell, the user
     screen, Ctrl+O, running programs;
   - 7d: copy, move and delete - ADR-0009's Linux path (`FICLONE`, `copy_file_range`), owners and extended
     attributes kept, the freedesktop trash;
   - 7e: the drive menu as mount points (`/proc/self/mountinfo`), removable media and their arrival (udev /
     UDisks2), free space, unmounting and ejecting;
   - 7f: the clipboard (Wayland, X11, OSC 52 in a terminal over SSH), file types and associations (shared MIME
     info, `.desktop` files, `xdg-open`);
   - 7g: elevation (ADR-0007 adapted: `sudo` or polkit, open question 3);
   - 7h: plugins as `.so` (`dlopen`; `xtask package` for Linux), the settings, themes and logs paths (open
     question 4);
   - 7i: packaging and the end-to-end suite on Linux.

## Open questions (decided at the stage's start, propose -> confirm)
1. Which terminals are supported fully (kitty, foot, WezTerm, Ghostty with the kitty keyboard protocol;
   gnome-terminal, Konsole, xterm, tmux without it), and what replaces the keys a terminal cannot tell (left and
   right Ctrl apart, some Ctrl+Shift combinations, key releases).
2. The look of the drive menu on Linux (mount points only, or also bookmarks of folders, as far2l does).
3. Elevation through `sudo` in the embedded terminal or a polkit helper.
4. Settings beside the program (portable, as on Windows) or in `~/.config/nova-commander` (ADR-0008 names
   the latter for themes).
5. The package: a folder (tarball), AppImage, `.deb` / `.rpm`.

## Consequences
- No work on Linux before the stage beyond keeping the gate's Linux check green and `[port]` notes in
  `docs/tech-debt.md` when Windows-only code is added.
- The keyboard (open question 1) is the main risk of the stage and the least certain part of its estimate.

## Addendum 2026-11-22 (review R7, E1 and E11): what the port changes outside `platform`
The review before stage 7 (`docs/reviews/2026-11-21-before-stage-7.md`) measured the stage again.
- **The figures**: about 10 300 lines in `platform/windows` (and 113 in the SDK's Windows files) against
  about 257 000 other lines of the workspace, tests included (the 9 400 / 91 000 above counted the program
  without its tests on 2026-11-12). The `[port]` items of `docs/tech-debt.md` are tagged `[S7]` (28 of them).
- **Decision 2 no longer holds whole**: the port is not only implementations. Code outside `platform`
  that the port changes, each an `[S7]` item of `docs/tech-debt.md`:
  - the shells and their dialects: `platform::process::ShellKind` names Windows' (`cmd`, `powershell`,
    `pwsh`, `wsl`, `gitbash`, their `*.exe`), and `terminal::shell` speaks to each by its kind (its prompt,
    its `cd`); a POSIX shell (`$SHELL`, bash, zsh, fish) is a new kind and a new dialect there;
  - code pages are Windows' numbers everywhere (`vfs::text`, the view's `Encoding::Page`, Find's
    `CodePage`, the settings' `codePage` keys, F8's cycle ANSI / OEM): the port maps them to its tables
    (ADR-0028) and decides what ANSI and OEM are on a UTF-8 system;
  - the stubs' `local_time` is `None`, so every date in the panels is blank off Windows until the port's
    locale (`platform/other/text.rs`);
  - a typed `/tmp` on a provider's panel stays inside the provider (`vfs::path`: a `/` path is a
    provider's, only `C:` and `\\server` are the OS's); on Linux an absolute path is the disk's;
  - the line break was Windows' by default outside `platform`: done (R7 E10, `platform::LINE_BREAK`);
  - the plugins: `proclist` and `network` are Windows only as crates, `emenu` builds everywhere with its
    menu left out.
- **The estimate**: about 15-20 steps, not 8-12 (some 10-13k new lines, about 1.5k changed outside
  `platform`). First the gate's Linux pipelines (R7 E2: clippy and tests in WSL; the gate's config is
  protected, so the owner's), an ADR on the Unix crates (`libc` / `nix`, the clipboard, D-Bus, code-page
  tables) and the triage of the tests that assume Windows; then 7a-7i above, the process and terminal
  part with a POSIX shell about half outside `platform`.
