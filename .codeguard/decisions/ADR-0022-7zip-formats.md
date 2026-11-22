# ADR-0022: 7-Zip's formats through 7z.dll, as FAR's arclite

- Date: 2026-11-16
- Status: accepted (roadmap S6.3; ADR-0012 §7 and ADR-0021 §2 ask for this ADR as the step begins; owner
  2026-11-16: archives as FAR does them; owner 2026-11-16 on this ADR:
  zip moves to 7z.dll and the `zip` crate goes; the program's package ships 7z.dll as FAR's does)

## Context
ADR-0012 §7 has the `archive` plugin read and write every format 7-Zip knows, "through the 7-Zip library, as
FAR's arclite does", its finding, shipping and licence left to this ADR. S6.1-S6.2 made zip with the `zip`
crate (ADR-0021 §2), with deviations written down for what the crate cannot do: no LZMA, LZMA2 or Deflate64
written, no split volumes, no self-extracting archive, no ZipCrypto written, no 7-Zip open warnings.

arclite (`far-source/plugins/arclite`) has no codec of its own. It loads 7-Zip's `7z.dll` and drives it
through its COM-style interfaces (`IInArchive`, `IOutArchive`, their callbacks and streams):

- **Finding the library** (`archive.cpp`, `ArcAPI::load`): every `*.dll` of the plugin's own folder that
  exports `CreateObject` and either `GetNumberOfFormats` with `GetHandlerProperty2` or `GetHandlerProperty`
  (FAR ships `7z.dll` there); when none is, `7z.dll` in the folder of `HKCU\Software\7-Zip`'s `Path`, else
  `HKLM`'s; when still none, `7z.dll` found on `PATH`. Then `Formats\*.format` and `Codecs\*.codec` beside
  it. The SFX modules (`*.sfx`) the same way: the plugin's folder, else 7-Zip's.
- **The formats** come from the library (`GetHandlerProperty2`): class id, name, extensions and the nested
  extension (`tgz` -> `tar`), signatures and their offset, updatable, flags (`kKeepName`, `kFindSignature`,
  `kPreArc`, `kByExtOnlyOpen`, ...), `GetIsArc`. The newest library wins a class id two of them give.
- **Opening** (`open.cpp`, `Archive::detect` and `Archive::open`): the file's first bytes searched for every
  enabled format's signatures (a zip local header after the start accepted only when the file's end has no
  end record of another volume); the formats of its extension put first; the rest after, but those opened
  by extension only; UDF before ISO, RAR before Split, DMG before HFS. Each is tried in turn (at a
  signature's offset through a sub-stream, `kPreArc` with `IArchiveAllowTail`), and the first that opens
  is opened again inside itself while it names one main file (`tar.gz` shows the tar). "Open (format menu)"
  lists every way the file opens.
- **Everything else** goes through the same interfaces: listing (`GetNumberOfItems`, `GetProperty`),
  extraction and test (`Extract` with `IArchiveExtractCallback`, a solid block in one pass), the password
  (`ICryptoGetTextPassword`, `...2`), writing (`UpdateItems` with `IArchiveUpdateCallback`, the level,
  method, solid and the advanced parameters through `ISetProperties`, volumes through `GetVolumeSize` /
  `GetVolumeStream`), deleting and new folders (`UpdateItems` without them / with one), SFX (a module put
  before a 7z archive), the formats' part of the configuration ("Enable / Disable archive formats",
  "Generate masks", "Default masks", "7z library info", "7z library not found").

The owner's machine has 7-Zip 26.03 x64 (`C:\Program Files\7-Zip`, both registry keys set), the version FAR's
build ships (`makefile_vc`'s `VERSION_7Z=26.03`, downloaded from FarGroup's `thirdparty` into arclite's folder).

## Decision
1. **7z.dll, as arclite uses it.** Every format, zip included at the end of S6.3, is read and written by
   7-Zip's library through its interfaces, found as arclite finds it (its folder's `*.dll`, the registry's
   7-Zip, `PATH`; `Formats\*.format`, `Codecs\*.codec`; the SFX modules). The plugin's folder is
   `plugins\archive`. No codec of the program's own, no other archive crate. The `zip` crate goes when zip
   moves to the library (S6.3c), and with it ADR-0021's zip deviations.
2. **The binding lives in `nc-plugin-sdk`** (`sevenzip*` modules), the only crate besides the program's
   own boundary where `unsafe` may live (`unsafe-only-in-platform`; proclist's and network's system calls
   set the pattern). It loads the library through `LoadLibraryW`, declares the interfaces as `repr(C)`
   tables in the order 7-Zip's `IArchive.h`, `IStream.h`, `IPassword.h`, `IProgress.h` and `ICoder.h` give
   them (their ids and layout; no 7-Zip source is copied), implements the callbacks and streams the
   library calls (`IInStream` over a file, `ISequentialOutStream`, the open, extract and update callbacks,
   the password) and frees `PROPVARIANT` / `BSTR` as 7-Zip's Windows build expects. The plugin sees safe
   types: the formats, an opened archive, its items and properties, extract / test / update requests with
   Rust callbacks for progress, the password, the file to write and its answer. The plugin keeps no
   `unsafe`.
3. **Licence.** 7-Zip's `7z.dll` is GNU LGPL 2.1 or later, part of it with the unRAR licence restriction
   (its RAR decoder may not be used to make a RAR compressor: the program only reads RAR, as 7-Zip does),
   part BSD. The program (MIT) loads the unchanged library at run time and takes none of its source, so
   its own licence is unchanged and nothing GPL comes in. Where the program ships `7z.dll` (beside the
   plugin, as FAR does), it ships 7-Zip's `License.txt` with it and says where its source is
   (7-zip.org), as that licence asks.
4. **Shipping.** S6.3 builds no and downloads no library: it finds one as arclite does. The development
   machine's is 7-Zip's installed one. A package of the program puts `7z.dll`, the `*.sfx` modules and
   `License.txt` into `plugins\archive`, as FAR's distribution does (the step that makes a package does
   it; until then the plugin needs 7-Zip installed for anything but zip, and for zip too after S6.3c).
   When no library is found the plugin says arclite's "7z library not found" where arclite does (the
   configuration's "7z library info") and opens nothing.
5. **Steps.** Each leaves a working program; zip stays on the crate until the library does all it does.
   - S6.3a: the binding (finding, formats, open, list, properties) and browsing every format but zip:
     arclite's detection and chains, the open password and "Reading archive...", "Open (format menu)",
     the info panel and Ctrl+A's properties; F5 out of these archives not yet (the plugin says so).
   - S6.3b: extraction and test of every format but zip through `Extract` (F5 / F6 / Alt+F6, F3 / F4,
     Shift+F2 / Shift+F3, the menu's Extract / Test, `arc:x e t`), the password and the error log as for
     zip; the formats' part of the configuration and its code pages given to the library.
   - S6.3c: writing every updatable format but zip (7z, tar, gzip, bzip2, xz, wim) through `UpdateItems`:
     adding, new archives with "Select archiver", deleting, new folders, `arc:c u d`, the methods and
     levels of each format as arclite's dialogs list them, split volumes and opening them.
   - S6.3d: zip moves to the library for everything and the `zip` crate is removed with its deviations
     (split from S6.3c on 2026-11-17: each leaves a working program and a reviewable change).
   - S6.3e: self-extracting archives (Create's "Create self-extracting archive" and "SFX options", the
     menu's "Convert archive into SFX"), the library's open warnings and "Headers Error" as arclite shows
     them, what of arclite is still put off by then.
6. **Linux (S7).** 7-Zip's Linux build has the same interfaces in `7z.so` (its own `IUnknown` without
   Windows' and its own `BSTR`); where it is found and how its `IUnknown` differs are S7's (ADR-0017), the
   binding's interface tables written so that the difference stays in one place.

## Addendum (S6.3b, 2026-11-17)
- The SDK's `Archive::extract(indices, test, &mut dyn Extracting)`: 7-Zip's `Extract` with the
  program's `IArchiveExtractCallback` (and `ICryptoGetTextPassword`) and an `ISequentialOutStream` per
  item; the plugin's `Extracting` answers each item (`item`: its data wanted or not, `write`, `done` with
  its `Outcome`, `password`), reachable during the call only. The library calls back from threads of its
  own (a 7z folder of more than one coder, a BCJ / BCJ2 filter before LZMA as 7-Zip packs programs, runs
  them on its mixer's threads), and the program's host answers a plugin's own thread only (review R45), so
  the SDK runs the library's call on a thread it starts and serves every callback on the caller's thread,
  one at a time, the library's thread waiting (`sevenzip_marshal`): the plugin's `Extracting` (and S6.3c's
  update callback) is always called on the plugin's thread.
- The quick view's and Find file's reads keep what the library gave of the file (in memory up to 16 MB,
  past it in a temporary file deleted on close) and extract again from its start to at least twice as far
  when a read goes past it: a file read through is decoded a few times its size at most, its size told by
  the archive a hint only (bzip2 tells none, gzip's is modulo 4 GB).
- The plugin's extraction keeps its windows and its log; a zip goes on through the crate until S6.3c.
- `Library::set_code_pages`: arclite's `Patch7zCP` - `7z.dll`'s kernel32 imports of the code page
  functions answered by the SDK's own while the configuration's pages differ from the system's, the
  imports put back before the library is freed (a `7z.dll` loaded by another user of the process, as
  7-Zip's shell extension, must never call into an unloaded plugin). `Win32_System_Memory` joins the
  SDK's `windows-sys` features (`VirtualProtect`).

## Addendum (S6.3c, 2026-11-17)
- The SDK's `Archive::update(count, properties, &mut dyn Updating)` (the archive's `IOutArchive`) and
  `Library::create_archive(format, count, properties, &mut dyn Updating)` (`CreateObject` with
  `IID_IOutArchive`): the parameters set through `ISetProperties` first, then `UpdateItems` with one
  object of the program's answering `IArchiveUpdateCallback`, `ICryptoGetTextPassword2` and the output's
  `IOutStream`, served on the caller's thread as the extraction's ([`sevenzip_marshal`]). The plugin's
  `Updating` answers each item (`item`: kept by its index or new, `property`, `stream`: a `File` it
  opened, nothing for a folder, skipped, stop; `done`; `password`) and takes the new archive's bytes
  (`write`, `seek`, `set_size`) wherever it puts them: one file beside the archive, or volumes
  (arclite's `MultiVolumeUpdateStream`, in the plugin). A new item's data is read by the SDK's
  `InStream` on the library's threads.
- The roadmap splits S6.3c: c writing every format but zip, d zip moved to the library and the crate
  gone, e SFX, partitions, open warnings.

## Addendum (S6.3d, 2026-11-17)
- zip is read, extracted, tested and written through the library like every other format; the plugin's
  `zip` crate, its zip reader (`zip_format`, `central`, `reading`) and writer are gone, with ADR-0021's
  zip deviations. With no library found nothing opens, a zip neither (arclite's).
- The program's tests and e2e stands read what an archive was written with through the plugin's
  `support` module (`crc32`, `opened`, `names_in`); the plugin's own tests make their archives through
  the installed library (`test_archives`).

## Addendum (S6.3e, 2026-11-17)
- The SDK's `Library::sfx_modules`: the `*.sfx` programs found with the library as arclite's
  `find_sfx_modules` finds them (the plugin's folder's; else, with the library, those of the folder
  7-Zip's registry key names, else beside `7z.dll` on the search path). `system::resources`,
  `resource`, `update_resources` (a program file loaded as data, its resources of a type by name and
  language, one read, changes written in one `BeginUpdateResource` / `EndUpdateResource`): the plugin
  replaces an SFX module's icon and version information through them as arclite's rsrc.cpp does,
  unsafe-free.
- The plugin: `sfx` (the options, modules, settings), `sfx_rsrc` (icon file, icon groups, the version
  information's encoding), `sfx_module` (the module made, the conversion), `sfx_dialog`, `sfx_convert`;
  a new 7z after its module through the output `Shifted` (arclite's `SfxUpdateStream`); `partition`
  and `partition_panel` (arclite's `part_mode`); `open_results` (arclite's `read_open_results`).
- A partition is entered on a panel of its own over the disk's: the plugin takes Enter and Ctrl+PgDn on a
  disk's panel and answers with the new panel; the program opens a plugin's panel answered to Enter or
  Ctrl+PgDn on an entry over the panel, as a file's plugin is (FAR's `OpenFilePlugin`), ".." at its root
  back to the entry. No API change: the answer is the existing `KeyGo::Open`.

## Consequences
- `architecture.md` names 7-Zip's library under the plugins' run-time dependencies (loaded, not linked) and
  the SDK's `sevenzip` binding; the `zip` crate's line goes at S6.3c.
- The SDK's tests and the plugin's tests that need the library find it as the plugin does (7-Zip installed
  on the development machine); small archives of each format are kept as test data.
- far-parity §14's arclite line follows the steps; ADR-0021's zip deviations are struck as S6.3c makes them
  void.
- The API (5, final) does not change for S6.3: everything is inside the plugin and the SDK.
