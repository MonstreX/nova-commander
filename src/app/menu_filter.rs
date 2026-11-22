//! A menu's filter (FAR's `VMenu` filter mode, `vmenu.cpp`): Ctrl+Alt+F turns it on or off, the text typed
//! then (Backspace takes a character off, Ctrl+V / Shift+Ins add the clipboard's) leaves only the lines that
//! hold it in any case, and Ctrl+Alt+L locks it so the keys go to the menu again. The title shows it as
//! `[text]`, locked as `<text>`.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// The filter of an open menu.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MenuFilter {
    pub text: String,
    pub locked: bool,
}

/// What a key did to a menu's filter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Filtered {
    /// Not a filter's key: the menu has it.
    No,
    /// Taken; the lines shown are as they were.
    Kept,
    /// Taken; the lines are to be filtered again.
    Changed,
}

const CTRL_ALT: KeyModifiers = KeyModifiers::CONTROL.union(KeyModifiers::ALT);

impl MenuFilter {
    /// Whether a line's text stays (FAR's `contains_icase` of the trimmed text, `&` marks taken off).
    pub fn keeps(&self, text: &str) -> bool {
        self.text.is_empty()
            || (text.replace('&', "").trim().to_lowercase()).contains(&self.text.to_lowercase())
    }

    /// The menu's title with the filter after it (FAR's `DisplayObject`: `title [text]`, `<text>` locked).
    pub fn title(&self, title: &str) -> String {
        let (open, close) = if self.locked { ('<', '>') } else { ('[', ']') };
        let gap = if title.is_empty() { "" } else { " " };
        format!("{title}{gap}{open}{}{close}", self.text)
    }
}

/// The filter's keys first, as FAR's `VMenu2` hands them to `ProcessFilterKey` before the menu's own:
/// `shown` is how many lines the filter leaves now (a character is not added when none is left);
/// `paste` gives the clipboard's text for Ctrl+V / Shift+Ins.
pub fn filter_key(
    filter: &mut Option<MenuFilter>,
    key: KeyEvent,
    shown: usize,
    paste: impl FnOnce() -> Option<String>,
) -> Filtered {
    if key.modifiers == CTRL_ALT && matches!(key.code, KeyCode::Char('f' | 'F')) {
        // FAR's `EnableFilter`: on with nothing typed, or off with every line back.
        let was = filter.take();
        if was.is_none() {
            *filter = Some(MenuFilter::default());
        }
        return if was.is_some_and(|was| !was.text.is_empty()) {
            Filtered::Changed
        } else {
            Filtered::Kept
        };
    }
    let Some(on) = filter.as_mut() else {
        return Filtered::No;
    };
    let locking = key.modifiers == CTRL_ALT && matches!(key.code, KeyCode::Char('l' | 'L'));
    let pasting = matches!(
        (key.code, key.modifiers),
        (KeyCode::Char('v' | 'V'), KeyModifiers::CONTROL) | (KeyCode::Insert, KeyModifiers::SHIFT)
    );
    // FAR's `VMenu::ProcessKey`: with no line left these close the menu as other keys do.
    if (locking || pasting) && shown == 0 {
        return Filtered::No;
    }
    if locking {
        on.locked = !on.locked;
        return Filtered::Kept;
    }
    if on.locked {
        return Filtered::No;
    }
    if pasting {
        // FAR's `AddToFilter`: the characters a filter takes, line ends left out.
        let text = paste().unwrap_or_default();
        on.text.extend(text.chars().filter(|ch| !ch.is_control()));
        return Filtered::Changed;
    }
    match key.code {
        KeyCode::Backspace if key.modifiers.is_empty() => {
            if on.text.pop().is_some() {
                Filtered::Changed
            } else {
                Filtered::Kept
            }
        }
        KeyCode::Char(ch) if (key.modifiers - KeyModifiers::SHIFT).is_empty() => {
            if shown == 0 {
                return Filtered::Kept;
            }
            on.text.push(ch);
            Filtered::Changed
        }
        _ => Filtered::No,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(filter: &mut Option<MenuFilter>, code: KeyCode, modifiers: KeyModifiers) -> Filtered {
        filter_key(filter, KeyEvent::new(code, modifiers), 1, || {
            Some("pa\r\nste".into())
        })
    }

    #[test]
    fn ctrl_alt_f_turns_it_on_typing_fills_it_and_ctrl_alt_f_again_turns_it_off() {
        let mut filter = None;
        let none = KeyModifiers::NONE;
        assert_eq!(press(&mut filter, KeyCode::Char('x'), none), Filtered::No);
        assert_eq!(
            press(&mut filter, KeyCode::Char('f'), CTRL_ALT),
            Filtered::Kept
        );
        assert_eq!(
            press(&mut filter, KeyCode::Char('W'), KeyModifiers::SHIFT),
            Filtered::Changed
        );
        assert_eq!(
            press(&mut filter, KeyCode::Char('o'), none),
            Filtered::Changed
        );
        assert_eq!(filter.as_ref().unwrap().text, "Wo");
        assert_eq!(
            press(&mut filter, KeyCode::Backspace, none),
            Filtered::Changed
        );
        assert_eq!(press(&mut filter, KeyCode::Enter, none), Filtered::No);
        assert_eq!(
            press(&mut filter, KeyCode::Char('c'), KeyModifiers::CONTROL),
            Filtered::No
        );
        assert_eq!(
            press(&mut filter, KeyCode::Char('f'), CTRL_ALT),
            Filtered::Changed
        );
        assert_eq!(filter, None);
    }

    #[test]
    fn locked_it_lets_the_keys_through_and_the_title_shows_which() {
        let mut filter = Some(MenuFilter {
            text: "ab".into(),
            locked: false,
        });
        assert_eq!(
            filter.as_ref().unwrap().title("Folders history"),
            "Folders history [ab]"
        );
        assert_eq!(
            press(&mut filter, KeyCode::Char('l'), CTRL_ALT),
            Filtered::Kept
        );
        assert_eq!(
            press(&mut filter, KeyCode::Char('x'), KeyModifiers::NONE),
            Filtered::No
        );
        assert_eq!(
            press(&mut filter, KeyCode::Backspace, KeyModifiers::NONE),
            Filtered::No
        );
        assert_eq!(filter.as_ref().unwrap().title(""), "<ab>");
    }

    #[test]
    fn the_clipboard_goes_in_without_line_ends_and_nothing_is_added_when_no_line_is_left() {
        let mut filter = Some(MenuFilter::default());
        assert_eq!(
            press(&mut filter, KeyCode::Insert, KeyModifiers::SHIFT),
            Filtered::Changed
        );
        assert_eq!(filter.as_ref().unwrap().text, "paste");
        let none_left = filter_key(&mut filter, KeyEvent::from(KeyCode::Char('z')), 0, || None);
        assert_eq!(none_left, Filtered::Kept);
        // Paste and the lock are the menu's then: they close it (FAR's `VMenu::ProcessKey`).
        let paste = KeyEvent::new(KeyCode::Insert, KeyModifiers::SHIFT);
        assert_eq!(filter_key(&mut filter, paste, 0, || None), Filtered::No);
        let lock = KeyEvent::new(KeyCode::Char('l'), CTRL_ALT);
        assert_eq!(filter_key(&mut filter, lock, 0, || None), Filtered::No);
        assert!(!filter.as_ref().unwrap().locked);
        assert_eq!(filter.as_ref().unwrap().text, "paste");
    }

    #[test]
    fn a_line_stays_when_it_holds_the_text_in_any_case() {
        let filter = MenuFilter {
            text: "WoRk".into(),
            locked: false,
        };
        assert!(filter.keeps(r"  D:\&Work\x  "));
        assert!(!filter.keeps(r"C:\wor\k"));
        assert!(MenuFilter::default().keeps("anything"));
    }
}
