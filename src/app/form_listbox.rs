//! FAR's list in a dialog (`DI_LISTBOX`, a `VMenu` with `VMENU_LISTBOX` in the item's box): its lines
//! in a frame with its title, the cursor moved by Up / Down / Home / End round the ends and PgUp / PgDn
//! by the lines shown, over the separators; Enter is the dialog's (FAR passes it on to close the
//! dialog) but on a list that does not close (`DIF_LISTNOCLOSE`: nothing), the mouse puts the cursor on
//! a line and a double click closes the dialog with the list (`mouse_dialogs`).

use ratatui::crossterm::event::{KeyCode, KeyEvent};

use super::ListLine;
use super::list::{OpenList, move_page};

/// A list shown in its box inside the dialog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListBox {
    /// In the top frame.
    pub title: String,
    pub lines: Vec<ListLine>,
    /// The line under the cursor and the first shown.
    pub list: OpenList,
    /// The lines shown, the box around them.
    pub rows: u16,
    /// FAR's `DIF_LISTNOCLOSE`: neither Enter nor a double click closes the dialog.
    pub no_close: bool,
}

impl ListBox {
    /// `lines` under `title`, the cursor on line `cursor` (the last when past it), `rows` shown.
    pub fn new(title: &str, lines: Vec<ListLine>, cursor: usize, rows: u16) -> Self {
        let mut list = Self {
            title: title.to_string(),
            lines,
            list: OpenList::default(),
            rows,
            no_close: false,
        };
        list.set_cursor(cursor);
        list
    }

    /// The cursor on line `cursor`, the last when past it.
    pub fn set_cursor(&mut self, cursor: usize) {
        self.list.cursor = cursor.min(self.lines.len().saturating_sub(1));
    }

    /// Its lines replaced, the cursor kept where it still is (FAR's `DM_LISTSET`).
    pub fn set_lines(&mut self, lines: Vec<ListLine>) {
        self.lines = lines;
        self.set_cursor(self.list.cursor);
    }

    /// The text of the line under the cursor (empty: none).
    pub fn current(&self) -> &str {
        (self.lines.get(self.list.cursor))
            .and_then(ListLine::label)
            .unwrap_or_default()
    }
}

/// A key on the focused list: whether it moved in it (the others are the dialog's).
pub(super) fn key(list: &mut ListBox, key: KeyEvent) -> bool {
    let moves = matches!(
        key.code,
        KeyCode::Up
            | KeyCode::Down
            | KeyCode::Home
            | KeyCode::End
            | KeyCode::PageUp
            | KeyCode::PageDown
    );
    if key.code == KeyCode::Enter && key.modifiers.is_empty() && list.no_close {
        return true;
    }
    if !moves || !key.modifiers.is_empty() {
        return false;
    }
    let page = usize::from(list.rows.max(1));
    move_page(&list.lines, &mut list.list, key, page);
    true
}

#[cfg(test)]
mod tests {
    use ratatui::crossterm::event::KeyModifiers;

    use super::*;

    fn lines(count: usize) -> Vec<ListLine> {
        (0..count)
            .map(|line| ListLine::Choice(format!("line {line}")))
            .collect()
    }

    fn press(list: &mut ListBox, code: KeyCode) -> bool {
        key(list, KeyEvent::new(code, KeyModifiers::NONE))
    }

    #[test]
    fn the_cursor_goes_round_the_ends_and_pages_by_the_rows_shown() {
        let mut list = ListBox::new("Title", lines(10), 0, 3);
        assert!(press(&mut list, KeyCode::Up));
        assert_eq!(list.list.cursor, 9, "round the end");
        assert!(press(&mut list, KeyCode::Home));
        assert!(press(&mut list, KeyCode::PageDown));
        assert_eq!(list.list.cursor, 3, "the rows shown");
        assert_eq!(list.current(), "line 3");
        assert!(!press(&mut list, KeyCode::Enter), "Enter is the dialog's");
        list.no_close = true;
        assert!(
            press(&mut list, KeyCode::Enter),
            "FAR's DIF_LISTNOCLOSE: nothing"
        );
        let ctrl_down = KeyEvent::new(KeyCode::Down, KeyModifiers::CONTROL);
        assert!(!key(&mut list, ctrl_down));
    }

    #[test]
    fn new_lines_keep_the_cursor_where_it_still_is() {
        let mut list = ListBox::new("", lines(5), 4, 3);
        list.set_lines(lines(2));
        assert_eq!(list.list.cursor, 1, "the last");
        list.set_lines(Vec::new());
        assert_eq!((list.list.cursor, list.current()), (0, ""));
        assert_eq!(ListBox::new("", lines(3), 7, 3).list.cursor, 2);
    }
}
