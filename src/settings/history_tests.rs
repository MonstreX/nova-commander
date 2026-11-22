use super::*;

const NOW: u64 = 1_000_000_000;

fn entry(time: u64, text: &str) -> Entry {
    Entry {
        time,
        ..Entry::new(text, Kind::Plain)
    }
}

fn texts(entries: &[Entry]) -> Vec<&str> {
    entries.iter().map(|entry| entry.text.as_str()).collect()
}

#[test]
fn lines_of_one_second_keep_their_order_when_put_onto_the_file() {
    let mut pending = Pending::default();
    pending.take(Change::Add(List::Commands.into(), entry(NOW, "one")));
    pending.take(Change::Add(List::Commands.into(), entry(NOW, "two")));
    let file = Histories {
        commands: vec![entry(NOW, "one")],
        ..Histories::default()
    };
    let saved = pending.apply(file, NOW);
    assert_eq!(texts(&saved.commands), ["one", "two"]);
}

#[test]
fn a_line_given_again_moves_to_the_end_whatever_its_case() {
    let kept = compact(
        vec![
            entry(NOW - 3, "dir"),
            entry(NOW - 2, "cd x"),
            entry(NOW - 1, "DIR"),
        ],
        NOW,
    );
    if crate::platform::files::NAMES_DIFFER_BY_CASE {
        let all = [
            entry(NOW - 3, "dir"),
            entry(NOW - 2, "cd x"),
            entry(NOW - 1, "DIR"),
        ];
        assert_eq!(kept, all, "two commands where case matters");
    } else {
        assert_eq!(kept, [entry(NOW - 2, "cd x"), entry(NOW - 1, "DIR")]);
    }
}

#[test]
fn a_line_given_again_keeps_its_lock() {
    let locked = Entry {
        locked: true,
        ..entry(NOW - 5, "dir")
    };
    let kept = compact(vec![locked.clone(), entry(NOW, "DIR")], NOW);
    assert_eq!(kept.len(), 1);
    assert!(kept[0].locked && kept[0].text == "DIR");

    let mut histories = Histories::default();
    histories.add(List::Commands, locked);
    histories.add(List::Commands, entry(NOW, "dir"));
    assert!(histories.commands[0].locked);
}

#[test]
fn a_view_and_an_edit_of_one_file_are_two_lines() {
    let view = Entry {
        kind: Kind::View,
        ..entry(NOW - 1, "a.txt")
    };
    let edit = Entry {
        kind: Kind::Edit,
        ..entry(NOW, "A.TXT")
    };
    assert_eq!(compact(vec![view, edit], NOW).len(), 2);
}

#[test]
fn old_lines_stay_while_they_are_among_the_latest_thousand() {
    let old = NOW - LIFETIME - 10;
    let kept = compact(vec![entry(old, "old")], NOW);
    assert_eq!(kept.len(), 1, "few lines: nothing is lost to age");
}

#[test]
fn beyond_the_thousand_only_the_recent_and_the_locked_stay() {
    let old = NOW - LIFETIME - 10_000;
    let mut entries: Vec<Entry> = (0..1005)
        .map(|index| entry(old + index, &format!("old {index}")))
        .collect();
    let kept = compact(entries.clone(), NOW);
    assert_eq!(kept.len(), KEPT);
    assert_eq!(kept[0].text, "old 5", "the oldest go first");

    entries[0].locked = true;
    let kept = compact(entries, NOW);
    assert_eq!(kept.len(), KEPT);
    assert_eq!(texts(&kept[..2]), ["old 0", "old 6"], "a locked one stays");

    // Recent ones are not limited by the count.
    let recent = NOW - 100;
    let entries: Vec<Entry> = (0..1005)
        .map(|index| entry(recent + index, &format!("new {index}")))
        .collect();
    assert_eq!(compact(entries, NOW + 1000).len(), 1005);
}

#[test]
fn a_missing_file_is_an_empty_history_and_a_broken_one_an_error() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("history.json");
    assert_eq!(load(&path).unwrap(), Histories::default());
    fs::write(&path, "not json").unwrap();
    assert_eq!(load(&path).unwrap_err().kind(), io::ErrorKind::InvalidData);
}

#[test]
fn a_file_of_the_commands_only_is_read() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("history.json");
    fs::write(&path, r#"{"commands":[{"time":5,"text":"dir"}]}"#).unwrap();
    let histories = load(&path).unwrap();
    assert_eq!(histories.commands, [entry(5, "dir")]);
    assert!(histories.folders.is_empty() && histories.views.is_empty());
}

#[test]
fn the_writer_saves_what_it_is_given_and_makes_the_folder() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("settings").join("history.json");
    let writer = Writer::start(path.clone());
    writer.add(List::Commands, &entry(NOW, "before"));
    writer.add(List::Commands, &Entry::new("first", Kind::Plain));
    writer.add(List::Folders, &Entry::new(r"C:\work", Kind::Plain));
    writer.add(List::Views, &Entry::new(r"C:\a.txt", Kind::Edit));
    writer.finish();

    let saved = load(&path).unwrap();
    assert_eq!(texts(&saved.commands), ["before", "first"]);
    assert_eq!(texts(&saved.folders), [r"C:\work"]);
    assert_eq!(saved.views[0].kind, Kind::Edit);
}

#[test]
fn two_copies_of_the_program_keep_each_others_lines() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("history.json");
    let first = Writer::start(path.clone());
    let second = Writer::start(path.clone());
    first.add(List::Commands, &Entry::new("from the first", Kind::Plain));
    first.finish();
    second.add(List::Folders, &Entry::new("from the second", Kind::Plain));
    second.finish();

    let saved = load(&path).unwrap();
    assert_eq!(texts(&saved.commands), ["from the first"]);
    assert_eq!(texts(&saved.folders), ["from the second"]);
}

#[test]
fn what_one_copy_takes_out_stays_out_of_the_file() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("history.json");
    let kept = Entry {
        locked: true,
        ..entry(NOW - 2, "kept")
    };
    let gone = entry(NOW - 1, "gone");
    let other = entry(NOW, "other");
    let all = Histories {
        commands: vec![kept.clone(), gone.clone(), other],
        ..Histories::default()
    };
    save(&path, &all).unwrap();

    let writer = Writer::start(path.clone());
    writer.remove(List::Commands, &gone);
    writer.finish();
    assert_eq!(texts(&load(&path).unwrap().commands), ["kept", "other"]);

    let writer = Writer::start(path.clone());
    writer.clear_unlocked(List::Commands);
    writer.finish();
    assert_eq!(load(&path).unwrap().commands, [kept]);
}

#[test]
fn an_unlocked_line_is_saved_unlocked() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("history.json");
    let mut line = Entry {
        locked: true,
        ..entry(NOW, "dir")
    };
    let all = Histories {
        commands: vec![line.clone()],
        ..Histories::default()
    };
    save(&path, &all).unwrap();
    let writer = Writer::start(path.clone());
    line.locked = false;
    writer.set_lock(List::Commands, &line);
    writer.finish();
    assert!(!load(&path).unwrap().commands[0].locked);
}

#[test]
fn a_failure_to_save_is_told_once() {
    let temp = tempfile::tempdir().unwrap();
    // A file where the folder should be.
    let blocker = temp.path().join("settings");
    fs::write(&blocker, "").unwrap();
    let writer = Writer::start(blocker.join("history.json"));
    writer.add(List::Commands, &Entry::new("x", Kind::Plain));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    let mut failure = writer.take_failure();
    while failure.is_none() && std::time::Instant::now() < deadline {
        std::thread::yield_now();
        failure = writer.take_failure();
    }
    assert!(failure.is_some());
    assert_eq!(writer.take_failure(), None);
    writer.finish();
}

/// Two copies started on one file, as the program starts them: neither holds the file's lines.
fn two_writers(path: &std::path::Path, lines: &[Entry]) -> (Writer, Writer) {
    let all = Histories {
        commands: lines.to_vec(),
        ..Histories::default()
    };
    save(path, &all).unwrap();
    (
        Writer::start(path.to_path_buf()),
        Writer::start(path.to_path_buf()),
    )
}

#[test]
fn what_one_copy_takes_out_the_other_does_not_bring_back() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("history.json");
    let gone = entry(NOW - 1, "gone");
    let (first, second) = two_writers(&path, std::slice::from_ref(&gone));
    first.remove(List::Commands, &gone);
    first.finish();
    second.add(List::Commands, &Entry::new("new", Kind::Plain));
    second.finish();
    assert_eq!(texts(&load(&path).unwrap().commands), ["new"]);

    let (first, second) = two_writers(&path, &[entry(NOW - 2, "a"), entry(NOW - 1, "b")]);
    first.clear_unlocked(List::Commands);
    first.finish();
    second.add(List::Folders, &Entry::new(r"C:\x", Kind::Plain));
    second.finish();
    assert!(load(&path).unwrap().commands.is_empty());
}

#[test]
fn what_one_copy_unlocks_the_other_does_not_lock_again() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("history.json");
    let mut line = Entry {
        locked: true,
        ..entry(NOW - 1, "dir")
    };
    let (first, second) = two_writers(&path, &[line.clone()]);
    line.locked = false;
    first.set_lock(List::Commands, &line);
    first.finish();
    second.add(List::Commands, &Entry::new("other", Kind::Plain));
    second.finish();
    let saved = load(&path).unwrap();
    assert_eq!(texts(&saved.commands), ["dir", "other"]);
    assert!(!saved.commands[0].locked);
}

#[test]
fn a_file_broken_meanwhile_is_not_saved_over_and_is_told() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("history.json");
    let writer = Writer::start(path.clone());
    fs::write(&path, "broken").unwrap();
    writer.add(List::Commands, &Entry::new("x", Kind::Plain));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    let mut failure = writer.take_failure();
    while failure.is_none() && std::time::Instant::now() < deadline {
        std::thread::yield_now();
        failure = writer.take_failure();
    }
    assert!(failure.is_some());
    writer.finish();
    assert_eq!(fs::read_to_string(&path).unwrap(), "broken");
}

#[test]
fn a_line_taken_out_and_given_again_stays() {
    let mut pending = Pending::default();
    let line = entry(NOW, "dir");
    pending.take(Change::Remove(List::Commands.into(), line.clone()));
    pending.take(Change::Add(List::Commands.into(), line.clone()));
    let file = Histories {
        commands: vec![line],
        ..Histories::default()
    };
    assert_eq!(texts(&pending.apply(file, NOW).commands), ["dir"]);
}

#[test]
fn a_lock_then_del_in_one_save_keeps_the_line_and_an_unlock_then_del_drops_it() {
    let file = |locked| Histories {
        commands: vec![Entry {
            locked,
            ..entry(NOW - 1, "dir")
        }],
        ..Histories::default()
    };
    let line = |locked| Entry {
        locked,
        ..entry(NOW - 1, "dir")
    };
    let mut pending = Pending::default();
    pending.take(Change::Lock(List::Commands.into(), line(true)));
    pending.take(Change::Clear(List::Commands.into(), NOW));
    assert_eq!(texts(&pending.apply(file(false), NOW).commands), ["dir"]);

    let mut pending = Pending::default();
    pending.take(Change::Lock(List::Commands.into(), line(false)));
    pending.take(Change::Clear(List::Commands.into(), NOW));
    assert!(pending.apply(file(true), NOW).commands.is_empty());
}

#[test]
fn a_dialogs_lines_are_kept_by_its_name_and_an_emptied_one_leaves_the_file() {
    let masks = || Key::Dialog("Masks".to_string());
    let mut pending = Pending::default();
    pending.take(Change::Add(masks(), entry(NOW - 1, "*.rs")));
    pending.take(Change::Add(masks(), entry(NOW, "*.md")));
    pending.take(Change::Add(Key::Dialog("Copy".to_string()), entry(NOW, "")));
    let saved = pending.apply(Histories::default(), NOW);
    assert_eq!(texts(saved.dialog("Masks")), ["*.rs", "*.md"]);
    assert_eq!(
        texts(saved.dialog("Copy")),
        [""],
        "FAR keeps a dialog's empty line"
    );
    assert!(saved.dialog("SearchText").is_empty() && saved.commands.is_empty());
    let text = serde_json::to_string(&saved).unwrap();
    assert_eq!(serde_json::from_str::<Histories>(&text).unwrap(), saved);
    let mut pending = Pending::default();
    pending.take(Change::Clear(Key::Dialog("Copy".to_string()), NOW));
    let cleared = pending.apply(saved, NOW);
    assert!(
        !cleared.dialogs.contains_key("Copy"),
        "{:?}",
        cleared.dialogs
    );
    assert_eq!(texts(cleared.dialog("Masks")).len(), 2);
}
