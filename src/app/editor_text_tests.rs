//! The editor's file: opened from its first block (its code page and break), the steps done as the
//! blocks come, the status line's counts.

use std::time::{Duration, Instant};

use super::*;

fn settle(text: &mut EditorText, done: impl Fn(&EditorText) -> bool) {
    text.room.set((40, 5));
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        text.pump();
        if done(text) {
            return;
        }
        assert!(Instant::now() < deadline, "never done");
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn the_first_break_is_the_file_s() {
    assert_eq!(first_break(b"a\nb\r\n", &Encoding::Utf8), "\n");
    assert_eq!(first_break(b"a\r\nb", &Encoding::Utf8), "\r\n");
    assert_eq!(first_break(b"a\rb", &Encoding::Utf8), "\r");
    assert_eq!(first_break(b"none", &Encoding::Utf8), "\r\n");
    let le: Vec<u8> = "a\nb".encode_utf16().flat_map(u16::to_le_bytes).collect();
    assert_eq!(first_break(&le, &Encoding::Utf16Le), "\n");
}

#[test]
fn a_file_opens_and_is_edited_as_its_blocks_come() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("e.txt");
    let lines: String = (1..=50).map(|n| format!("line {n}\n")).collect();
    std::fs::write(&path, &lines).unwrap();
    let mut text = EditorText::open(VfsPath::local(&path));
    settle(&mut text, |text| text.place().is_some());
    assert_eq!(text.place(), Some((1, 51, 1, 1)));
    text.press(Step::Finish);
    text.press(Step::Type('!'));
    settle(&mut text, |text| {
        text.place().is_some_and(|place| place.0 == 51)
    });
    assert!(text.is_changed());
    let screen = text.screen(40, 5).unwrap();
    assert_eq!(screen.rows.last().map(String::as_str), Some("!"));
    assert!(text.document().is_some());
}

#[test]
fn a_new_file_is_empty_and_typed_into() {
    let mut text = EditorText::new_file();
    text.press(Step::Type('a'));
    settle(&mut text, |text| text.is_changed());
    assert_eq!(text.screen(40, 5).unwrap().rows, ["a"]);
}

#[test]
fn a_rust_file_is_highlighted_by_colorer_s_scheme() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("main.rs");
    std::fs::write(&path, "fn main() {\n    let x = 1; // one\n}\n").unwrap();
    let mut text = EditorText::open(VfsPath::local(&path));
    settle(&mut text, |text| {
        text.highlighted() && text.paints(40, 5).iter().any(|row| !row.is_empty())
    });
    let paints = text.paints(40, 5);
    let classes = |row: usize| -> Vec<(usize, usize, &str)> {
        paints[row]
            .iter()
            .map(|p| (p.from, p.to, p.class))
            .collect()
    };
    assert!(classes(0).contains(&(0, 2, "Keyword")), "{:?}", classes(0));
    assert!(classes(1).contains(&(4, 7, "Keyword")), "{:?}", classes(1));
    assert!(
        classes(1)
            .iter()
            .any(|&(from, _, class)| from == 15 && class.contains("Comment")),
        "{:?}",
        classes(1)
    );
    assert!(text.syntax.is_on());
}

/// S2d.6d: FAR's line numbers take the lines' digits, three at least, and a space.
#[test]
fn line_numbers_take_far_s_width() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("n.txt");
    let lines: String = (1..=1200).map(|n| format!("{n}\n")).collect();
    std::fs::write(&path, lines).unwrap();
    let mut text = EditorText::open(VfsPath::local(&path));
    settle(&mut text, |text| text.place().is_some());
    assert_eq!(text.number_width(), 0, "off by default");
    text.numbers = true;
    assert_eq!(text.lines(), (1, 1201));
    assert_eq!(text.number_width(), 5, "four digits and a space");
}

/// S2d.6c: the editor's settings - FAR's tab size laid out, the highlighting switched off.
#[test]
fn the_editor_s_settings_set_its_tabs_and_its_highlighting() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("t.rs");
    std::fs::write(&path, "\tfn x() {}\n").unwrap();
    let mut text = EditorText::open(VfsPath::local(&path));
    let settings = crate::settings::EditorSettings {
        tab_size: 4,
        highlighting: false,
        ..crate::settings::EditorSettings::default()
    };
    text.apply_settings(&settings);
    settle(&mut text, |text| text.place().is_some());
    assert_eq!(
        text.screen(40, 5).unwrap().rows[0],
        "    fn x() {}",
        "a tab of four"
    );
    for _ in 0..50 {
        text.pump();
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(!text.highlighted(), "no highlighting");
}
