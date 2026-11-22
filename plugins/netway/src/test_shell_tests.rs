use super::*;

#[test]
fn words_read_quotes_and_escapes() {
    assert_eq!(words("cd \"/a b\" && pwd"), ["cd", "/a b", "&&", "pwd"]);
    assert_eq!(words("mv -f \"x\\\"y\" \"\""), ["mv", "-f", "x\"y", ""]);
}

#[test]
fn paths_resolve_under_the_folder() {
    assert_eq!(resolve("/", "docs"), "/docs");
    assert_eq!(resolve("/a/b", "../c/./d"), "/a/c/d");
    assert_eq!(resolve("/a", "/x/.."), "/");
}

#[test]
fn the_commands_work_on_the_root_and_read_back_by_ls() {
    let temp = tempfile::tempdir().unwrap();
    let mut shell = Shell {
        root: temp.path().to_path_buf(),
        cwd: "/".to_string(),
    };
    let run = |shell: &mut Shell, command: &str| shell.run(&words(command));
    assert_eq!(run(&mut shell, "mkdir \"/d\"").0, 0);
    std::fs::write(temp.path().join("d/f.txt"), b"abc").unwrap();
    assert_eq!(run(&mut shell, "cd \"d\" && pwd").1, "/d\n");
    assert_eq!(
        run(&mut shell, "TZ=UTC touch -m -t 202403101030.45 \"f.txt\"").0,
        0
    );
    assert_eq!(run(&mut shell, "chmod 0600 \"f.txt\"").0, 0);
    let listed = run(&mut shell, "ls -la --full-time \"/d\"").1;
    let read = crate::scp_listing::listing(&listed, (2026, 11, 18));
    assert_eq!(read.len(), 1);
    assert_eq!(read[0].entry.name, "f.txt");
    assert_eq!(read[0].entry.size, 3);
    assert_eq!(read[0].entry.permissions, Some(0o100_600));
    let utc = crate::scp_listing::seconds_of((2024, 3, 10), (10, 30, 45));
    assert_eq!(read[0].entry.modified, Some(u32::try_from(utc).unwrap()));
    assert_eq!(run(&mut shell, "cp -p -r -f \"/d\" \"/e\"").0, 0);
    assert!(temp.path().join("e/f.txt").exists());
    let missing = run(&mut shell, "rmdir \"/nothing\"");
    assert_eq!(missing.0, 1);
    assert!(missing.2.starts_with("rmdir: '/nothing'"));
    assert_eq!(run(&mut shell, "frobnicate").0, 127);
}
