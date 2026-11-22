# ADR-0006: Own command shell (FAR model) and a guest shell on Alt+Shift+T

- Date: 2026-11-07
- Status: accepted (owner, 2026-11-07)
- Replaces: ADR-0004 §4 (terminal modes and shell integration) where they differ

## Context
ADR-0004 §4 runs every command line in one persistent shell (cmd or PowerShell) inside a ConPTY. The shell is a
separate process; its folder, variables and history are visible only to it, and the only way to influence it is to
type into it. Stage 2a/3 showed what that costs:
- keeping the shell in the panel's folder means typing `cd` lines (visible, in its history; now hidden by
  `Session::view` and a PowerShell history filter, but cmd's history still gets them);
- whether a command finished, whether the shell waits for input and what its folder is are guessed from the
  prompt text and quiet periods (`dir_sync.rs`, `at_prompt`, `RETURN_AFTER_QUIET`);
- the shell sits in the folder it was last sent to and locks it: moving or deleting that folder from the panels
  fails with "access denied" (os error 5);
- sending a command before the shell answered the previous one needs special cases.

FAR has no persistent shell: it runs each command as a new process in the panel's folder and handles `cd`, `set`
and `chcp` itself. The owner chose that model for the command line, extended, plus the possibility to switch to a
real shell when wanted.

A prototype (2026-11-07, scratchpad `ptyproto`, portable-pty 0.9 + vt100 0.18) showed:
- several processes can be started one after another in one ConPTY (`SlavePty::spawn_command` again); the screen
  is continuous, each process gets its own working directory, the exit code is known (`Child::try_wait`), and
  interactive input works (`set /p`);
- a process started that way inherits "ignore Ctrl+C"; a runner that calls `SetConsoleCtrlHandler(NULL, FALSE)`
  before starting the command makes Ctrl+C (byte 0x03 written to the ConPTY) stop it (`ping -t` ended with
  `STATUS_CONTROL_C_EXIT`), and the next command runs normally;
- after `^C` the cursor is not at the start of a line: the runner must start its prompt line on a new line then.

## Decision

### 1. Own command shell (default; the command line under the panels)
- One ConPTY lives as long as the program: FAR's "user screen". Ctrl+O and Esc on an empty command line show it
  with our command line under it, as FAR does; the panels hide it as now.
- Each command line runs as a new process in that ConPTY: this program's own executable in runner mode
  (`nova-commander --run ...`), started in the active panel's folder with the program's environment. The
  runner re-enables Ctrl+C, prints the prompt line (`C:\dir>command`, on a new line if the cursor is not at the
  start of one), runs the line and exits with its exit code. Lines the runner does not handle itself go to
  `cmd /c` (cmd syntax: `%VAR%`, `&&`, `|`, `>`, `.bat`).
- The program handles itself, without starting anything (as FAR): `cd` / `chdir` / a drive letter (the panel
  changes, as now), `set NAME=value` / `set NAME=` (the program's environment, inherited by later commands).
  As FAR, it shows such a line on the user screen (a runner that only prints the prompt line). `cls` runs in
  cmd, which clears the user screen: the program cannot clear the pseudo console's buffer from outside
  (implementation note, 2026-11-07). Others may follow (`chcp`, `pushd`/`popd`).
- A command has finished when its process exited; while it runs, keys go to the ConPTY (as `CommandOutput` now)
  and Ctrl+C stops it.
- Shift+Enter keeps running the command in a separate console window (ADR-0004 §4).
- Text selection and copying in the user screen (keyboard and mouse) belong to our terminal view, not to any
  shell; they come with the mouse work.

### 2. Guest shell (Alt+Shift+T)
- Alt+Shift+T shows a real shell chosen in the settings (cmd, Windows PowerShell, PowerShell 7, WSL), started on
  first use and kept running until the program quits, in its own ConPTY and screen. Every key goes to it except
  Alt+Shift+T, and Esc at its prompt with nothing typed (Volkov Commander's Esc), which return to the panels.
- On entry, when the shell waits at its prompt in another folder, the program moves it to the active panel's
  folder with a hidden `cd` (the existing `dir_sync.rs` / `Session::view`, the PowerShell history filter). A busy
  shell is not touched.
- On leaving, a `cd` done in the guest moves the active panel (as Ctrl+O does now).
- The guest shell keeps its own state (variables, functions, history). It may lock the folder it is in, as any
  program does; whether to park it in a neutral folder on leaving is decided when it is built.
- Alt+Shift+T is used instead of Ctrl+Shift+T, which Windows Terminal takes for a new tab. Quick search with
  Alt+Shift+T is lost; Alt+T finds the same names (the search ignores case).

### 3. Linux-style commands (later, own ADR)
`ls`, `cat`, `cp`, `mv`, `rm`, `touch`, `head`, `tail`, `wc`, `du`, `pwd`, ... in the runner, from uutils/coreutils
(MIT) or written here, and made available to `cmd /c` and the guest shell through a folder put on the children's
`PATH`. Windows names (`find`, `sort`, `more`, `mkdir`, `dir`, `echo`, `type`) are never shadowed. The new
dependency needs its own ADR.

### 4. What changes in ADR-0004 §4
- "Ctrl+O: full-screen terminal" becomes Alt+Shift+T (guest shell); Ctrl+O shows the user screen (FAR).
- Shell integration (OSC 9;9 / OSC 133) is needed only for the guest shell, as an improvement over reading its
  prompt; the command line no longer depends on it.
- The `cd` prefix and prompt detection for the command line are removed.

## Consequences
- The command line always runs in the panel's folder, nothing is typed behind the user's back, the end of a
  command and its exit code are exact, and no shell holds the panels' folders.
- `src/terminal`: a user-screen ConPTY that spawns runners; `--run` mode in `main.rs` with the runner in
  `src/terminal/runner.rs` (no UI). `Session` with `dir_sync.rs` / `view.rs` stays for the guest shell only.
  `app/shell_link.rs` is rewritten around the two screens. `architecture.md` is updated with the change.
- Lost compared with the persistent shell: state set by a command line other than `cd` / `set` (PowerShell
  variables, doskey macros, `call` of a script that changes the environment). The guest shell covers that need.
  Command lines use cmd syntax; a setting to run them through PowerShell may come later (slower start).
- Order: this work comes before stage 3d. Steps: (1) own shell, (2) guest shell, (3) Linux-style commands.
