//! The viewer's byte editing in hex (ADR-0027): F2 on, typing over bytes, Tab, F2's save, Esc.

use std::time::{Duration, Instant};

use super::*;
use crate::editor::view::Screen;

fn press(viewer: &mut Viewer, code: KeyCode) -> ViewerAction {
    viewer.handle_key(KeyEvent::new(code, KeyModifiers::NONE))
}

/// Pumps until the rows of an 80 by 5 room are what `done` waits for.
fn settle(viewer: &mut Viewer, done: impl Fn(&Screen) -> bool) -> Screen {
    viewer.text.room.set((80, 5));
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        viewer.pump();
        if let Some(screen) = viewer.text.screen(80, 5)
            && done(&screen)
        {
            return screen;
        }
        assert!(Instant::now() < deadline, "not shown");
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// A viewer on `bytes` in hex.
fn hex_viewer(temp: &tempfile::TempDir, bytes: &[u8]) -> Viewer {
    let path = temp.path().join("v.bin");
    std::fs::write(&path, bytes).unwrap();
    let mut viewer = Viewer::open(&path);
    settle(&mut viewer, |screen| !screen.rows.is_empty());
    press(&mut viewer, KeyCode::F(4));
    settle(&mut viewer, |screen| {
        screen.rows[0].starts_with("0000000000: ")
    });
    viewer
}

#[test]
fn f2_in_hex_types_over_the_bytes_shown() {
    let temp = tempfile::tempdir().unwrap();
    let mut viewer = hex_viewer(&temp, b"abcdefgh");
    assert_eq!(viewer.labels()[0], "Edit");
    press(&mut viewer, KeyCode::F(2));
    assert_eq!(viewer.labels()[0], "Save");
    assert_eq!(viewer.text.mode(), Mode::Hex, "F2 no longer goes to dump");
    let screen = settle(&mut viewer, |screen| screen.cursor.is_some());
    assert_eq!(screen.cursor, Some((0, 12)), "on the first byte");
    for code in [
        KeyCode::Char('4'),
        KeyCode::Char('1'),
        KeyCode::Tab,
        KeyCode::Char('Z'),
    ] {
        press(&mut viewer, code);
    }
    let screen = settle(&mut viewer, |screen| screen.rows[0].contains("41 5A 63"));
    let row: Vec<char> = screen.rows[0].chars().collect();
    let text_at = row.windows(3).position(|three| three == ['A', 'Z', 'c']);
    assert_eq!(text_at, Some(63), "{:?}", screen.rows[0]);
    assert_eq!(
        screen.spans[0],
        vec![(12, 14), (63, 64), (15, 17), (64, 65)]
    );
    assert_eq!(
        screen.cursor,
        Some((0, 63 + 2)),
        "the text column's third byte"
    );
    let typed: Vec<_> = viewer.typed_bytes().unwrap().clone().into_iter().collect();
    assert_eq!(typed, vec![(0, b'A'), (1, b'Z')]);
}

#[test]
fn a_byte_typed_back_is_unchanged_and_f2_saves_esc_asks() {
    let temp = tempfile::tempdir().unwrap();
    let mut viewer = hex_viewer(&temp, b"abcdefgh");
    for code in [
        KeyCode::F(2),
        KeyCode::Char('4'),
        KeyCode::Char('1'),
        KeyCode::Tab,
        KeyCode::Char('Z'),
    ] {
        press(&mut viewer, code);
    }
    settle(&mut viewer, |screen| screen.rows[0].contains("41 5A 63"));
    // A byte typed back to the file's is no longer changed.
    press(&mut viewer, KeyCode::Left);
    press(&mut viewer, KeyCode::Char('b'));
    settle(&mut viewer, |screen| screen.rows[0].contains("41 62 63"));
    assert_eq!(viewer.typed_bytes().unwrap().len(), 1);
    assert!(matches!(
        press(&mut viewer, KeyCode::F(4)),
        ViewerAction::None
    ));
    assert!(
        matches!(press(&mut viewer, KeyCode::F(6)), ViewerAction::None),
        "F6 would drop the bytes typed (review C4)"
    );
    assert_eq!(viewer.text.mode(), Mode::Hex, "the modes stay");
    assert!(matches!(
        press(&mut viewer, KeyCode::F(2)),
        ViewerAction::SaveBytes
    ));
    assert!(matches!(
        press(&mut viewer, KeyCode::Esc),
        ViewerAction::AskBytes
    ));
}

#[test]
fn the_bytes_written_stay_shown_and_esc_leaves_the_editing() {
    let temp = tempfile::tempdir().unwrap();
    let mut viewer = hex_viewer(&temp, b"abcdefgh");
    press(&mut viewer, KeyCode::F(2));
    press(&mut viewer, KeyCode::Char('4'));
    press(&mut viewer, KeyCode::Char('2'));
    settle(&mut viewer, |screen| screen.rows[0].contains("42 62"));
    let changed = viewer.typed_bytes().unwrap().clone();
    write_bytes(&viewer.path, &changed, None).unwrap();
    assert_eq!(std::fs::read(&viewer.path).unwrap(), b"Bbcdefgh");
    viewer.text.view_mut().unwrap().bytes_written();
    assert!(viewer.typed_bytes().unwrap().is_empty());
    assert!(matches!(
        press(&mut viewer, KeyCode::Esc),
        ViewerAction::None
    ));
    assert!(viewer.typed_bytes().is_none(), "the editing left");
    let screen = settle(&mut viewer, |screen| screen.cursor.is_none());
    assert!(
        screen.rows[0].contains("42 62"),
        "the blocks show what was written"
    );
    assert!(matches!(
        press(&mut viewer, KeyCode::Esc),
        ViewerAction::Close
    ));
}

#[test]
fn write_bytes_writes_each_run_in_place() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("f.bin");
    std::fs::write(&path, b"0123456789").unwrap();
    let changed = BTreeMap::from([(1, b'a'), (2, b'b'), (8, b'z')]);
    write_bytes(&path, &changed, Some(10)).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), b"0ab34567z9");
}

/// R7 review C9: a file no longer as long as it was read (rotated, cut) takes no bytes.
#[test]
fn write_bytes_refuses_a_file_changed_since_it_was_read() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("f.bin");
    std::fs::write(&path, b"0123").unwrap();
    let changed = BTreeMap::from([(8, b'z')]);
    assert!(
        write_bytes(&path, &changed, Some(10)).is_err(),
        "shorter than read"
    );
    assert!(write_bytes(&path, &changed, None).is_err(), "past its end");
    assert_eq!(std::fs::read(&path).unwrap(), b"0123", "untouched");
}

/// R7 review C6: a plugin's file's copy is not byte edited (the bytes would stay in the copy, thrown
/// away with it): F2 in hex starts no editing, its label is empty.
#[test]
fn a_plugin_files_copy_is_not_byte_edited() {
    let temp = tempfile::tempdir().unwrap();
    let mut viewer = hex_viewer(&temp, b"abcdefgh");
    viewer.copy = true;
    assert_eq!(viewer.labels()[0], "", "F2's label");
    press(&mut viewer, KeyCode::F(2));
    assert!(viewer.typed_bytes().is_none(), "no editing");
}

/// R7 review C4: the bytes go into `a `, never into its sibling `a` (the plain name as a panel gives it).
#[cfg(windows)]
#[test]
fn write_bytes_writes_the_exact_name() {
    let temp = tempfile::tempdir().unwrap();
    let exact = |name: &str| crate::platform::files::exact_path(&temp.path().join(name));
    std::fs::write(exact("a"), b"sibling").unwrap();
    std::fs::write(exact("a "), b"0123").unwrap();
    write_bytes(
        &temp.path().join("a "),
        &BTreeMap::from([(0, b'x')]),
        Some(4),
    )
    .unwrap();
    assert_eq!(std::fs::read(exact("a ")).unwrap(), b"x123");
    assert_eq!(std::fs::read(exact("a")).unwrap(), b"sibling");
}
