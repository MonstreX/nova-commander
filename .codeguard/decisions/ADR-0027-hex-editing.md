# ADR-0027: hex editing in the editor and the viewer

- Date: 2026-11-20
- Status: accepted (roadmap step S2d.8; ADR-0015's "binary (hex) editing comes later"; owner 2026-11-18:
  "as Volkov Commander's", 2026-11-20: in both, the keys below)

## Context
FAR's editor and viewer edit no bytes: the viewer's hex mode (F4) only shows them. The owner wants
binary editing as Volkov Commander has it. VC 4.99's working code has none either: its viewer's hex mode is
read-only and its own editor, which has a hex mode, is switched off in its sources (`VCEDIT.INC` under a
`COMMENT` block, read in the machine translation `github.com/evoleinik/vc-linux`). That disabled editor is
the design taken: F4 switches text / hex; a line is an 8-digit offset, 16 bytes with a gap after every 4,
then the 16 characters (a zero byte as `.`); Tab moves between the hex and the text columns; typing only
overwrites - a hex digit sets the byte's high half, then its low half, Right moves by half a byte first;
Del does nothing; F2 saves, Shift+F2 saves as, Shift+F10 saves and quits, Esc / F10 ask about the changes.

## Decision
- **The editor**: Alt+F4 switches the document between its text and hex modes (F4 stays FAR's: it closes
  the editor). In hex mode the screen shows VC's lines over the same piece table (ADR-0015): only the rows
  shown are read, so a file of any size opens and goes to its end at once; the cursor stands on a byte, in
  the hex column on one of its halves or in the text column (Tab); a hex digit or a character typed
  overwrites the byte (no insertion, Del and BS move only) as one of the editor's steps, so undo, the
  changed mark, saving (F2, Shift+F2, Shift+F10) and the question on Esc / F10 are the editor's own. Past
  the last byte nothing is typed. Alt+F4 again shows the text at the same byte.
- **The viewer**: in its hex mode (F4) F2 turns byte editing on - the same cursor and typing over the
  viewer's file - and F2 again writes the changed bytes into the file in place (the file keeps its size,
  so only the changed bytes are written); Esc with changes asks to save, discard or go on. The key bar's F2
  says "Edit", then "Save".
- The bytes changed and not saved are shown in the colours of the block.

## Addendum 2026-11-21: Ctrl+F4, not Alt+F4
Owner: Alt+F4 closes the whole console window (Windows Terminal's `closeWindow`) and the program with
it, so the editor's hex mode is switched by Ctrl+F4 (free in FAR's editor and in Windows Terminal's
defaults); the key bar shows "Hex" / "Text" on Ctrl's row. Everything else above stays.

## Consequences
- The editor's view gets a hex layout beside its text one; the steps get an overwrite of a byte.
- The viewer gets a small overlay of changed bytes over the file it reads (it never holds the file).
