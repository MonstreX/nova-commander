# ADR-0002: Rust with ratatui/crossterm, portable-pty and a ready-made VT emulator

- Date: 2026-11-06
- Status: accepted

## Context
nova-commander is a Windows-only console file manager (Norton Commander / FAR style) with an embedded
terminal below the panels whose shell can be switched (cmd, PowerShell, pwsh, WSL). The embedded terminal needs
ConPTY plus a full VT emulator - the hardest part of the product. Code is written by agents and not read by the owner.
Alternatives considered:
- C++20 (FTXUI or own Win32 console renderer, libvterm, ConPTY): maximum control, the FAR approach, but the most
  manual integration work and memory-safety/UB risk that only tests and sanitizers catch; heavier quality gates.
- C# / .NET 10 (Terminal.Gui, ConPTY via P/Invoke): fast development, but no mature VT emulator library -
  it would have to be written; data races are not caught by the compiler.
- TypeScript/Node (blessed, node-pty, @xterm/headless): best VT emulator, but a runtime dependency and abandoned TUI libraries.

## Decision
Rust (edition 2024, MSVC toolchain), with:
- `ratatui` + `crossterm` for rendering and input;
- `portable-pty` for ConPTY;
- `alacritty_terminal` or `vt100` (+ `tui-term` widget) for VT emulation - chosen in a follow-up ADR with a spike;
- `windows` crate for Win32 calls crossterm does not cover (e.g. modifier-only key events), confined to `src/platform`.

## Consequences
- The compiler and clippy act as the main reviewer: memory errors and data races do not compile.
- Single small executable, no runtime.
- Some key combinations may need raw Win32 console input instead of crossterm; the host console (Windows Terminal)
  intercepts some shortcuts regardless of language.
- Slower first builds than C#.
