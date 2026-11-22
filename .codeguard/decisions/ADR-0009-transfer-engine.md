# ADR-0009: The copy and move engine: background jobs, a fast path per OS, safe overwrite

- Date: 2026-11-08
- Status: accepted (owner, 2026-11-08)
- Extends: ADR-0004 (file operations are a critical area; `jobs` on worker threads, stage 4 brought forward),
  ADR-0005 (`platform` owns OS calls), ADR-0007 (elevation, when accepted, plugs in as a retry of a failed step)

## Context
F5 / F6 are the most used functions (owner, 2026-11-08). Today `fs::ops` copies synchronously on the UI thread
with `std::fs::copy` per file: no progress, no cancel, the screen freezes on a big copy; every existing name is an
error (no overwrite, no merge into an existing folder, no question); a move across drives deletes the source tree
only at the end. FAR's dialog (F5/F6, `far/copy.cpp`) has the options users expect: already existing files
(Ask, Overwrite, Skip, Rename, Append, Only newer, Also ask on R/O), access rights (Default, Copy, Inherit),
preserve all timestamps, copy contents of symbolic links, several destinations, filter, a progress dialog with
totals. FAR's own engine is Windows-only, single-threaded around its dialog loop, copies with its own buffer loop
or `CopyFileEx`, writes over an existing file in place (a failure in the middle leaves neither the old nor the new
file), and counts totals by a full scan before it starts. The owner asks for an engine that is efficient and
right on other platforms and file systems too.

## Decision
1. **Where.** A new core module `src/jobs` (no UI, no `app`; `core-no-ui` applies): `jobs::transfer` plans and
   runs a copy or move. OS calls (the fast copy, metadata, security, file identity, volume facts) go through
   `platform`; `fs::ops` keeps the safety checks it has and shrinks to what `jobs` calls. When `vfs` arrives
   (stage 4) the same engine works between providers; the local provider keeps the fast path.
2. **A worker thread per operation.** The app starts a job and gets a handle: a channel of events (progress,
   item done, question, finished) and a way to answer questions, pause and cancel. The UI never waits for the
   disk: it shows FAR's progress dialog and keeps drawing. Questions (exists, read-only, error) block only the
   worker until answered. One operation runs at a time at first (FAR's model); the handle allows a queue later.
3. **Streaming, not a full scan first.** The tree is walked while copying; the totals for the progress bar are
   counted by a second, cheap walker running ahead (sizes from the directory listing, no file opened). The copy
   starts at once; the totals fill in; on a slow network share nothing waits for the count.
4. **Fast copy per OS, one fallback.** `platform::copy_file(source, destination, progress, cancel)`:
   - Windows: `CopyFileExW` with a progress routine and a cancel flag: the kernel's copy (server-side copy on SMB,
     block cloning on ReFS, alternate data streams, attributes, sparse files, encryption kept);
     `COPY_FILE_NO_BUFFERING` above a size threshold so a big copy does not flush the file cache.
   - Linux: `ioctl(FICLONE)` (reflink on Btrfs, XFS, bcachefs), else `copy_file_range` in chunks (in-kernel,
     server-side on NFS 4.2 / SMB), else the fallback.
   - macOS: `clonefile` on APFS, else `fcopyfile`.
   - Fallback (any OS, any file system, plugins later): a read/write loop with a 1 MiB buffer.
   Progress is reported per chunk; cancel stops within one chunk and removes the partial file.
5. **Safe overwrite.** Overwriting writes a temporary file next to the target (`.<name>.nc-tmp`), then replaces the
   target in one step (`ReplaceFileW` / `MoveFileExW(MOVEFILE_REPLACE_EXISTING)` on Windows, `rename` on Unix):
   until the copy is complete the old file stays as it was. Append writes in place (it cannot be atomic) and
   says so in the question. A failed copy never leaves a partial file under the real name.
   **Not enough room for both** (owner, 2026-11-08): the temporary copy needs the new file's size free while
   the old file still exists. Before overwriting, the free space the user may use (quotas counted:
   `GetDiskFreeSpaceExW`'s available bytes, `statvfs` `f_bavail`) is compared with the new file's size:
   - enough: the safe way, no question;
   - not enough for both, but enough once the old file is gone: a warning names the file, both sizes and the
     free space, and asks: Overwrite in place (the old file is lost if the copy fails), All in place (the rest
     of this operation), Skip, Cancel. In place deletes the old file and copies into its name;
   - not enough even then: an error for that file (Skip / Skip all / Cancel); nothing is deleted.
   File systems that cannot say their free space exactly (compression, deduplication, network shares that
   report none) count as enough; a copy that runs out of space anyway fails cleanly, the old file kept.
6. **Moves.** The same volume: a rename of the top item (instant, whatever the tree size; a case-only rename on
   a case-insensitive file system is done, not refused). Another volume: copy and delete per file, the source
   file deleted right after its copy is complete, folders removed when they are empty; a cancelled or failed move
   leaves every file in exactly one place, never in neither.
7. **Existing names.** A folder that exists is merged into (FAR). A file that exists follows the dialog's choice:
   Ask (FAR's question: Overwrite, Skip, Rename, Append, Cancel, with "Remember choice" for the rest of the
   operation; sizes and dates of both shown), Overwrite, Skip, Rename (`name (2).ext`), Append, Only newer.
   "Also ask on R/O files" asks before replacing a read-only file. Copying an item onto itself or into itself
   stays refused (`fs::ops`).
8. **Errors.** A failed item asks Retry / Skip / Skip all / Cancel (FAR), never ends the whole operation silently;
   the final report lists what was skipped. Access denied is the point where ADR-0007's elevation, if accepted,
   is offered.
9. **Metadata.** Always: last write time, read-only / hidden / system / archive (Windows), mode bits (Unix).
   "Preserve all timestamps": creation and last access too. Access rights: Default (what the target folder
   gives), Copy (the source's security descriptor on Windows; owner, mode and ACL xattrs on Unix where allowed),
   Inherit (the target folder's inheritable rights). What a file system cannot keep (FAT has no ACL, no creation
   time precision, a 4 GiB limit; names with `:` or `\` from Linux on NTFS) is reported per item, not a failure
   of everything; the 4 GiB limit and lack of free space are found before copying a file.
10. **Links.** As now: links are copied as links. "Copy contents of symbolic links" follows them, with a loop
    guard on file identity (volume serial + file ID on Windows, device + inode on Unix): a link back to a folder
    being copied is reported and skipped, never followed forever.
11. **Several destinations** (copy only, FAR's option): the target line is split at `;` and `,` outside quotes;
    each destination is a pass over the same sources.
12. **Not now:** filter (comes with FAR's filters, Ctrl+I) and F10-Tree (comes with the tree panel): the dialog
    shows them as FAR does, disabled; parallel copying of small files (measured first, then decided); a queue
    of operations; background (minimized) progress.

## Consequences
- `src/jobs` exists from now (architecture table: "stage 4" becomes "exists since F5/F6"); `app` gets the job
  handle and FAR's progress and question dialogs; `fs::ops::copy` / `move_to` are replaced by the engine.
- `platform` grows: `copy_file` with progress (Windows now; Linux and macOS as stated when they are built), file
  identity, set file times, security descriptor copy, volume facts (file system name, free space, limits). New
  `windows-sys` features as needed; no new crate.
- Tests: the engine runs against a temporary tree with an in-memory answerer for questions: overwrite safety
  (a cancelled overwrite keeps the old file), move never loses a file (cancel after N files), merge, rename
  numbering, only newer, link loop, read-only question. The OS fast path is tested where it runs; the fallback
  everywhere.
- Steps: (1) dialog engine and FAR's F5/F6 dialog (done in the UI part); (2) `jobs::transfer` with the worker,
  progress dialog, existing-file choices and questions, safe overwrite, per-file move; (3) metadata, access
  rights, timestamps, link contents, several destinations; (4) read-only question for delete (F8) on the same
  question machinery.
