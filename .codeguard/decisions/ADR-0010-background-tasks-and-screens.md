# ADR-0010: Background tasks and FAR's Screens list (F12)

- Date: 2026-11-08
- Status: accepted (owner, 2026-11-08)
- Extends: ADR-0009 (the copy and move engine: a worker thread per operation, a handle with progress,
  questions, pause and cancel), ADR-0004 (`jobs`)

## Context
A copy or move now runs on a worker thread (ADR-0009), but the UI still waits for it: the progress dialog is
modal, as in FAR. The owner wants to keep working while an operation runs: put it in the background, find it
again, see that something runs, and hear when it ends. More background work will come (delete, search,
archives, listing slow folders).

What FAR has (sources: `manager.cpp`, `panel.cpp`, `taskbar.cpp`):
- F12 "Screens": a menu of the open windows (panels, viewers and editors put aside with F12 / Ctrl+Tab,
  non-modal dialogs), hotkeys 0-9 and A-Z, `*` for a modified file; Ctrl+Tab / Ctrl+Shift+Tab cycle them.
  These are windows, not processes.
- "Show background screens number": `[2+1]` (viewers + editors (+ dialogs)) in the left panel's top-left
  corner (`COL_PANELSCREENSNUMBER`).
- File operations are modal and cannot be put in the background. The progress goes into the console title
  (`{35%} Copying`) and the Windows taskbar button; when a long operation ends while the console window is
  not active, the taskbar button flashes (`taskbar::flash`).
- Background copying with a queue exists in DOS Navigator and Total Commander ("Background", F2).

## Decision
1. **Tasks in FAR's Screens list.** F12 lists the panels, later the viewers and editors (stage 2d), and the
   tasks. A task line: `Copy  45%  120/340  -> D:\backup`, `Move  waiting for an answer`, `Copy  done`. Enter
   on a running task brings its progress dialog back; on a task that waits, its question; on a finished one,
   its report. Ctrl+Tab cycles as in FAR. Del on a finished task removes it.
2. **To the background.** The progress dialog gets a button row: `{ Background } [ Pause ] [ Cancel ]`
   (owner: an extra button is fine); F2 does Background too, as in Total Commander. Esc keeps FAR's
   "Operation has been interrupted" question. In the background the panels take keys again; the task goes on.
3. **Questions in the background.** A task that needs an answer (exists, read-only, low space, error) waits;
   the status row marks it and Enter on it (F12 or the row) shows the question. Nothing is answered for
   the user.
4. **The status row** (owner): while at least one task exists, the top row of the screen shows the tasks and
   the panels move down one row (the place FAR's "Always show menu bar" uses):
   `Copy 45% [####...]  Move: question!  F12`. With no tasks the row is hidden and the panels take the
   screen again. Amended by the owner (2026-11-08): the row is for tasks in the background only; the
   operation in front (its progress dialog, its question, its next destinations) does not show it, so
   starting an operation does not move the panels. F9 shows the menu bar on that row as now. FAR's `[n]` counter in the panel corner counts
   viewers and editors when they come.
5. **The end of a task.** For a few seconds the status row says how it ended (`Copy done: 340 files, 2
   skipped`). FAR's signals too: the percentage in the console title, the taskbar button's progress
   (Windows Terminal: `OSC 9;4`), and a flash of the taskbar button when the console window is not active.
6. **Finished tasks leave by themselves** (owner): a finished task that needs nothing from the user (done,
   nothing skipped) is removed from the Screens list after a timeout (30 s; a setting). A task with
   failures, or one that was cancelled with something left behind, stays until its report is opened or it is
   removed with Del.
7. **A queue per disk** (owner): a new task whose source or target disk is in use by a running task waits
   in the queue (`Copy  queued`) and starts when that task ends: parallel writes to one disk only slow both
   down. Tasks on other disks run at once. "The same disk" is the volume (`platform`: volume of a path);
   network shares count per share.
8. **Leaving the program** (F10) while tasks run or wait: "N tasks are running: Wait / Cancel all /
   Back". Wait keeps the program open until they end; Cancel all cancels them (partial files removed, as
   ADR-0009 says).
9. **Where.** `jobs` gets a task list (`jobs::tasks`): ids, kinds, state (queued, running, waiting for an
   answer, done), the per-disk queue, the timeout of finished tasks. No UI in it. `app` keeps the Screens
   list, the status row state and which task's dialog is shown; `ui` draws the row, the list and the
   dialogs.

## Consequences
- The modal `App::transfer` becomes a list of tasks; the progress dialog shows one of them.
- Panels reread their folders when a task that wrote into them ends (also in the background).
- Tests: the queue (two tasks on one disk run one after another, tasks on two disks at once), the timeout
  (a clean finished task leaves, one with failures stays), a question asked in the background waits.
- `platform` grows: the volume of a path, the console title, the taskbar flash and progress.
- docs/far-parity.md: F12 Screens with tasks (FAR has windows only), the status row, Background in the
  progress dialog are additions to FAR.
- Steps: (1) task list with the queue, Background / F2, the status row, F12 with tasks, the timeout;
  (2) the end signals (title, taskbar, flash); (3) viewers and editors in Screens when stage 2d comes.

## Addendum 2026-11-21: step 3, the windows (S2d.9a)
FAR's `Manager` (`manager.cpp`) followed: the panels, each viewer and each editor are windows. `App::screen`
stays the window in front (all the viewer's and the editor's code works on it); `app::windows` keeps the
others behind in the order they were last in front, so closing a window brings back the one in front
before it (FAR's `DeleteCommit` activating `m_windows.back()`), and numbers them in the order they were
opened, the order of the Screens list and of Ctrl+Tab (FAR's `GetSortedWindows` by `ID`). The list shows
the panels, the viewers and editors ("View" / "Edit", `*` when modified, FAR's `WindowMenu` line), then
our tasks, searches and commands; its cursor starts on the window in front. Modal windows as FAR's
(`ExecuteModal`, no `EnableSwitch`): a viewer or an editor opened from Find file's results, the user
menu's editor and a plugin's file's editor - told from what holds them, as FAR's `GetCanLoseFocus` - have
no F12 and no Ctrl+Tab and no "Screen" on the key bar. Quitting asks about each window with unsaved
changes from the one in front (FAR's `ExitAll`: the editor's "Save?", ours the viewer's bytes typed) and
stays when one keeps them; the rest close with their places kept. A plugin's file's copy goes once no
window has it open. Gray + / Gray - in the viewer keep its window; F6 between the viewer and the editor
makes a new one, as FAR's.
