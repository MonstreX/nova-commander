//! A pop-up menu (FAR's `VMenu` outside the menu bar): a framed list with a title, a bottom title,
//! check marks, accelerator keys, separators and `&` hotkeys. Ctrl+F12 "Sort by" is one.

use ratatui::crossterm::event::{KeyCode, KeyEvent};

use super::command::Side;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PopupItem {
    /// Label with `&` before the hotkey letter ("" for a separator).
    pub label: String,
    /// Accelerator key shown on the right.
    pub key: String,
    /// Character in the check-mark column (`√`, `▲`, `▼`).
    pub check: Option<char>,
    pub separator: bool,
}

impl PopupItem {
    pub fn new(label: &str, key: &str, check: Option<char>) -> Self {
        Self {
            label: label.to_string(),
            key: key.to_string(),
            check,
            separator: false,
        }
    }

    pub fn separator() -> Self {
        Self {
            label: String::new(),
            key: String::new(),
            check: None,
            separator: true,
        }
    }

    fn hotkey(&self) -> Option<char> {
        super::vmenu_keys::marked_hotkey(&self.label)
    }
}

/// Where the menu opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PopupAnchor {
    /// Four columns into the panel, centred vertically (FAR's sort menu).
    Panel(Side),
}

/// What choosing an item does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PopupAction {
    Sort(Side),
    /// The info panel's "Show Info" menu.
    InfoSections,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Popup {
    pub title: String,
    pub bottom_title: String,
    pub items: Vec<PopupItem>,
    pub selected: usize,
    pub anchor: PopupAnchor,
    pub action: PopupAction,
}

/// How an item was chosen: Enter (or its hotkey), or one of FAR's sort-menu keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChooseKey {
    Enter,
    Plus,
    Minus,
    Star,
}

pub enum PopupOutcome {
    Stay,
    Close,
    Choose(usize, ChooseKey),
}

pub fn handle_key(popup: &mut Popup, key: KeyEvent) -> PopupOutcome {
    let count = popup.items.len();
    if count == 0 {
        return PopupOutcome::Close;
    }
    match key.code {
        KeyCode::Up => popup.selected = step(&popup.items, popup.selected, count - 1),
        KeyCode::Down => popup.selected = step(&popup.items, popup.selected, 1),
        KeyCode::Home => popup.selected = step(&popup.items, count - 1, 1),
        KeyCode::End => popup.selected = step(&popup.items, 0, count - 1),
        KeyCode::Enter => return PopupOutcome::Choose(popup.selected, ChooseKey::Enter),
        KeyCode::Esc | KeyCode::F(10) => return PopupOutcome::Close,
        KeyCode::Char('+') => return PopupOutcome::Choose(popup.selected, ChooseKey::Plus),
        KeyCode::Char('-') => return PopupOutcome::Choose(popup.selected, ChooseKey::Minus),
        KeyCode::Char('*') => return PopupOutcome::Choose(popup.selected, ChooseKey::Star),
        KeyCode::Char(ch) => {
            let ch = super::vmenu_keys::lower(ch);
            if let Some(index) = popup
                .items
                .iter()
                .position(|item| item.hotkey() == Some(ch))
            {
                return PopupOutcome::Choose(index, ChooseKey::Enter);
            }
        }
        _ => {}
    }
    PopupOutcome::Stay
}

/// The next item `offset` positions away (wrapping), skipping separators.
fn step(items: &[PopupItem], from: usize, offset: usize) -> usize {
    let mut index = from;
    for _ in 0..items.len() {
        index = (index + offset) % items.len();
        if !items[index].separator {
            return index;
        }
    }
    from
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::crossterm::event::KeyModifiers;

    fn popup() -> Popup {
        Popup {
            title: "Sort by".into(),
            bottom_title: "+ - *".into(),
            items: vec![
                PopupItem::new("&Name", "Ctrl+F3", Some('▲')),
                PopupItem::new("E&xtension", "Ctrl+F4", None),
                PopupItem::separator(),
                PopupItem::new("Show directories &first", "", Some('√')),
            ],
            selected: 0,
            anchor: PopupAnchor::Panel(Side::Left),
            action: PopupAction::Sort(Side::Left),
        }
    }

    fn press(popup: &mut Popup, code: KeyCode) -> PopupOutcome {
        handle_key(popup, KeyEvent::new(code, KeyModifiers::NONE))
    }

    #[test]
    fn moves_over_separators_and_wraps() {
        let mut menu = popup();
        press(&mut menu, KeyCode::Down);
        press(&mut menu, KeyCode::Down);
        assert_eq!(menu.selected, 3);
        press(&mut menu, KeyCode::Down);
        assert_eq!(menu.selected, 0);
        press(&mut menu, KeyCode::Up);
        assert_eq!(menu.selected, 3);
        press(&mut menu, KeyCode::Home);
        assert_eq!(menu.selected, 0);
    }

    #[test]
    fn keys_choose_items() {
        let mut menu = popup();
        assert!(matches!(
            press(&mut menu, KeyCode::Char('X')),
            PopupOutcome::Choose(1, ChooseKey::Enter)
        ));
        assert!(matches!(
            press(&mut menu, KeyCode::Char('-')),
            PopupOutcome::Choose(0, ChooseKey::Minus)
        ));
        assert!(matches!(
            press(&mut menu, KeyCode::Esc),
            PopupOutcome::Close
        ));
        assert!(matches!(
            press(&mut menu, KeyCode::Char('q')),
            PopupOutcome::Stay
        ));
    }
}
