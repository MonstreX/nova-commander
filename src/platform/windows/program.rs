//! Programs and documents: the shells Windows offers, what kind of program an executable file is (read
//! from its PE header), the command line that runs a program file, and opening a file with the program
//! Windows associates with it.

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;
use std::ptr::null;
use std::sync::OnceLock;

use super::wide_os;
use crate::platform::files::Attributes;
use crate::platform::process::{ProgramStart, ShellKind};

use windows_sys::Win32::UI::Shell::{
    SEE_MASK_FLAG_NO_UI, SEE_MASK_NOASYNC, SHELLEXECUTEINFOW, ShellExecuteExW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

/// `IMAGE_SUBSYSTEM_WINDOWS_GUI`.
const WINDOWS_GUI: u16 = 2;
/// The PE signature, the COFF file header and the optional header up to and with its subsystem.
const NT_HEADERS_TO_SUBSYSTEM: usize = 4 + 20 + 70;
/// The subsystem's place in the optional header (the same in PE32 and PE32+).
const SUBSYSTEM_OFFSET: usize = 4 + 20 + 68;

/// Whether `path` is a Windows GUI program (FAR does not wait for those); false for console programs,
/// scripts and anything that cannot be read.
pub fn is_gui_program(path: &Path) -> bool {
    subsystem(path).is_ok_and(|subsystem| subsystem == WINDOWS_GUI)
}

/// Opens `path` as Explorer does on a double click: a document with its associated program, a folder in
/// Explorer (FAR: `ShellExecuteEx` for files that are not programs). No command interpreter is involved,
/// so nothing in the name (`&`, `%VAR%`) can become a command.
pub fn open_with_association(path: &Path) -> io::Result<()> {
    // The shell's extensions are COM objects: COM on the thread that asks (it is a worker's).
    let _com = super::com::Apartment::enter();
    let file = wide_os(path.as_os_str());
    let folder = path.parent().map(|folder| wide_os(folder.as_os_str()));
    let mut info = SHELLEXECUTEINFOW {
        cbSize: size_of::<SHELLEXECUTEINFOW>() as u32,
        // As FAR: no message box of the shell's own (the caller reports the error), done on return.
        fMask: SEE_MASK_FLAG_NO_UI | SEE_MASK_NOASYNC,
        lpFile: file.as_ptr(),
        lpDirectory: folder.as_ref().map_or(null(), |folder| folder.as_ptr()),
        nShow: SW_SHOWNORMAL,
        ..Default::default()
    };
    // SAFETY: `info` is a fully initialised SHELLEXECUTEINFOW of the right size; its strings are
    // NUL-terminated and live until the call returns. No verb (the default one), no parameters.
    if unsafe { ShellExecuteExW(&mut info) } != 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

fn subsystem(path: &Path) -> io::Result<u16> {
    let mut file = File::open(path)?;
    let mut dos_header = [0u8; 64];
    file.read_exact(&mut dos_header)?;
    let [b'M', b'Z', ..] = dos_header else {
        return Err(io::Error::from(io::ErrorKind::InvalidData));
    };
    let new_header = u32::from_le_bytes([
        dos_header[0x3C],
        dos_header[0x3D],
        dos_header[0x3E],
        dos_header[0x3F],
    ]);
    file.seek(SeekFrom::Start(u64::from(new_header)))?;
    let mut nt_headers = [0u8; NT_HEADERS_TO_SUBSYSTEM];
    file.read_exact(&mut nt_headers)?;
    let [b'P', b'E', 0, 0, ..] = nt_headers else {
        return Err(io::Error::from(io::ErrorKind::InvalidData));
    };
    Ok(u16::from_le_bytes([
        nt_headers[SUBSYSTEM_OFFSET],
        nt_headers[SUBSYSTEM_OFFSET + 1],
    ]))
}

/// The shells offered on Windows, in menu order; cmd, the first, is always there.
pub fn shells() -> &'static [ShellKind] {
    &[
        ShellKind::Cmd,
        ShellKind::Powershell,
        ShellKind::Pwsh,
        ShellKind::Wsl,
        ShellKind::GitBash,
    ]
}

/// The shell when the saved one is missing or not installed.
pub const DEFAULT_SHELL: ShellKind = ShellKind::Cmd;

/// How the program file `name` is started from its folder (Enter on it, FAR's `<exec>` group): a
/// PowerShell script by a line for Windows PowerShell, other programs as they are; `None` for files that
/// are not programs.
pub fn program_start(name: &str, attributes: Attributes) -> Option<ProgramStart> {
    if name.to_lowercase().ends_with(".ps1") {
        return Some(ProgramStart::Line(format!(
            "powershell.exe -NoLogo -File \"{name}\""
        )));
    }
    is_program(name, attributes).then_some(ProgramStart::File)
}

/// The Windows folder (`%SystemRoot%`): a folder nobody moves or deletes, where a shell can wait without
/// holding a folder of the user's open.
pub fn system_folder() -> std::path::PathBuf {
    std::env::var_os("SystemRoot").map_or_else(|| "C:\\Windows".into(), Into::into)
}

/// Whether the file `name` is a program (FAR's `<exec>` highlighting group): its extension is one of
/// `exe, cmd, bat, com` or `%PATHEXT%`. Windows asks nothing of the attributes.
pub fn is_program(name: &str, _attributes: Attributes) -> bool {
    let Some(extension) = Path::new(name).extension() else {
        return false;
    };
    let extension = extension.to_string_lossy().to_lowercase();
    executable_extensions().contains(&extension)
}

/// Lower-case extensions (without the dot) that Windows runs as programs: the FAR `<exec>` group,
/// `exe, cmd, bat, com` plus `%PATHEXT%`.
fn executable_extensions() -> &'static [String] {
    static EXTENSIONS: OnceLock<Vec<String>> = OnceLock::new();
    EXTENSIONS.get_or_init(|| {
        let mut list: Vec<String> = ["exe", "cmd", "bat", "com"].map(String::from).to_vec();
        let path_ext = std::env::var("PATHEXT").unwrap_or_default();
        for extension in path_ext.split(';') {
            let extension = extension.trim().trim_start_matches('.').to_lowercase();
            if !extension.is_empty() && !list.contains(&extension) {
                list.push(extension);
            }
        }
        list
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn system_file(relative: &str) -> std::path::PathBuf {
        let root = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".to_string());
        Path::new(&root).join(relative)
    }

    #[test]
    fn tells_gui_programs_from_console_programs() {
        assert!(is_gui_program(&system_file("explorer.exe")));
        assert!(is_gui_program(&system_file("System32\\notepad.exe")));
        assert!(!is_gui_program(&system_file("System32\\cmd.exe")));
    }

    #[test]
    fn a_missing_file_is_reported_and_nothing_is_opened() {
        let temp = tempfile::tempdir().unwrap();
        let error = open_with_association(&temp.path().join("a&calc %PATH%.txt")).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::NotFound);
    }

    #[test]
    fn other_files_are_not_gui_programs() {
        let temp = tempfile::tempdir().unwrap();
        let script = temp.path().join("run.bat");
        std::fs::write(&script, "@echo off\r\n").unwrap();
        assert!(!is_gui_program(&script));
        assert!(!is_gui_program(&temp.path().join("missing.exe")));
    }

    #[test]
    fn executable_extensions_include_the_far_defaults() {
        let list = executable_extensions();
        for extension in ["exe", "cmd", "bat", "com"] {
            assert!(list.iter().any(|item| item == extension));
        }
        let none = Attributes::default();
        assert!(is_program("Setup.EXE", none));
        assert!(is_program("run.bat", none));
        assert!(!is_program("notes.txt", none));
        assert!(!is_program("exe", none), "no extension");
    }

    #[test]
    fn program_files_are_started_as_they_are_and_scripts_by_their_interpreter() {
        let start = |name| program_start(name, Attributes::default());
        assert_eq!(
            start("My Script.PS1"),
            Some(ProgramStart::Line(
                "powershell.exe -NoLogo -File \"My Script.PS1\"".to_string()
            ))
        );
        assert_eq!(start("setup.exe"), Some(ProgramStart::File));
        assert_eq!(start("Setup.EXE"), Some(ProgramStart::File));
        assert_eq!(start("build.cmd"), Some(ProgramStart::File));
        assert_eq!(start("notes.txt"), None);
        assert_eq!(start("exe"), None);
    }
}
