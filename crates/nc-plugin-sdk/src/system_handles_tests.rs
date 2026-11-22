use std::io::{Read, Write};
use std::time::Instant;

use super::*;

fn own_handles() -> Vec<HandleInfo> {
    process_handles(std::process::id(), &|| false).expect("the tests' own handles")
}

#[test]
fn a_file_a_thread_and_a_process_are_named() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("proclist-handle-test.txt");
    let _file = std::fs::File::create(&path).unwrap();
    let (stop, stopped) = std::sync::mpsc::channel::<()>();
    let thread = std::thread::spawn(move || {
        let _ = stopped.recv();
    });
    let mut child = std::process::Command::new("ping")
        .args(["-n", "30", "127.0.0.1"])
        .stdout(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let handles = own_handles();
    let file = handles.iter().find(|handle| {
        matches!(&handle.name, HandleName::Text(name) if name.ends_with("proclist-handle-test.txt"))
    });
    let files: Vec<&HandleInfo> = (handles.iter())
        .filter(|handle| handle.type_name == "File")
        .collect();
    assert_eq!(
        file.map(|handle| handle.type_name.as_str()),
        Some("File"),
        "{files:#?}"
    );
    assert!(
        (handles.iter()).any(|handle| handle.name == HandleName::Process(child.id())),
        "the child by its number"
    );
    assert!(
        (handles.iter())
            .any(|handle| matches!(handle.name, HandleName::Thread { id, .. } if id != 0)),
        "a thread by its number"
    );
    assert!(
        handles.iter().all(|handle| handle.value % 4 == 0),
        "handles' values"
    );
    let _ = child.kill();
    let _ = child.wait();
    let _ = stop.send(());
    let _ = thread.join();
}

/// The name the list gives the handle `value` of the tests' own process.
fn name_of_value(value: u64) -> Option<HandleName> {
    (own_handles().into_iter())
        .find(|handle| handle.value == value)
        .map(|handle| handle.name)
}

/// A pipe read by another thread hangs the name's call (FAR's reason for its thread): the list still
/// comes, the pipe by its kind; a pipe that hung is not asked again, even once it would answer.
#[test]
fn a_pipe_waited_on_is_named_by_its_kind_without_hanging() {
    use std::os::windows::io::AsRawHandle;
    let (mut reader, mut writer) = std::io::pipe().unwrap();
    let value = reader.as_raw_handle() as u64;
    let (ready, reading_now) = std::sync::mpsc::channel();
    let reading = std::thread::spawn(move || {
        let mut byte = [0u8; 1];
        let _ = ready.send(());
        let _ = reader.read(&mut byte);
        // Kept open for the last look.
        reader
    });
    reading_now.recv().unwrap();
    let pipe = HandleName::Text("<pipe>".to_string());
    let started = Instant::now();
    // The reader is in its read a moment after it said so: the list is asked until it finds it there.
    let named = (0..50).find_map(|_| name_of_value(value).filter(|name| *name == pipe));
    assert!(started.elapsed() < Duration::from_secs(30), "no hang");
    assert_eq!(named, Some(pipe.clone()), "the pipe by its kind");
    writer.write_all(b"x").unwrap();
    let _reader = reading.join().unwrap();
    assert_eq!(
        name_of_value(value),
        Some(pipe),
        "it hung once: not asked again"
    );
}

#[test]
fn stopping_ends_the_list_at_once() {
    let handles = process_handles(std::process::id(), &|| true).unwrap();
    assert!(handles.is_empty());
}

#[test]
fn the_tables_entries_are_read_at_their_places() {
    let mut buffer = vec![0u64; 2 + 5 * 2];
    buffer[0] = 2;
    // Entry 0 at byte 16: object, pid 7, handle 0x40, access 0x1F0003 and type 5 in the flags word.
    buffer[3] = 7;
    buffer[4] = 0x40;
    buffer[5] = 0x1F_0003 | (5u64 << 48);
    // Entry 1 at byte 56: another process's.
    buffer[8] = 9;
    buffer[9] = 0x44;
    assert_eq!(entries_of(&buffer, 7), [(0x40, 0x1F_0003)]);
    assert!(entries_of(&buffer, 8).is_empty());
    assert_eq!(file_kind(FILE_TYPE_PIPE), "<pipe>");
    assert_eq!(file_kind(7), "<unknown> (7)");
}
