use super::*;
use std::time::Instant;

use crate::vfs::testing::endless;
use crate::vfs::{SortMode, SortOrder};

/// Polls the panel until `done` holds (the reading runs on its own thread), at most 10 s.
fn poll_until(panel: &mut Panel, done: impl Fn(&Panel, &Polled) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let polled = panel.poll();
        if done(panel, &polled) || Instant::now() > deadline {
            return;
        }
        std::thread::yield_now();
    }
}

#[path = "panel_reading_tests.rs"]
mod reading;

fn sample() -> (tempfile::TempDir, Panel) {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir(temp.path().join("docs")).unwrap();
    std::fs::write(temp.path().join("a.txt"), "aaa").unwrap();
    std::fs::write(temp.path().join("b.txt"), "b").unwrap();
    let panel = Panel::new(VfsPath::local(temp.path()));
    (temp, panel)
}

fn current_name(panel: &Panel) -> &str {
    panel.current().map_or("", |entry| entry.name.as_str())
}

#[test]
fn enters_a_folder_link_as_a_folder_and_comes_back_to_the_link() {
    let (temp, _) = sample();
    std::fs::write(temp.path().join("docs").join("inside.txt"), "x").unwrap();
    let status = std::process::Command::new("cmd")
        .args(["/C", "mklink", "/J", "zlink", "docs"])
        .current_dir(temp.path())
        .output()
        .unwrap()
        .status;
    assert!(status.success());
    let mut panel = Panel::new(VfsPath::local(temp.path()));
    panel.move_cursor(2);
    assert_eq!(current_name(&panel), "zlink", "sorted with the folders");

    assert!(panel.enter().unwrap());
    assert_eq!(panel.path, temp.path().join("zlink"));
    assert!(panel.entries.iter().any(|entry| entry.name == "inside.txt"));
    panel.go_parent().unwrap();
    assert_eq!(current_name(&panel), "zlink");
}

#[test]
fn enters_directory_and_returns_to_it_from_parent() {
    let (temp, mut panel) = sample();
    panel.move_cursor(1);
    assert_eq!(current_name(&panel), "docs");

    assert!(panel.enter().unwrap());
    assert_eq!(panel.path, temp.path().join("docs"));
    assert_eq!(current_name(&panel), "..");

    assert!(panel.enter().unwrap());
    assert_eq!(panel.path, temp.path());
    assert_eq!(current_name(&panel), "docs");
}

#[test]
fn enter_on_file_does_not_change_directory() {
    let (temp, mut panel) = sample();
    panel.end();
    assert!(!panel.enter().unwrap());
    assert_eq!(panel.path, temp.path());
}

#[test]
fn selection_skips_parent_and_drives_targets() {
    let (temp, mut panel) = sample();
    panel.toggle_selection();
    assert!(panel.selected.is_empty());

    panel.toggle_selection();
    panel.toggle_selection();
    assert_eq!(panel.selected.len(), 2);
    assert_eq!(
        panel.targets(),
        vec![temp.path().join("docs"), temp.path().join("a.txt")],
        "in the panel's order, the folder first (review R41)"
    );
    panel.toggle_selection();
    panel.set_sort_order(SortOrder {
        reverse: true,
        directories_first: false,
        ..panel.sort
    });
    assert_eq!(
        panel.targets(),
        vec![
            temp.path().join("docs"),
            temp.path().join("b.txt"),
            temp.path().join("a.txt")
        ],
        "sorted backwards: taken backwards"
    );
}

#[test]
fn targets_fall_back_to_cursor_but_never_parent() {
    let (temp, mut panel) = sample();
    assert!(panel.targets().is_empty());
    panel.end();
    assert_eq!(panel.targets(), vec![temp.path().join("b.txt")]);
}

#[test]
fn reload_keeps_cursor_and_drops_vanished_selection() {
    let (temp, mut panel) = sample();
    panel.end();
    panel.toggle_selection();
    panel.move_cursor(-1);
    assert_eq!(current_name(&panel), "a.txt");
    std::fs::remove_file(temp.path().join("b.txt")).unwrap();
    std::fs::write(temp.path().join("0.txt"), "").unwrap();

    panel.reload();

    assert_eq!(current_name(&panel), "a.txt");
    assert!(panel.selected.is_empty());
}

#[test]
fn sort_change_keeps_cursor_on_same_entry() {
    let (_temp, mut panel) = sample();
    panel.end();
    assert_eq!(current_name(&panel), "b.txt");
    panel.set_sort(SortMode::Size);
    assert_eq!(current_name(&panel), "b.txt");
    assert_eq!(panel.entries[2].name, "a.txt");
}

#[test]
fn selecting_with_selected_first_moves_the_entry_up() {
    let (_temp, mut panel) = sample();
    panel.toggle_directories_first();
    panel.toggle_selected_first();
    panel.end();
    assert_eq!(current_name(&panel), "docs");
    panel.toggle_selection();
    assert_eq!(panel.entries[1].name, "docs", "selected entries come first");
    assert_eq!(
        current_name(&panel),
        "docs",
        "the cursor stays on the same entry"
    );
}

#[test]
fn same_sort_key_twice_reverses_as_in_far() {
    let (_temp, mut panel) = sample();
    panel.set_sort(SortMode::Size);
    assert!(panel.sort.reverse, "size starts largest first");
    panel.set_sort(SortMode::Size);
    assert!(!panel.sort.reverse);
    assert_eq!(panel.entries[2].name, "b.txt", "smallest first now");
    panel.set_sort(SortMode::Name);
    assert!(
        !panel.sort.reverse,
        "another mode starts in its own direction"
    );
    panel.set_sort_direction(SortMode::Name, true);
    assert_eq!(panel.entries[2].name, "b.txt");
    panel.toggle_directories_first();
    assert!(!panel.sort.directories_first);
    assert_eq!(
        panel.entries[1].name, "docs",
        "reverse name order without grouping"
    );
}

#[test]
fn failed_change_dir_keeps_previous_directory() {
    let (temp, mut panel) = sample();
    assert!(
        panel
            .change_dir(&VfsPath::local(temp.path().join("missing")), None)
            .is_err()
    );
    assert_eq!(panel.path, temp.path());
}

#[test]
fn scrolling_follows_the_cursor() {
    let (_temp, mut panel) = sample();
    panel.set_view_metrics(2, 1);
    panel.end();
    panel.set_view_metrics(2, 1);
    assert_eq!(panel.offset, 2);
    // FAR pages by one screen minus one entry: 2 rows -> 1 entry, view and cursor together.
    panel.page(-1);
    assert_eq!(panel.cursor, 2);
    assert_eq!(panel.offset, 1);
}

fn many(count: usize) -> (tempfile::TempDir, Panel) {
    let temp = tempfile::tempdir().unwrap();
    for index in 0..count {
        std::fs::write(temp.path().join(format!("f{index:02}.txt")), "").unwrap();
    }
    let panel = Panel::new(VfsPath::local(temp.path()));
    (temp, panel)
}

#[test]
fn columns_move_by_height_and_scroll_by_a_column() {
    // ".." + 20 files, Brief-like view: 3 stripes of 4 rows.
    let (_temp, mut panel) = many(20);
    panel.set_view_metrics(4, 3);
    panel.move_column(1);
    assert_eq!(panel.cursor, 4);
    panel.move_column(1);
    panel.move_column(1);
    assert_eq!(panel.cursor, 12);
    assert_eq!(
        panel.offset, 4,
        "the view shifts by one column at the right edge"
    );
    panel.move_column(-1);
    assert_eq!(panel.cursor, 8);
    assert_eq!(panel.offset, 4);
    panel.move_column(-1);
    panel.move_column(-1);
    assert_eq!(panel.cursor, 0);
    assert_eq!(panel.offset, 0);
}

#[test]
fn pages_cover_all_stripes() {
    let (_temp, mut panel) = many(30);
    panel.set_view_metrics(5, 2);
    panel.page(1);
    assert_eq!(panel.cursor, 9);
    assert_eq!(panel.offset, 9);
    panel.end();
    panel.set_view_metrics(5, 2);
    assert_eq!(panel.offset, 21, "the last screen is full");
}

fn names(panel: &Panel) -> Vec<&str> {
    panel
        .entries
        .iter()
        .map(|entry| entry.name.as_str())
        .collect()
}

/// An automatic refresh read in the background, taken when it ends.
fn refreshed(panel: &mut Panel) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while panel.reading.is_some() {
        assert!(Instant::now() < deadline, "the refresh did not end");
        panel.poll();
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn a_folder_changed_on_disk_is_read_again_keeping_the_cursor_and_the_selection() {
    use crate::vfs::testing::put;
    let dir = put("changing://kept/", &["a", "b", "c"]);
    let mut panel = Panel::new(dir.clone());
    panel.cursor = 1;
    panel.toggle_selection();
    panel.cursor = 2;
    assert!(!panel.refresh_if_changed(true, 0), "nothing changed yet");

    put("changing://kept/", &["a", "b", "c", "d"]);
    assert!(panel.refresh_if_changed(true, 0));
    assert_eq!(
        panel.reading(),
        None,
        "the title says nothing of it at once"
    );
    refreshed(&mut panel);

    assert_eq!(names(&panel), vec!["a", "b", "c", "d"]);
    assert_eq!(current_name(&panel), "c");
    assert!(panel.selected.contains("b"));
}

#[test]
fn a_change_while_the_panel_is_not_on_screen_waits_for_it() {
    use crate::vfs::testing::put;
    let dir = put("changing://hidden/", &["a"]);
    let mut panel = Panel::new(dir);
    put("changing://hidden/", &["a", "b"]);

    assert!(!panel.refresh_if_changed(false, 0));
    assert!(!panel.refresh_if_changed(false, 0));
    assert_eq!(names(&panel), vec!["a"]);
    assert!(panel.refresh_if_changed(true, 0), "shown again");
    refreshed(&mut panel);
    assert_eq!(names(&panel), vec!["a", "b"]);
}

#[test]
fn changes_are_shown_at_most_once_a_second() {
    use crate::vfs::testing::put;
    let dir = put("changing://often/", &["a"]);
    let mut panel = Panel::new(dir);
    put("changing://often/", &["a", "b"]);
    assert!(panel.refresh_if_changed(true, 0));
    refreshed(&mut panel);

    put("changing://often/", &["a", "b", "c"]);
    assert!(!panel.refresh_if_changed(true, 0), "within a second");
    assert_eq!(names(&panel), vec!["a", "b"]);
    panel.last_auto_refresh = Some(Instant::now() - AUTO_REFRESH_GAP);
    assert!(panel.refresh_if_changed(true, 0), "the change waited");
    refreshed(&mut panel);
    assert_eq!(names(&panel), vec!["a", "b", "c"]);
}

#[test]
fn a_panel_with_more_entries_than_the_limit_is_not_updated() {
    use crate::vfs::testing::put;
    let dir = put("changing://limit/", &["a", "b"]);
    let mut panel = Panel::new(dir);
    put("changing://limit/", &["a", "b", "c"]);
    assert!(!panel.refresh_if_changed(true, 1));
    assert_eq!(names(&panel), vec!["a", "b"]);
    assert!(
        !panel.refresh_if_changed(true, 0),
        "the change is dropped, not kept for later"
    );
    put("changing://limit/", &["a", "b", "c", "d"]);
    assert!(panel.refresh_if_changed(true, 3), "within the limit");
}

#[test]
fn a_folder_gone_leaves_the_panel_in_the_nearest_existing_one_above() {
    use crate::vfs::testing::{put, remove};
    put("changing://gone/", &["a", "x"]);
    put("changing://gone/a", &["b"]);
    let deep = put("changing://gone/a/b", &["file"]);
    let mut panel = Panel::new(deep);
    remove("changing://gone/a/b");
    remove("changing://gone/a");

    assert!(panel.refresh_if_changed(true, 0));
    refreshed(&mut panel);
    assert_eq!(panel.path.to_string(), "changing://gone/");
    assert_eq!(names(&panel), vec!["a", "x"]);
    assert_eq!(panel.error, None);
    put("changing://gone/", &["x"]);
    panel.last_auto_refresh = None;
    assert!(
        panel.refresh_if_changed(true, 0),
        "the folder above is watched"
    );
}

#[test]
fn a_change_of_a_real_folder_reaches_its_panel() {
    let (temp, mut panel) = sample();
    std::fs::write(temp.path().join("new.txt"), b"x").unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while !panel.entries.iter().any(|entry| entry.name == "new.txt") && Instant::now() < deadline {
        panel.last_auto_refresh = None;
        panel.refresh_if_changed(true, 0);
        panel.poll();
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(panel.entries.iter().any(|entry| entry.name == "new.txt"));
}

#[test]
fn a_folder_that_turns_unreadable_is_read_once_more_a_moment_later() {
    use crate::vfs::testing::{deny, put, remove};
    put("changing://pending/", &["a"]);
    let dir = put("changing://pending/a", &["file"]);
    let mut panel = Panel::new(dir);
    // Deleted while another program holds it: access is denied until it is gone.
    deny("changing://pending/a", true);
    assert!(panel.refresh_if_changed(true, 0));
    refreshed(&mut panel);
    assert!(panel.error.is_some());
    assert!(
        !panel.refresh_if_changed(true, 0),
        "not at once: a moment later"
    );
    remove("changing://pending/a");
    deny("changing://pending/a", false);
    panel.last_auto_refresh = Some(Instant::now() - AUTO_REFRESH_GAP);
    assert!(
        panel.refresh_if_changed(true, 0),
        "read once more without a watch"
    );
    refreshed(&mut panel);
    assert_eq!(panel.path.to_string(), "changing://pending/");
    assert_eq!(panel.error, None);
}

#[test]
fn a_folder_that_stays_unreadable_is_not_read_again_and_again() {
    use crate::vfs::testing::{deny, put};
    let dir = put("changing://denied/", &["a"]);
    let mut panel = Panel::new(dir);
    deny("changing://denied/", true);
    assert!(panel.refresh_if_changed(true, 0));
    refreshed(&mut panel);
    panel.last_auto_refresh = None;
    assert!(panel.refresh_if_changed(true, 0), "once more");
    refreshed(&mut panel);
    panel.last_auto_refresh = None;
    assert!(!panel.refresh_if_changed(true, 0), "no more");
    assert!(panel.error.is_some());
}

#[test]
fn a_change_seen_while_the_folder_is_read_again_does_not_read_it_twice() {
    use crate::vfs::testing::put;
    let dir = put("changing://twice/", &["a"]);
    let mut panel = Panel::new(dir);
    put("changing://twice/", &["a", "b"]);
    panel.change_pending = true;
    panel.reload();
    assert_eq!(names(&panel), vec!["a", "b"]);
    panel.last_auto_refresh = None;
    assert!(!panel.refresh_if_changed(true, 0), "the reading had it");
}

/// The panel's files `names`, sorted by name, the reading done.
fn with_files(names: &[&str]) -> (tempfile::TempDir, Panel) {
    let temp = tempfile::tempdir().unwrap();
    for name in names {
        std::fs::write(temp.path().join(name), "x").unwrap();
    }
    let panel = Panel::new(VfsPath::local(temp.path()));
    (temp, panel)
}

fn at(panel: &mut Panel, name: &str) {
    panel.cursor = panel
        .entries
        .iter()
        .position(|entry| entry.name == name)
        .unwrap();
}

/// FAR's `ReadFileNames` / `UpdatePlugin`: the cursor's entry gone, the cursor stays on its row (the
/// next entry, the last past the end); a selected one gives the first unselected after it.
#[test]
fn the_cursors_entry_gone_leaves_the_cursor_on_its_row_or_the_next_unselected() {
    let (temp, mut panel) = with_files(&["a", "b", "c", "d"]);
    at(&mut panel, "b");
    std::fs::remove_file(temp.path().join("b")).unwrap();
    panel.reload();
    poll_until(&mut panel, |panel, _| panel.reading().is_none());
    assert_eq!(current_name(&panel), "c", "the next one, on the same row");
    at(&mut panel, "d");
    std::fs::remove_file(temp.path().join("d")).unwrap();
    panel.reload();
    poll_until(&mut panel, |panel, _| panel.reading().is_none());
    assert_eq!(current_name(&panel), "c", "past the end: the last one");
    let (temp, mut panel) = with_files(&["a", "b", "c", "d"]);
    panel.selected.extend(["b".to_string(), "c".to_string()]);
    at(&mut panel, "b");
    std::fs::remove_file(temp.path().join("b")).unwrap();
    panel.reload();
    poll_until(&mut panel, |panel, _| panel.reading().is_none());
    assert_eq!(current_name(&panel), "d", "the first unselected after it");
    assert!(panel.selected.contains("c"));
}

#[test]
fn a_pushd_entered_is_kept_for_the_loop_when_another_reading_starts() {
    let (_temp, mut panel) = sample();
    // Entered in the background, not taken by the loop yet.
    panel.pushd = Some(("before".to_string(), true));
    // Ctrl+R before the loop's turn.
    panel.reload();
    assert_eq!(panel.take_pushed().as_deref(), Some("before"));
    panel.pushd = Some(("other".to_string(), false));
    panel.reload();
    assert_eq!(
        panel.pushd, None,
        "one not entered is not for the new reading"
    );
}
