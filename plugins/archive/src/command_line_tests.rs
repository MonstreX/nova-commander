use super::*;

fn none(_: &str) -> bool {
    false
}

fn all(_: &str) -> bool {
    true
}

fn update_of(text: &str, exists: &dyn Fn(&str) -> bool) -> UpdateCommand {
    match parse(text, exists) {
        Ok(Command::Update(command)) => command,
        other => panic!("{text}: {other:?}"),
    }
}

/// The open command stays the open command; the others by their first token whatever its case.
#[test]
fn the_command_is_told_by_its_first_token() {
    assert!(matches!(parse("a.zip", &none), Ok(Command::Open(_))));
    assert!(matches!(parse("T a.zip", &none), Ok(Command::Test(_))));
    assert!(matches!(
        parse("x a.zip out", &none),
        Ok(Command::Extract(_))
    ));
    let Ok(Command::Items(items)) = parse("E a.zip one", &none) else {
        panic!("e")
    };
    assert!(!items.delete);
    let Ok(Command::Items(items)) = parse("d a.zip one", &none) else {
        panic!("d")
    };
    assert!(items.delete);
    assert_eq!(parse("", &none), Err(BadFormat));
    assert_eq!(parse("-q a.zip", &none), Err(BadFormat));
}

/// `c` and `u`: the parameters before the archive, the files after it, `@` a list file; `u` of an
/// archive not there is a new one.
#[test]
fn an_update_is_read_as_arclite_reads_it() {
    let command = update_of(
        r#"c -l:9 -m:PPMd -s:n -p:"se cret" -mf -ie:n -adv:em=AES128 "C:\a b\new.zip" one.txt @list"#,
        &none,
    );
    assert!(command.create);
    assert_eq!(command.level, Some(9));
    assert_eq!(command.method.as_deref(), Some("PPMd"));
    assert_eq!(command.solid, Some(false));
    assert_eq!(command.password.as_deref(), Some("se cret"));
    assert_eq!(command.move_files, Some(true));
    assert_eq!(command.ignore_errors, Some(false));
    assert_eq!(command.advanced.as_deref(), Some("em=AES128"));
    assert_eq!(command.archive, r"C:\a b\new.zip");
    assert_eq!(command.files, ["one.txt"]);
    assert_eq!(command.listfiles, ["list"]);
}

/// `u` of an archive there takes `-o`; of one not there it makes it, `-pr` then allowed.
#[test]
fn u_of_an_archive_not_there_makes_it() {
    let existing = update_of("u -o:s a.zip one", &all);
    assert!(!existing.create);
    assert_eq!(existing.overwrite, Some(Overwrite::Skip));
    assert_eq!(
        update_of("u -o a.zip one", &all).overwrite,
        Some(Overwrite::Overwrite)
    );
    let missing = update_of("u a.zip one", &none);
    assert!(missing.create, "u of an archive not there makes it");
    assert_eq!(
        update_of("u -pr:mine a.zip one", &none).profile.as_deref(),
        Some("mine")
    );
    assert_eq!(
        update_of("c -eh:a a.zip one", &none).encrypt_header,
        Some(Tri::Undef)
    );
}

/// What arclite refuses in `c` / `u`: no file, a parameter after the archive, a level or method it
/// does not know, `-o` for a new archive, `-pr` / `-t` / `-sfx` / `-v` for one there, empty values.
#[test]
fn a_bad_update_is_refused() {
    for (text, exists) in [
        ("c a.zip", false),
        ("c", false),
        ("c a.zip -l:5", false),
        ("c a.zip one -l:5", false),
        ("c -l:4 a.zip one", false),
        ("c -m:lzx a.zip one", false),
        ("c -s:maybe a.zip one", false),
        ("c -o a.zip one", false),
        ("u -pr:mine a.zip one", true),
        ("u -t:zip a.zip one", true),
        ("u -sfx a.zip one", true),
        ("u -v:10m a.zip one", true),
        ("c -p: a.zip one", false),
        ("c -v: a.zip one", false),
        ("c -adv: a.zip one", false),
        ("c -q a.zip one", false),
    ] {
        let exists = move |_: &str| exists;
        assert_eq!(parse(text, &exists), Err(BadFormat), "{text}");
    }
}

/// `x`: arclite's defaults (overwrite, no folder of its own), the folder last; `--` ends the
/// parameters.
#[test]
fn an_extraction_is_read_as_arclite_reads_it() {
    let Ok(Command::Extract(command)) = parse(
        r#"x -ie -o:r -mf:a -p:pw -sd -da:y a.zip "b c.zip" out"#,
        &none,
    ) else {
        panic!("x")
    };
    assert_eq!(command.archives, ["a.zip", "b c.zip"]);
    let options = &command.options;
    assert_eq!(options.dst_dir, std::path::PathBuf::from("out"));
    assert!(options.ignore_errors);
    assert_eq!(options.overwrite, Overwrite::Rename);
    assert_eq!(options.move_files, Tri::Undef);
    assert_eq!(options.password, "pw");
    assert_eq!(options.separate_dir, Tri::True);
    assert!(options.delete_archive);
    let Ok(Command::Extract(plain)) = parse("x a.zip out", &none) else {
        panic!("x")
    };
    assert_eq!(
        plain.options.overwrite,
        Overwrite::Overwrite,
        "arclite's default"
    );
    assert_eq!(plain.options.separate_dir, Tri::False);
    assert_eq!(plain.options.open_dir, Tri::False);
    let Ok(Command::Extract(dashed)) = parse("x -- -a.zip out", &none) else {
        panic!("x --")
    };
    assert_eq!(dashed.archives, ["-a.zip"]);
    for text in [
        "x a.zip",
        "x -out:o a.zip out",
        "x -o:q a.zip out",
        "x -p: a.zip out",
    ] {
        assert_eq!(parse(text, &none), Err(BadFormat), "{text}");
    }
}

/// `e` / `d`: the archive first, its items after; `-out` only for `e`, `-o` / `-mf` not for `d`.
#[test]
fn items_are_read_as_arclite_reads_them() {
    let Ok(Command::Items(command)) = parse(r#"e -out:"C:\o u t" a.zip docs\a.txt *.md"#, &none)
    else {
        panic!("e")
    };
    assert_eq!(command.archive, "a.zip");
    assert_eq!(command.items, [r"docs\a.txt", "*.md"]);
    assert_eq!(
        command.options.dst_dir,
        std::path::PathBuf::from(r"C:\o u t")
    );
    for text in [
        "e a.zip",
        "d -out:o a.zip x",
        "d -o a.zip x",
        "d -mf a.zip x",
        "e -sd a.zip x",
    ] {
        assert_eq!(parse(text, &none), Err(BadFormat), "{text}");
    }
    assert!(matches!(
        parse("d -ie -p:pw a.zip x", &none),
        Ok(Command::Items(_))
    ));
}

/// `t`: archives; none is no failure (nothing tested); arclite checks only the last for a parameter
/// (one before it is taken as an archive's name).
#[test]
fn a_test_takes_archives_only() {
    assert_eq!(
        parse(r#"t a.zip "b c.zip""#, &none),
        Ok(Command::Test(vec![
            "a.zip".to_string(),
            "b c.zip".to_string()
        ]))
    );
    assert_eq!(parse("t", &none), Ok(Command::Test(Vec::new())));
    assert_eq!(parse("t a.zip -ie", &none), Err(BadFormat));
    assert_eq!(
        parse("t -x a.zip", &none),
        Ok(Command::Test(vec!["-x".to_string(), "a.zip".to_string()]))
    );
}

/// `-l` read as arclite's `_wtoi` (the digits it starts with, 0 for none), then checked; `-t` names a
/// format (none: not understood); only the quotes around a value go.
#[test]
fn levels_formats_and_quotes_as_arclite_reads_them() {
    assert_eq!(update_of("c -l:5x a.zip one", &none).level, Some(5));
    assert_eq!(update_of("c -l: a.zip one", &none).level, Some(0));
    assert_eq!(update_of("c -l:abc a.zip one", &none).level, Some(0));
    assert_eq!(parse("c -l:4 a.zip one", &none), Err(BadFormat));
    assert_eq!(parse("c -t: a.zip one", &none), Err(BadFormat));
    assert_eq!(
        update_of("c -t:7z a.zip one", &none).format.as_deref(),
        Some("7z")
    );
    assert_eq!(
        update_of(r#"c -p:"se cret" "a b.zip" one"#, &none)
            .password
            .as_deref(),
        Some("se cret")
    );
    // A quote inside a token opens a quoted part to the text's end (arclite's tokens): one token.
    assert_eq!(parse(r#"c -p:pa"ss a.zip one"#, &none), Err(BadFormat));
}

/// A list file: a name a line, whatever the line ends, empty lines passed over.
#[test]
fn a_list_file_names_a_file_a_line() {
    assert_eq!(
        list_file("a.txt\r\nb c.txt\n\n\rd\r\n"),
        ["a.txt", "b c.txt", "d"]
    );
    assert!(list_file("").is_empty());
}
