# ADR-0023: remote file systems as the plugin NetWay, after FAR's NetBox

- Date: 2026-11-17
- Status: accepted (owner, 2026-11-17: the name NetWay, the steps and the protocols' order below; the
  background transfers later, after S6.6); its API needs are recorded in addenda as the steps find them

## Context
Stage 6's remote part (roadmap S6.4-S6.6) takes FAR's NetBox (`github.com/michaellukashov/Far-NetBox`, read in
`netbox-source/`, kept out of git as `far-source/` is): SFTP, SCP, FTP / FTPS, WebDAV and S3 panels behind a
panel of saved sessions. NetBox is GPL and is itself WinSCP's, PuTTY's and FileZilla's code; the program is MIT
(owner, 2026-11-13, far-parity 11a), so none of its code can be taken. Its behaviour can: its panels, keys,
dialogs, texts, prefixes and defaults, read in its sources as FAR's are (`src/NetBox/WinSCPFileSystem.cpp`'s
`ProcessKeyEx`, `GetFindDataEx`, `GetFilesEx` / `PutFilesEx`; `WinSCPDialogs.cpp`'s session dialog with its 15
tabs, the copy dialog, the properties; `NetBox.en.hlf`; `NetBoxEng.lng`).

## Decision
1. **The plugin.** `plugins/netway`, the program's own name **NetWay** where NetBox says NetBox (owner: a
   project of its own, no other project's name), prefix `netway:` where NetBox's is `netbox:`, with the
   protocols' own prefixes `sftp:`, `scp:`, `ftp:`, `ftps:`, `webdav:`, `s3:` as NetBox's defaults. NetBox's
   behaviour is taken unless a difference is written down in far-parity with its reason (as ADR-0021 §1 for
   arclite).
2. **The protocols from Rust libraries under permissive licences**, the plugin's dependencies alone (the
   program links none of them):
   - SSH, SFTP, the agent (Pageant), `known_hosts`: `russh` and `russh-sftp` (Apache-2.0; pure Rust, the
     `ring` backend, no OpenSSL); SCP over its exec channel as WinSCP's shell protocol;
   - FTP / FTPS: `suppaftp` (MIT or Apache-2.0);
   - WebDAV: `reqwest` (MIT or Apache-2.0) with `quick-xml` (MIT), the WebDAV requests written in the plugin;
   - S3: `rust-s3` (MIT) or `aws-sdk-s3` (Apache-2.0), chosen in S6.6c;
   - TLS: `rustls` (Apache-2.0, ISC or MIT): the same on Windows and Linux (ADR-0017);
   - `tokio` (MIT) inside the plugin's library: the plugin's calls come on its thread (ADR-0016) and wait on
     its runtime there, Esc through `Host::cancelled` checked while they wait.
3. **Sessions and secrets.** One file per session in the plugin's settings folder, its folders NetBox's
   session folders (export and import as files). Passwords and passphrases are kept by the system's
   protection for the user (Windows DPAPI through the SDK's `system`; the Linux keyring with S7), not as
   WinSCP's reversible obfuscation (a documented difference: safer, the same to the user). Host keys in the
   plugin's own `known_hosts`.
4. **Tests without outside servers.** The unit tests start servers inside the test process, from crates
   under permissive licences (dev-dependencies of the plugin only): `russh`'s server side with `russh-sftp`'s,
   `libunftp`, `dav-server`, `s3s`. The end-to-end stands use real servers in WSL Ubuntu (OpenSSH, vsftpd, a
   WebDAV server, MinIO), started and removed by the stands.
5. **Steps** (roadmap S6.4-S6.6): S6.4a this ADR's proof (the plugin with `tokio` in its library, `russh` on
   Windows and Linux, a test server in the process, PuTTY's `.ppk` keys and Pageant checked); S6.4b the
   sessions panel; S6.5a-d SFTP browsing, SFTP operations, SCP with the commands menu, the rest of the SSH
   tabs; S6.6a-d FTP / FTPS, WebDAV, S3, synchronization and the commands menu's rest. Then S6.8: transfers
   in the background (NetBox's queue as the program's own background tasks: the copy's Background button and
   F12's list, a connection of their own), later by the owner.
6. **Put off** (owner may take them later): starting PuTTY / PuTTYgen, GSSAPI (Kerberos: no mature library
   under a permissive licence), client certificates until S6.6.

## Consequences
- `architecture.md` lists `plugins/netway` and its crates; far-parity 11a's sftp line names NetWay.
- An API change found by S6.4a (a timer for keep-alive, a background transfer for S6.8) is a new API version
  with its addendum here, as ADR-0021's.
- `netway`'s crates must build for Linux too (`cargo check --workspace --target x86_64-unknown-linux-gnu`).

## Addendum (S6.4a, 2026-11-17): the proof
- `plugins/netway` (no manifest yet: not packaged until S6.4b): `runtime` (a `tokio` runtime of two threads
  inside the library; `Runtime::wait` waits on the calling thread and asks the stop every 50 ms, dropping the
  work; `spawn` for work between calls), `ssh` (connect, the host key, the logins), `sftp` (a folder's
  listing, the home folder).
- `russh` 0.65 with the `ring` backend (`aws-lc-rs` needs CMake and NASM). Its keys (`ssh-key` 0.7) read
  OpenSSH, PEM and PuTTY's `.ppk` 2 and 3 (an encrypted `.ppk` through Argon2 or SHA-1, as PuTTY's): tested
  with OpenSSH's plain and encrypted keys and a `.ppk` 3. The agent is Pageant on Windows (the `pageant`
  crate), `SSH_AUTH_SOCK` elsewhere (NetBox's too is Pageant only); built, not yet run against one.
- **No API change.** Keep-alive and other work between the program's calls run on the plugin's runtime;
  the host is needed only for windows, which come on the plugin's thread. The host key's question is asked
  from the runtime's thread (`ssh::Trust`): S6.5a has it answered on the plugin's thread, the wait serving
  the question while it waits.
- **Linux.** `ring` compiles C for its target, which the Windows build has no compiler for: the cross check
  from Windows is `cargo check --workspace --exclude netway --target x86_64-unknown-linux-gnu --all-targets`,
  and `netway` is built and tested natively in WSL Ubuntu (`cargo test --locked -p netway` with a target
  folder of its own in WSL's `/tmp`, removed after): its 11 tests pass there as on Windows.
- Tests: a server inside the tests' process (`test_server.rs`: `russh`'s server side, `russh-sftp`'s,
  a temporary root), keys in `plugins/netway/testdata` made by `ssh-keygen` (the `.ppk` written from the
  plain key); the logins (password, wrong password, the three keys, an encrypted key without its
  passphrase), the host key (new and kept, once, changed and refused), a host dropping the connection, a
  listing. The keys are kept byte for byte (`.gitattributes`: `-text`, never CRLF on checkout).

## Addendum (S6.4b, 2026-11-17): the sessions panel, API 7
- **The plugin's data folder.** The API had no folder for what a plugin keeps of the user's (the plugin's
  own folder is replaced by its update, its settings are text values in `settings.json`): API 7 adds
  `NcHost::data_folder`, `settings\Plugins\<name>` (ADR-0016 addendum S6.4b). The sessions are there, in
  `Sessions`: a file `<name>.netway` per session, the session folders as folders, a session named by its
  path from there (`Work/Prod/db`, NetBox's names).
- **A session's file**: `Key=Value` lines (`Protocol`, `Host`, `Port`, `User`, `Password`, `PrivateKey`,
  `ScpFallback`, `Encryption`); keys a version does not know are kept as they were, so the later tabs add
  keys without a new format. The password is DPAPI's protected bytes (the SDK's `system::protect` with
  the plugin's entropy) as hex; off Windows `protect` is unsupported until the port's keyring (tech-debt
  [S7]) and no password is kept (the user is told). A password kept by another user does not read back:
  the session comes without it, and F4 says so.
- **The panel** (NetBox's session list): the folders and sessions, NetBox's hint line in a folder of none,
  the title "NetWay: Stored sessions [/folder]", every mode the names under "Session name", the key bar's
  NewSes / Duplicate / Rename and the keys NetBox blanks; F4, Shift+F4 ("Add session", then "Save session
  as"), Shift+F5 / Shift+F6, Ctrl+F, F7 (a folder: made on the disk, so an empty one stays, where NetBox's
  goes when the panel leaves it), F8 (a folder with all its sessions). The session dialog's Session tab
  (the protocol, the encryption, the host and port, the login or S3's access keys, the private key);
  NetBox's port switch with the protocol. The Plugins menu (F11) and the Disks menu open the panel.
- **Split.** S6.4b is the panel and its keys; S6.4c is F5 / F6's export and import, the prefixes
  (`netway:`, `sftp:` ...) and the quick connect. Enter's and the dialog's Connect come with S6.5a's
  connection (Enter on a session says so until then).
- **Notices.** The package's `THIRD-PARTY-NOTICES.md` gains the plugin's libraries whose licences ask for
  their text (Apache-2.0: `russh`'s crates, `russh-sftp`; ISC: `ring`, `untrusted`; BSD-3-Clause: the
  dalek crates, `subtle`).

## Addendum (S6.4c, 2026-11-17): export, import, prefixes
- **Export** (NetBox's `GetFilesEx` / `ExportSession` on the session list): the panel gives its files
  (`PanelFlags::gives_files`), so F5 / F6 to a disk come to the plugin, which asks NetBox's "Export
  session "db" to:" (or "Export N session(s) to:") with the other panel's folder (history "Copy"; a
  relative folder is the program's) and copies each session's file there as WinSCP's
  `ValidLocalFileName` of its whole name with `.netway` (`Work/Prod/db`: `Work-Prod-db.netway`); a folder
  exports all its sessions; a file there is asked over (NetBox's "File ... already exists. Overwrite?",
  Yes / No / Cancel - Cancel stops the export, where NetBox's goes on with the next). The file goes out as
  it is: its password is DPAPI's, readable by its user on this computer alone (NetBox's XML carries
  WinSCP's reversible obfuscation, readable anywhere). F6 copies as F5 does (NetBox's export takes no
  move). F3, Ctrl+PgDn and Find file read a session's file under its own name.
- **Import** (NetBox's `PutFilesEx` / `ImportSessions`): the panel takes paths (`PanelFlags::takes_paths`),
  so F5 / F6 from a disk's panel into it come to the plugin after the program's copy dialog (as FAR's,
  which asks its own before `PutFiles`); "Import session(s) from selected file(s)?" (Yes / No), then each
  file that is a session's (a line `Protocol=` of a protocol known) comes in under its name without its
  extension in the panel's folder; none of them: NetBox's "File "..." does not contain any session data."
  A session of the name is asked over (NetBox's import overwrites it). The sources stay (NetBox's import
  takes no move). Not imported: NetBox's and WinSCP's own XML or INI files (their passwords are WinSCP's
  obfuscation); a later step may read them.
- **Prefixes** (NetBox's `CommandPrefixes`, `OpenPluginEx` with `OPEN_COMMANDLINE`, WinSCP's
  `TSessionData::ParseUrl`): `netway:` (NetBox's `netbox:`), `sftp:`, `scp:`, `ftp:`, `ftps:`,
  `webdav:`, `s3:`. Nothing after them opens the sessions panel. A stored session is named by its name
  without its folders or by its whole name (whatever the case), a host's folder after a `/`; WinSCP also
  takes a name the address only starts with (`db` for `dbserver`), the plugin needs the whole name or a
  `/` after it. Otherwise the address is a session of its own: WinSCP's protocols (`scp`, `sftp`, `ftp`,
  `ftps`, `ftpes`, `dav`, `webdav`, `http`, `davs`, `https`, `s3plain`, `s3`, `ssh`) with their
  encryption and port, `user:password@host:port` (`[v6 address]`, `%XX` and `+` decoded), WebDAV to an S3
  host taken as S3, the folder and the file to put the cursor on; the connection's parameters after `;`
  come with the SSH tabs. A name in quotes (Ctrl+F's with a space) is taken without them. Connecting to
  what is named comes with S6.5a (until then it says so), as does Enter on a `.netway` file of a disk
  (NetBox's `OPEN_ANALYSE`).

## Addendum (S6.5a, 2026-11-17): connecting, browsing SFTP
- **Questions from the network's work.** The login runs on the plugin's runtime; what it asks (the host
  key, the user's name, the password, a key's passphrase, the server's questions) and its statuses go
  through a channel (`ask`) to the plugin's thread, which waits for the work (`Runtime::wait_answering`)
  and shows them in the program's windows: the program's windows are shown from the plugin's thread only
  (ADR-0016). The host key's trust is awaited, not called (`ssh::Trust` is a future), so a runtime thread
  never blocks on the user. Esc (`Host::cancelled`) drops the work.
- **The login** in WinSCP's order: the private key (its passphrase asked when it does not read without
  one) or else the agent (Pageant; its failures pass silently to the password), then the session's
  password or "&Password:", each tried as a password and then with the server's questions
  (keyboard-interactive, a round of one hidden question answered by that password, others shown); three
  passwords at most; "&Username:" when the session has none.
- **Host keys** are the plugin's own `known_hosts` in its data folder (not the user's `~/.ssh`). A
  changed key kept replaces the old one of its kind: `russh` calls any other key of the kind kept for the
  host "changed", so WinSCP's "Add" (keep both) cannot work and is not offered (Update / Skip / Cancel).
- **The panel** is an instance of its own (`sftp-N`) opened over the sessions panel (".." at its root back
  to the session, as an archive opened from a file) or in the active panel from the command line. The
  program opens a plugin's panel at its root, so the folder it opens at (the session's, else the home
  folder) is reached as the network plugin's start is: its root answered empty at once, a synchro then
  setting the panel's folder - no API change. Its connection closes with the panel.
- **Only SFTP** connects in S6.5a; SCP (and SFTP's fallback to it) comes with S6.5e (renumbered by S6.5b's split), the other protocols
  with S6.6 (their sessions say so).
- **Stands** run against the plugin's own test server (`cargo run -p netway --example sftp_server`, the
  server of its tests) rather than an `sshd` in WSL, which is not installed there and would need the owner's
  system changed; the WSL servers stay for the protocols the test server lacks.

## Addendum (S6.5b, 2026-11-17): copying, the split of S6.5
- **Split.** NetBox's operations are more than one step: S6.5b is copying both ways with F3 / F4; S6.5c
  the transfer settings (NetBox's "Transfer settings" dialog, its presets, "Use same settings next time",
  resume through a temporary name, the speed limit) and the editor's upload after every save; S6.5d the
  other operations (F7, F8, rename, Shift+F5 / Shift+F6 on the server, Ctrl+A rights and owner, Alt+F6
  links, a dropped session reconnected); SCP with the commands menu becomes S6.5e, the SSH tabs S6.5f.
- **The panel gives its files and takes paths** (`PanelFlags::gives_files`, `takes_paths`), as the
  sessions panel does: F5 / F6 to a disk come to `get`, which asks NetBox's copy dialog; a disk's files
  come to `put` after the program's copy dialog (FAR's before `PutFiles`), and `put` asks NetBox's own as
  NetBox's `UploadFiles` does - two dialogs, as FAR with NetBox shows them. The viewer's, the editor's and
  Find file's copies come with no dialog and no question; the editor's copy goes back through `put` with
  `GetMode::edit` when the editor is left (NetBox's default uploads at every save, which needs the editor's
  save told to the plugin: an API change, S6.5c).
- **The work** runs on the runtime (`copy_work`, `download`, `upload`): the items walked first for their
  total, then each file sent in pieces of 32 KB; its questions (the overwrite question, append or resume,
  the new name, a failure's Abort / Retry / Skip / Skip all) go through `ask` to the plugin's thread, its
  progress through shared counters the plugin's thread reads while it waits, showing NetBox's window twice
  a second. Esc asks NetBox's "Cancel current operation?"; OK ends the work at its next piece, a file
  being written anew removed (NetBox's "Cancel file transfer?" with the connection closed is not needed:
  an SFTP request in flight is answered and dropped); a second Esc after it drops the work at once.
- **Names** as WinSCP's: the dialog's text after the folder a mask of the first level's names
  (`MaskFileName`); characters a Windows name cannot hold written `%XX` coming down and read back going up
  (`ValidLocalFileName` / `RestoreChars` with WinSCP's trailing dot or space and reserved names, which
  NetBox's build leaves out).
- Differs from NetBox (far-parity 15): no "Never ask me again" in the overwrite question (the program's
  message has no check box); the queue's check boxes come with the background transfers (S6.8).

## Addendum (S6.5c, 2026-11-17): transfer settings, API 8
- **Split again.** Resume through a temporary name goes with NetBox's other "Endurance settings"
  (reconnecting) into S6.5d; S6.5c is the transfer settings, their presets and defaults, the plugin's
  configuration and the editor's upload at every save.
- **API 8** (ADR-0016's rule: any change of the ABI is a new version, every manifest "api": 8): a
  dialog's box (`ITEM_BOX`, FAR's `DI_SINGLEBOX`: a frame, or one row high a line with its title in its
  middle; NetBox's transfer settings are drawn with such lines), and `PANEL_PUTS_ON_SAVE`: the program
  hands the editor's saved copy to the plugin's `put` (`GET_EDIT`) at each save while the editor stays,
  the copy counted as made then (leaving the editor puts it back only when saved since); with another put
  not ended it waits for the editor's leaving. This is the program's side of NetBox's `EE_SAVE`: the copy
  and its origin are the program's, so the plugin is told by the put, not by an editor event.
- **WinSCP's copy parameters** (`copy_params`): every field NetBox's dialog sets with WinSCP's defaults and
  storage names; `GetInfoStr` as NetBox builds it ("Transfer type: Binary" first - the "Default transfer
  settings" of S6.5b is WinSCP's Borland build only); what a direction cannot use
  (`UsableCopyParamAttrs`: a download sets no rights and clears no archive bit, an upload keeps no
  read-only) left out of the line and disabled in the dialog. WinSCP's file masks (`file_masks`) for "File
  mask" and the text mode's masks: include | exclude, folders' masks, paths, sizes and times.
- **At work**: names by `ChangeFileName` (mask, case, tokens), items by the file mask, the text mode
  (`TFileBuffer::Convert`: LF as CRLF down, CRLF as LF up; no resume offered then), the rights of uploaded
  files and new folders (X where they read for folders), "Ignore permission errors", "Preserve
  read-only", "Clear 'Archive' attribute" of a source sent and not moved (the SDK's
  `system::clear_archive`), the speed limit (pieces waited after), "Calculate total size" off showing the
  start time and the files' bar (NetBox's `TotalSizeSet`).
- **Configuration** in the program's settings of the plugin (FAR's `PluginSettings`): the default
  transfer settings ("Use same settings next time", the configuration's "Transfer settings") under
  "CopyParam.<WinSCP's name>", NetBox's editor options. Options > Plugins configuration > NetWay opens
  NetBox's menu with "Transfer settings" and "Editor/viewer settings"; its other items come with their
  steps. The viewer's and the editor's copies take `EditViewCopyParam`'s settings; the editor's upload
  the download's (`FLastEditCopyParam`).
- Differs from NetBox (far-parity 15): a preset is taken when the list was changed (NetBox's takes it at
  each leaving, setting "Default" back over custom settings when the list was only passed by); the speed
  is an edit line with a history, not a combo box (the program has no editable combo); "Allow more than
  one edited file" is not offered (the program's editor holds one file); presets are NetBox's four fixed
  ones (NetBox has no dialog to edit them either).

## Addendum (S6.5d, 2026-11-17): operations on the server, the steps renumbered
- **Renumbered.** NetBox's "Endurance settings" (resume through a temporary name, reconnecting) become
  S6.5e; SCP with the commands menu S6.5f; the SSH tabs S6.5g.
- **The connected panel deletes and makes folders itself** (`PanelFlags::deletes`, `makes_folders`, as
  the sessions panel): F8 asks NetBox's question, F7 shows NetBox's "Make folder" with the rights of new
  folders (kept in the program's settings of the plugin as WinSCP's `NewDirectoryProperties`). Its own
  keys (`panel_keys`): Shift+F5 / Shift+F6 "Duplicate" / "MoveTo", Alt+Shift+F6 "Rename", Ctrl+A, Alt+F6,
  Ctrl+Alt+F, Alt+Shift+Ins; Shift+F1-F3 blank (NetBox clears the archives' keys).
- **The operations** (`remote_ops`) run as the transfers do (`copy_panel::transfer` with an operation's
  window: the file and the items' bar), each step's failure asked with WinSCP's headline (`failed_about`).
  A walk on the server takes links as themselves (`sftp::read`, `symlink_metadata`): deleting a link
  never touches what it points to. "Duplicate" copies through SFTP piece by piece with the times and
  rights; WinSCP's copy runs `cp` through a command session, which comes with S6.5f's shell. The rights'
  box (`rights_box`, `TRightsContainer`) with three states for several files, the specials, the octal
  line and None / Default / All. SFTP 3 changes an owner and a group by their numbers
  (`fcGroupOwnerChangingByID`): "Attributes" takes the numbers and shows the names beside them.
- Differs from NetBox (far-parity 15): the command line gets the address set, not inserted (the
  program's key answer sets the line); no server's Recycle Bin until the SSH tabs.

## Addendum (S6.5e, 2026-11-18): endurance
- **Resume through a temporary name** as WinSCP's (`AllowResume`, `PartialExt`): a file the settings
  let (all, from a threshold, none; never in the text mode, over a link, for the viewer and the editor) is
  written as `<name>.filepart` and put in place when whole - on the server the file there removed first
  (SFTP 3 renames over nothing) and its rights kept on the new one. A partial file found asks
  `SFTPConfirmResume`'s question; a stop leaves it for the next transfer.
- **Reconnecting** as WinSCP's `QueryReopen`: the panel's session keeps its login (`Remote::plan` with
  the user and the password that logged in, `login::Started::password`); lost (`russh`'s `is_closed`), it
  is logged into again before the panel's next operation, after "Lost connection." counted down (the
  configuration's "Reconnect after") or asked, at most the configured retries in a row. A panel's reading
  (`list`) may show no window: there the login runs with its questions refused. A transfer losing its
  session mid-way asks its failure: reconnecting inside a file is left to S6.8's queue (WinSCP's reopen
  inside `FILE_OPERATION_LOOP` restarts the file through the queue's terminal as well).
- **Configuration**: NetBox's "Endurance settings" between "Transfer settings" and "Editor/viewer
  settings" (`endurance`), the resume in the default transfer settings, the reconnecting under WinSCP's
  names (`SessionReopenAuto`, `SessionReopenAutoMaximumNumberOfRetries`, `SessionReopenAutoStall`).

## Addendum (S6.5f, 2026-11-18): SCP, the file system split, the commands menu moved
- **The file system** (WinSCP's `TCustomFileSystem`): `fs::Fs` answers the work's calls (`SftpSession`'s
  names: metadata, set_metadata, remove, rename, links, open / create / open for writing, list) for SFTP
  or SCP and says what it cannot (`Capability`, WinSCP's `IsCapable`: SCP writes no file from an offset,
  so no Append, resume or `.filepart`). The copy, delete, move and properties work runs on it unchanged;
  FTP, WebDAV and S3 (S6.6) become its further protocols.
- **SCP** as WinSCP's `TSCPFileSystem`: folders by `ls -la` (with `--full-time` while the server's `ls`
  reads it, else the plain form, WinSCP's `ScpLsFullTime` auto), read by WinSCP's `SetListingStr` rules
  (`scp_listing`); an item by `ls -la -d` (`-L` for what a link points to); `rm -f`, `rmdir`, `mkdir`,
  `mv -f`, `ln [-s]`, `cp -p -r -f`, `chmod`, `chown`, `chgrp` with paths in WinSCP's double quotes; files
  down by `scp -f` and up by `scp -t` (the `C` record with the rights and the size, `\0` acknowledging,
  `\x01` / `\x02` failures). A time kept on upload by `TZ=UTC touch -m -t` after the file (WinSCP sends a
  `T` record before it: the work sets times after writing, for every protocol).
- **Differs from WinSCP**: each command runs on an exec channel of its own instead of one interactive
  shell read through markers - no prompt, startup text or `LANG` to pass by, exit status and error output
  told apart by SSH itself; the session's shell options (S6.5g's SCP tab) apply to what remains of them.
- **Connecting**: an SCP session (or `scp://` address) starts no SFTP; an SFTP one with "Allow SCP
  fallback" falls back to SCP when the server refuses the `sftp` subsystem, else fails with WinSCP's
  "Cannot initialize SFTP protocol. Is the host running an SFTP server?"; the session's address
  (Ctrl+Alt+F) names the protocol that runs.
- **Duplicate through `cp`** as WinSCP's secondary shell: an SFTP session copies on the server by `cp -p
  -r -f` when the server runs commands, else through the client as before.
- **Tests' server**: `test_shell` runs those commands and `scp` over the server's root; the user `shell`
  has no SFTP (the fallback's).
- **The split**: NetBox's commands menu (Alt+Shift+W), Ctrl+G "Apply command", the server's information
  and the log move to S6.5h, after S6.5g.

## Addendum (S6.5g, 2026-11-18): the session dialog's tabs, Environment to SCP/Shell, the split
- **Tabs** as NetBox's `TTabbedDialog` / `TSessionDialog` (69 x 25): a row of the tabs' names above them,
  the shown one in brackets, those the protocol has no use for left out (SFTP only for SFTP, SCP/Shell for
  SSH), scrolling with `◄` / `►` when they do not fit; Ctrl+PgDn / Ctrl+PgUp step round them, a click on a
  name shows its tab; each tab's items are shown and hidden as a group. **Differs from NetBox**: the bar is
  one text line, not buttons (the program's dialogs cannot move items, which NetBox's scrolling bar does):
  the tabs are reached by Ctrl+PgUp / Ctrl+PgDn or the mouse, not by Tab; the dialog's caption stays (NetBox
  writes "Tab - Edit session": the shown tab is the bracketed name and its group's heading).
- **Settings** (`session_options`) under WinSCP's names and defaults, written only when they differ
  (`WRITE_DATA`), kept among the session's other keys: `EOLType`, `TimeDifference` (days, as WinSCP),
  `ConsiderDST`, `DeleteToRecycleBin`, `OverwrittenToRecycleBin`, `RecycleBinPath`, `UpdateDirectories`,
  `RemoteDirectory`, `ResolveSymlinks`, `SftpServer`, `Shell`, `ListingCommand`, `IgnoreLsWarnings`,
  `SCPLsFullTime`, `UnsetNationalVars`.
- **Applied**: CR/LF servers need no text-mode conversion; the time zone offset shifts SCP's times read
  without a zone, which are the server's wall clock in the client's zone (WinSCP's `ShiftTimeInSeconds`);
  deleting moves into the recycle bin as `name-yyyymmdd-hhnnss.ext` (`RecycleFile`; in the bin itself it
  deletes), overwriting over SFTP moves the old file there first; the remote directory opens when the
  address has none; "Remember last used directory" writes the panel's last folder into the stored session
  when the panel closes; links to folders are folders only while "Resolve symbolic links"; the SFTP server
  runs as that command instead of the `sftp` subsystem; the shell runs each command (`shell -c "..."`);
  the listing command, "Ignore LS warnings" (a failed `ls` with output read), "Try to get full timestamp"
  (off: plain `ls` only), "Clear national variables" (`unset LANG ... ;` before the listing).
- **Left out, written down**: the filenames' charset (names are UTF-8), the DST modes kept but not applied
  (the program's times are UTC both ways, as WinSCP's Unix mode), the directory cache options (the panel
  reads anew), the SFTP protocol version, real path and packet sizes (`russh-sftp` speaks version 3 as it
  does), POSIX rename, SCP's return code variable, user groups, clearing aliases and SCP1 compatibility (no
  interactive shell: each command's exit status comes from SSH).
- **The split**: S6.5h Connection, Proxy, Tunnel; S6.5i SSH, KEX, Authentication, Bugs; the commands
  menu becomes S6.5j.

## Addendum (S6.5h, 2026-11-18): Connection, Proxy, Tunnel
- **Settings** (`link_options`) under WinSCP's names and defaults: `Timeout` 15, `PingType`,
  `PingIntervalSecs` 30, `AddressFamily`, `ProxyMethod` (`none`, `socks4`, `socks5`, `http`, `telnet`,
  `cmd`), `ProxyHost` "proxy", `ProxyPort` 80, `ProxyUsername`, `ProxyPassword`, `ProxyTelnetCommand`
  "connect %host %port\n", `ProxyLocalCommand`, `ProxyDNS` (WinSCP's shifted value), `ProxyLocalhost`,
  `Tunnel`, `TunnelHostName`, `TunnelPortNumber` 22, `TunnelUserName`, `TunnelPassword`,
  `TunnelPublicKeyFile`. The proxy's and the tunnel's passwords are kept protected as the session's
  (§3; off Windows not kept yet).
- **The way to the host** (`route`, `proxy`; PuTTY's `proxy.c` that WinSCP uses): straight by the IP version
  asked for; or through SOCKS4 (SOCKS4a when the proxy looks the name up), SOCKS5 (user and password),
  HTTP `CONNECT` (Basic authentication), Telnet's command with PuTTY's `%host`-style fields and escapes, or
  a local command whose input and output carry the session (run by the system's shell, its pipes bridged
  by threads: no new dependency); the loopback straight unless "Consider proxying local host
  connections"; "Do DNS name lookup at proxy end" No looks the name up here. A refused proxy login is
  WinSCP's "Proxy authentication failed.". `russh`'s `connect_stream` runs the session over the way.
- **Tunnel** as WinSCP's: "Opening tunnel...", the tunnel host logged into by its own user, password or
  key (its host key asked as any), then the session through it. **Differs**: a direct-tcpip channel of the
  tunnel's connection carries the session itself, no local port is listened on ("Local tunnel port" left
  out); the tunnel connection closes with the session.
- **Connection**: the timeout bounds reaching the host (the connection and the proxy's handshake; the
  replies' wait stays the endurance's); keepalives every interval as OpenSSH's keepalive requests for both
  "null SSH packets" and "dummy protocol commands" (`russh`'s `keepalive_interval`; it sends no
  `SSH_MSG_IGNORE`); "Optimize connection buffer size" left out (`russh` sizes its own windows).
- **Tests**: `test_proxy` (SOCKS4, SOCKS5, HTTP, Telnet proxies in the process) and the tests' server's
  direct-tcpip channels (the tunnel). The local command is not run by the tests (no portable relay
  command on the machine): its fields are.

## Addendum (S6.5i, 2026-11-18): SSH, KEX, Authentication; Bugs left out
- **Settings** (`ssh_options`) under WinSCP's names and defaults: `Compression`, `Cipher` (WinSCP's list of
  `aes`, `chacha20`, `aesgcm`, `3des`, `WARN`, `des`, `blowfish`, `arcfour`, kept as `SetAlgoList` keeps it),
  `KEX` (WinSCP's 13 names), `RekeyTime` (minutes), `RekeyBytes` ("1G"), `SshNoUserAuth`, `TryAgent`,
  `AuthKI`, `AuthKIPassword`.
- **Applied**: `russh`'s preferred ciphers and key exchanges in the session's order, each WinSCP name
  standing for the `russh` algorithms of its kind (AES: the CTR and CBC sizes in PuTTY's order; AES-GCM;
  ChaCha20; ECDH: Curve25519 and the NIST curves; DH group exchange; groups 14-18 and 1; ML-KEM with
  Curve25519), compression (`zlib@openssh.com`, `zlib`) only when on; its rekey limits from the KEX tab
  (`russh` rekeys after 1 GB at most); the `none` method first when authentication is bypassed; Pageant only
  when asked; keyboard-interactive only when on, its first prompt answered by the password only when asked.
- **Differs from NetBox**: the names `russh` has nothing for (3DES - `russh`'s `des` feature is off -,
  DES, Blowfish, Arcfour, NTRU Prime, ML-KEM with NIST, RSA key exchange) stay in the lists but are not
  offered; "-- warn below here --" is kept as a line without PuTTY's question (`russh` does not tell which
  algorithm was chosen before the session is up); left out: "Enable legacy use of single-DES in SSH-2",
  GSSAPI (ADR-0023 §6), "Allow agent forwarding" (the commands' channels do not forward the agent), the
  detached certificate; the Bugs tab (PuTTY's workarounds for broken servers; `russh` has none of them,
  and rekeying follows the KEX tab).
- The tab bar reaches its eleven SSH tabs by Ctrl+PgUp / Ctrl+PgDn round them, scrolling as NetBox's.

## Addendum (S6.5j, 2026-11-18): the commands menu, Apply command, information
- **Alt+Shift+W** shows NetBox's "NetBox commands" with what this version has: attributes, link, "Apply
  command", "Server and protocol information", "Display session log file" (disabled: no session log is
  kept, NetBox's state while logging is off), "Home directory", "Configure". The other items come with
  their steps (synchronizing, comparing, bookmarks, location profiles, edit history: S6.6d; the queue:
  S6.8); PuTTY, Pageant and PuTTYgen are put off (§6).
- **Ctrl+G "Apply command"** as WinSCP's `TRemoteCustomCommand`: the patterns `!!`, `!`, `!&`, `!/`, `!@`,
  `!U`, `!P`, `!?prompt?default!` (asked in turn), once with `!&` else per file, folders with "Apply to
  directories", their items with "Execute recursively", each run in the panel's folder by the session's
  commands (SFTP's too, WinSCP's secondary shell); "Show results in console" opens the output in the viewer
  (the program has no console to show the server's output in), "Copy results to clipboard"; the command
  and the choices kept in the plugin's settings (`ApplyCommandCommand`, `ApplyCommandParams` with
  WinSCP's `cc*` bits). "Local command" comes later (disabled).
- **Information** shows the session and file protocols, the host key's SHA256 fingerprint (kept when the
  key is checked) and the capabilities, with "Copy to Clipboard"; not the cipher and compression chosen
  (`russh` does not tell them) nor the space available.
- With this the S6.5 rows end; the session dialog's FTP, S3, TLS and WebDAV tabs come with S6.6.

## Addendum (S6.6a, 2026-11-18): FTP and FTPS
- **Libraries** as §1 chose: `suppaftp` 13 (tokio, rustls with `ring`; its `deprecated` feature for the
  implicit TLS connect), `rustls-native-certs` for the system's roots, `ring` (already the TLS and SSH
  backend) for the certificates' fingerprints; the tests' `libunftp` with `unftp-sbe-fs` (ring only).
- **The file system** (`ftp`, `ftp_listing`): `Fs`'s FTP side over one control connection behind a lock;
  MLSD's facts when `FEAT` has it, else LIST read as Unix's `ls -l` with the time zone offset; RETR with REST
  (resuming), STOR, APPE (Append and resuming uploads), MKD, RMD, DELE, RNFR / RNTO, `SITE CHMOD`, MFMT;
  `OPTS UTF8 ON`; binary type. A file's data connection opens at its first read or write, a seek before it
  setting where a download starts. Passive mode by default ("Passive mode" on the Connection tab,
  `FtpPasvMode`), active mode otherwise; passive data connections through the session's proxy.
- **Login** (`ftp_login`): WinSCP's statuses; the password asked when the session has a user and none;
  WinSCP's anonymous login (`anonymous` / `anonymous@example.com`) without a user; a 530 refusal is "Access
  denied.". **TLS**: explicit (`AUTH TLS`) over the session's way, implicit straight (`suppaftp` opens it
  itself); the certificate checked against the system's roots (`ftp_tls`), one that does not verify asked
  with WinSCP's `MSG_VERIFY_CERT_PROMPT3` (its SHA-256 and SHA-1 fingerprints and why) - Yes keeps it in
  the plugin's `known_certificates`, No takes it this time, Cancel stops.
- **Differs from WinSCP**: no FTP through a local proxy command or an SSH tunnel (a TCP connection is
  needed for the control and data connections); "Duplicate" is not offered (one control connection carries
  no two transfers; WinSCP's `fcRemoteCopy` is off for FTP too); no links; the certificate's issuer,
  subject and dates are not shown (no X.509 parser among the libraries); the FTP tab's other options and the
  TLS tab come with S6.6e (done: addendum S6.6e).

## Addendum (S6.6b, 2026-11-18): WebDAV
- **Libraries** as §1 chose: `reqwest` 0.13 (no default features: `rustls-no-provider`, `stream`, `socks`)
  given the plugin's own rustls configuration (FTPS's certificate check and question, `ring`), `quick-xml`
  for the XML, `tokio-util`'s reader stream for uploads; the tests' `dav-server` (its local file system)
  served by `hyper` 1.
- **The file system** (`webdav`, `webdav_xml`): PROPFIND (depth 1 for a folder, 0 for an item, a folder
  asked again with its `/`; redirects not followed: they would turn PROPFIND into GET), its `multistatus`
  read (paths percent-decoded, `collection`, `getcontentlength`, RFC 1123 `getlastmodified`); MKCOL,
  DELETE, MOVE and COPY with `Overwrite: F` (COPY is "Duplicate": the server copies); GET with `Range` for a
  download started at an offset; PUT streamed from what the work writes; Basic authentication; the
  session's timeout; SOCKS4, SOCKS5 and HTTP proxies (`reqwest`'s), Telnet and local commands refused.
  WebDAV tells no rights and sets no times: the panel shows a writable file's or folder's rights, times and
  rights are not kept on upload (as WinSCP's WebDAV), uploads do not resume.
- **Login** (`webdav_login`): the password asked when the session has a user and none; the session's
  folder (the root when none) looked up first, HTTPS's unknown certificate asked about as FTPS's; 401 is
  "Access denied.".
- **Differs from WinSCP**: no Digest authentication or client certificates yet, the WebDAV tab's options
  (compression, liberal escaping) come with S6.6f.

## Addendum (S6.6c, 2026-11-18): S3
- **Own client, not an S3 library.** WinSCP drives libs3 over neon; the Rust choices were `rust-s3` and the
  AWS SDK. Both bring an HTTP stack and TLS setup of their own (the SDK a large tree of crates), so neither
  would share WebDAV's `reqwest` client with its certificate check and question, its proxies and timeout.
  The plugin signs requests itself (`sigv4`: AWS Signature Version 4, checked against AWS's published GET
  Object example) and reads S3's XML with `quick-xml` (`s3_xml`); the dependency set does not grow. The
  tests' server is `s3s` with `s3s-fs` 0.14 (later versions need a newer rustc) checking the signatures.
- **The file system** (`s3`, WinSCP's `TS3FileSystem`): the root lists the buckets (ListBuckets); a
  bucket's keys are folders by their `/`s (ListObjectsV2 with the delimiter, page after page by the
  continuation token); an item by HEAD, a folder by its `dir/` object or its prefix, a bucket by HeadBucket;
  GET with `Range` for a download started at an offset; PUT streamed with the length known first; F7 a
  bucket at the root, else an empty `dir/` object; rename of a file is CopyObject and DELETE (a folder:
  "Operation not supported.", as WinSCP's `NotSupported`); no rights or times kept. Virtual-host
  addresses (`bucket.host`), path-style ones for an IP address or a single name. The keys are signed for
  libs3's default region `us-east-1`; a bucket elsewhere answers with its region (`x-amz-bucket-region`, the
  error's `Region`) and is asked again there, remembered for the session (WinSCP's "Unknown bucket, will
  detect its region").
- **Login** (`s3_login`): the access key ID is the session's user and the secret its password, asked when
  missing (WinSCP's `S3_ACCESS_KEY_ID_PROMPT`, `S3_SECRET_ACCESS_KEY_PROMPT`); the session's folder (the
  root: its buckets listed) looked up first, HTTPS's unknown certificate asked about as WebDAV's (the loop
  now shared, `webdav_login::certified`); `AccessDenied`, `InvalidAccessKeyId`, `SignatureDoesNotMatch` and
  403 are "Access denied.". `s3plain://` is HTTP, `s3://` HTTPS (WinSCP's `ParseUrl`); NetBox's prefixes have
  `s3:` only.
- **Better than WinSCP**: "Duplicate" of a folder copies its objects through the client (WinSCP refuses:
  `DUPLICATE_FOLDER_NOT_SUPPORTED`, "Use a duplication via a local temporary copy"); its files still by
  CopyObject. The duplicate's loop now leaves to the client only the names the server cannot copy.
- **Differs from WinSCP, for S6.6f**: the S3 tab (default region, URL style, requester pays, session token,
  role, `~/.aws` profiles and credentials from the environment) and S3's ACLs and tags; multipart uploads
  (an object up to 5 GB is one PUT).

## Addendum (S6.6e, 2026-11-18): the FTP(S) and TLS/SSL tabs
- **The tabs** (`session_protocol_tabs`, settings in `protocol_options` under WinSCP's names and defaults):
  their items follow the SSH tabs' (no item before them moves), the bar shows them in NetBox's order (`there`:
  FTP(S) before Connection for FTP; TLS/SSL after Proxy for FTPS, WebDAV over HTTPS and S3). "Passive mode"
  moved from Connection to FTP(S), where NetBox has it; FTP through a proxy checks it with NetBox's
  `NB_FTP_PASV_MODE_REQUIRED`. `TAutoSwitch` lists are Auto / Off / On (`2 -` the value).
- **Applied as WinSCP's FileZilla engine does** (`ftp_connect`): MLSD by `FEAT` (Auto), never or always;
  `LIST -a` settled by the first listings (WinSCP's `FListAll`: an error or an empty listing drops it, empty
  again without it reverts to Auto); the passive reply's address or the server's (`CheckForcePasvIp` with
  `IsRoutableAddress`; through a proxy the host name); HOST before the login when On (Auto is off, as
  `OPTION_MPEXT_HOST`); the login done by the plugin (USER, PASS, ACCT on 332 with the account or
  "default": `suppaftp` stops at 332); the commands after the login (`DoStartup`, a reply other than 2xx /
  3xx fails the session); "Allow empty password" asks none. The client certificate (`TlsCertificateFile`,
  PEM: the chain and the key) is offered by FTPS, WebDAV and S3 (the certificate loop of WebDAV's login now
  serves FTP too); "Session ID re-use" off disables rustls' resumption.
- **Differs from NetBox**: "Display all file revisions on VMS servers", "Dup FF in commands" and "UnDup FF
  from PWD" are kept in the session but disabled (no VMS listing is read; commands and replies are UTF-8,
  which has no 0xFF byte); the certificate's "..." browse is put off as the private key's (§6); a .pfx /
  .p12 client certificate is not read (PEM only: no PKCS#12 parser among the libraries); the TLS versions
  stay hidden as NetBox hides them (rustls offers TLS 1.2 and 1.3).

## Addendum (S6.6f, 2026-11-18): the WebDAV and S3 tabs, Digest, multipart uploads
- **The tabs** (`session_protocol_tabs`, `protocol_options`' `S3` and `WebDav`): their items after the
  TLS/SSL tab's, shown in NetBox's order (S3 before Connection, WebDAV last). WebDAV's "Enable compression"
  is WinSCP's one `Compression` (the SSH tab's too) and its "TLS client certificate:" the FTP(S) tab's
  `TlsCertificateFile`: a setting on two tabs takes the line that was changed.
- **WebDAV** (`webdav`, `digest`): Basic first (as before; since the S6.6 review no login before the
  server's 401, see that addendum), and when the server answers 401 with a Digest
  challenge it is taken and the request sent again with its answer (RFC 2617 / 7616: MD5, MD5-sess,
  SHA-256, `qop=auth`, the nonce count, a client nonce from `ring`), later requests answering it at once
  (neon's way; checked against RFC 2617's and RFC 7616's examples). "Enable compression" asks listings and
  whole downloads gzipped (`Accept-Encoding: gzip`) and unpacks them with `flate2` (a ranged download is
  never asked gzipped). `md5` and `flate2` are russh's already: no crate is added (`reqwest`'s own gzip
  would add `async-compression`).
- **S3** (`s3`, `s3_login`, `s3_multipart`): the default region signs the requests (empty: libs3's
  `us-east-1`), "URL style: Path" asks path-style addresses, "Requester pays" adds the signed
  `x-amz-request-payer: requester`. A file over 5 MiB goes up in parts as WinSCP's `TS3FileSystem::Sink`
  (`S3MinMultiPartChunkSize`, `S3MaxMultiPartChunks`): CreateMultipartUpload, UploadPart, CompleteMultipartUpload
  (its error in a 200's body read), AbortMultipartUpload when a part, the completion or the work fails.
- **Differs from NetBox**: the default region is typed in an edit line with its history (NetBox's editable
  list of regions is a combo box the program's dialogs do not have; an edit also takes other services'
  regions); "Tolerate non-encoded special characters in filenames" is kept but disabled (the names are
  always taken as they come, encoded or not - neon's strict mode only fails a listing); the session token,
  role, profiles and credentials from the environment stay hidden as NetBox hides them.

## Addendum (S6.6d, 2026-11-18): bookmarks, Open directory, edit history, Generate URL, cleanup
- **Split**: the roadmap's S6.6d keeps bookmarks, Open directory, the edit history, Generate URL and the
  cleanup; synchronized browsing, Compare directories and Location profiles are S6.6g, Synchronize and Full
  synchronize S6.6h (the program's panels API already moves and selects in the other panel).
- **Open directory** (`bookmarks`, `open_directory`): a session's bookmarks by its session key (the stored
  session's name, else `user@host`) in the plugin's `Bookmarks` file (NetBox keeps them in its
  configuration); the path history is the panel's (`Remote::moved_to`: the folder left kept last, the one
  come to out of it, as `FPathHistory`). The menu is NetBox's with its break keys; NetBox's own swap of the
  lines shown and the paths taken (`BookmarkItems` / `BookmarkPaths` for bookmarks) is not repeated.
  Ctrl+Enter sets the command line's text (the program's way; NetBox inserts at the cursor).
- **Edit history**: the files the editor got in the session (`Remote::edited`); one chosen is edited again
  through the panel - its folder, the cursor on it, F4 posted (the program's `post_keys`) - so the save
  goes back to the server as any edit's does; a file gone from the server is not opened (NetBox's).
- **Generate URL** (`generate_url`): WinSCP's `GenerateSessionUrl` of the connected session with the
  folder it is at; the host key is the connection's SHA-256 fingerprint (the session keeps no expected
  key), URL-safe (`Base64ToUrlSafe`). The Script tab (commands for NetBox's console program) is left out;
  "Create session" and "Include raw settings" are shown disabled (NetBox's FAR build ignores them).
- **Cleanup Configuration** (`cleanup`): "Configuration" sets the transfer, editor and endurance settings
  back to their defaults and removes the bookmarks (the program's settings have no removal), "Stored
  sessions" the sessions' folder, "Caches" the known host keys and certificates; NetBox's INI file and
  random seed file have no counterpart.

## Addendum (S6.6g, 2026-11-18): synchronized browsing, Compare directories
- **The panel's moves** (`Remote::noticed`, `seen`): the program reads a plugin's folders for the panel and
  also for Find, a folder's size and the like, and its panel state shows no reading to the plugin's call, so
  a reading is no move. The panel's move is the folder its title is asked for (only the panel's own folder
  is drawn): noticed there, taken at the panel's synchro (`go_to_start`) - the path history (S6.6d's, which
  took every reading before) and synchronized browsing follow it. The first reading still sets the folder.
- **Synchronized browsing** (`browsing`, `browsing_panel`): NetBox's rules - the local path follows the
  server's into subfolders or up as many levels, elsewhere it stops; the other panel is set at the synchro
  (not inside a reading), a missing folder made when the user says so (`ForceDirectories`), browsing off
  with `NB_SYNC_DIR_BROWSE_ERROR` when it cannot follow. NetBox's "Never ask me again" on the on / off
  message is left out (the program's messages have no check box, as in S6.5b).
- **Compare directories**: WinSCP's `SynchronizeCollect` as NetBox uses it for comparing - the first level's
  files (folders left out), a file only on one side or modified at another second differs (names matched
  without case, Windows's); NetBox's `CompareBySize` (off by default, in no dialog) is not offered. The
  differing files are selected in both panels (the panels API's selection) and counted in NetBox's message.
- **Location profiles** move to S6.6i.

## Addendum (S6.6h, 2026-11-18): Synchronize
- **Comparing** (`sync_collect`): WinSCP's `DoSynchronizeCollectFile` rules over both folders' trees read
  whole first (`sync_work`: the disk's by `read_dir`, the server's by its raw listings, a link not
  entered): which side changed by time (to the second) - mirroring or timestamps toward the target whatever
  side is newer - or by size; a new item downloaded / uploaded when the mode takes that way, deleted on the
  target with "Delete files", unchecked with "Existing files only"; a file and a folder of one name left
  alone; names matched without case; the checklist sorted by its paths.
- **The dialog** (`sync_dialog`): NetBox's items placed as NetBox places them, its `UpdateControls`
  rules, the options kept as WinSCP's `SynchronizeParams` (`sp*` flags) and `SynchronizeModeAuto` (the
  direction kept only when the user agrees, NetBox's `NB_SAVE_SYNCHRONIZE_MODE`); "Synchronize timestamps"
  disabled where the server sets no times (WebDAV, S3: NetBox's `fsoDisableTimestamp`).
- **Doing it** (`sync_panel`, `Work::Synchronize`): the checked items through the copying's `Job` -
  uploads, downloads, deletions on the server, the disk's deletions, times set for timestamps - in the
  copying's window, a file there overwritten without asking (`Job::overwrite_all`, WinSCP's
  `spNoConfirmation`; transfers stay resumable, unlike the editor's `temporary`); both panels read again.
- **Differs from NetBox**: the checklist is the program's menu of checked lines (Space / Ins, "+" / "-",
  " N of M " below; NetBox's list dialog with columns, maximize and custom commands is not there); no
  transfer settings group in the dialog (the default transfer settings apply); the comparison has no
  progress window of its own (the program's busy message, Esc stops the server's reading). "Keep Remote
  Directory Up To Date" (watching the disk's folder) is S6.6j.

## Addendum (S6.6i, 2026-11-18): Location Profiles
- **Bookmarks as WinSCP's `TBookmark`** (`bookmarks::Mark`): a name, a remote and a local folder; the
  `Bookmarks` file's lines grow to `key<TAB>remote<TAB>local<TAB>name` (S6.6d's two-field lines still
  read, named after their folder), the shared profiles under the empty key. Open directory works on the
  session's marks (its bookmarks are their remote folders), so both see one list, as in NetBox.
- **The dialog** (`location_profiles`): NetBox's items placed, the list shown by "Session profiles" /
  "Shared profiles" (radio buttons where NetBox has tab buttons), the folders' lines read only, Add /
  Remove (with NetBox's question) / Rename (a name taken refused) keeping the focus in the list, Open
  closing with both panels taken to the folders (the disk's only when the bookmark has one); OK and Open
  keep the lists.
- **Better than NetBox**: Add names the panels' folders now; NetBox's takes the read-only lines, which show
  the bookmark under the cursor (or nothing), so its new bookmark repeats another or is empty.

## Addendum (S6.6j, 2026-11-18): Keep Remote Directory Up To Date
- **No idle time in a plugin's dialog**: NetBox's dialog stays open and synchronizes from FAR's idle
  events (`DN_ENTERIDLE`) with WinSCP's `TSynchronizeController` on directory change notifications. The
  program's dialogs give a plugin no idle event, and adding one (and a notification thread) to the API is
  not worth this one command. So the dialog takes the options and Start, and the watching runs as the
  copying's work (`Work::KeepUpToDate`, `Job::keep_up_to_date`): the disk's folder read again each second
  (`sync_work::local_tree`, trimmed to the first level without "Update subdirectories" and to the
  selection), and on a change compared with the server's (`sync_collect`, to the server, by time, its
  "Delete files" and "Existing files only") and the differences sent without questions; "Synchronize on
  start" does it once first; a listing failure asks WinSCP's `LIST_DIR_ERROR` question; Esc in the window
  stops it. The options are kept as WinSCP's `SynchronizeParams` flags and `SynchronizeOptions`.
- **Also in this step**: the program's test `a_plugin_unloads_another_whose_requests_are_dropped` waits for
  the plugin's log line instead of reading the log at once - the gate's flaky test, caught under load (the
  log is written as the plugin's call ends, which can come after the other plugin is let go).

## Addendum (S6.6 review, 2026-11-18): the package's review and its fixes
Three `cg-reviewer`s over a55c476..41a21ab (protocols and logins; the panel, its transfers and the program's
side; the commands menu and the session dialog). Fixed:
- **Retries write once**: a download resumed or appended and tried again (Retry after a failure part-way)
  cuts the disk's file back to where it stood first (it kept the failed attempt's bytes before the rest);
  the times and the read-only mark of a download, the rights and times of an upload are a step retried on
  their own once the file is in place (a retry of the whole wrote a complete file again or failed on a
  temporary name gone).
- **Keep up to date never compares a failed read**: the disk's folder that cannot be read (a subfolder
  gone between its listing and its reading, no right, the share away) is passed by for the round, the user
  asked WinSCP's `LIST_DIR_ERROR` after five in a row; it was read as empty and, with "Delete files",
  emptied the server's folder.
- **Names from a server stay in their folder**: a listing's name that is empty, "." or "..", or has a "/"
  or a NUL is no item (WinSCP's `TTerminal::IsValidFile`, for every protocol: WebDAV and S3 kept ".." and
  S3 keys outside the prefix); a downloaded name with "\", "/" or ":" or only dots is written with
  WinSCP's tokens even when "Replace invalid characters" is off (unlike WinSCP: never a path out of the
  target folder).
- **The panel's silent reopening tries once per reading** (it tried without end with "Maximum number of
  retries" 0, and a latched Esc made each try stop at once: the plugin's thread never came back).
- **A session file opened from anywhere** (Enter or Ctrl+PgDn on a `*.netway` file outside the store)
  with a local proxy command asks before running it (NetBox connects at once).
- **WebDAV sends no login before the server's 401** (neon's way): then Digest when offered, else Basic;
  a second refusal of Basic is the answer (Basic was sent first, in clear over HTTP).
- **Times compared at the server's precision**: `ls -l`'s "hh:mm" (FTP's LIST, SCP) is to the minute, its
  year to the day (`sftp::Precision`, WinSCP's `TModificationFmt`); Synchronize, Keep up to date and Compare
  directories cut the disk's time to it first (`ReduceDateTimePrecision`; the day in UTC). Every file
  differed on such a server.
- **Synchronize**: names matched without case for any letter (`same_name`, WinSCP's `SameText`); a
  deletion is listed unchecked without "Delete files" (WinSCP's `Checked = spDelete`; none for
  timestamps), and without the preview only checked items count for "No differences found."; "Selected
  files only" takes either panel's selection (NetBox's `SynchronizeAllowSelectedOnly`).
- **Moves a command makes** (Open directory, Home directory, Location profiles, Edit history) are not
  followed by synchronized browsing (NetBox's `ChangeDirectory` past `SetDirectoryEx`): `Remote.unfollowed`.
- **Bookmarks**: Del removes the one bookmark of the folder (it removed every mark of it, profiles too);
  "Add bookmark" adds a name once (`TBookmarkList::Insert`); a tab, a line end or "%" in a field is kept as
  `%09`, `%0A`, `%0D`, `%25` (it moved the fields or split the line).
- **"Server and protocol information"** tells each protocol's own lines (it said SSH-2 / SFTP-3 for FTP,
  WebDAV and S3) and what each can do.
- **Smaller ones**: "New name" with the recycle bin bins (and keeps the rights of) what is under the new
  name; "Edit link" makes the new link only once the old one went; a command killed by a signal or ending
  with no status is a failure (SCP's `rm`, `mv`, `cp`); a local proxy command that fails to start is told by
  its template (its `%pass` was on the screen); implicit FTPS's connection within the session's timeout;
  an S3 key outside the prefix asked for no longer panics.
- **The program's side**: a plugin's box (`ITEM_BOX`) reaching past the screen is cut, not a panic of the
  program; a copy saved from the editor's "Save" as it closes waits for that save's put to end (a second put
  said "the edited copy is kept" and left its folder).
Known and left (differ from WinSCP / NetBox):
- A lost WebDAV or S3 session has nothing to open again (HTTP connects anew as needed); FTP's is opened
  again since S6.6k (its addendum). The SSH one is
  checked before an operation: a connection that breaks while one runs (or before russh has heard of it)
  fails that operation, the next one reopens it (WinSCP's `QueryReopen` also retries the failed one).
- The SSH handshake and login have no timeout of their own (the host key's question is asked inside
  russh's handshake; a timeout would run while the user reads it): a server that takes the connection and
  says nothing waits for Esc.
- A multipart S3 upload stopped by Esc is not aborted on the bucket (it is when a part or the completion
  fails); its parts stay until the bucket's lifecycle rule removes them.
- A Telnet or local proxy command's `\xNN` of 0x80 or more goes as its UTF-8 bytes (PuTTY sends one byte).
- A session file's key or certificate path on a share (UNC) is read as given, as NetBox does.

## Addendum (S6.6k, 2026-11-18): FTP sessions opened again
- **Found before the operation, not by its failure**: WinSCP learns that an FTP server dropped the
  control connection from the next command's failure (`QueryReopen` on a fatal error, the command tried
  again). Here the reopening runs before the panel's operations and readings (S6.5e), so the control
  connection is asked instead: once it was not used for 10 s, a NOOP (within WinSCP's default 15 s
  timeout); no answer (a 421, a closed connection, the time out) and the session is lost - for good, so the
  next look asks nothing - and opened again as SSH's is: "Lost connection." or its countdown, the login
  with the same user and password at the panel's folder, "Maximum number of retries". A connection used
  less than 10 s ago is taken as alive (no NOOP per panel's reading). An operation already running when
  the connection drops still fails as before (the next one reopens).

## Addendum (S6.8a, 2026-11-18): background transfers through the program's engine, API 9
- **Owner's choice (2026-11-18)**: NetWay's background transfers are the program's own background tasks
  run by its engine (ADR-0010's tasks: the progress dialog with Background, the queue, F12's list, the
  program's overwrite questions and retries) reading and writing through the plugin's file calls
  (ADR-0016 §13), not NetBox's queue re-made in the plugin. What NetBox's transfer settings add there
  (resume through `.filepart`, the text mode, the rights) is not in the background copy; the owner may
  ask for it later.
- **API 9** (any change of the ABI is a new version, every manifest "api": 9):
  - `NcHost::transfer(instance, folder, names, count, disk, flags)` with `TRANSFER_UPLOAD`,
    `TRANSFER_MOVE`, appended after `data_folder`: the entries of the panel's folder copied (moved) into
    the disk's folder, or the disk's entries into the panel's folder, as one of the program's tasks in
    front; asked from the plugin's thread (its get / put), answered at once (0 started, -1 nothing to
    copy). The SDK's `Host::background_transfer(&Transfer)`. The program: `PanelRequest::Transfer`,
    `app::plugins::background` (the request, `VfsPath::in_session` for the panel's side), `start_in_front`
    (what "Retry skipped" did).
  - `WRITE_END` (`NcPlugin::write`'s third mode, no data): the program's writer of a plugin's file let go
    (the file whole, or its copy stopped) tells the plugin, so one that streams a file to a server (FTP's
    STOR, SFTP's handle, a WebDAV PUT) finishes it; the SDK's `Plugin::write_end` (by default nothing).

## Addendum (S6.8b-c, 2026-11-18): NetWay's file calls and "Transfer on background"
- **The program's file calls** (`background_panel`, `background_files`): `read`, `write`, `write_end`,
  `make_dir`, `rename` (an entry there replaced only when asked, never a folder), `remove`, `set_times` (the
  modification time where the protocol keeps one) on a connection of the panel's own, opened at the
  first call with the user and password that logged in, its questions refused (WinSCP's queue opens its
  own connections too); the panel stays free to browse. `stat` goes through the panel's own connection
  (the quick view asks every second: no reason to open another). A read going on where the last one
  stopped reads on, another place opens the file again there; a file written anew is streamed as it
  comes (SFTP, FTP, WebDAV) and finished at `WRITE_END`, or kept in a file of the disk's temporary folder
  and sent whole at its end where the protocol must know its size first (SCP's record, S3's PUT or its
  multipart plan).
- **"Transfer on background (add to queue)"** (NetBox's `NB_TRANSFER_QUEUE`, WinSCP's `Queue`, kept with
  the transfer settings as `Queue`): checked in NetBox's copy dialog (not the viewer's and the editor's),
  the copy is handed to the program (`Host::background_transfer`) instead of NetWay's own work; an upload
  is handed by the sources' folders. Not there (documented in far-parity): NetBox's queue list and its
  "No confirmations", "Transfer on background by default", NetBox's transfer settings in the background
  copy. Stand `e2e_netway_background_body` (a download and an upload by the program's task).

## Addendum (S6.8 review, 2026-11-18): a file's end or its abandonment; the background connection kept up
The stage's review (cg-reviewer over 8c1ed9a^..178e495) found that a stopped copy sent no end (a stop
sends no call) and a failed one sent `WRITE_END`, so a kept file (SCP, S3) went up partial at the next
upload or stayed in the temporary folder; that the background connection was never opened again; that
Esc did not stop the plugin's waits. Fixed:
- **API 9's `WRITE_ABORT` (3)** beside `WRITE_END` (API 9 was not yet released: amended, no new
  version): the program's writer tells the file whole at its flush - the copy's last step, a failure of
  it the copy's - and abandoned when let go unflushed (failed or stopped; sent past the stop, as the
  cleanup's removal is). SDK `Plugin::write_abort` (by default nothing). NetWay lets go of what it kept
  (a spool removed, a stream dropped), never sends it; a file made anew while another is open lets that
  one go the same way, a removal of an open file too, and the session's end removes its spool.
- **The background connection kept up**: before a call, its SSH connection closed or - after a pause of
  ten seconds or a failed call, never while a file is written - an FTP one no longer answering a NOOP
  is opened again; a call stopped halfway drops it (what the server still sends for it is nobody's).
  A read or a write taken out while done: one stopped halfway keeps nothing whose place is not known.
- **Esc** stops the waits of the background calls and of `stat`; `stat` reopens the panel's own
  connection first (the panel's silent reopening) and says not found only when the protocol does
  (SFTP's status; the others' texts cannot tell, as the panel's copy takes them).
- **An upload of sources from several folders** is one task named from their common folder (two tasks
  started at once collided on the program's single wait); sources on two disks are refused.
- Not a finding: an FTP data connection dropped unfinished leaves its reply for the next command, which
  suppaftp 13 reads first (`TransferStream`'s drop).
