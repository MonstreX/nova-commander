//! FAR's sort commands: Ctrl+F3..Ctrl+F10 (again = reverse), Shift+F11 "use sort groups", Shift+F12
//! "selected first" and the Ctrl+F12
//! "Sort by" menu (`FileList::SelectSortMode`): modes in FAR's menu order with `▲`/`▼` on the current
//! one, then the options; `+`/`-` choose a direction, `*`/Enter behave like the mode's key.

use super::command::Side;
use super::popup::{ChooseKey, Popup, PopupAction, PopupAnchor, PopupItem};
use super::{App, Overlay};
use crate::vfs::{SortMode, SortOrder};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SortEntry {
    Mode(SortMode),
    SortGroups,
    SelectedFirst,
    DirectoriesFirst,
}

/// FAR's menu (`MMenuSortBy*`, `MenuPosition` order) without the modes deferred in docs/far-parity.md
/// (owner, hard links, streams).
const SORT_MENU: [(&str, &str, Option<SortEntry>); 15] = [
    ("&Name", "Ctrl+F3", Some(SortEntry::Mode(SortMode::Name))),
    ("Name only", "", Some(SortEntry::Mode(SortMode::NameOnly))),
    (
        "E&xtension",
        "Ctrl+F4",
        Some(SortEntry::Mode(SortMode::Extension)),
    ),
    (
        "&Write time",
        "Ctrl+F5",
        Some(SortEntry::Mode(SortMode::Modified)),
    ),
    ("&Size", "Ctrl+F6", Some(SortEntry::Mode(SortMode::Size))),
    (
        "&Unsorted",
        "Ctrl+F7",
        Some(SortEntry::Mode(SortMode::Unsorted)),
    ),
    (
        "&Creation time",
        "Ctrl+F8",
        Some(SortEntry::Mode(SortMode::Created)),
    ),
    (
        "&Access time",
        "Ctrl+F9",
        Some(SortEntry::Mode(SortMode::Accessed)),
    ),
    ("Chan&ge time", "", Some(SortEntry::Mode(SortMode::Changed))),
    (
        "&Descriptions",
        "Ctrl+F10",
        Some(SortEntry::Mode(SortMode::Description)),
    ),
    (
        "Allocated size",
        "",
        Some(SortEntry::Mode(SortMode::Allocated)),
    ),
    ("", "", None),
    ("Use sort groups", "Shift+F11", Some(SortEntry::SortGroups)),
    (
        "Show selected f&irst",
        "Shift+F12",
        Some(SortEntry::SelectedFirst),
    ),
    (
        "Show directories &first",
        "",
        Some(SortEntry::DirectoriesFirst),
    ),
];

/// The mode bound to Ctrl+F`number`.
pub fn mode_for_ctrl_f(number: u8) -> Option<SortMode> {
    match number {
        3 => Some(SortMode::Name),
        4 => Some(SortMode::Extension),
        5 => Some(SortMode::Modified),
        6 => Some(SortMode::Size),
        7 => Some(SortMode::Unsorted),
        8 => Some(SortMode::Created),
        9 => Some(SortMode::Accessed),
        10 => Some(SortMode::Description),
        _ => None,
    }
}

/// The sort-mode letter FAR shows in the panel's corner: the menu hotkey (or first letter), upper-case
/// when the order is reversed.
pub fn mode_letter(mode: SortMode, reverse: bool) -> char {
    let label = SORT_MENU
        .iter()
        .find(|(_, _, entry)| *entry == Some(SortEntry::Mode(mode)))
        .map_or("", |(label, _, _)| *label);
    let letter = label
        .split_once('&')
        .map_or(label, |(_, after)| after)
        .chars()
        .next()
        .unwrap_or(' ')
        .to_ascii_lowercase();
    if reverse {
        letter.to_ascii_uppercase()
    } else {
        letter
    }
}

impl App {
    /// Ctrl+F12.
    pub(super) fn open_sort_menu(&mut self, side: Side) {
        let order = self.panels[side.index()].sort;
        let items = SORT_MENU
            .iter()
            .map(|(label, key, entry)| match entry {
                None => PopupItem::separator(),
                Some(SortEntry::Mode(mode)) => {
                    let direction = if order.reverse { '▼' } else { '▲' };
                    PopupItem::new(label, key, (*mode == order.mode).then_some(direction))
                }
                Some(SortEntry::SortGroups) => {
                    PopupItem::new(label, key, order.sort_groups.then_some('√'))
                }
                Some(SortEntry::SelectedFirst) => {
                    PopupItem::new(label, key, order.selected_first.then_some('√'))
                }
                Some(SortEntry::DirectoriesFirst) => {
                    PopupItem::new(label, key, order.directories_first.then_some('√'))
                }
            })
            .collect();
        let selected = SORT_MENU
            .iter()
            .position(|(_, _, entry)| *entry == Some(SortEntry::Mode(order.mode)))
            .unwrap_or(0);
        self.overlay = Some(Overlay::Popup(Popup {
            title: "Sort by".to_string(),
            bottom_title: "+ - *".to_string(),
            items,
            selected,
            anchor: PopupAnchor::Panel(side),
            action: PopupAction::Sort(side),
        }));
    }

    pub(super) fn finish_sort_menu(&mut self, side: Side, index: usize, key: ChooseKey) {
        let Some((_, _, Some(entry))) = SORT_MENU.get(index) else {
            return;
        };
        let panel = &mut self.panels[side.index()];
        match (entry, key) {
            (SortEntry::Mode(mode), ChooseKey::Plus | ChooseKey::Minus) => {
                panel.set_sort_direction(*mode, key == ChooseKey::Minus);
            }
            (SortEntry::Mode(mode), _) => panel.set_sort(*mode),
            (SortEntry::SortGroups, _) => {
                if wanted(key, panel.sort.sort_groups) != panel.sort.sort_groups {
                    panel.toggle_sort_groups();
                }
            }
            (SortEntry::SelectedFirst, _) => {
                if wanted(key, panel.sort.selected_first) != panel.sort.selected_first {
                    panel.toggle_selected_first();
                }
            }
            (SortEntry::DirectoriesFirst, _) => {
                if wanted(key, panel.sort.directories_first) != panel.sort.directories_first {
                    panel.toggle_directories_first();
                }
            }
        }
    }
}

/// FAR's option rule in the sort menu: `+` turns on, `-` turns off, Enter and `*` toggle.
pub(super) fn wanted(key: ChooseKey, current: bool) -> bool {
    match key {
        ChooseKey::Plus => true,
        ChooseKey::Minus => false,
        ChooseKey::Enter | ChooseKey::Star => !current,
    }
}

/// The panel's own order.
impl super::Panel {
    /// FAR's `SetSortMode`: another mode starts in its default direction, the same mode again
    /// reverses the order.
    pub fn set_sort(&mut self, mode: SortMode) {
        let reverse = if self.sort.mode == mode && self.options.allow_reverse {
            !self.sort.reverse
        } else {
            mode.descending_by_default()
        };
        self.set_sort_direction(mode, reverse);
    }

    /// `+` / `-` in the sort menu: `mode` with an explicit direction.
    pub fn set_sort_direction(&mut self, mode: SortMode, reverse: bool) {
        self.set_sort_order(SortOrder {
            mode,
            reverse,
            ..self.sort
        });
    }

    /// Shift+F12.
    pub fn toggle_selected_first(&mut self) {
        self.set_sort_order(SortOrder {
            selected_first: !self.sort.selected_first,
            ..self.sort
        });
    }

    /// Shift+F11.
    pub fn toggle_sort_groups(&mut self) {
        self.set_sort_order(SortOrder {
            sort_groups: !self.sort.sort_groups,
            ..self.sort
        });
    }

    /// The sort groups made again: the panel sorted by them when it uses them.
    pub(super) fn set_sort_groups(&mut self, groups: super::highlighting::SortGroups) {
        self.sort_groups = groups;
        if self.sort.sort_groups {
            self.set_sort_order(self.sort);
        }
    }

    pub fn toggle_directories_first(&mut self) {
        self.set_sort_order(SortOrder {
            directories_first: !self.sort.directories_first,
            ..self.sort
        });
    }

    pub fn set_sort_order(&mut self, order: SortOrder) {
        self.sort = order;
        let focus = self.current().map(|entry| entry.name.clone());
        let entries = std::mem::take(&mut self.entries);
        self.place_entries(entries, focus.as_deref());
    }
}

#[cfg(test)]
#[path = "sort_groups_tests.rs"]
mod sort_groups_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letters_follow_far_hotkeys_and_direction() {
        assert_eq!(mode_letter(SortMode::Name, false), 'n');
        assert_eq!(mode_letter(SortMode::Extension, false), 'x');
        assert_eq!(mode_letter(SortMode::Size, true), 'S');
        assert_eq!(mode_letter(SortMode::Changed, false), 'g');
        assert_eq!(mode_letter(SortMode::NameOnly, false), 'n');
        assert_eq!(mode_letter(SortMode::Description, false), 'd');
    }

    #[test]
    fn options_follow_far_plus_minus_rule() {
        assert!(wanted(ChooseKey::Plus, true));
        assert!(!wanted(ChooseKey::Minus, false));
        assert!(wanted(ChooseKey::Enter, false));
        assert!(!wanted(ChooseKey::Star, true));
    }

    #[test]
    fn ctrl_function_keys_map_like_far() {
        assert_eq!(mode_for_ctrl_f(3), Some(SortMode::Name));
        assert_eq!(mode_for_ctrl_f(7), Some(SortMode::Unsorted));
        assert_eq!(mode_for_ctrl_f(9), Some(SortMode::Accessed));
        assert_eq!(mode_for_ctrl_f(10), Some(SortMode::Description));
        assert_eq!(mode_for_ctrl_f(11), None, "the owner's: not yet");
    }
}
