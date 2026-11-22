# C4 level 1 - System context: nova-commander

> Architecture as code (Mermaid). Shows the system, its user and the external systems it talks to.
> nova-commander is a single container (one executable), so the container level is folded into this diagram.
> Update it in the same change that adds or removes a user type or an external system.

```mermaid
C4Context
  title System context - nova-commander
  Person(user, "User", "Navigates and manages files with hotkeys, runs commands in the embedded terminal")
  System(nc, "nova-commander", "Portable console file manager in FAR style (Rust, Windows now): two panels + embedded terminal; settings\settings.json and plugins\ next to the exe")
  System_Ext(host, "Host console", "Windows Terminal or conhost: font, window, keyboard input")
  System_Ext(fsys, "File system", "Local and network drives")
  System_Ext(shells, "Shells", "%COMSPEC% (cmd) runs each command line; cmd, Windows PowerShell, pwsh, WSL bash, Git Bash or the user's own as the guest shell")
  Rel(user, host, "types keys, reads screen")
  Rel(host, nc, "console input events / VT output", "Win32 Console API")
  Rel(nc, fsys, "lists, copies, moves, deletes", "std::fs / Win32")
  Rel(nc, shells, "runs command lines through its runner in, hosts the guest shell in", "ConPTY")
  System_Ext(assoc, "Associated programs", "Windows file associations (Enter on a non-executable file)")
  Rel(nc, assoc, "opens files with", "ShellExecuteExW")
  System_Ext(plugins, "Plugins", "Native DLLs in plugins\<name>\ (ADR-0004, ADR-0016: loaded since 5a; tmppanel 5d, samefolder and filecase P6.1, commands P6.2a, emenu P6.3, compare P6.4, proclist P6.5a, network P6.6a, archive S6.1a, netway S6.4a, the editor's align, autowrap, brackets, drawline, editcase S2d.7; loaded and unloaded while running P6.0e)")
  Rel(nc, plugins, "loads, unloads, calls", "C ABI")
  System_Ext(sevenzip, "7-Zip's library", "7z.dll: beside the archive plugin, else the installed 7-Zip's (registry), else on the search path (ADR-0022, S6.3a)")
  Rel(plugins, sevenzip, "the archive plugin loads it and reads archives through", "COM-style interfaces")
  System_Ext(remote, "Remote servers", "SFTP, SCP, FTP(S), WebDAV and S3 hosts, through the netway plugin (ADR-0023, S6.4-S6.8)")
  Rel(plugins, remote, "lists and transfers files", "SSH, FTP, HTTP(S)")
```
