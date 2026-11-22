use super::*;

#[test]
fn a_working_folder_comes_between_bars() {
    assert_eq!(
        working_folder(r"|C:\x| dir"),
        (Some(r"C:\x".to_string()), "dir")
    );
    assert_eq!(working_folder("dir"), (None, "dir"));
    assert_eq!(working_folder("|open dir"), (None, "|open dir"));
}

#[test]
fn the_program_is_split_from_its_parameters_out_of_quotes() {
    assert_eq!(
        split("dir /b"),
        ("dir".to_string(), Some(" /b".to_string()))
    );
    assert_eq!(
        split("tool.exe/?"),
        ("tool.exe".to_string(), Some("/?".to_string()))
    );
    assert_eq!(
        split(r#""C:\my tools\x.exe" -a"#),
        (r"C:\my tools\x.exe".to_string(), Some(" -a".to_string()))
    );
    assert_eq!(split("echo a|sort").0, "echo");
    assert_eq!(split("ver"), ("ver".to_string(), None));
}

#[test]
fn the_command_line_quotes_a_path_with_a_space_and_quoted_parameters() {
    let temp = tempfile::tempdir().unwrap();
    let line = command_line(r#""C:\my tools\x.exe" -a"#, temp.path(), temp.path());
    assert_eq!(line, r#""C:\my tools\x.exe" -a"#);
    let quoted = command_line(r#"nosuch-vc "hi there""#, temp.path(), temp.path());
    assert_eq!(quoted, r#""nosuch-vc "hi there"""#);
    assert_eq!(
        command_line("nosuch-vc /b", temp.path(), temp.path()),
        "nosuch-vc /b"
    );
}

#[test]
fn the_streams_taken_follow_the_catch() {
    assert_eq!(streams(Catch::Both, true), (true, false));
    assert_eq!(streams(Catch::Output, true), (true, false));
    assert_eq!(streams(Catch::Errors, false), (false, true));
    assert_eq!(streams(Catch::Apart, true), (true, true));
    assert_eq!(streams(Catch::Apart, false), (true, false), "run: as one");
}

#[test]
fn the_sizes_line_is_fars() {
    let temp = tempfile::tempdir().unwrap();
    let (out, err) = (temp.path().join("o"), temp.path().join("e"));
    std::fs::write(&out, "12345").unwrap();
    std::fs::write(&err, "").unwrap();
    let files = Files {
        out: Some(out),
        err: Some(err.clone()),
    };
    assert_eq!(sizes(&files), "5");
    std::fs::write(&err, "abc").unwrap();
    assert_eq!(sizes(&files), "5/3");
}
