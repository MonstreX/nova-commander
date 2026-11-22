//! The mouse in menus and lists as FAR's `VMenu::ProcessMouse` (the F9 pull-downs with the menu bar
//! of `HMenu::ProcessMouse`, pop-up menus, the drive menu, lists, the list under the command line):
//! the item under the mouse is selected as it moves, a button pressed and let go on an item is Enter,
//! the middle button is Enter, a click outside closes the menu (FAR's `LBtnClick` / `RBtnClick`
//! "cancel") or chooses with the middle button (`MBtnClick` "apply"), a button held on the box's top
//! or bottom moves on, the wheel moves by one. The list under the command line takes the left button
//! on an item at once as Enter and closes on the right or middle one there (`editcontrol.cpp`).
//! What the renderer reported is dropped as soon as a key changes the menu in front: the next
//! records wait for the next draw rather than hit the old one.

use std::borrow::Cow;
use std::cell::Cell;
use std::time::Instant;

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::completion::Item;
use super::console_input::{Mouse, MouseAction};
use super::form::{Control, ListLine};
use super::menu::{MenuItem, MenuState, title_positions};
use super::options::MenuClick;
use super::{App, Completion, Dialog, HistoryLine, Overlay};
use crate::platform;

#[path = "mouse_menu_bar.rs"]
mod bar;

/// A rectangle of the screen.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Cells {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
}

impl Cells {
    pub fn contains(self, column: u16, row: u16) -> bool {
        (self.x..self.x + self.width).contains(&column)
            && (self.y..self.y + self.height).contains(&row)
    }
}

/// Where the renderer put the menu or list in front: its box (the frame included), the box with its
/// margin, and the item on the first row inside the frame.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MenuArea {
    pub frame: Cells,
    pub outer: Cells,
    pub first: usize,
}

impl MenuArea {
    /// The item under the mouse, inside the frame.
    fn item_at(self, column: u16, row: u16) -> Option<usize> {
        let frame = self.frame;
        let inside = column > frame.x
            && column + 1 < frame.x + frame.width
            && row > frame.y
            && row + 1 < frame.y + frame.height;
        inside.then(|| self.first + usize::from(row - frame.y - 1))
    }
}

/// FAR's `TopPos`: the first line shown moves only when the selected one leaves the view.
pub fn keep_in_view(top: &Cell<usize>, selected: usize, lines: usize, rows: usize) -> usize {
    let rows = rows.min(lines);
    let mut first = top.get().min(lines - rows);
    if selected < first {
        first = selected;
    } else if rows > 0 && selected >= first + rows {
        first = selected + 1 - rows;
    }
    top.set(first);
    first
}

/// Which menu is in front.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Bar,
    Popup,
    Drive,
    List,
    Completion,
    /// The open list of a dialog's drop-down field.
    DropDown,
    History,
    UserMenu,
    EditList,
    Shortcuts,
    Filters,
    PluginMenu,
}

/// A button held on a menu's top (-1) or bottom (+1) edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MenuHold {
    step: isize,
    next: Instant,
}

impl App {
    fn menu_kind(&self) -> Option<Kind> {
        if self.open_drop_down().is_some() {
            return Some(Kind::DropDown);
        }
        match &self.overlay {
            Some(Overlay::Menu(_)) => Some(Kind::Bar),
            Some(Overlay::Popup(_)) => Some(Kind::Popup),
            Some(Overlay::DriveMenu(_)) => Some(Kind::Drive),
            Some(Overlay::History(_)) => Some(Kind::History),
            Some(Overlay::UserMenu(_)) => Some(Kind::UserMenu),
            Some(Overlay::EditList(_)) => Some(Kind::EditList),
            Some(Overlay::Shortcuts(_)) => Some(Kind::Shortcuts),
            Some(Overlay::Filters(_)) => Some(Kind::Filters),
            Some(Overlay::PluginMenu(_)) => Some(Kind::PluginMenu),
            Some(Overlay::Dialog(Dialog::List { .. })) => Some(Kind::List),
            None if self.completion.as_ref().is_some_and(Completion::is_shown) => {
                Some(Kind::Completion)
            }
            _ => None,
        }
    }

    /// The open drop-down list of the dialog in front, or its edit line's history list without Del's
    /// question in front: its lines and the line under the cursor.
    fn open_drop_down(&self) -> Option<(Cow<'_, [ListLine]>, usize)> {
        let Some(Overlay::Dialog(Dialog::Form { form, .. })) = &self.overlay else {
            return None;
        };
        if let Some(history) = form.open_history() {
            let cursor = history.open.as_ref()?.cursor;
            return (history.question.is_none())
                .then(|| (Cow::Owned(history.lines().lines), cursor));
        }
        match form.control(form.focus) {
            Some(Control::DropDown {
                lines,
                open: Some(list),
                ..
            }) => Some((Cow::Borrowed(lines.as_slice()), list.cursor)),
            _ => None,
        }
    }

    /// Whether item `index` of the menu in front can be selected (no separator).
    fn menu_item_selectable(&self, kind: Kind, index: usize) -> bool {
        if kind == Kind::DropDown {
            return self.open_drop_down().is_some_and(|(lines, _)| {
                lines.get(index).is_some_and(|line| !line.is_separator())
            });
        }
        match (kind, &self.overlay) {
            (Kind::Bar, Some(Overlay::Menu(state))) => {
                matches!(state.items().get(index), Some(MenuItem::Item { .. }))
            }
            (Kind::Popup, Some(Overlay::Popup(popup))) => {
                popup.items.get(index).is_some_and(|item| !item.separator)
            }
            (Kind::Drive, Some(Overlay::DriveMenu(menu))) => menu.choosable(index),
            (Kind::History, Some(Overlay::History(menu))) => {
                matches!(menu.lines.get(index), Some(HistoryLine::Entry { .. }))
            }
            (Kind::UserMenu, Some(Overlay::UserMenu(menu))) => {
                menu.lines.get(index).is_some_and(|line| !line.separator)
            }
            (Kind::EditList, Some(Overlay::EditList(list))) => {
                index < list.lines.len() && !list.separators.contains(&index)
            }
            (Kind::Shortcuts, Some(Overlay::Shortcuts(menu))) => {
                menu.lines.get(index).is_some_and(|line| !line.separator)
            }
            (Kind::Filters, Some(Overlay::Filters(menu))) => menu
                .lines
                .get(index)
                .is_some_and(|line| line.item != crate::app::filters_menu::FilterItem::Separator),
            (Kind::List, Some(Overlay::Dialog(Dialog::List { items, .. }))) => index < items.len(),
            (Kind::PluginMenu, Some(Overlay::PluginMenu(menu))) => menu.choosable(index),
            (Kind::Completion, _) => self.completion.as_ref().is_some_and(|completion| {
                matches!(completion.items.get(index), Some(Item::Text { .. }))
            }),
            _ => false,
        }
    }

    fn menu_selected(&self, kind: Kind) -> Option<usize> {
        if kind == Kind::DropDown {
            return self.open_drop_down().map(|(_, cursor)| cursor);
        }
        match (kind, &self.overlay) {
            (Kind::Bar, Some(Overlay::Menu(state))) => Some(state.item),
            (Kind::Popup, Some(Overlay::Popup(popup))) => Some(popup.selected),
            (Kind::Drive, Some(Overlay::DriveMenu(menu))) => Some(menu.selected),
            (Kind::History, Some(Overlay::History(menu))) => Some(menu.selected),
            (Kind::UserMenu, Some(Overlay::UserMenu(menu))) => Some(menu.selected),
            (Kind::EditList, Some(Overlay::EditList(list))) => Some(list.selected),
            (Kind::Shortcuts, Some(Overlay::Shortcuts(menu))) => Some(menu.selected),
            (Kind::Filters, Some(Overlay::Filters(menu))) => Some(menu.selected),
            (Kind::PluginMenu, Some(Overlay::PluginMenu(menu))) => Some(menu.selected),
            (Kind::List, Some(Overlay::Dialog(Dialog::List { cursor, .. }))) => Some(*cursor),
            (Kind::Completion, _) => self
                .completion
                .as_ref()
                .map(|completion| completion.selected),
            _ => None,
        }
    }

    /// Selects item `index`; true when it was not selected.
    fn menu_select(&mut self, kind: Kind, index: usize) -> bool {
        if self.menu_selected(kind) == Some(index) || !self.menu_item_selectable(kind, index) {
            return false;
        }
        match (kind, &mut self.overlay) {
            (Kind::DropDown, Some(Overlay::Dialog(Dialog::Form { form, .. }))) => {
                form.set_open_line(index);
            }
            (Kind::Bar, Some(Overlay::Menu(state))) => state.item = index,
            (Kind::Popup, Some(Overlay::Popup(popup))) => popup.selected = index,
            (Kind::Drive, Some(Overlay::DriveMenu(menu))) => menu.selected = index,
            (Kind::History, Some(Overlay::History(menu))) => menu.selected = index,
            (Kind::UserMenu, Some(Overlay::UserMenu(menu))) => menu.selected = index,
            (Kind::EditList, Some(Overlay::EditList(list))) => list.selected = index,
            (Kind::Shortcuts, Some(Overlay::Shortcuts(menu))) => menu.selected = index,
            (Kind::Filters, Some(Overlay::Filters(menu))) => menu.selected = index,
            (Kind::PluginMenu, Some(Overlay::PluginMenu(menu))) => menu.selected = index,
            (Kind::List, Some(Overlay::Dialog(Dialog::List { cursor, .. }))) => *cursor = index,
            (Kind::Completion, _) => {
                if let Some(completion) = &mut self.completion {
                    completion.selected = index;
                }
                // FAR's `DN_LISTCHANGE`: the line shows the item.
                self.put_selected_on_the_line();
            }
            _ => return false,
        }
        true
    }

    fn menu_len(&self, kind: Kind) -> usize {
        if kind == Kind::DropDown {
            return self.open_drop_down().map_or(0, |(lines, _)| lines.len());
        }
        match (kind, &self.overlay) {
            (Kind::Bar, Some(Overlay::Menu(state))) => state.items().len(),
            (Kind::Popup, Some(Overlay::Popup(popup))) => popup.items.len(),
            (Kind::Drive, Some(Overlay::DriveMenu(menu))) => menu.line_count(),
            (Kind::History, Some(Overlay::History(menu))) => menu.lines.len(),
            (Kind::UserMenu, Some(Overlay::UserMenu(menu))) => menu.lines.len(),
            (Kind::EditList, Some(Overlay::EditList(list))) => list.lines.len(),
            (Kind::Shortcuts, Some(Overlay::Shortcuts(menu))) => menu.lines.len(),
            (Kind::Filters, Some(Overlay::Filters(menu))) => menu.lines.len(),
            (Kind::PluginMenu, Some(Overlay::PluginMenu(menu))) => menu.lines.len(),
            (Kind::List, Some(Overlay::Dialog(Dialog::List { items, .. }))) => items.len(),
            (Kind::Completion, _) => self
                .completion
                .as_ref()
                .map_or(0, |completion| completion.items.len()),
            _ => 0,
        }
    }

    /// One selectable item up or down, not past the ends (FAR clears its wrap mode for this).
    fn menu_step(&mut self, kind: Kind, step: isize) -> bool {
        let Some(mut at) = self.menu_selected(kind) else {
            return false;
        };
        let len = self.menu_len(kind);
        while let Some(next) = at.checked_add_signed(step).filter(|&next| next < len) {
            if self.menu_item_selectable(kind, next) {
                return self.menu_select(kind, next);
            }
            at = next;
        }
        false
    }

    /// A record while a menu or list is in front; `None` when none is.
    pub(super) fn menu_mouse(&mut self, mouse: &Mouse) -> Option<bool> {
        let Some(kind) = self.menu_kind() else {
            self.mouse.menu_down = None;
            self.mouse.menu_hold = None;
            self.mouse.menu_thumb = false;
            return None;
        };
        // What was held over the panels or the key bar is over now.
        self.mouse.hold = None;
        self.mouse.key_press = None;
        self.end_drag();
        if !(mouse.buttons.left || mouse.buttons.right) {
            self.mouse.menu_hold = None;
        }
        let Some(area) = self.mouse.menu else {
            return Some(false);
        };
        if kind == Kind::Bar && mouse.row == 0 {
            self.mouse.menu_hold = None;
            return Some(self.menu_bar_mouse(mouse));
        }
        let interior = area.item_at(mouse.column, mouse.row);
        match mouse.action {
            MouseAction::Wheel(notches) => {
                let step = if notches > 0 { -1 } else { 1 };
                let mut changed = false;
                for _ in 0..notches.unsigned_abs() {
                    changed |= self.menu_step(kind, step);
                }
                return Some(changed);
            }
            MouseAction::HorizontalWheel(_) => return Some(false),
            _ => {}
        }
        Some(self.menu_button(kind, area, interior, mouse))
    }

    fn menu_button(
        &mut self,
        kind: Kind,
        area: MenuArea,
        interior: Option<usize>,
        mouse: &Mouse,
    ) -> bool {
        // The scroll bar, and its slider dragged wherever the mouse goes.
        if self.menu_scroll_bar(kind, area, mouse) {
            return true;
        }
        let button_event = matches!(mouse.action, MouseAction::Button | MouseAction::DoubleClick);
        let pressed = mouse.pressed;
        if button_event && !area.outer.contains(mouse.column, mouse.row) {
            let click = if pressed.middle {
                self.prefs.menu_middle
            } else if pressed.left {
                self.prefs.menu_left
            } else if pressed.right {
                self.prefs.menu_right
            } else {
                MenuClick::Ignore
            };
            match click {
                MenuClick::Cancel => self.menu_key(KeyCode::Esc),
                MenuClick::Apply => self.menu_key(KeyCode::Enter),
                MenuClick::Ignore => return false,
            }
            return true;
        }
        if kind == Kind::Completion && button_event && interior.is_some() && pressed.any() {
            return self.completion_click(interior, pressed.left);
        }
        if pressed.middle && interior.is_some() {
            self.menu_key(KeyCode::Enter);
            return true;
        }
        let held = mouse.buttons.left || mouse.buttons.right;
        if held && self.menu_edge_hold(kind, area, mouse) {
            return true;
        }
        self.mouse.menu_hold = None;
        let Some(index) = interior.filter(|&index| self.menu_item_selectable(kind, index)) else {
            return false;
        };
        // FAR's `KEY_MSRCLICK` in the drive menu: EMenu for the drive.
        if kind == Kind::Drive
            && button_event
            && pressed.right
            && self.drive_right_click(index, mouse)
        {
            return true;
        }
        let changed = self.menu_select(kind, index);
        if button_event && held {
            self.mouse.menu_down = Some(area.frame);
        } else if button_event && self.mouse.menu_down.take() == Some(area.frame) {
            self.menu_key(KeyCode::Enter);
            return true;
        }
        changed
    }

    /// A key for the menu in front; its box is unknown until the next draw.
    fn menu_key(&mut self, code: KeyCode) {
        self.mouse.menu = None;
        self.mouse.menu_down = None;
        self.mouse.menu_hold = None;
        self.dispatch_key(KeyEvent::new(code, KeyModifiers::NONE));
    }

    /// FAR's `KEY_MSLCLICK` in the list under the command line: Enter with the item under the mouse;
    /// any other button there closes the list (its `default:`).
    fn completion_click(&mut self, interior: Option<usize>, left: bool) -> bool {
        if !left {
            self.completion = None;
            self.mouse.menu = None;
            return true;
        }
        if let Some(index) = interior {
            self.menu_select(Kind::Completion, index);
        }
        self.menu_key(KeyCode::Enter);
        true
    }

    /// A button on the box's top or bottom edge: the selection moves that way now and on while it
    /// stays there.
    fn menu_edge_hold(&mut self, kind: Kind, area: MenuArea, mouse: &Mouse) -> bool {
        let frame = area.frame;
        let across = mouse.column > frame.x && mouse.column + 1 < frame.x + frame.width;
        let step = if mouse.row == frame.y {
            -1
        } else if mouse.row + 1 == frame.y + frame.height {
            1
        } else {
            return false;
        };
        if !across {
            return false;
        }
        if self.mouse.menu_hold.is_some_and(|hold| hold.step == step) {
            return true;
        }
        self.menu_step(kind, step);
        let (delay, _) = platform::input::keyboard_repeat();
        self.mouse.menu_hold = Some(MenuHold {
            step,
            next: Instant::now() + delay,
        });
        true
    }

    /// The held edge moves the selection on at the keyboard's repeat rate.
    pub(super) fn pump_menu_hold(&mut self) -> bool {
        let Some(hold) = self.mouse.menu_hold else {
            return false;
        };
        let Some(kind) = self.menu_kind() else {
            self.mouse.menu_hold = None;
            return false;
        };
        let now = Instant::now();
        if now < hold.next {
            return false;
        }
        let (_, interval) = platform::input::keyboard_repeat();
        self.mouse.menu_hold = Some(MenuHold {
            next: now + interval,
            ..hold
        });
        self.menu_step(kind, hold.step)
    }

    /// The menu bar while a pull-down is open (FAR's `HMenu::TestMouse`): the mouse over another
    /// title opens that one's menu.
    fn menu_bar_mouse(&mut self, mouse: &Mouse) -> bool {
        let menu = title_positions()
            .iter()
            .rposition(|&x| x <= mouse.column)
            .unwrap_or(0);
        match &mut self.overlay {
            Some(Overlay::Menu(state)) if state.menu != menu => {
                *state = MenuState {
                    menu,
                    item: 0,
                    ..*state
                };
                self.mouse.menu = None;
                self.mouse.menu_down = None;
                true
            }
            _ => false,
        }
    }
}
