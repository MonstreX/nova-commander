//! "Find folder" drawn as FAR's `FolderTree`.

use std::path::{Path, PathBuf};

use ratatui::Terminal;
use ratatui::backend::TestBackend;

use super::*;
use crate::app::Side;
use crate::app::tree_panel::TreePanel;

fn rows(found: &mut FolderTree, width: u16, height: u16) -> (Vec<String>, Option<Position>) {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    let mut cursor = None;
    terminal
        .draw(|frame| {
            fit(found, frame.area());
            cursor = draw(frame, frame.area(), found);
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    let shown = (0..height)
        .map(|y| {
            (0..width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect();
    (shown, cursor)
}

fn found() -> FolderTree {
    let folders = [r"R:\a", r"R:\a\b", r"R:\d"].map(PathBuf::from);
    let tree = TreePanel::showing(Side::Left, Path::new("R:\\"), folders.to_vec());
    FolderTree::showing(tree, "b")
}

#[test]
fn the_window_has_the_tree_the_separator_a_row_up_and_the_search_line() {
    let mut found = found();
    let (shown, cursor) = rows(&mut found, 40, 16);
    // The window from the fifth column and third row, four from the right and the bottom.
    let line = |y: usize| shown[y].chars().skip(4).collect::<String>();
    assert!(line(2).starts_with('╔'), "{shown:#?}");
    assert!(line(2).contains(" Find folder "), "{shown:#?}");
    assert!(shown[1].chars().all(|ch| ch == ' '), "nothing above it");
    assert!(line(3).starts_with("║ \\"), "{shown:#?}");
    assert!(line(8).starts_with("╟───"), "the separator: {shown:#?}");
    assert!(line(9).starts_with("║Search:  b"), "{shown:#?}");
    assert!(line(10).starts_with("║R:\\"), "the path: {shown:#?}");
    assert!(line(11).starts_with('╚'), "{shown:#?}");
    assert_eq!(found.tree.rows, 5);
    assert_eq!(cursor, Some(Position::new(15, 9)), "after the b typed");
    let area = found.area.unwrap();
    assert_eq!((area.x, area.y, area.width, area.height), (4, 2, 32, 10));
}

#[test]
fn f5_fills_the_screen() {
    let mut found = found();
    found.full_screen = true;
    let (shown, _) = rows(&mut found, 40, 16);
    assert!(shown[0].starts_with('╔') && shown[0].contains(" Find folder "));
    assert!(shown[15].starts_with('╚'));
    assert_eq!(found.tree.rows, 11);
}

#[test]
fn a_dialog_s_window_stands_from_a_third_of_the_screen() {
    let mut found = found();
    let dialog = crate::app::Dialog::message("Copy", vec![]);
    found.purpose = crate::app::tree_panel::Purpose::Target(Box::new(dialog));
    let (shown, _) = rows(&mut found, 40, 16);
    // FAR's `ScrX / 3` to `ScrX - 7`.
    let area = found.area.unwrap();
    assert_eq!((area.x, area.y, area.width, area.height), (13, 2, 20, 10));
    let line: String = shown[2].chars().skip(13).collect();
    assert!(
        line.starts_with('╔') && line.contains(" Find folder "),
        "{shown:#?}"
    );
}
