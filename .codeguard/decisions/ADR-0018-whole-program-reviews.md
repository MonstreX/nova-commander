# ADR-0018: Whole-program reviews by area at milestones

- Date: 2026-11-12
- Status: accepted (owner, 2026-11-12: by a subagent, not in the cloud)
- Details: ADR-0001 (codeguard: each step's review by `cg-reviewer`), ADR-0016 (stage 5), ADR-0017 (stage 7)

## Context
Every step is reviewed on its own diff. What a diff review does not see: the same problem solved two ways in
two modules, assumptions that held when written and no longer do, gaps where subsystems meet (plugins, `vfs`,
background tasks), modules grown too far, code that has drifted from its ADR. A reviewer with fresh context and
the whole program in view finds those better than the one who wrote the steps.

## Decision
1. **When**: after stage 5 (plugins whole, before stage 6), before stage 7 (Linux), and once the first working
   version is ready.
2. **Who**: a subagent of the session on the session's own model, read only. Not a cloud review (owner). Not
   Fable 5.2 (owner, 2026-11-13: it uses up the limits so fast that nothing else would get done).
3. **By area, one pass each**, not "the whole program at once":
   - the plugin boundary and every `unsafe` (`src/platform`, `src/plugin_host/ffi`, the crates);
   - threads and channels: background tasks, plugins' threads, the file reader, the tree workers, waiting work;
   - the safety of file operations: delete, move, rename, permissions, the Recycle Bin, elevation;
   - FAR parity against `docs/far-parity.md`;
   - architecture and debt: layers and dependency directions, module sizes, `docs/tech-debt.md`, drift from
     the ADRs;
   - processes under control (ADR-0019, owner 2026-11-13): every long process against
     `docs/long-processes.md` - its window, its Esc, how a stop ends, calls that may hang - and the ones the
     inventory missed.
4. **Output**: findings with `file:line` and a failure scenario, sorted by weight. The owner decides on each;
   fixes go the usual way (a step with its gate, `cg-reviewer`, a commit), never straight from the review.

## Consequences
- Each review is a planned step of its own between stages; its findings go into `docs/tech-debt.md` or into the
  next stage's steps.
- Reviews complement the tests and end-to-end checks, they do not replace them.
