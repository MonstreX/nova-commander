use super::*;

/// A folder of the test's own with two files and a folder in it.
fn place() -> tempfile::TempDir {
    let place = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(place.path().join("sub")).unwrap();
    std::fs::write(place.path().join("a.txt"), "a").unwrap();
    std::fs::write(place.path().join("b.txt"), "b").unwrap();
    place
}

fn never() -> bool {
    false
}

const BOTH: Strings = Strings {
    verb: true,
    help: true,
};

fn verbs(items: &[ShellItem]) -> Vec<String> {
    items.iter().filter_map(|item| item.verb.clone()).collect()
}

#[test]
fn a_files_menu_has_explorers_commands_with_their_verbs() {
    let place = place();
    let mut shell = Shell::start().unwrap();
    shell
        .menu_for_paths(&[place.path().join("a.txt")], &never)
        .unwrap();
    let items = shell.items(&[], BOTH, &never).unwrap();
    let verbs = verbs(&items);
    for verb in ["delete", "properties"] {
        assert!(verbs.iter().any(|seen| seen == verb), "{verb} in {verbs:?}");
    }
    let delete = items
        .iter()
        .find(|item| item.verb.as_deref() == Some("delete"))
        .unwrap();
    assert!(delete.id >= session::FIRST && !delete.text.is_empty());
    assert_eq!(
        shell.verb(delete.id, &never).unwrap().as_deref(),
        Some("delete")
    );
    if let Some(at) = items.iter().position(|item| item.submenu) {
        assert!(shell.items(&[at], BOTH, &never).is_ok(), "a submenu opens");
    }
    assert!(
        shell.items(&[items.len() + 5], BOTH, &never).is_err(),
        "no such submenu"
    );
}

#[test]
fn the_strings_not_asked_for_are_not_read() {
    let place = place();
    let mut shell = Shell::start().unwrap();
    shell
        .menu_for_paths(&[place.path().join("a.txt")], &never)
        .unwrap();
    let items = shell.items(&[], Strings::default(), &never).unwrap();
    let plain = items.iter().filter(|item| !item.owner_drawn);
    assert!(
        plain
            .clone()
            .all(|item| item.verb.is_none() && item.help.is_none())
    );
    let verbs_only = Strings {
        verb: true,
        help: false,
    };
    let items = shell.items(&[], verbs_only, &never).unwrap();
    assert!(verbs(&items).iter().any(|verb| verb == "delete"));
    assert!((items.iter().filter(|item| !item.owner_drawn)).all(|item| item.help.is_none()));
}

#[test]
fn files_and_a_folder_of_one_folder_and_a_drives_root_have_menus() {
    let place = place();
    let mut shell = Shell::start().unwrap();
    let paths = ["a.txt", "b.txt", "sub"].map(|name| place.path().join(name));
    shell.menu_for_paths(&paths, &never).unwrap();
    assert!(!shell.items(&[], BOTH, &never).unwrap().is_empty());
    let drive: String = place.path().to_string_lossy().chars().take(2).collect();
    shell
        .menu_for_paths(&[PathBuf::from(format!("{drive}\\"))], &never)
        .unwrap();
    assert!(
        !shell.items(&[], BOTH, &never).unwrap().is_empty(),
        "a drive's own menu"
    );
    assert!(
        shell
            .menu_for_paths(&[place.path().join("no such file")], &never)
            .is_err()
    );
    assert!(
        shell.items(&[], BOTH, &never).is_err(),
        "no menu open after a failure"
    );
}

#[test]
fn this_pcs_items_have_the_file_systems_first_and_menus() {
    let mut shell = Shell::start().unwrap();
    let (title, names) = shell.computer(&never).unwrap();
    assert!(!title.is_empty());
    assert!(!names.is_empty(), "the system drive at least");
    shell.menu_for_computer(0, &never).unwrap();
    assert!(!shell.items(&[], BOTH, &never).unwrap().is_empty());
    shell.computer(&never).unwrap();
    assert!(
        shell.items(&[], BOTH, &never).is_err(),
        "the menu goes before the lists it was made of"
    );
    assert!(shell.menu_for_computer(names.len(), &never).is_err());
}

#[test]
fn a_gui_menu_cancelled_before_it_shows_is_never_shown() {
    let place = place();
    let mut shell = Shell::start().unwrap();
    shell
        .menu_for_paths(&[place.path().join("a.txt")], &never)
        .unwrap();
    shell.shown.store(gui::CANCELLED, Ordering::SeqCst);
    let chosen = shell.call(Ask::Track(GuiAt::Mouse), &never).unwrap();
    assert!(matches!(chosen, Answer::Chosen(None)));
}

#[test]
fn esc_on_a_call_that_does_not_answer_lets_the_thread_go() {
    let mut shell = Shell::start().unwrap();
    let started = Instant::now();
    let stopped = shell.call(Ask::Sleep(Duration::from_secs(3)), &|| true);
    assert_eq!(
        stopped.err().map(|error| error.kind()),
        Some(io::ErrorKind::Interrupted)
    );
    assert!(
        started.elapsed() < Duration::from_millis(2500),
        "not waited for"
    );
    assert!(shell.abandoned(), "let go");
    assert_eq!(
        shell.items(&[], BOTH, &never).unwrap_err().kind(),
        io::ErrorKind::Interrupted,
        "nothing more asked of it"
    );
    let ending = Arc::clone(&shell.ending);
    drop(shell);
    let deadline = Instant::now() + Duration::from_secs(10);
    while !ending.lock().unwrap().finished {
        assert!(Instant::now() < deadline, "the thread ends by itself");
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn a_call_answered_in_time_is_not_stopped() {
    let mut shell = Shell::start().unwrap();
    let answered = shell.call(Ask::Sleep(Duration::from_millis(100)), &never);
    assert!(matches!(answered, Ok(Answer::Done)));
    assert!(!shell.abandoned());
}
