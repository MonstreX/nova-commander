# ADR-0019: No process out of control

- Date: 2026-11-13
- Status: accepted (owner, 2026-11-13: the principle, the inventory before the whole-program review, the work as
  a stage of its own after it; plugins in a process of their own only as a note for the future)
- Details: ADR-0009 (transfer: progress, cancel), ADR-0010 (background tasks), ADR-0013 §5 (a folder read on a
  worker), ADR-0014 (tree reading), ADR-0015 (huge files), ADR-0016 (plugins), ADR-0018 (whole-program reviews);
  the inventory: `docs/long-processes.md`

## Context
The owner found TmpPanel's F7 on 18 000 entries holding the program for a minute, and asked that no process
- the program's or a plugin's - be one the user cannot control. In FAR many are not: a huge file read, a network
share that does not answer, a plugin busy in a synchronous call - FAR waits and the user can do nothing. The
owner (2026-11-13): every process can be stopped, ending at what is already done with a notice that the work
is not finished; where that cannot be, the state goes back to what it was before the process, with a notice.
Look at the best practices for it.

## Decision
1. **Every process is seen.** Work that lasts more than about 300 ms shows a window: the share done when the
   whole is known, else a count and the item at hand. The screen never stays held: the work runs off the UI's
   thread, the keys that are not for it wait or work.
2. **Every process stops on Esc**, at any moment, after FAR's "Do you really want to cancel it?" where FAR asks
   (`ConfirmAbort`).
3. **A stop ends one of two ways, always with a notice:**
   - **at what is done** where a part is worth having (a folder read, a search, a copy, a deletion, a file read,
     a plugin's list): "Stopped: 1 240 of 18 000 done";
   - **back to before** where a part would harm (a file saved, settings, a list file written, a plugin's panel
     changed): "Cancelled: everything is as it was".
4. **How - the common practice:**
   - *Cooperative cancellation* (as .NET's `CancellationToken`, Go's `context`): one control in `jobs`
     (cancel, pause, progress) handed to every process, which looks at it at safe points (each item, each block)
     - in place of the flags each job has now.
   - *System calls that hang* (a share that does not answer, `CreateFile` on a dead network path, a read from a
     stuck device): only on a worker thread; on Esc `CancelSynchronousIo` on that thread (`CancelIoEx` for
     overlapped I/O) - Windows breaks synchronous I/O, network I/O too. A call that still does not come back
     within a few seconds: "Not answering: Wait / Abandon"; Abandon lets the thread go (its late result thrown
     away, the state back to before), as a folder read already does (D3). Network calls have time limits.
   - *Back to before*: files written to a temporary file and put in place at once (`ReplaceFileW`,
     `MoveFileEx`; settings do so already, `ops::write_atomic`), the original untouched until the new one is
     whole; state in memory (panels, lists, settings) kept as it was before the process and put back on a stop.
5. **Plugins.** A plugin runs in the program's process: its thread cannot be killed safely. So:
   - the ABI gets two host functions: `progress(done, total, text)` and `cancelled()` - the plugin asks whether
     Esc was pressed (FAR's `CheckForEsc`, without blocking); a call returns "cancelled" with what it did; the
     SDK wraps a plugin's loop so that it looks by itself. `API_VERSION` becomes 2 (`docs/tech-debt.md`, ADR-0016
     §10);
   - the host shows a window for every plugin call past 300 ms (the plugin's progress when it gives one, else
     "working"); Esc asks the plugin to stop; one that has not stopped within a few seconds: "The plugin does not
     answer: Wait / Abandon"; Abandon puts the plugin's panel back as it was before the call, marks the plugin
     busy until its call returns and lets its late answer be.
   - **For the future, not now** (owner): plugins in a process of their own (as editors' extension hosts and
     browsers' tabs), so that one that hangs can be killed and started again; the cost is a protocol between
     processes and much more complexity.
6. **Order** (owner): this ADR and the inventory of every long process (`docs/long-processes.md`: its window, its
   Esc, how a stop ends, the system calls that may hang) before the whole-program review (ADR-0018), which then
   checks the program against it; the work - the common control, breaking hung I/O, the plugins' ABI 2, the
   gaps of the inventory - a stage of its own right after the review. Each gap gets an end-to-end stand that
   makes it hang (the probe plugin asleep, a named pipe that never answers) and checks Esc, the window, the
   notice and the state after it.

## Consequences
- Some processes FAR shows no window for get one (deviations in `docs/reviews/2026-11-12-stage5-far-audit.md`, as D23).
- Every new process is written with the control from the start; a review (ADR-0018, `cg-reviewer`) asks for it.
- Abandoned threads may still hold a handle until the system lets them go; the notice says the disk or the
  plugin does not answer.
