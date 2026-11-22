# ADR-0014: The tree panel reads its disk in the background, by priority, and keeps it as nodes

- Date: 2026-11-11
- Status: accepted (owner, 2026-11-11)
- Details: docs/far-parity.md (2c step 1). Replaces how 2c steps 1a-1b read and keep the tree
- Related: ADR-0013 (`vfs`, listing off the UI thread)

## Context
FAR's tree (`treelist.cpp`) reads the whole disk at once behind a modal "Tree" message that holds the keys, and
keeps it in `tree3.far` / `tree3.cache`, read back as it is until Ctrl+R. Steps 1a-1b did the same. On a disk of
tens of thousands of folders the first reading does not end in a time anyone waits for, so no file is ever
written (FAR saves only a reading that was not stopped), and the next Ctrl+T starts over. The owner: the tree's
logic stays FAR's (a tree of the whole disk, its keys, the cursor after the other panel); its reading may differ
for speed and convenience, without losing anything the tree can do.

## Decision
1. **No modal reading.** Ctrl+T shows the tree at once with what is known; the keys work all along; the tree
   panel says "Reading" with its count while its worker goes on. Closing the tree stops the worker.
2. **Reading by priority.** One worker per open tree lists folders one at a time, the most wanted first:
   the folder under the cursor when its folders are not known yet, then the folders from the root to the other
   panel's folder (their neighbours come with them), then that folder's subtree, then the rest of the disk to
   its end (owner). What is read shows in the tree as it comes.
3. **The disk's file keeps nodes.** One file per volume in `settings\TreeCache` (as step 1b): a line per known
   folder (FAR's `\a\b`), with the folder's last write time when its folders were read. What was read is saved
   as the worker goes and when it stops, so a reading left halfway goes on from there next time (a tree
   closed saves on its own, the screen not held: a big disk's file takes a second or more; the program's
   end waits up to 3 s for those saves). FAR's
   "Minimum number of folders" still keeps a tree that small out of a file.
4. **Checked by the folders' times.** A folder's last write time changes when a folder in it is made, deleted
   or renamed (NTFS; other file systems not checked yet). With a file, the tree shows at once from it and the
   worker checks each read folder's time, listing again only those that changed (a folder gone takes what was
   under it along; a folder renamed in its case takes its new name); with the other panel's folder first
   (owner). A time less than 3 s old is not kept (a folder made in the same tick of the clock, 2 s on FAT,
   would not change it), nor that of a listing cut short; a folder that cannot be looked at now (no rights, a
   device not ready) keeps what is known of it, only one not found leaves. Ctrl+R forgets the times and reads
   the disk again, the tree staying in use.
5. **Operations.** After an operation or F7 the open tree has the folders of both panels and the operation's
   target listed again first.
   The operations still put what they did into the file of a disk that has one (step 1b), which spares the
   next opening a listing; with the tree open its worker writes the file last, and what an operation did
   that the worker did not see is found by the folders' times next time.

## Consequences
- The tree is in use at once on any disk; a disk read once is shown at once later and only its changes are
  listed.
- A file system that does not change a folder's time when a folder in it changes (some network ones) shows
  what was read until Ctrl+R, as FAR always does.
- The NTFS change journal would find changes without checking each folder, but needs the administrator's
  rights; not taken.
