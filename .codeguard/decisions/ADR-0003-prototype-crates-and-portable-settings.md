# ADR-0003: Prototype crates, module split and portable settings

- Date: 2026-11-06
- Status: accepted (crates, `editor` module and `ui -> app` direction approved by the owner in chat;
  portable JSON settings requested by the owner)

## Context
The first goal is a fast look & feel prototype: panels, pull-down menu, navigation, viewer, simple editor,
embedded terminal and command-line/panel interaction. The owner added a requirement: the program must run
without installation (copy the folder), with all settings in a subfolder next to the executable, in JSON,
like VS Code.

## Decision
- Crates: `ratatui` 0.30 (its bundled `crossterm` is used through `ratatui::crossterm` to avoid version
  mismatches), `portable-pty` 0.9 (ConPTY), `vt100` 0.18 (VT emulation - simpler than `alacritty_terminal` for
  the prototype; revisit when full-screen programs inside the terminal matter), `serde` + `serde_json`
  (settings), `tempfile` (tests only).
- New module `src/editor`: documents and the edit buffer, without UI.
- New module `src/settings`: `settings\settings.json` next to the executable, flat dotted keys
  (`"terminal.shell"`), missing keys fall back to defaults, saved atomically on exit; a file that fails to parse
  is reported and never overwritten.
- Direction `ui -> app`: `ui` reads the application state and writes only view metrics; `app` never depends on `ui`.
- Command line model "A" (FAR style): the program owns the command line. When the shell shows its default
  prompt (`C:\...>`, `PS ...>`, `...$`/`#`), every command is sent prefixed with a change to the active panel's
  directory (`cd /d "<dir>" && <command>` and equivalents), so it can never run elsewhere after `pushd`, scripts
  or a failed `cd`. When anything else is on the shell's cursor line, a program is waiting for input
  (`(Y/N)?`, `set /p`, REPL) and the text is sent as typed. Shift+Enter always sends as typed (for custom
  prompts). `cd <dir>` and `D:` typed in the command line move the panel instead of the shell (except in WSL,
  where every command still runs in the panel's directory).
- Text files keep the terminator of every line, so saving never rewrites untouched lines.

## Consequences
- The `cd` prefix is visible in the terminal output for every command (safety over looks).
- Known prototype limitations: blocking copy/move (large files freeze the UI), dates shown in UTC (no time zone
  crate yet), no pass-through mode for full-screen programs; settings.json must be plain JSON (a file with
  comments is reported as broken and left untouched, defaults are used), unknown keys are dropped when the
  program saves the file; a target created by another process between the check and the copy/rename can still
  be replaced, and saving does not keep file attributes/ACL/hard links/alternate streams (both need Win32 calls).
- `architecture.md` module table updated; the forbid rule `core-no-ui` covers `fs` and `terminal` only - extending
  it to `editor`, `settings` and `app -> ui` needs a human edit of the protected config.
