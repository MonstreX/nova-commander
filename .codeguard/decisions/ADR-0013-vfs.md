# ADR-0013: `vfs` - paths, providers, listing off the UI thread, transfers across providers

- Date: 2026-11-08
- Status: accepted (owner, 2026-11-08)
- Details: ADR-0004 §2-3 (the sketch). Order: ADR-0012 §3 (3b, step 1 and on). Amends: ADR-0009 §1 (the engine on
  `vfs`)
- Related: ADR-0010 (tasks), ADR-0011 (properties model, 3b step 5), stage 5 (plugins), stage 6 (`sftp`, `archive`)

## Context
Panels, `jobs` and `app` work on `PathBuf`, and listing, sorting, masks, path resolution, make folder and delete
live in `fs`, which ADR-0004 dissolves into `vfs` and `jobs`. ADR-0004 §2 sketches a `Provider` trait and a
`VfsPath` (`C:\work`, `sftp://user@host/home`, `zip:D:\a.zip!/dir`), nested providers and the local disk as the
built-in `local` provider. It leaves open what this ADR decides: the form of `VfsPath`, a blocking or an
asynchronous provider, errors and capabilities, how listing leaves the UI thread, and how the transfer engine of
ADR-0009 works across providers and keeps its fast path between local files.

Today: `Panel.path: PathBuf`; `fs::list_directory` runs on the UI thread (a slow share freezes the screen);
about 35 files outside `platform` take `Path`/`PathBuf`, most of them in `jobs::transfer` and `app`; the
terminal, the command line and the clipboard need a real OS path. FAR's model: a panel is a folder or a plugin
panel; plugin calls are blocking (`GetFindData`, `SetDirectory`, `GetFiles`, `PutFiles`); while a folder is
read the panel's title shows "Reading: N files" and Esc asks whether to stop (`filelist.cpp`), the files read so
far are kept; a plugin panel has no OS folder, so commands run in the last real folder.

## Decision
1. **`VfsPath`: a local path stays an OS path.**
   ```rust
   pub enum VfsPath {
       Local(PathBuf),                       // C:\work, \\server\share\dir, /home/user
       Mounted { provider: ProviderId,       // "sftp", "zip", "tmp", ...
                 base: Base,                 // Session("user@host:22") | File(Box<VfsPath>)
                 path: String },             // absolute, '/'-separated, normalized: "/home/user"
   }
   ```
   - `Local` keeps `PathBuf`, so the OS calls, the fast copy, `cd` in the terminal and every non-UTF-8 name on
     Unix stay as they are; `as_local() -> Option<&Path>` is how the terminal, the command line, the clipboard,
     the editor and the shell link get an OS path. On a mounted panel they act as FAR does on a plugin panel
     (commands run in the last local folder; Ctrl+Enter inserts the path as shown).
   - `Mounted` paths are text inside the provider: `/` separated, always absolute, `..` and `.` resolved by
     `VfsPath`, never by the OS. A nested provider has its container as `base` (`zip:D:\a.zip!/dir`,
     `zip:sftp://user@host/a.zip!/dir`).
   - Pure operations live on `VfsPath`, not on a provider, and never touch a disk: `parent`, `join(name)`,
     `file_name`, `is_root`, `display` (the forms above), `parse` (the command line's `cd`, the drive menu, the
     settings file), `resolve(base, input)` (today's `fs::resolve_path`), the case rule (`platform` for local,
     the provider's capability for mounted).
   - The settings file stores a panel's folder as its displayed text; a mounted path that cannot be reopened on
     start (no plugin, no session) falls back to the program's folder, as a missing local folder does now.

2. **A blocking provider, called only from worker threads.**
   ```rust
   pub trait Provider: Send + Sync {
       fn id(&self) -> ProviderId;
       fn capabilities(&self, path: &VfsPath) -> Capabilities;
       fn list(&self, dir: &VfsPath, sink: &mut dyn ListSink) -> io::Result<()>;   // entries in chunks
       fn stat(&self, path: &VfsPath) -> io::Result<Entry>;
       fn open_read(&self, path: &VfsPath) -> io::Result<Box<dyn Read + Send>>;
       fn open_write(&self, path: &VfsPath, mode: WriteMode) -> io::Result<Box<dyn Write + Send>>;
       fn make_dir(&self, path: &VfsPath) -> io::Result<()>;
       fn remove(&self, path: &VfsPath, how: Removal) -> io::Result<()>;          // file, empty folder, to the bin
       fn rename(&self, from: &VfsPath, to: &VfsPath, replace: bool) -> io::Result<()>; // within the provider; `replace`: the safe overwrite (3b step 4)
       fn set_times(...) / set_properties(...)                                    // 3b step 4-5, default Unsupported
   }
   ```
   - Blocking, no async runtime: the program's threads are already the model (ADR-0009, ADR-0010), FAR's plugin
     calls are blocking, and the C ABI of stage 5 stays plain functions. A provider whose library is async
     (an SSH crate) runs its own runtime inside and answers blocking calls.
   - `&self` and `Send + Sync`: several tasks may call one provider at once; a provider that cannot (one SSH
     channel) serializes inside. Sessions (connections) belong to the provider, keyed by `Base::Session`.
   - Cancel: long calls get a `&Cancel` (the flag `jobs` already has) where a provider can stop in the middle
     (`list`, a remote open); streams are cancelled between chunks by the caller.
   - The UI thread never calls a provider. It calls `VfsPath`'s pure operations and starts jobs.
   - `vfs::Registry` maps a `ProviderId` to an `Arc<dyn Provider>`: `local` built in; plugins register theirs
     in stage 5. `Entry` (today's `fs::Entry`) moves to `vfs` unchanged.

3. **Errors are `io::Error`.** The kinds `jobs` and the dialogs already tell apart (`NotFound`,
   `PermissionDenied`, `AlreadyExists`, `StorageFull`, `ReadOnlyFilesystem`, `Unsupported`, `Interrupted` for a
   cancel) come through every provider; the text is the OS's or the provider's. Plugins (stage 5) return a kind
   code and a message through the ABI and get the same `io::Error`. No error type of our own until a case
   needs one.

4. **Capabilities per path**, asked before a key acts (FAR's `OPIF_*` / missing exports grey out a command):
   write, make folder, rename, delete, recycle bin, links, properties (ADR-0011), real OS path (run, `cd`, open
   by association), change notifications, case-sensitive names. A key whose capability is missing does what
   FAR does on such a plugin panel (nothing, or FAR's message); the local provider has all of them.

5. **Listing on a worker thread** (`jobs::listing`):
   - the panel asks for a folder and gets a handle; entries arrive in chunks; a newer request supersedes an older
     one (a generation number: a late result of a folder the user has left is dropped);
   - FAR's look: after about 0.3 s without a result the panel's title shows "Reading: N files"; Esc asks
     whether to stop (FAR's question) and keeps what was read;
   - entering a folder: the panel shows the new folder when its listing (or its first chunk) arrives; until
     then keys act on the old one, and a failure leaves the panel where it was with FAR's "cannot read the
     folder" message. Refreshing the same folder keeps the old entries on screen until the new ones are in
     (cursor and selection kept as `reload` keeps them now);
   - sorting and filtering stay on the UI thread (in memory, cheap); masks and sorting move to `vfs`;
   - tests wait for a listing through the handle, never by sleeping.

6. **Transfers across providers** (ADR-0009's engine on `VfsPath`, 3b step 4). For each file the engine picks:
   - both `Local`: today's path unchanged: `platform::files::copy_file` (`CopyFileExW`), a rename on the same
     volume for a move, safe overwrite with `ReplaceFileW`/`MoveFileExW`, free-space check;
   - the same mounted provider: its `rename` for a move; a server-side copy when a provider offers one
     (an optional `copy_within`, default `Unsupported`);
   - otherwise streaming: `open_read` -> `open_write` in 1 MiB chunks, progress and cancel per chunk, the
     safe overwrite through a temporary name and `rename` where the target provider can rename (else in
     place, said in the question as for Append); metadata the target cannot keep is reported per item
     (ADR-0009 §9);
   - optional `download(path, local)` / `upload(local, path)` on a provider (FAR's `GetFiles` / `PutFiles`)
     for providers that do it faster themselves (SFTP pipelining); the default streams.
   The walker, questions, existing-name choices, tasks and the per-volume queue stay as they are; a
   mounted path's "volume" for the queue is its provider and base.

7. **`fs` dissolves in 3b step 1**: `listing` and the local reading become `vfs::local`; `sort` and `mask` move to
   `vfs`; `paths::resolve_path` becomes `VfsPath::resolve`; `drives` moves to `vfs::local::drives` as it is
   (its model changes in 3c); `ops::make_dir` becomes the provider's `make_dir`; `ops::delete` stays until
   3b step 3 replaces it with `jobs::delete`; `ops::write_atomic` (settings, editor) becomes
   `vfs::local::write_atomic`. The architecture table and the forbid rule list follow (`fs` row removed).

## Consequences
- Step 1 is wide: `Panel.path` becomes `VfsPath`, about 35 files change their path types, most of them by
  `as_local()` at the edges; behaviour stays the same for local folders except that listing no longer
  freezes the screen. The real program is driven once (e2e) before the commit, as for 3a.
- `jobs::transfer` keeps `PathBuf` until step 4, called through `as_local()`; until a mounted provider exists
  (stage 5) the streaming path is tested with an in-memory test provider in `vfs`'s tests.
- Non-UTF-8 names: `Entry.name` stays `String` (lossy on Unix) for now; the port changes it (`docs/tech-debt.md`,
  [port]).
- The plugin ABI of stage 5 mirrors the trait: blocking calls, kind codes for errors, capability flags.
- Not decided here: the plugin ABI itself, sessions' passwords and keys (stage 6), a remote archive's temporary
  download (stage 6).
