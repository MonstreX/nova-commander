use std::process::{Child, Command, Stdio};

use super::*;

const SLASH: char = '\\';
const NEW_LINE: u8 = 10;

/// A ping of the tests' with its own folder and a variable of its own, ended when dropped.
struct Pinging(Child);

impl Drop for Pinging {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn pinging(program: &str, folder: &std::path::Path) -> Pinging {
    let child = Command::new(program)
        .args(["-n", "30", "127.0.0.1"])
        .current_dir(folder)
        .env("NC_PROCLIST_TEST", "seen")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    Pinging(child)
}

/// A ping that has started: its first line written, its modules loaded.
fn started(program: &str, folder: &std::path::Path) -> Pinging {
    use std::io::BufRead;
    let mut child = Command::new(program)
        .args(["-n", "30", "127.0.0.1"])
        .current_dir(folder)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    // Its text is in the console's code page: read as bytes.
    let output = child.stdout.take().unwrap();
    let first = std::io::BufReader::new(output)
        .split(NEW_LINE)
        .map_while(Result::ok)
        .find(|line| line.iter().any(|byte| !byte.is_ascii_whitespace()));
    assert!(first.is_some(), "the ping wrote nothing");
    Pinging(child)
}

/// The 32-bit ping, where Windows has its WOW64 (not on every edition).
fn wow64_ping() -> Option<String> {
    let path = system32(r"SysWOW64\PING.EXE");
    std::path::Path::new(&path).exists().then_some(path)
}

fn system32(name: &str) -> String {
    let root = std::env::var("SystemRoot").unwrap();
    format!(r"{root}\{name}")
}

/// The child's parameters read: its folder (FAR shows it with its last `\`) and its environment.
fn read_child(program: &str) {
    let folder = tempfile::tempdir().unwrap();
    let child = pinging(program, folder.path());
    let pid = child.0.id();
    // The parameters are written as the process is made: no wait for it.
    let memory = process_memory(pid, true).expect("its memory read");
    let shown = memory.current_folder.expect("its current folder");
    let wanted = folder.path().to_string_lossy().to_lowercase();
    assert!(
        shown.to_lowercase().trim_end_matches(SLASH) == wanted.trim_end_matches(SLASH),
        "{shown} for {wanted}"
    );
    assert!(
        memory
            .environment
            .iter()
            .any(|text| text == "NC_PROCLIST_TEST=seen"),
        "{:?}",
        memory.environment
    );
    let without = process_memory(pid, false).unwrap();
    assert!(without.environment.is_empty(), "not asked for");
}

#[test]
fn a_64_bit_childs_folder_and_environment_are_read() {
    read_child(&system32(r"System32\PING.EXE"));
}

#[test]
fn a_32_bit_childs_folder_and_environment_are_read_through_its_32_bit_peb() {
    match wow64_ping() {
        Some(ping) => read_child(&ping),
        None => eprintln!("no WOW64 here: the 32-bit PEB is not tried"),
    }
}

fn has_ntdll(modules: &[Module]) -> bool {
    modules
        .iter()
        .any(|module| module.path.to_lowercase().ends_with("ntdll.dll") && module.size > 0)
}

#[test]
fn the_modules_are_listed_with_their_places_and_files() {
    let pid = std::process::id();
    let modules = process_modules(pid).expect("its own modules");
    assert!(has_ntdll(&modules), "{modules:?}");
    let exe = std::env::current_exe()
        .unwrap()
        .to_string_lossy()
        .to_lowercase();
    assert_eq!(modules[0].path.to_lowercase(), exe, "the image first");
    assert!(modules.iter().all(|module| module.base != 0));
}

/// FAR's other way, the loader's list in the PEB, for both bitnesses.
#[test]
fn the_loaders_list_gives_the_same_modules() {
    let reader = Reader::new(std::process::id()).unwrap();
    let modules = reader.loader_modules();
    assert!(has_ntdll(&modules), "{modules:?}");
    assert_eq!(
        Some(modules[0].base),
        reader.pointer(reader.peb + 0x10),
        "the image first"
    );
    let folder = tempfile::tempdir().unwrap();
    let Some(ping) = wow64_ping() else {
        eprintln!("no WOW64 here: the 32-bit loader's list is not tried");
        return;
    };
    let child = started(&ping, folder.path());
    let reader = Reader::new(child.0.id()).expect("the 32-bit child opened");
    let modules = reader.loader_modules();
    assert!(
        modules[0]
            .path
            .to_lowercase()
            .ends_with(r"syswow64\ping.exe"),
        "{modules:?}"
    );
    assert!(
        modules
            .iter()
            .all(|module| module.base < u64::from(u32::MAX)),
        "32-bit places"
    );
}

#[test]
fn an_environment_block_is_split_at_its_nuls() {
    let units: Vec<u16> = "A=1\0B=2\0\0junk".encode_utf16().collect();
    assert_eq!(strings(&units), ["A=1", "B=2"]);
    assert!(strings(&[0, 0]).is_empty());
}
