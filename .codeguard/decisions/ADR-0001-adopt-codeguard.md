# ADR-0001: Adopt codeguard (tier mvp)

- Date: 2026-11-06
- Status: accepted

## Context
The project is written mostly by AI agents (Claude Code, Codex); the owner does not plan to read the code.
Quality therefore has to be enforced by tools, not by human review. The project is new (empty crate), Windows only,
Rust, with a data-loss risk in file operations.

## Decision
Install codeguard 0.2.0 at tier `mvp`:
- Gates (block): rustfmt, clippy with `-D warnings` plus `unwrap_used`, `dbg_macro`, `undocumented_unsafe_blocks`,
  `cargo test`, protected tool configs, no test weakening, no lint suppressions, no hook bypass, `unsafe` only in
  `src/platform`, no lint levels in Cargo.toml.
- Sensors (warn): cognitive complexity 15 / function length 80 (clippy), file size 600 lines, `fs`/`terminal` must
  not depend on `ui`/`app`.
- Triggers: post-edit `format`; agent stop `quick` (format, lint); git pre-commit `standard` (+ complexity, tests).
- Roles in the workflow: cg-reviewer for standard tasks; specifier and hardener for critical tasks (file operations).
- No legacy strategy needed (new project, no baselines).

## Consequences
- Every commit compiles cleanly, is formatted and passes tests; agents cannot silence clippy.
- Editing protected files and the first commit of the setup need a human `CODEGUARD_ALLOW=protected`.
- Deferred (see brief.md): secret scanning when a remote appears; complexity and boundaries to block at production.
