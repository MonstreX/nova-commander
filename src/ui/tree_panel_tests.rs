//! The tree panel drawn as FAR's `DisplayTree`.

use std::path::{Path, PathBuf};

use ratatui::Terminal;
use ratatui::backend::TestBackend;

use super::*;
use crate::app::Side;

fn tree() -> TreePanel {
    let folders = ["R:\\a", "R:\\a\\b", "R:\\a\\c", "R:\\d"].map(PathBuf::from);
    TreePanel::showing(Side::Left, Path::new("R:\\"), folders.to_vec())
}

fn rows(tree: &mut TreePanel, focused: bool) -> Vec<String> {
    let mut terminal = Terminal::new(TestBackend::new(24, 10)).unwrap();
    terminal
        .draw(|frame| draw(frame, frame.area(), tree, focused))
        .unwrap();
    let buffer = terminal.backend().buffer();
    (0..10)
        .map(|y| (0..24).map(|x| buffer[(x, y)].symbol()).collect::<String>())
        .collect()
}

#[test]
fn folders_hang_on_fars_lines_and_the_cursor_is_bracketed_without_the_focus() {
    let mut tree = tree();
    tree.cursor = 2;
    tree.work = Some(1);
    let shown = rows(&mut tree, false);
    assert!(shown[0].contains(" Tree "), "{shown:#?}");
    assert_eq!(shown[1], "║ \\                    ║");
    assert_eq!(shown[2], "║ ├─a                  ║");
    assert_eq!(shown[3], "║ │ ├[b]               ║");
    assert_eq!(shown[4], "║ │ └─c                ║");
    assert_eq!(shown[5], "║ └─d                  ║");
    assert!(shown[7].starts_with("╟───"), "the separator");
    assert_eq!(shown[8], "║R:\\a\\b                ║", "the cursor's path");
    assert_eq!(tree.rows, 6);
}

#[test]
fn with_the_focus_the_cursor_has_its_colour_and_the_other_panels_folder_the_selected_one() {
    let mut tree = tree();
    tree.cursor = 4;
    tree.work = Some(1);
    let mut terminal = Terminal::new(TestBackend::new(24, 10)).unwrap();
    terminal
        .draw(|frame| draw(frame, frame.area(), &mut tree, true))
        .unwrap();
    let buffer = terminal.backend().buffer();
    let row: String = (0..24).map(|x| buffer[(x, 5)].symbol()).collect();
    assert_eq!(
        row, "║ └ d                  ║",
        "a space each side of the name"
    );
    assert_eq!(buffer[(4, 5)].bg, theme::cursor(false).bg.unwrap());
    assert_eq!(
        buffer[(4, 2)].fg,
        theme::selected().fg.unwrap(),
        "a: the work folder"
    );
}

#[test]
fn while_the_worker_reads_the_separator_says_so_and_the_tree_is_drawn() {
    let mut tree = tree();
    tree.reading = true;
    tree.count = 1234;
    let rows = rows(&mut tree, true);
    assert_eq!(rows[7], "╟─── Reading: 1234 ────╢", "on the separator");
    assert!(rows[2].contains('a'), "no message over the folders");
    tree.reading = false;
    assert!(!rows_text(&mut tree).contains("Reading"));
}

fn rows_text(tree: &mut TreePanel) -> String {
    rows(tree, true).join("\n")
}
