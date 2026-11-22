# ADR-0005: `windows-sys`, `src/platform` from stage 2a, `ui -> platform`

- Date: 2026-11-07
- Status: accepted (approved by the owner in chat on 2026-11-07; raised by the independent review of stage 2a
  step 1)

## Context
Stage 2a shows dates and times as FAR does: in local time and in the user's regional format, with FAR's
attribute letters and the `%PATHEXT%`-based `<exec>` highlighting group. The prototype showed UTC because it had
no time zone data. Local time, regional formats and file attributes are OS-specific; ADR-0004 §1 puts such code
in `src/platform`, planned for stage 3. ADR-0002 approved the `windows` crate for Win32 calls.

## Decision
- Create `src/platform` now (stage 2a) with what the panels need: local time of a time stamp
  (`SystemTimeToTzSpecificLocalTime`), the user's date order and date/time/number separators (`GetLocaleInfoEx`),
  file attributes and their FAR letters, and the executable extensions (`PATHEXT`). Stage 3 completes the module
  (drives, associations, shell discovery, process flags, dynamic libraries, reparse points).
- Amendment (2026-11-07, owner: Detailed mode "as in FAR"): directory allocation sizes for FAR's "Allocated"
  column are read in 2a too, in one pass per directory (`GetFileInformationByHandleEx`).
- Use `windows-sys` instead of `windows`: the same Microsoft bindings without the COM/WinRT wrappers, smaller
  and faster to build; the raw `unsafe` calls stay inside `platform` behind safe functions either way. Declared as
  a `[target.'cfg(windows)'.dependencies]` entry; `platform/windows.rs` is behind `#[cfg(windows)]`.
- Allow `ui -> platform` (read-only helpers: local time, regional formats, attribute letters). `ui` formats what it
  shows; converting a time stamp to local time is OS-specific and belongs to `platform`.

## Consequences
- Dates, the clock and sizes follow Windows' regional settings, as in FAR.
- `architecture.md` lists `platform` as existing and `ui -> platform` as an allowed direction.
- On another OS the build fails until `platform` gets an implementation for it - intended (ADR-0004 §1).
