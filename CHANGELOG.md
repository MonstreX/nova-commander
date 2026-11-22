# Changelog

## 0.1.0-alpha.2 - 2026-11-22

- The program's own icon and version information in its executable.
- A command left running when its screen closed (the program exited with a command in the background, a
  background command removed) no longer stays behind: what the program waits for ends with it, while
  what a command started to go on running (`start notepad`) still does.
- Built with Rust 1.100; the dependencies on their latest versions.
- Linux (in progress, not released yet): local dates, permissions as far2l shows them, links to folders,
  change times and folders watched by inotify in the panels.
- CI on GitHub Actions.

## 0.1.0-alpha.1 - 2026-11-22

The first public build.

- Two FAR-style panels with FAR's view modes, sorting, highlighting, the drive menu, the user menu,
  associations, shortcuts, histories, Find file / Find folder, the tree panel, quick view, the info panel.
- File operations on worker threads with FAR's dialogs, progress and questions.
- The embedded terminal (ConPTY) and the guest shells.
- The viewer and the editor for files of any size: code pages, search and replace, bookmarks, vertical
  blocks, Colorer's syntax highlighting, hex editing, the "Code editing mode".
- Plugins through the C ABI (API 11): archive (7-Zip), netway (SFTP, SCP, FTP(S), WebDAV, S3),
  tmppanel, network, proclist, compare, emenu, commands, samefolder, filecase and the editor plugins.
