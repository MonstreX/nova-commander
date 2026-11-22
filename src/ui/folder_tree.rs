//! Alt+F10: FAR's "Find folder" (`FolderTree::DisplayObject`, `DrawEdit`): the tree in a window from the
//! fifth column and the third row to four from the screen's edges with a shadow (F5: the whole screen),
//! " Find folder " on its frame, the separator a row higher than the panel's and "Search:" with its edit
//! field (to the window's 26th column) on the row below it, the folder's path on the last.

use ratatui::Frame;
use ratatui::layout::{Position, Rect};

use super::{theme, tree_panel, vmenu};
use crate::app::Cells;
use crate::app::tree_panel::FolderTree;

const SEARCH: &str = "Search:";

/// The window's place on a screen of `screen` (FAR's `SetCoords`): Alt+F10's from the fifth column,
/// a dialog's from a third of the screen to the eighth column from its right edge.
fn place(screen: Rect, found: &FolderTree) -> Rect {
    if found.full_screen || screen.width < 16 || screen.height < 12 {
        return screen;
    }
    if found.standalone() {
        return Rect::new(4, 2, screen.width - 8, screen.height - 6);
    }
    let x = (screen.width - 1) / 3;
    Rect::new(x, 2, screen.width - 7 - x, screen.height - 6)
}

/// Before drawing: the window's place for the mouse and the tree's rows.
pub fn fit(found: &mut FolderTree, screen: Rect) {
    let rect = place(screen, found);
    found.area = Some(Cells {
        x: rect.x,
        y: rect.y,
        width: rect.width,
        height: rect.height,
    });
    found.tree.set_rows(tree_panel::rows_of(rect, true));
}

/// The window; the cursor's place in the search line.
pub fn draw(frame: &mut Frame, screen: Rect, found: &FolderTree) -> Option<Position> {
    let rect = place(screen, found);
    tree_panel::draw_box(frame, rect, &found.tree, true, " Find folder ", true);
    if !found.full_screen {
        vmenu::draw_shadow(frame.buffer_mut(), rect, screen);
    }
    if rect.width < 16 || rect.height < 6 {
        return None;
    }
    let y = rect.bottom() - 3;
    let buffer = frame.buffer_mut();
    vmenu::set_text(buffer, rect.x + 1, y, SEARCH, theme::panel());
    let left = rect.x + 1 + width_of(SEARCH) + 2;
    let right = (rect.right() - 2).min(rect.x + 25);
    let width = usize::from(right.saturating_sub(left) + 1);
    let from = found.search.shown_from(width);
    let shown: String = found.search.text().chars().skip(from).take(width).collect();
    let field = format!("{shown:<width$}");
    vmenu::set_text(buffer, left, y, &field, theme::input_field());
    let column = found.search.cursor().saturating_sub(from);
    Some(Position::new(left + u16::try_from(column).unwrap_or(0), y))
}

fn width_of(text: &str) -> u16 {
    u16::try_from(text.chars().count()).unwrap_or(u16::MAX)
}

#[cfg(test)]
#[path = "folder_tree_tests.rs"]
mod tests;
