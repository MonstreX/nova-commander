//! `arc:`'s commands (S6.2f, arclite's cmdline.cpp and `cmdline_*`) typed in the command line, through
//! the program and the shipped plugin: `x` extracts archives into a folder, `t` tests them, `c` makes a
//! zip of files, `u` adds to it, `e` extracts items by name or masks, `d` deletes them; a command not
//! understood shows arclite's syntax.

use super::*;

fn run_line(app: &mut App, line: &str) {
    app.command_line.set(line);
    app.run_command_line();
}

/// The zip's item names, sorted.
fn names_in(path: &std::path::Path) -> Vec<String> {
    let mut names = zip_names(path);
    names.sort();
    names
}

/// `x` and `t`: the archives named from the panel's folder, the folder last; arclite's defaults (no
/// folder of the archive's own); the test says so.
#[test]
fn x_extracts_and_t_tests_from_the_command_line() {
    let (_plugins, files, out, mut app) = app_with_archives("cmdx");
    let into = out.path().display().to_string();
    run_line(&mut app, &format!(r#"arc:x a.zip "{into}""#));
    let made = out.path().join("docs").join("readme.txt");
    tick_until(&mut app, |app| settled(app) && made.exists());
    assert_eq!(std::fs::read(out.path().join("top.txt")).unwrap(), b"top");
    run_line(&mut app, "arc:t a.zip c.zip");
    tick_until(&mut app, |app| message(app).is_some());
    assert_eq!(message(&app).unwrap().1, ["Archive test complete"]);
    key(&mut app, KeyCode::Enter);
    tick_until(&mut app, settled);
    assert!(files.path().join("a.zip").exists(), "x keeps the archive");
}

/// `c` makes a zip of files (their shared folder taken off), the cursor on it; `u` adds a file to it,
/// keeping what it had.
#[test]
fn c_makes_and_u_adds_from_the_command_line() {
    let (_plugins, files, _out, mut app) = app_with_archives("cmdc");
    std::fs::create_dir(files.path().join("sub")).unwrap();
    std::fs::write(files.path().join("sub").join("deep.txt"), "deep").unwrap();
    run_line(&mut app, r"arc:c -l:0 made.zip plain.txt sub\deep.txt");
    let made = files.path().join("made.zip");
    tick_until(&mut app, |app| settled(app) && names_in(&made).len() == 2);
    assert_eq!(names_in(&made), ["plain.txt", "sub/deep.txt"]);
    tick_until(&mut app, |app| {
        app.panels[0]
            .current()
            .is_some_and(|entry| entry.name == "made.zip")
    });
    std::fs::write(files.path().join("more.txt"), "more").unwrap();
    run_line(&mut app, "arc:u made.zip more.txt");
    tick_until(&mut app, |app| settled(app) && names_in(&made).len() == 3);
    assert_eq!(names_in(&made), ["more.txt", "plain.txt", "sub/deep.txt"]);
}

/// `e` extracts the items named (by their paths whatever the case, or masks) into `-out` by their
/// names; `d` deletes them from the archive.
#[test]
fn e_extracts_and_d_deletes_items_from_the_command_line() {
    let (_plugins, files, out, mut app) = app_with_archives("cmde");
    let into = out.path().display().to_string();
    run_line(
        &mut app,
        &format!(r#"arc:e -out:"{into}" a.zip DOCS\readme.txt"#),
    );
    let made = out.path().join("readme.txt");
    tick_until(&mut app, |app| settled(app) && made.exists());
    assert_eq!(std::fs::read(&made).unwrap(), b"hello");
    assert!(!out.path().join("top.txt").exists(), "only the item named");
    run_line(&mut app, "arc:d a.zip *.txt");
    let archive = files.path().join("a.zip");
    tick_until(&mut app, |app| {
        settled(app) && !names_in(&archive).contains(&"top.txt".to_string())
    });
    assert!(
        names_in(&archive)
            .iter()
            .all(|name| !name.ends_with(".txt")),
        "every .txt matched: {:?}",
        names_in(&archive)
    );
}

/// `d` of the archive a panel shows, named another way (`..`, `/`): the panel reads the archive written
/// anew (made whole as FAR's `get_absolute_path`), not its old one.
#[test]
fn d_reaches_the_archives_open_panel_whatever_its_name_is_written() {
    let (_plugins, files, _out, mut app) = app_with_archives("cmdpanel");
    point(&mut app, "a.zip");
    key(&mut app, KeyCode::Enter);
    tick_until(&mut app, |app| in_archive(app, "/"));
    let folder_name = files.path().file_name().unwrap().to_string_lossy();
    let other_way = format!("../{folder_name}/a.zip");
    let line = format!("arc:d {other_way} top.txt");
    // The command line's folder: the one the archive's panel was opened from.
    run_line(&mut app, &line);
    tick_until(&mut app, |app| {
        settled(app) && in_archive(app, "/") && !names(app).contains(&"top.txt".to_string())
    });
    assert_eq!(names(&app), ["docs"]);
}

/// `e` without `-out`: the panel's folder; items of two folders each into it by their names.
#[test]
fn e_without_out_extracts_into_the_panels_folder() {
    let (_plugins, files, _out, mut app) = app_with_archives("cmdeout");
    run_line(&mut app, r"arc:e a.zip top.txt docs\readme.txt");
    let readme = files.path().join("readme.txt");
    tick_until(&mut app, |app| settled(app) && readme.exists());
    assert_eq!(std::fs::read(files.path().join("top.txt")).unwrap(), b"top");
    assert_eq!(std::fs::read(readme).unwrap(), b"hello");
}

/// A profile not there is arclite's `E_BAD_FORMAT` (the syntax shown); a file that is no archive for
/// `e` says so; `c` of another format than zip says why.
#[test]
fn a_missing_profile_a_file_no_archive_and_another_format_are_told() {
    let (_plugins, files, _out, mut app) = app_with_archives("cmdtold");
    run_line(&mut app, "arc:c -pr:nosuch made.zip plain.txt");
    tick_until(&mut app, |app| message(app).is_some());
    let (_, lines) = message(&app).unwrap();
    assert!(
        lines.iter().any(|line| line.starts_with("arc:c ")),
        "{lines:?}"
    );
    key(&mut app, KeyCode::Enter);
    tick_until(&mut app, settled);
    assert!(!files.path().join("made.zip").exists());
    let shown = |app: &App| match &app.overlay {
        Some(Overlay::Dialog(dialog)) => format!("{dialog:?}"),
        _ => String::new(),
    };
    run_line(&mut app, "arc:e plain.txt x");
    tick_until(&mut app, |app| !shown(app).is_empty());
    assert!(
        shown(&app).contains("File is not recognized as archive"),
        "{}",
        shown(&app)
    );
    key(&mut app, KeyCode::Enter);
    tick_until(&mut app, settled);
    run_line(&mut app, "arc:c -t:Rar made.rar plain.txt");
    tick_until(&mut app, |app| !shown(app).is_empty());
    assert!(
        shown(&app).contains("Archive modification not supported"),
        "{}",
        shown(&app)
    );
    assert!(!files.path().join("made.rar").exists());
    key(&mut app, KeyCode::Enter);
    tick_until(&mut app, settled);
    run_line(&mut app, "arc:c -sfx:none.sfx made.7z plain.txt");
    tick_until(&mut app, |app| !shown(app).is_empty());
    assert!(shown(&app).contains("Select SFX module"), "{}", shown(&app));
    assert!(!files.path().join("made.7z").exists());
}

/// `arc:c -sfx`: a 7z (the format of the name's extension, as arclite takes it) after 7-Zip's `7z.sfx`
/// (arclite's default module), opened again as the 7z.
#[test]
fn a_command_makes_a_self_extracting_7z() {
    let (_plugins, files, _out, mut app) = app_with_archives("cmdsfx");
    run_line(&mut app, "arc:c -sfx made.7z plain.txt");
    let made = files.path().join("made.7z");
    tick_until(&mut app, |app| settled(app) && made.exists());
    let bytes = std::fs::read(&made).unwrap();
    assert_eq!(&bytes[..2], b"MZ");
    let at = bytes.windows(6).position(|w| w == b"7z\xBC\xAF\x27\x1C");
    assert!(at.is_some_and(|at| at > 0), "the 7z after the module");
    assert_eq!(
        archive::support::names_in(&made),
        ["plain.txt"],
        "the 7z opened after its stub"
    );
}

/// `arc:c` of a 7z, a tar and split volumes: written through 7-Zip's library in the formats asked
/// (arclite's `find_by_ext`, `-t`, `-v`); `arc:u` adds to the 7z, `arc:d` deletes from it.
#[test]
fn commands_write_7zips_formats() {
    let (_plugins, files, _out, mut app) = app_with_archives("cmd7z");
    run_line(&mut app, "arc:c made.7z plain.txt");
    let made = files.path().join("made.7z");
    tick_until(&mut app, |app| settled(app) && made.exists());
    assert_eq!(&std::fs::read(&made).unwrap()[..2], b"7z");
    run_line(&mut app, "arc:c -t:tar made.bin plain.txt b.docx");
    let tar = files.path().join("made.bin");
    tick_until(&mut app, |app| settled(app) && tar.exists());
    assert_eq!(&std::fs::read(&tar).unwrap()[257..262], b"ustar");
    run_line(&mut app, "arc:c -v:16k -m:copy split.tar plain.txt");
    let first = files.path().join("split.tar.001");
    tick_until(&mut app, |app| settled(app) && first.exists());
    run_line(&mut app, "arc:u made.7z b.docx");
    tick_until(&mut app, settled);
    run_line(&mut app, "arc:d made.7z plain.txt");
    tick_until(&mut app, settled);
    point(&mut app, "made.7z");
    key(&mut app, KeyCode::Enter);
    tick_until(&mut app, |app| in_archive(app, "/"));
    assert_eq!(names(&app), ["b.docx"]);
}

/// A command not understood: arclite's syntax (its help topic "Prefix"), nothing done.
#[test]
fn a_command_not_understood_shows_the_syntax() {
    let (_plugins, _files, _out, mut app) = app_with_archives("cmdbad");
    run_line(&mut app, "arc:x a.zip");
    tick_until(&mut app, |app| message(app).is_some());
    let (title, lines) = message(&app).unwrap();
    assert_eq!(title, "Archive");
    assert!(
        lines.iter().any(|line| line.starts_with("arc:x ")),
        "{lines:?}"
    );
    key(&mut app, KeyCode::Enter);
    tick_until(&mut app, settled);
}
