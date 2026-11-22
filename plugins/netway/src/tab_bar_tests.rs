use super::*;

const NAMES: [&str; 5] = ["Session", "Environment", "Directories", "SFTP", "SCP/Shell"];

#[test]
fn the_shown_tab_is_in_brackets_and_clicks_find_their_tab() {
    let drawn = row(&NAMES, &[0, 1, 2, 3, 4], 0, 59);
    assert_eq!(
        drawn.text,
        "[Session] Environment  Directories  SFTP  SCP/Shell "
    );
    assert_eq!(tab_at(&drawn, 3), Some(0));
    assert_eq!(tab_at(&drawn, 12), Some(1));
    assert_eq!(tab_at(&drawn, 9), Some(1));
    assert_eq!(tab_at(&drawn, 58), None);
}

#[test]
fn tabs_not_there_are_left_out() {
    let drawn = row(&NAMES, &[0, 1, 2, 4], 4, 59);
    assert_eq!(drawn.text, " Session  Environment  Directories [SCP/Shell]");
    assert_eq!(tab_at(&drawn, 40), Some(4));
}

#[test]
fn a_narrow_row_scrolls_to_the_shown_tab() {
    let start = row(&NAMES, &[0, 1, 2, 3, 4], 0, 30);
    assert_eq!(start.text, "[Session] Environment \u{25ba}");
    let end = row(&NAMES, &[0, 1, 2, 3, 4], 4, 30);
    assert!(end.text.starts_with('\u{25c4}'), "{}", end.text);
    assert!(end.text.contains("[SCP/Shell]"), "{}", end.text);
    assert!(end.text.chars().count() <= 30);
    assert_eq!(tab_at(&end, 0), None);
}

#[test]
fn steps_go_round_the_tabs_there() {
    let there = [0, 1, 2, 4];
    assert_eq!(step(&there, 2, true), 4);
    assert_eq!(step(&there, 4, true), 0);
    assert_eq!(step(&there, 0, false), 4);
    assert_eq!(
        step(&there, 3, true),
        0,
        "a tab not there goes to the first"
    );
}
