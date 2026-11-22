//! `tmp:<command`'s folder and its output as a list's lines: the command interpreter in the program's
//! folder or in a `|dir|` of its own, the bars never handed to it.

use super::*;

#[test]
fn the_folder_is_the_programs_or_a_dir_of_its_own() {
    let folder = Path::new(r"C:\work");
    assert_eq!(
        work_folder("dir /b", Some(folder)),
        (Some(folder.to_path_buf()), "dir /b".to_string())
    );
    assert_eq!(
        work_folder(r"|D:\logs|  dir /b", Some(folder)),
        (Some(PathBuf::from(r"D:\logs")), "dir /b".to_string()),
        "the bars not handed on"
    );
    assert_eq!(
        work_folder("||dir", None),
        (None, "||dir".to_string()),
        "an empty dir is none"
    );
}

#[cfg(windows)]
#[test]
fn the_commands_output_and_errors_are_its_lines() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("one.txt"), "1").unwrap();
    std::fs::write(temp.path().join("two.txt"), "2").unwrap();
    let bytes = output(
        "echo first>&2& dir /b *.txt& echo oops>&2",
        Some(temp.path()),
        &|| false,
    )
    .unwrap();
    let lines = listfile::lines(&bytes, &|line| String::from_utf8_lossy(line).into_owned());
    assert_eq!(
        lines,
        ["first", "one.txt", "two.txt", "oops"],
        "the errors in their place, as in FAR's one file"
    );
}

#[cfg(windows)]
#[test]
fn a_folder_that_is_not_there_runs_nothing() {
    let temp = tempfile::tempdir().unwrap();
    assert_eq!(
        output("echo x", Some(&temp.path().join("gone")), &|| false),
        None
    );
}

#[cfg(windows)]
#[test]
fn a_command_asked_to_stop_ends_with_what_it_started_and_gives_nothing() {
    // ADR-0019 §5: `ping` started by the interpreter would run 30 s.
    let started = std::time::Instant::now();
    let asked = std::cell::Cell::new(0);
    let stopped = || {
        asked.set(asked.get() + 1);
        asked.get() > 4
    };
    assert_eq!(output("ping -n 30 127.0.0.1", None, &stopped), None);
    assert!(
        started.elapsed() < std::time::Duration::from_secs(10),
        "ended at once, not after the command"
    );
}
