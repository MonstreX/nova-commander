//! Folders read in the background (`panel_reading`).

use super::*;

#[test]
fn a_slow_folder_is_entered_when_it_arrives_and_esc_keeps_what_was_read() {
    let (temp, mut panel) = sample();
    panel.read_wait = Duration::ZERO;
    panel.change_dir(&endless(), None).unwrap();
    assert_eq!(
        panel.path,
        temp.path(),
        "the keys act on the old folder meanwhile"
    );
    assert_eq!(panel.entries.len(), 4);

    poll_until(&mut panel, |panel, _| panel.reading() == Some(2));
    assert_eq!(panel.reading(), Some(2), "FAR's Reading: 2 files");
    panel.stop_reading();

    assert_eq!(panel.reading(), None);
    assert_eq!(panel.path, endless());
    assert_eq!(current_name(&panel), "first");
    assert_eq!(panel.entries.len(), 2);
}

#[test]
fn a_reload_in_the_background_keeps_the_entries_and_the_cursor_where_it_is_then() {
    let (_temp, mut panel) = sample();
    panel.read_wait = Duration::ZERO;
    panel.change_dir(&endless(), None).unwrap();
    panel.stop_reading();
    panel.toggle_selection();
    assert_eq!(current_name(&panel), "second");

    panel.reload();
    assert!(panel.reading().is_some());
    assert_eq!(panel.entries.len(), 2, "the old entries stay while reading");
    panel.home();
    panel.stop_reading();

    assert_eq!(current_name(&panel), "first", "where the cursor is now");
    assert!(panel.selected.contains("first"), "the selection kept");
}

#[test]
fn a_reload_stopped_part_way_goes_back_to_the_entries_and_selection_before() {
    let (temp, mut panel) = sample();
    panel.read_wait = Duration::ZERO;
    // ".." is passed over; the folder after it is selected.
    panel.toggle_selection();
    panel.toggle_selection();
    let before: Vec<String> = panel.entries.iter().map(|e| e.name.clone()).collect();
    let selected = panel.selected.clone();
    assert_eq!(selected.len(), 1, "a name the folder read again has not");
    let cursor = panel.cursor;
    // Read again from a folder that yields two names and then hangs: a part of what it would be.
    panel.path = endless();
    panel.reload();
    poll_until(&mut panel, |panel, _| panel.reading() == Some(2));
    assert_eq!(panel.reading(), Some(2));

    panel.stop_reading();

    assert_eq!(panel.reading(), None);
    let after: Vec<String> = panel.entries.iter().map(|e| e.name.clone()).collect();
    assert_eq!(after, before, "the part read does not replace the list");
    assert_eq!(panel.selected, selected, "the selection whole");
    assert_eq!(panel.cursor, cursor);
    assert_eq!(panel.path, endless(), "the folder is the same one");
    drop(temp);
}

#[test]
fn a_reading_stopped_before_anything_came_leaves_the_panel_where_it_was() {
    let (temp, mut panel) = sample();
    panel.read_wait = Duration::ZERO;
    panel
        .change_dir(&crate::vfs::testing::gate("never-opened"), None)
        .unwrap();
    panel.stop_reading();
    assert_eq!(panel.reading(), None);
    assert_eq!(panel.path, temp.path());
    assert_eq!(panel.entries.len(), 4);

    panel
        .change_dir(&crate::vfs::testing::gate("never-opened-reload"), None)
        .unwrap();
    panel.path = crate::vfs::testing::gate("never-opened-reload");
    panel.reload();
    panel.stop_reading();
    assert_eq!(
        panel.entries.len(),
        4,
        "a stopped reload keeps what is shown"
    );
}

#[test]
fn a_folder_that_fails_late_leaves_the_panel_where_it_was() {
    let (temp, mut panel) = sample();
    panel.read_wait = Duration::ZERO;
    let missing = VfsPath::parse("nosuch://host/");
    // The worker may answer within the zero wait already: then `change_dir` reports it.
    let failed_at_once = panel.change_dir(&missing, None).is_err();
    let failed_later = std::cell::Cell::new(false);
    if !failed_at_once {
        poll_until(&mut panel, |_, polled| {
            failed_later.set(matches!(polled, Polled::Failed(..)));
            failed_later.get()
        });
    }
    assert!(failed_at_once || failed_later.get());
    assert_eq!(panel.path, temp.path());
    assert_eq!(panel.error, None);
}

#[test]
fn a_newer_folder_supersedes_a_slow_one_and_a_reload_does_not() {
    let (temp, mut panel) = sample();
    panel.read_wait = Duration::ZERO;
    panel.change_dir(&endless(), None).unwrap();
    assert!(panel.reading().is_some());
    panel.reload();
    assert!(
        panel.reading().is_some(),
        "a reload does not undo entering a folder"
    );
    panel.read_wait = READ_WAIT;
    panel
        .change_dir(&VfsPath::local(temp.path()), None)
        .unwrap();
    assert_eq!(panel.reading(), None, "the slow reading was dropped");
    assert_eq!(panel.path, temp.path());
    assert!(matches!(panel.poll(), Polled::Quiet));
}

#[test]
fn a_stopped_reload_after_ctrl_h_puts_the_old_list_through_it() {
    let (temp, mut panel) = sample();
    let hidden = temp.path().join("secret.txt");
    std::fs::write(&hidden, "x").unwrap();
    let status = std::process::Command::new("attrib")
        .arg("+h")
        .arg(&hidden)
        .status()
        .unwrap();
    assert!(status.success());
    panel.set_show_hidden(true);
    assert!(panel.entries.iter().any(|entry| entry.name == "secret.txt"));
    panel.selected.insert("secret.txt".to_string());
    panel.selected.insert("a.txt".to_string());
    panel.read_wait = Duration::ZERO;
    // Read again from a folder that never answers: Ctrl+H's reading stopped.
    panel.path = crate::vfs::testing::gate("never-opened-hidden");
    panel.set_show_hidden(false);
    panel.stop_reading();
    assert!(
        !panel.entries.iter().any(|entry| entry.name == "secret.txt"),
        "hidden as Ctrl+H asked"
    );
    assert!(panel.entries.iter().any(|entry| entry.name == "a.txt"));
    assert_eq!(
        panel.selected.iter().collect::<Vec<_>>(),
        ["a.txt"],
        "the selection kept on what stays"
    );
}

/// `a.txt` selected, the cursor on `b.txt` (the last entry), a folder `made` new on the disk.
fn selected_and_made() -> (tempfile::TempDir, Panel) {
    let (temp, mut panel) = sample();
    panel.selected.insert("a.txt".to_string());
    panel.end();
    assert_eq!(current_name(&panel), "b.txt");
    std::fs::create_dir(temp.path().join("made")).unwrap();
    (temp, panel)
}

fn selected(panel: &Panel) -> Vec<&str> {
    panel.selected.iter().map(String::as_str).collect()
}

/// FAR's `Update(UPDATE_KEEP_SELECTION)` then `GoToFile` after F7: the folder read again shows the new
/// name with the cursor on it, the selection kept.
#[test]
fn a_reload_onto_a_name_puts_the_cursor_on_it_and_keeps_the_selection() {
    let (_temp, mut panel) = selected_and_made();
    panel.reload_onto("made");
    poll_until(&mut panel, |panel, _| !panel.is_reading());
    assert_eq!(current_name(&panel), "made");
    assert_eq!(selected(&panel), ["a.txt"]);
}

/// Read in the background (nothing waited for): the name is kept with the reading and the cursor put on
/// it as it ends, not left where it was.
#[test]
fn a_reload_onto_a_name_in_the_background_puts_the_cursor_on_it_as_it_ends() {
    let (_temp, mut panel) = selected_and_made();
    panel.read_wait = Duration::ZERO;
    panel.reload_onto("made");
    assert!(
        panel.selected.contains("a.txt"),
        "kept while it is read too"
    );
    poll_until(&mut panel, |panel, _| !panel.is_reading());
    assert_eq!(current_name(&panel), "made");
    assert_eq!(selected(&panel), ["a.txt"]);
    // A plain reload after it keeps the cursor where it is now, the name not remembered.
    panel.home();
    panel.reload();
    poll_until(&mut panel, |panel, _| !panel.is_reading());
    assert_eq!(current_name(&panel), "..");
}

/// The name not there (a plugin's name of a folder deeper down): the cursor stays on the entry it is on,
/// the selection kept.
#[test]
fn a_reload_onto_a_missing_name_leaves_the_cursor_on_its_entry() {
    let (_temp, mut panel) = sample();
    panel.selected.insert("a.txt".to_string());
    panel.end();
    panel.reload_onto("missing");
    poll_until(&mut panel, |panel, _| !panel.is_reading());
    assert_eq!(current_name(&panel), "b.txt");
    assert_eq!(selected(&panel), ["a.txt"]);
}

/// The name not there and a new entry before the cursor's: the cursor stays on its entry's name as FAR's
/// `Update(UPDATE_KEEP_SELECTION)` keeps it (`GoToFile` of a missing name moves nothing), as `reload`
/// does - not on the row it was on.
#[test]
fn a_reload_onto_a_missing_name_keeps_the_cursor_on_its_name_when_rows_move() {
    let (temp, mut panel) = selected_and_made();
    std::fs::write(temp.path().join("0.txt"), "").unwrap();
    panel.reload_onto("missing");
    poll_until(&mut panel, |panel, _| !panel.is_reading());
    assert_eq!(current_name(&panel), "b.txt");
}

/// While another folder is being entered a reload onto a name does nothing: the entering goes on.
#[test]
fn a_reload_onto_a_name_does_not_undo_entering_a_folder() {
    let (temp, mut panel) = sample();
    panel.read_wait = Duration::ZERO;
    panel.change_dir(&endless(), None).unwrap();
    panel.reload_onto("a.txt");
    assert_eq!(panel.reading_target(), Some(&endless()), "still entering");
    assert_eq!(panel.path, temp.path());
    panel.stop_reading();
    assert_eq!(panel.path, endless());
}

/// A plugin's own panel read at its asking: no "Reading" in the title for its first moment, then shown
/// as for any reading that goes on.
#[test]
fn a_quiet_reading_tells_of_itself_only_after_a_moment() {
    let (_temp, mut panel) = sample();
    panel.read_wait = Duration::ZERO;
    panel.path = endless();
    panel.reload();
    panel.quiet_reading();
    assert!(panel.is_reading(), "it is read");
    assert_eq!(panel.reading(), None, "the title not told at first");
    poll_until(&mut panel, |panel, _| panel.reading() == Some(2));
    assert_eq!(panel.reading(), Some(2), "a reading that goes on is told");
}
