//! Ctrl+I, Commands > File panel filter: FAR's "Filters menu" (`filters::EditFilters`). The user's filters,
//! a separator, "Folders" and the types of the panel's files (those its filter hid too, and the marked
//! ones with no file here), each line FAR's `MenuString`: the title in 21 columns, `│`, the attributes
//! (`+` set, `-` clear), `S` size, `D` / `R` date (relative), `H` hard links, `│`, the mask. `+`, `-`,
//! `I`, `X` mark the line for this panel (again: off), Space `+` or off, BS off, each going down;
//! Shift+BS takes every mark off; Del takes a filter of the user's out after a question; Ctrl+Up /
//! Ctrl+Down move it; Ins, F4 and F5 make, change and copy one through its dialog
//! (`filter_edit`). The marks are kept when the menu closes, however it closes, and the panel is
//! read again through them.

use crate::app::text_cut::{fit_to_left, truncate_right};
use std::cell::Cell;
use std::collections::BTreeMap;

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::dialog::{ConfirmAction, Dialog};
use super::filters::{Area, mark_of, marked_anywhere, set_mark, type_mask};
use super::vmenu_keys;
use super::{App, Overlay, Side};
use crate::platform::files::Attributes;
use crate::settings::{Filter, FilterMarks, TypeFilter};
use crate::vfs::filter::{DIRECTORY, Mark};

pub const FILTERS_TITLE: &str = "Filters menu";
/// FAR's `KeysToLocalizedText` of the menu.
pub const FILTERS_KEYS: &str =
    "+, -, Space, I, X, BS, Shift+BS, Ins, Del, F4, F5, Ctrl+Up, Ctrl+Down";
/// FAR's `MaxNameSize`.
const NAME_WIDTH: usize = 21;

/// What a line stands for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FilterItem {
    User(usize),
    Separator,
    Folders,
    /// A file type's mask.
    Type(String),
}

/// A line of the menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilterLine {
    pub text: String,
    pub check: Option<char>,
    pub item: FilterItem,
}

/// The open menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FiltersMenu {
    /// The panel whose file types are listed (FAR's host panel).
    pub side: Side,
    /// Whose marks the menu shows and sets.
    pub area: Area,
    pub lines: Vec<FilterLine>,
    pub selected: usize,
    pub top: Cell<usize>,
    pub rows: Cell<usize>,
    /// A mark or a filter changed: kept when the menu closes.
    pub(super) changed: bool,
}

impl FiltersMenu {
    fn skip(&self) -> Vec<bool> {
        (self.lines.iter())
            .map(|line| line.item == FilterItem::Separator)
            .collect()
    }
}

/// FAR's `AttributesString`: `+` the set ones, `-` the clear ones, four letters each at most (`ABC…`).
fn attributes_text(set: u32, clear: u32) -> String {
    let letters = |bits: u32| {
        if bits == 0 {
            return String::new();
        }
        truncate_right(&Attributes::from_windows_bits(bits).letters(usize::MAX), 4)
    };
    let (plus, minus) = (letters(set), letters(clear & !set));
    let mut text = String::new();
    if !plus.is_empty() {
        text = format!("+{plus}");
    }
    if !minus.is_empty() {
        if !text.is_empty() {
            text.push(' ');
        }
        text.push('-');
        text.push_str(&minus);
    }
    fit_to_left(&text, 11)
}

/// FAR's `MenuString` of a filter of the user's.
fn filter_line(filter: &Filter) -> String {
    let (set, clear) = if filter.use_attributes {
        (filter.attributes_set, filter.attributes_clear)
    } else {
        (0, 0)
    };
    let flag = |on: bool, letter: char| if on { letter } else { '.' };
    let date = if filter.date_relative { 'R' } else { 'D' };
    let flags: String = [
        flag(filter.use_size, 'S'),
        flag(filter.use_date, date),
        flag(filter.use_hard_links, 'H'),
    ]
    .iter()
    .collect();
    let mask = if filter.use_mask {
        vmenu_keys::escape(filter.mask.trim_end())
    } else {
        String::new()
    };
    let amp = usize::from(vmenu_keys::marked_hotkey(&filter.title).is_some());
    let name = fit_to_left(&filter.title, NAME_WIDTH + amp);
    format!(
        "{name} \u{2502} {} {flags} \u{2502} {mask}",
        attributes_text(set, clear)
    )
}

/// The line of "Folders" or of a file type, with its hotkey (FAR's `NameAmpFix` one column for it).
fn fixed_line(hotkey: Option<char>, name: &str, set: u32, clear: u32, mask: &str) -> String {
    let width = NAME_WIDTH + usize::from(hotkey.is_some());
    let hotkey = hotkey.map_or_else(|| "   ".to_string(), |key| format!("&{key}. "));
    format!(
        "{hotkey}{} \u{2502} {} ... \u{2502} {}",
        fit_to_left(name, width),
        attributes_text(set, clear),
        vmenu_keys::escape(mask)
    )
}

/// What a mark key does to the line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MarkKey {
    /// BS.
    Off,
    /// Space: `+`, or off.
    Toggle,
    /// `+`, `-`, `I`, `X`: it, or off when it is there.
    Set(char),
}

/// FAR takes the characters as typed (Shift makes `+` on the main row) and the gray keys.
fn typed(key: KeyEvent) -> bool {
    key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT
}

/// The mark a key puts on the line, if it is a mark key.
fn mark_key(key: KeyEvent) -> Option<MarkKey> {
    match key.code {
        KeyCode::Char(ch @ ('+' | '-')) if typed(key) => Some(MarkKey::Set(ch)),
        KeyCode::Char(ch) if typed(key) && matches!(ch.to_ascii_uppercase(), 'I' | 'X') => {
            Some(MarkKey::Set(ch.to_ascii_uppercase()))
        }
        KeyCode::Char(' ') if key.modifiers.is_empty() => Some(MarkKey::Toggle),
        KeyCode::Backspace if key.modifiers.is_empty() => Some(MarkKey::Off),
        _ => None,
    }
}

/// A type as its filter is found: FAR's case-blind order, upper case.
fn type_key(mask: &str) -> String {
    mask.to_uppercase()
}

/// The hotkeys of the types: 1..9, then A..Z, then none.
fn type_hotkey(index: usize) -> Option<char> {
    let digits = "123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ";
    digits.chars().nth(index)
}

impl App {
    /// Ctrl+I on the active panel.
    pub(super) fn open_filters_menu(&mut self) {
        self.open_area_filters_menu(Area::of(self.active));
    }

    /// The menu of `area`'s marks with the active panel's file types (Find file's and Copy's Filter:
    /// FAR's `EditFilters(area, panel)`).
    pub(super) fn open_area_filters_menu(&mut self, area: Area) {
        if area != Area::Select {
            // A selection by Select's filters waits for Select's menu only.
            self.filters.then_select = None;
        }
        let side = self.active;
        let mut menu = FiltersMenu {
            side,
            area,
            lines: Vec::new(),
            selected: 0,
            top: Cell::new(0),
            rows: Cell::new(1),
            changed: false,
        };
        self.fill_filters_menu(&mut menu, None);
        self.overlay = Some(Overlay::Filters(menu));
    }

    /// The lines as the filters are now; `marks` keeps the marks the menu has (`None`: the filters').
    pub(super) fn fill_filters_menu(&self, menu: &mut FiltersMenu, marks: Option<&[FilterLine]>) {
        let area = menu.area;
        let kept = |item: &FilterItem, mark: Mark| {
            marks.map_or(mark.check(), |lines| {
                (lines.iter())
                    .find(|line| &line.item == item)
                    .and_then(|line| line.check)
            })
        };
        let mut lines = Vec::new();
        for (at, filter) in self.filters.list.iter().enumerate() {
            let item = FilterItem::User(at);
            let check = kept(&item, mark_of(&filter.marks, area));
            lines.push(FilterLine {
                text: filter_line(filter),
                check,
                item,
            });
        }
        lines.push(FilterLine {
            text: String::new(),
            check: None,
            item: FilterItem::Separator,
        });
        let folders = kept(&FilterItem::Folders, mark_of(&self.filters.folders, area));
        lines.push(FilterLine {
            text: fixed_line(Some('0'), "Folders", DIRECTORY, 0, ""),
            check: folders,
            item: FilterItem::Folders,
        });
        for (index, (mask, check)) in self.panel_types(menu.side, area).into_iter().enumerate() {
            let item = FilterItem::Type(mask.clone());
            let check = marks.map_or(check, |_| kept(&item, Mark::None));
            lines.push(FilterLine {
                text: fixed_line(type_hotkey(index), "Panel file type", 0, DIRECTORY, &mask),
                check,
                item,
            });
        }
        menu.lines = lines;
        let last = menu.lines.len().saturating_sub(1);
        menu.selected = vmenu_keys::first_from(&menu.skip(), menu.selected.min(last));
    }

    /// FAR's list of types: the marked ones of this area, then those of the panel's files and of the
    /// files its filter hid, once each whatever their case, in the order of their masks upper-cased
    /// (FAR's `less_icase`).
    fn panel_types(&self, side: Side, area: Area) -> Vec<(String, Option<char>)> {
        let mut types: BTreeMap<String, (String, Option<char>)> = BTreeMap::new();
        for kind in &self.filters.types {
            let mark = mark_of(&kind.marks, area);
            if mark != Mark::None {
                types
                    .entry(type_key(&kind.mask))
                    .or_insert((kind.mask.clone(), mark.check()));
            }
        }
        // An info panel lists no files (FAR's `InfoList::GetFileName`): only the marked types.
        if !self.shows_files(side) || !self.panels[side.index()].visible {
            return types.into_values().collect();
        }
        let panel = &self.panels[side.index()];
        let shown = panel.entries.iter().filter_map(type_mask);
        for mask in shown.chain(panel.filtered_types.iter().cloned()) {
            types.entry(type_key(&mask)).or_insert((mask, None));
        }
        types.into_values().collect()
    }

    pub(super) fn handle_filters_menu_key(&mut self, mut menu: FiltersMenu, key: KeyEvent) {
        let typed = typed(key);
        if let Some(check) = mark_key(key) {
            self.mark_filter_line(&mut menu, check);
            return self.overlay = Some(Overlay::Filters(menu));
        }
        let ctrl = key.modifiers == KeyModifiers::CONTROL;
        match key.code {
            KeyCode::Esc | KeyCode::F(10) | KeyCode::Enter => return self.close_filters_menu(menu),
            KeyCode::Backspace if key.modifiers == KeyModifiers::SHIFT => {
                for line in &mut menu.lines {
                    line.check = None;
                }
                menu.changed = true;
            }
            KeyCode::Delete if key.modifiers.is_empty() => return self.ask_remove_filter(menu),
            KeyCode::F(4) if key.modifiers.is_empty() => {
                let item = menu.lines.get(menu.selected).map(|line| line.item.clone());
                if !matches!(item, Some(FilterItem::User(_))) {
                    self.filters.behind = Some(menu);
                    return self.show_dialog(Dialog::error(
                        FILTERS_TITLE,
                        vec!["Only custom filter can be edited".to_string()],
                    ));
                }
                return self.edit_filter_line(menu, false, true);
            }
            KeyCode::Insert if key.modifiers.is_empty() => {
                return self.edit_filter_line(menu, false, false);
            }
            KeyCode::F(5) if key.modifiers.is_empty() => {
                return self.edit_filter_line(menu, true, false);
            }
            KeyCode::Up | KeyCode::Down if ctrl => {
                self.move_filter(&mut menu, key.code == KeyCode::Up)
            }
            KeyCode::Char(ch) if typed || key.modifiers == KeyModifiers::ALT => {
                let hotkey = ch.to_ascii_lowercase();
                if let Some(line) = (menu.lines.iter())
                    .position(|line| vmenu_keys::marked_hotkey(&line.text) == Some(hotkey))
                {
                    menu.selected = line;
                }
            }
            code => {
                menu.selected =
                    vmenu_keys::step(&menu.skip(), menu.selected, code, menu.rows.get());
            }
        }
        self.overlay = Some(Overlay::Filters(menu));
    }

    /// A mark key, then down (FAR's `Key(KEY_DOWN)`).
    fn mark_filter_line(&mut self, menu: &mut FiltersMenu, check: MarkKey) {
        let Some(line) = menu.lines.get_mut(menu.selected) else {
            return;
        };
        if line.item == FilterItem::Separator {
            return;
        }
        line.check = match check {
            MarkKey::Off => None,
            MarkKey::Toggle if line.check.is_some() => None,
            MarkKey::Toggle => Some('+'),
            MarkKey::Set(key) => (line.check != Some(key)).then_some(key),
        };
        menu.changed = true;
        menu.selected = vmenu_keys::step(&menu.skip(), menu.selected, KeyCode::Down, 1);
    }

    /// Ctrl+Up / Ctrl+Down on a filter of the user's: it and the next one change places.
    fn move_filter(&mut self, menu: &mut FiltersMenu, up: bool) {
        let Some(FilterItem::User(at)) =
            menu.lines.get(menu.selected).map(|line| line.item.clone())
        else {
            return;
        };
        let other = if up { at.checked_sub(1) } else { Some(at + 1) };
        let Some(other) = other.filter(|&other| other < self.filters.list.len()) else {
            return;
        };
        self.filters.list.swap(at, other);
        // Each line keeps its mark as it moves.
        menu.lines.swap(at, other);
        menu.lines[at].item = FilterItem::User(at);
        menu.lines[other].item = FilterItem::User(other);
        menu.selected = other;
        menu.changed = true;
    }

    fn ask_remove_filter(&mut self, menu: FiltersMenu) {
        let item = menu.lines.get(menu.selected).map(|line| line.item.clone());
        let Some(FilterItem::User(at)) = item else {
            self.filters.behind = Some(menu);
            return self.show_dialog(Dialog::error(
                FILTERS_TITLE,
                vec!["Only custom filter can be deleted".to_string()],
            ));
        };
        let title = self.filters.list[at].title.clone();
        self.filters.behind = Some(menu);
        self.show_dialog(Dialog::Confirm {
            title: FILTERS_TITLE.to_string(),
            lines: vec![
                "Do you wish to delete the filter".to_string(),
                format!("\"{title}\""),
            ],
            buttons: vec!["Delete", "Cancel"],
            focused: 0,
            danger: false,
            action: ConfirmAction::RemoveFilter(at),
        });
    }

    /// Del's first button: the filter goes, the menu keeps the other lines' marks.
    pub(super) fn remove_filter(&mut self, at: usize) {
        if at >= self.filters.list.len() {
            return;
        }
        self.filters.list.remove(at);
        let Some(mut menu) = self.filters.behind.take() else {
            return;
        };
        let mut lines = std::mem::take(&mut menu.lines);
        lines.retain(|line| line.item != FilterItem::User(at));
        for line in &mut lines {
            if let FilterItem::User(index) = &mut line.item
                && *index > at
            {
                *index -= 1;
            }
        }
        self.fill_filters_menu(&mut menu, Some(&lines));
        menu.changed = true;
        self.filters.behind = Some(menu);
    }

    /// FAR's `ProcessSelection`: the marks into the filters, the types' filters made or dropped, the
    /// panels read again.
    fn close_filters_menu(&mut self, menu: FiltersMenu) {
        let then_select = self.filters.then_select.take();
        if menu.changed {
            self.keep_filter_marks(&menu);
        }
        if let Some((side, on)) = then_select {
            self.select_by_filter(side, on);
        }
    }

    fn keep_filter_marks(&mut self, menu: &FiltersMenu) {
        let area = menu.area;
        for line in &menu.lines {
            let mark = Mark::from_check(line.check);
            match &line.item {
                FilterItem::User(at) => {
                    if let Some(filter) = self.filters.list.get_mut(*at) {
                        set_mark(&mut filter.marks, area, mark);
                    }
                }
                FilterItem::Folders => set_mark(&mut self.filters.folders, area, mark),
                FilterItem::Type(mask) => self.mark_type(mask, area, mark),
                FilterItem::Separator => {}
            }
        }
        self.apply_filters();
    }

    /// A file type's filter marked or not; one marked nowhere is dropped.
    fn mark_type(&mut self, mask: &str, area: Area, mark: Mark) {
        let types = &mut self.filters.types;
        let at = types
            .iter()
            .position(|kind| type_key(&kind.mask) == type_key(mask));
        match at {
            Some(at) => {
                set_mark(&mut types[at].marks, area, mark);
                if !marked_anywhere(&types[at].marks) {
                    types.remove(at);
                }
            }
            None if mark != Mark::None => {
                let mut marks = FilterMarks::default();
                set_mark(&mut marks, area, mark);
                types.push(TypeFilter {
                    mask: mask.to_string(),
                    marks,
                });
            }
            None => {}
        }
    }

    /// The menu a dialog of it stood over comes back; true when it did.
    pub(super) fn resume_filters_menu(&mut self) -> bool {
        if self.overlay.is_some() || self.waiting.is_some() {
            return false;
        }
        let Some(menu) = self.filters.behind.take() else {
            return false;
        };
        self.overlay = Some(Overlay::Filters(menu));
        true
    }
}

#[cfg(test)]
#[path = "filters_menu_tests.rs"]
mod tests;
