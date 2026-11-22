//! Starting a command line or a file as FAR's `Execute` (`execute.cpp`): a program found is started
//! directly, without the command interpreter; what cannot be started that way goes to the shell's
//! association; what the shell does not find, and every complex line, to the interpreter.

use std::io;
use std::path::Path;
use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
use windows_sys::Win32::System::Console::GetConsoleWindow;
use windows_sys::Win32::System::Threading::{
    CreateProcessW, EXTENDED_STARTUPINFO_PRESENT, GetExitCodeProcess, INFINITE,
    PROCESS_INFORMATION, PROCESS_NAME_WIN32, QueryFullProcessImageNameW, STARTUPINFOEXW,
    STARTUPINFOW, WaitForSingleObject,
};
use windows_sys::Win32::UI::Shell::{
    SEE_MASK_CLASSNAME, SEE_MASK_FLAG_NO_UI, SEE_MASK_NO_CONSOLE, SEE_MASK_NOASYNC,
    SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, ShellExecuteExW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

use super::interpreter::{find_program, interpreter_cannot_run_in, part_line};
use super::job::{JobBirth, KillJob};
use super::program::{is_gui_program, is_program};
use super::{wide, wide_os};
use crate::platform::files::Attributes;
use crate::platform::process::{ExecuteFailure, ExecuteSource, Execution};

const ERROR_FILE_NOT_FOUND: i32 = 2;
const ERROR_EXE_MACHINE_TYPE_MISMATCH: i32 = 216;
/// The user said no to the administrator's prompt: nothing to report (FAR).
const ERROR_CANCELLED: i32 = 1223;

const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;
const CREATE_DEFAULT_ERROR_MODE: u32 = 0x0400_0000;

/// FAR's `source_mode`: what is known of the thing to start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Known {
    /// A typed line: whatever it names.
    Nothing,
    /// A file of the panel that is a program.
    Program,
    /// Another file of the panel: only the shell can open it.
    Document,
}

/// What is started and how.
#[derive(Debug, PartialEq, Eq)]
struct Target {
    command: String,
    parameters: String,
    interpreter: bool,
    known: Known,
}

/// Starts what `request` asks for; the exit code of what was waited for, 0 for the rest.
pub fn execute(request: &Execution) -> Result<i32, ExecuteFailure> {
    let Some(target) = target(request) else {
        return Ok(0);
    };
    check_folder(&target, request.dir)?;
    let target = administrator_target(target, request);
    let error = match attempt(&target, request) {
        Ok(code) => return Ok(code),
        Err(error) => error,
    };
    // FAR: what neither the system nor the shell found may be one of the interpreter's own.
    if let ExecuteSource::Line(line) = request.source
        && !target.interpreter
        && error.raw_os_error() == Some(ERROR_FILE_NOT_FOUND)
    {
        let target = interpreter_target(line, request);
        check_folder(&target, request.dir)?;
        return attempt(&target, request).or_else(|error| failed(error, true));
    }
    failed(error, target.interpreter)
}

fn failed(error: io::Error, interpreter: bool) -> Result<i32, ExecuteFailure> {
    if error.raw_os_error() == Some(ERROR_CANCELLED) {
        Ok(0)
    } else {
        Err(ExecuteFailure::CannotExecute { interpreter, error })
    }
}

/// `None`: the line has only spaces in it.
fn target(request: &Execution) -> Option<Target> {
    match request.source {
        ExecuteSource::File(path) => {
            let name = path.file_name()?.to_string_lossy();
            Some(Target {
                command: path.to_string_lossy().into_owned(),
                parameters: String::new(),
                interpreter: false,
                known: if is_program(&name, Attributes::default()) {
                    Known::Program
                } else {
                    Known::Document
                },
            })
        }
        ExecuteSource::Line(line) => {
            let Some((command, parameters)) = part_line(line) else {
                return Some(interpreter_target(line, request));
            };
            if command.is_empty() {
                return None;
            }
            let command = find_program(&command, request.dir)
                .map_or(command, |found| found.to_string_lossy().into_owned());
            Some(Target {
                command,
                parameters,
                interpreter: false,
                known: Known::Nothing,
            })
        }
    }
}

/// FAR's `UseComspec` with its default `ComspecArguments`: `/S` makes cmd strip exactly the outer quotes.
/// An elevated cmd starts in the system folder whatever it is told, so with the administrator's rights
/// it goes to the panel's folder first, and runs nothing if it cannot (a mapped drive it does not see).
/// Not FAR: `del *` would delete in the system folder.
fn interpreter_target(line: &str, request: &Execution) -> Target {
    let parameters = if request.run_as {
        format!("/S /C \"cd /d \"{}\" && {line}\"", request.dir.display())
    } else {
        format!("/S /C \"{line}\"")
    };
    Target {
        command: std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".to_string()),
        parameters,
        interpreter: true,
        known: Known::Nothing,
    }
}

/// With the administrator's rights, what the interpreter runs goes through [`interpreter_target`].
fn administrator_target(target: Target, request: &Execution) -> Target {
    if !request.run_as || target.interpreter || !runs_in_interpreter(&target) {
        return target;
    }
    let line = full_line(&target);
    interpreter_target(&line, request)
}

/// The quoted program and its parameters (FAR's `full_command`).
fn full_line(target: &Target) -> String {
    if target.parameters.is_empty() {
        format!("\"{}\"", target.command)
    } else {
        format!("\"{}\" {}", target.command, target.parameters)
    }
}

/// cmd cannot stand in such a folder and would run the line in the Windows folder instead; a program
/// started directly can. Not FAR: it lets that happen (`del *` there).
fn check_folder(target: &Target, dir: &Path) -> Result<(), ExecuteFailure> {
    if interpreter_cannot_run_in(dir) && (runs_in_interpreter(target) || names_a_batch(target, dir))
    {
        Err(ExecuteFailure::FolderRefused)
    } else {
        Ok(())
    }
}

/// The interpreter itself, however it is named, or a batch file, which only it can run. The system drops
/// spaces and dots at a name's end, so they do not hide what the file is.
fn runs_in_interpreter(target: &Target) -> bool {
    if target.interpreter {
        return true;
    }
    let name = file_name(&target.command);
    let interpreter = std::env::var("COMSPEC").unwrap_or_default();
    name.ends_with(".bat")
        || name.ends_with(".cmd")
        || name == "cmd"
        || name == "cmd.exe"
        || name == file_name(&interpreter)
}

/// The last part of a path in lower case, without spaces and dots at its end.
fn file_name(path: &str) -> String {
    path.rsplit(['\\', '/'])
        .next()
        .unwrap_or(path)
        .trim_end_matches([' ', '.'])
        .to_lowercase()
}

/// A typed name the search did not find, but the shell's own search of extensions would.
fn names_a_batch(target: &Target, dir: &Path) -> bool {
    target.known == Known::Nothing
        && ["bat", "cmd"].iter().any(|extension| {
            dir.join(format!("{}.{extension}", target.command))
                .is_file()
        })
}

/// FAR's `execute_impl`: the system first, unless only the shell can do it, then the shell.
fn attempt(target: &Target, request: &Execution) -> io::Result<i32> {
    if target.known != Known::Document && !request.run_as {
        match start_process(target, request) {
            Ok(code) => return Ok(code),
            Err(error) if error.raw_os_error() == Some(ERROR_EXE_MACHINE_TYPE_MISMATCH) => {
                return Err(error);
            }
            Err(_) => {}
        }
    }
    shell_execute(target, request)
}

/// FAR's `execute_createprocess`. No application name: with one, a batch file with a funny name fails
/// (FAR's comment), and the name given is the file started.
fn start_process(target: &Target, request: &Execution) -> io::Result<i32> {
    let mut line = wide(&full_line(target));
    let directory = wide_os(request.dir.as_os_str());
    if request.no_wait {
        let process = create_process(&mut line, &directory, CREATE_NEW_CONSOLE, None)?;
        return Ok(finish(process, false, None));
    }
    // What is waited for is born in a job that dies with this process: the runner killed with its
    // user screen takes the command along, whenever that happens.
    let job = KillJob::new();
    let mut birth = job.as_ref().and_then(KillJob::birth);
    match create_process(&mut line, &directory, 0, birth.as_mut()) {
        Ok(process) => Ok(finish(process, true, job)),
        // A job the system refuses: the command is started untied rather than not at all.
        Err(_) if birth.is_some() => {
            let process = create_process(&mut line, &directory, 0, None)?;
            Ok(finish(process, true, None))
        }
        Err(error) => Err(error),
    }
}

/// `CreateProcessW` of `line` in `directory`, inside the job of `birth` if one is given; the process's
/// handle (its thread's is closed).
pub(super) fn create_process(
    line: &mut [u16],
    directory: &[u16],
    flags: u32,
    birth: Option<&mut JobBirth>,
) -> io::Result<HANDLE> {
    let mut startup = STARTUPINFOEXW {
        StartupInfo: STARTUPINFOW {
            cb: size_of::<STARTUPINFOW>() as u32,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut flags = CREATE_DEFAULT_ERROR_MODE | flags;
    if let Some(birth) = birth {
        startup.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
        startup.lpAttributeList = birth.list();
        flags |= EXTENDED_STARTUPINFO_PRESENT;
    }
    let mut process = PROCESS_INFORMATION::default();
    // SAFETY: `line` is a writable NUL-terminated buffer, as the call requires; `directory` is
    // NUL-terminated; `startup` (its attribute list alive in `birth`, its size in `cb`) and `process`
    // are live and of the right size for the call.
    let started = unsafe {
        CreateProcessW(
            null(),
            line.as_mut_ptr(),
            null(),
            null(),
            0,
            flags,
            null_mut(),
            directory.as_ptr(),
            &raw const startup.StartupInfo,
            &raw mut process,
        )
    } != 0;
    if !started {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: the thread's handle is owned here and closed once.
    unsafe { CloseHandle(process.hThread) };
    Ok(process.hProcess)
}

/// The class given to the shell for a file known to be there, so that it does not turn
/// `path\name` into `path\name.exe`; a link needs none (FAR).
fn known_class(target: &Target) -> Option<String> {
    if target.known == Known::Nothing {
        return None;
    }
    let extension = Path::new(&target.command)
        .extension()
        .map(|extension| extension.to_string_lossy().into_owned());
    match extension {
        Some(extension) if extension.eq_ignore_ascii_case("lnk") => None,
        Some(extension) => Some(format!(".{extension}")),
        None => Some(".".to_string()),
    }
}

/// FAR's `execute_shell`: the association, or the administrator's rights with the `runas` verb.
fn shell_execute(target: &Target, request: &Execution) -> io::Result<i32> {
    let wait = !request.no_wait;
    let file = wide(&target.command);
    let parameters = wide(&target.parameters);
    let directory = wide_os(request.dir.as_os_str());
    let verb = wide("runas");
    let class = known_class(target).map(|class| wide(&class));
    let mut mask = SEE_MASK_FLAG_NO_UI | SEE_MASK_NOASYNC | SEE_MASK_NOCLOSEPROCESS;
    if wait {
        mask |= SEE_MASK_NO_CONSOLE;
    }
    if class.is_some() {
        mask |= SEE_MASK_CLASSNAME;
    }
    let mut info = SHELLEXECUTEINFOW {
        cbSize: size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: mask,
        // SAFETY: no arguments; a process without a console gets a null window.
        hwnd: unsafe { GetConsoleWindow() },
        lpVerb: if request.run_as {
            verb.as_ptr()
        } else {
            null()
        },
        lpFile: file.as_ptr(),
        lpParameters: if target.parameters.is_empty() {
            null()
        } else {
            parameters.as_ptr()
        },
        lpDirectory: directory.as_ptr(),
        lpClass: class.as_ref().map_or(null(), |class| class.as_ptr()),
        nShow: SW_SHOWNORMAL,
        ..Default::default()
    };
    // SAFETY: `info` is a fully initialised SHELLEXECUTEINFOW of the right size; its strings are
    // NUL-terminated and live until the call returns.
    if unsafe { ShellExecuteExW(&mut info) } == 0 {
        return Err(io::Error::last_os_error());
    }
    // The shell starts the process running: what it starts at once may escape the job.
    let job = (wait && !info.hProcess.is_null())
        .then(|| KillJob::around(info.hProcess))
        .flatten();
    Ok(finish(info.hProcess, wait, job))
}

/// Waits for a process, unless it has a window of its own (FAR's `after_process_creation`); its handle
/// is closed. While it is waited for, `job` ties it to this process (the runner killed with its user
/// screen takes it along); then it is released, so what it left running goes on.
fn finish(process: HANDLE, wait: bool, job: Option<KillJob>) -> i32 {
    if process.is_null() {
        return 0;
    }
    let mut code = 0u32;
    if wait && !image_is_gui(process) {
        // SAFETY: `process` is a live handle the shell gave; `code` is a live local.
        unsafe {
            WaitForSingleObject(process, INFINITE);
            GetExitCodeProcess(process, &mut code);
        }
    }
    if let Some(job) = job {
        job.release();
    }
    // SAFETY: `process` is owned here and closed once.
    unsafe { CloseHandle(process) };
    i32::from_ne_bytes(code.to_ne_bytes())
}

fn image_is_gui(process: HANDLE) -> bool {
    let mut name = [0u16; 32768];
    let mut length = name.len() as u32;
    // SAFETY: `process` is a live handle; `name` holds `length` characters.
    let read = unsafe {
        QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            name.as_mut_ptr(),
            &raw mut length,
        )
    } != 0;
    read && is_gui_program(Path::new(&String::from_utf16_lossy(
        &name[..length as usize],
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain<'a>(source: ExecuteSource<'a>, dir: &'a Path) -> Execution<'a> {
        Execution {
            source,
            dir,
            no_wait: false,
            run_as: false,
        }
    }

    fn request(source: ExecuteSource<'_>, dir: &Path) -> Target {
        target(&plain(source, dir)).unwrap()
    }

    #[test]
    fn a_program_is_found_and_its_parameters_are_kept_as_typed() {
        let temp = tempfile::tempdir().unwrap();
        let found = request(ExecuteSource::Line("notepad \"a b\"  c"), temp.path());
        assert!(found.command.to_lowercase().ends_with("notepad.exe"));
        assert_eq!(found.parameters, "\"a b\"  c");
        assert!(!found.interpreter);
        assert_eq!(found.known, Known::Nothing);
        assert!(is_gui_program(Path::new(&found.command)));
    }

    #[test]
    fn a_program_that_is_not_found_is_left_as_written() {
        let temp = tempfile::tempdir().unwrap();
        let found = request(ExecuteSource::Line("no-such-4e1 x"), temp.path());
        assert_eq!(found.command, "no-such-4e1");
        assert!(!found.interpreter);
    }

    #[test]
    fn complex_lines_and_the_interpreters_own_commands_go_to_it() {
        let temp = tempfile::tempdir().unwrap();
        for line in ["dir /b", "notepad x | more", "a && b"] {
            let found = request(ExecuteSource::Line(line), temp.path());
            assert!(found.interpreter, "{line}");
            assert_eq!(found.parameters, format!("/S /C \"{line}\""));
        }
    }

    #[test]
    fn a_line_of_spaces_starts_nothing() {
        let temp = tempfile::tempdir().unwrap();
        assert_eq!(
            target(&plain(ExecuteSource::Line("   "), temp.path())),
            None
        );
    }

    #[test]
    fn a_files_name_is_taken_literally_and_its_kind_comes_from_its_extension() {
        let dir = Path::new(r"C:\dir");
        let path = dir.join("a%PATH%b.exe");
        let program = request(ExecuteSource::File(&path), dir);
        assert_eq!(program.command, r"C:\dir\a%PATH%b.exe");
        assert_eq!(program.known, Known::Program);
        let path = dir.join("notes.txt");
        let document = request(ExecuteSource::File(&path), dir);
        assert_eq!(document.known, Known::Document);
        assert_eq!(known_class(&document).as_deref(), Some(".txt"));
    }

    #[test]
    fn the_shell_is_told_the_class_of_a_known_file_but_for_a_link() {
        let of = |command: &str, known| {
            known_class(&Target {
                command: command.to_string(),
                parameters: String::new(),
                interpreter: false,
                known,
            })
        };
        assert_eq!(of(r"C:\a\b", Known::Document).as_deref(), Some("."));
        assert_eq!(of(r"C:\a\b.LNK", Known::Document), None);
        assert_eq!(of(r"C:\a\b.exe", Known::Program).as_deref(), Some(".exe"));
        assert_eq!(of("b.txt", Known::Nothing), None);
    }

    #[test]
    fn only_what_the_interpreter_runs_is_refused_in_a_network_folder() {
        let network = Path::new(r"\\server\share");
        let plain = Path::new(r"C:\dir");
        let with = |command: &str, interpreter| Target {
            command: command.to_string(),
            parameters: String::new(),
            interpreter,
            known: Known::Nothing,
        };
        let refused = |target: &Target, dir| {
            matches!(
                check_folder(target, dir),
                Err(ExecuteFailure::FolderRefused)
            )
        };
        assert!(!refused(&with(r"C:\a\tool.exe", false), network));
        for through_the_interpreter in [
            r"C:\a\run.BAT",
            r"C:\a\run.cmd",
            r"C:\a\run.bat ",
            r"C:\a\run.bat.",
            r"C:\a\.bat",
            r"C:\Windows\System32\CMD.EXE",
            "cmd",
        ] {
            assert!(
                refused(&with(through_the_interpreter, false), network),
                "{through_the_interpreter}"
            );
            assert!(!refused(&with(through_the_interpreter, false), plain));
        }
        assert!(refused(&with("cmd.exe", true), network));
        assert!(!refused(&with("cmd.exe", true), plain));
    }

    #[test]
    fn a_name_the_shell_would_complete_to_a_batch_file_counts_as_one() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("run.bat"), "").unwrap();
        let named = |command: &str| Target {
            command: command.to_string(),
            parameters: String::new(),
            interpreter: false,
            known: Known::Nothing,
        };
        assert!(names_a_batch(&named("run"), temp.path()));
        assert!(!names_a_batch(&named("other"), temp.path()));
    }

    #[test]
    fn with_the_administrators_rights_the_interpreter_goes_to_the_folder_first() {
        let dir = Path::new(r"C:\dir");
        let elevated = Execution {
            run_as: true,
            ..plain(ExecuteSource::Line("del *"), dir)
        };
        let line = target(&elevated).unwrap();
        assert!(line.interpreter);
        assert_eq!(line.parameters, r#"/S /C "cd /d "C:\dir" && del *""#);

        let batch = Target {
            command: r"C:\dir\run.bat".to_string(),
            parameters: "-x".to_string(),
            interpreter: false,
            known: Known::Program,
        };
        let batch = administrator_target(batch, &elevated);
        assert!(batch.interpreter);
        assert_eq!(
            batch.parameters,
            r#"/S /C "cd /d "C:\dir" && "C:\dir\run.bat" -x""#
        );

        let program = Target {
            command: r"C:\dir\tool.exe".to_string(),
            parameters: String::new(),
            interpreter: false,
            known: Known::Program,
        };
        assert!(!administrator_target(program, &elevated).interpreter);
    }

    #[test]
    fn a_refused_prompt_is_no_failure_and_a_failure_keeps_a_system_error() {
        assert_eq!(
            failed(io::Error::from_raw_os_error(ERROR_CANCELLED), false).unwrap(),
            0
        );
        let failure = failed(io::Error::from_raw_os_error(5), true).unwrap_err();
        let ExecuteFailure::CannotExecute { interpreter, error } = failure else {
            panic!("a failure");
        };
        assert!(interpreter);
        assert_eq!(error.raw_os_error(), Some(5));
    }
}
