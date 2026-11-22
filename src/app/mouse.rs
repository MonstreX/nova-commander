//! The mouse on the panels as FAR has it (`FilePanels::ProcessMouse`, `FileList::ProcessMouse`): a
//! click moves the cursor and makes the panel active, a double click is Enter, the middle button is
//! Enter with the modifiers held, the right button selects (also dragged along), held over the panel's
//! top or bottom it scrolls on as a key held down, the wheel scrolls the active panel, the sort letter
//! opens the drive menu (left) or the sort menu (right).

use std::time::Instant;

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::console_input::{Mouse, MouseAction};
use super::mouse_bars::{CommandLineArea, KeyBarArea, KeyBarPress};
use super::mouse_dialogs::{ButtonPress, DialogArea};
use super::mouse_drag::{Drag, DragText};
use super::mouse_menus::{Cells, MenuArea, MenuHold};
use super::{App, Command, Completion, MainView, Screen, Side};
use crate::platform;

/// Where the renderer put a panel on the last draw.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PanelArea {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
    /// The rows of entries; `None` for the info panel and a panel too small for any.
    pub list: Option<ListArea>,
}

/// The rows of a panel's entries.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ListArea {
    pub top: u16,
    pub rows: u16,
    /// The first screen column of each stripe; the divider before a stripe belongs to the one before
    /// (FAR's `MoveToMouse`).
    pub stripe_lefts: Vec<u16>,
}

impl PanelArea {
    pub(super) fn contains(&self, column: u16, row: u16) -> bool {
        (self.x..self.x + self.width).contains(&column)
            && (self.y..self.y + self.height).contains(&row)
    }

    pub(super) fn bottom(&self) -> u16 {
        self.y + self.height.saturating_sub(1)
    }
}

/// The mouse's state between its records.
#[derive(Debug, Default)]
pub struct MouseState {
    /// `interface.mouse`: the program takes the mouse (FAR's `Mouse`).
    pub enabled: bool,
    /// Written by the renderer on every draw; `None` where no panel is drawn.
    pub areas: [Option<PanelArea>; 2],
    /// The key bar and the command line on the last draw, `None` when not drawn.
    pub key_bar: Option<KeyBarArea>,
    pub command_line: Option<CommandLineArea>,
    /// A key of the bar the left or right button went down on.
    pub(super) key_press: Option<KeyBarPress>,
    /// The last double click on the command line, for FAR's click after it.
    pub(super) last_double: Option<(Instant, (u16, u16))>,
    /// Where the program's screen is, while one owns the screen.
    pub terminal: Option<Cells>,
    /// The program told of a button held, and what it was told: it must hear the button let go.
    pub(super) told: Option<(MainView, super::mouse_terminal::Told)>,
    pub(super) drag: Option<Drag>,
    /// The note following the mouse during a drag.
    pub drag_text: Option<DragText>,
    /// The menu or list in front on the last draw.
    pub menu: Option<MenuArea>,
    /// The dialog in front on the last draw.
    pub dialog: Option<DialogArea>,
    /// A dialog button the left button went down on.
    pub(super) button_press: Option<ButtonPress>,
    /// The box a button went down in over an item: let go there it is Enter (FAR's
    /// `VMENU_MOUSEDOWN`).
    pub(super) menu_down: Option<Cells>,
    pub(super) menu_hold: Option<MenuHold>,
    /// The left button pressed on a menu's scroll bar between its arrows: the slider follows it.
    pub(super) menu_thumb: bool,
    pub(super) hold: Option<Hold>,
    /// The panel whose scroll bar's thumb the left button holds (`mouse_scroll_bar`).
    pub(super) thumb: Option<Side>,
    /// What the right button does to the entries it goes over: select or deselect (FAR's
    /// `MouseSelection`, decided by the first one).
    selecting: bool,
    /// The right button went down on an entry for Explorer's menu: shown where the buttons are let go
    /// over the panels (FAR's `delayedShowEMenu`); forgotten when they are let go anywhere else, as a
    /// key, a window or the focus changes what is in front.
    pub(super) context_menu: bool,
}

impl MouseState {
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            ..Self::default()
        }
    }

    /// A key or a resize changed what is in front: where it was is not known until the next draw, and
    /// what a button was doing there is over.
    pub(super) fn forget_front(&mut self) {
        self.menu = None;
        self.dialog = None;
        self.button_press = None;
        self.menu_down = None;
        self.menu_hold = None;
        self.key_press = None;
        self.drag = None;
        self.drag_text = None;
        self.thumb = None;
        self.context_menu = false;
    }

    /// Before a draw: only what is drawn now is anywhere.
    pub fn forget_areas(&mut self) {
        self.areas = [None, None];
        self.key_bar = None;
        self.command_line = None;
        self.menu = None;
        self.dialog = None;
        self.terminal = None;
    }
}

/// A button held over a panel's top (`step` -1) or bottom (+1): the cursor goes on moving.
#[derive(Debug)]
pub(super) struct Hold {
    side: Side,
    step: isize,
    /// The right button alone is held: the entries passed are selected as it decided.
    select: bool,
    /// Something was selected on the way: "selected first" sorts when the hold ends.
    selected: bool,
    next: Instant,
    /// Held on a scroll bar's arrow, not on the panel's top or bottom.
    bar: bool,
}

impl App {
    /// A mouse record; true when the screen may have changed.
    pub fn handle_mouse(&mut self, mouse: Mouse) -> bool {
        // Explorer's menu pending: the buttons let go take it, wherever they are let go.
        let context_menu = !mouse.buttons.any() && std::mem::take(&mut self.mouse.context_menu);
        if self.mouse.enabled
            && let Some(changed) = self.terminal_mouse(&mouse)
        {
            return changed;
        }
        let elsewhere = !matches!(self.screen, Screen::Panels)
            || self.main_view != MainView::Panels
            || self.waiting.is_some()
            || self.plugin_key_waits();
        if self.mouse.enabled && !elsewhere {
            if matches!(self.overlay, Some(super::Overlay::FolderTree(_))) {
                return self.folder_tree_mouse(&mouse);
            }
            if let Some(changed) = self.menu_mouse(&mouse) {
                return changed;
            }
            if let Some(changed) = self.plugin_dialog_mouse(&mouse) {
                return changed;
            }
            if let Some(changed) = self.dialog_mouse(&mouse) {
                self.tell_plugin_dialog();
                return changed;
            }
        }
        if !self.mouse_on_panels() {
            self.forget_panels_mouse();
            return self.viewer_mouse(&mouse) || self.editor_mouse(&mouse);
        }
        if let Some(changed) = self.key_bar_release(&mouse) {
            return changed;
        }
        // The wheel would change what is dragged under the note.
        if self.mouse.drag.is_some()
            && matches!(
                mouse.action,
                MouseAction::Wheel(_) | MouseAction::HorizontalWheel(_)
            )
        {
            return false;
        }
        match mouse.action {
            MouseAction::Wheel(notches) => return self.wheel(notches, mouse.modifiers),
            MouseAction::HorizontalWheel(notches) => {
                return self.horizontal_wheel(notches, mouse.modifiers);
            }
            _ => {}
        }
        if let Some(changed) = self.thumb_mouse(&mouse) {
            return changed;
        }
        if context_menu {
            return self.show_context_menu(&mouse);
        }
        if let Some(changed) = self.drag_mouse(&mouse) {
            return changed;
        }
        if !mouse.buttons.any() {
            return self.end_hold();
        }
        // The look for the command line ends on any other input (FAR); the list is not up yet.
        if is_button_event(mouse.action) {
            self.completion = None;
        }
        if self.go_on_holding(&mouse) {
            return false;
        }
        if self.top_line_mouse(&mouse) {
            return true;
        }
        if mouse.buttons.middle {
            if mouse.pressed.middle {
                self.dispatch_key(KeyEvent::new(KeyCode::Enter, mouse.modifiers));
            }
            return mouse.pressed.middle;
        }
        if let Some((side, area)) = self.panel_under(&mouse) {
            return self.panel_mouse(side, &area, &mouse);
        }
        self.key_bar_press(&mouse) || self.command_line_mouse(&mouse)
    }

    /// The panel under the mouse, the active one first.
    pub(super) fn panel_under(&self, mouse: &Mouse) -> Option<(Side, PanelArea)> {
        [self.active, self.active.other()]
            .into_iter()
            .find_map(|side| {
                let area = self.mouse.areas[side.index()].as_ref()?;
                area.contains(mouse.column, mouse.row)
                    .then(|| (side, area.clone()))
            })
    }

    /// The panels are on screen with nothing over them. The list under the command line is modal
    /// as FAR's (`editcontrol.cpp`): the mouse outside it does nothing.
    fn mouse_on_panels(&self) -> bool {
        self.mouse.enabled
            && matches!(self.screen, Screen::Panels)
            && self.main_view == MainView::Panels
            && self.overlay.is_none()
            && !self.completion.as_ref().is_some_and(Completion::is_shown)
            && self.shown_task.is_none()
            && self.waiting.is_none()
            // A panel key waits for its plugin: the panels are its until it answers.
            && !self.plugin_key_waits()
            && self.panels.iter().all(|panel| panel.size_count.is_none())
    }

    /// FAR's `Scroll`: the view and the cursor by the system's wheel lines, by one with Alt.
    fn wheel(&mut self, notches: i32, modifiers: KeyModifiers) -> bool {
        let lines = match modifiers {
            KeyModifiers::NONE => platform::input::wheel_scroll_lines(),
            KeyModifiers::ALT => 1,
            _ => return false,
        };
        let delta = i64::from(notches) * i64::from(lines);
        let delta = isize::try_from(-delta).unwrap_or(0);
        if self.tree_wheel(delta) || self.quick_view_wheel(delta) || self.info_wheel(delta) {
            return true;
        }
        if !self.active_panel().visible || !self.shows_files(self.active) {
            return false;
        }
        self.active_panel_mut().scroll(delta);
        true
    }

    /// Left and Right as many times as the system's wheel characters, once with Alt.
    fn horizontal_wheel(&mut self, notches: i32, modifiers: KeyModifiers) -> bool {
        let chars = match modifiers {
            KeyModifiers::NONE => platform::input::wheel_scroll_chars(),
            KeyModifiers::ALT => 1,
            _ => return false,
        };
        // FAR repeats Left and Right: with one stripe and a typed line they move in the line.
        let typing = !self.command_line.is_empty() && self.active_panel().stripes() == 1;
        if !self.active_panel().visible || !self.shows_files(self.active) || typing {
            return false;
        }
        let direction = notches.signum() as isize;
        for _ in 0..notches.unsigned_abs().saturating_mul(chars) {
            self.active_panel_mut().move_column(direction);
        }
        true
    }

    fn panel_mouse(&mut self, side: Side, area: &PanelArea, mouse: &Mouse) -> bool {
        if self.scroll_bar_mouse(side, area, mouse) {
            return true;
        }
        if self.tree_side() == Some(side) {
            return self.tree_mouse(side, area, mouse);
        }
        let button_event = is_button_event(mouse.action);
        let (column, row) = (mouse.column, mouse.row);
        let list = match &area.list {
            Some(list) if self.shows_files(side) => list,
            _ => {
                if button_event {
                    self.active = side;
                }
                // FAR acts on a button going down, not on one let go with another held.
                let pressed = mouse.pressed.left || mouse.pressed.right;
                if mouse.action == MouseAction::Button && pressed && self.info_side() == Some(side)
                {
                    let left = mouse.pressed.left;
                    return self.description_click(column, row, left) || button_event;
                }
                return button_event;
            }
        };
        // FAR's sort letter in the corner: the drive menu, or the sort menu with the right button.
        if button_event && row == area.y + 1 && (area.x + 1..=area.x + 2).contains(&column) {
            let command = if mouse.buttons.left {
                Command::ChangeDrive(side)
            } else {
                Command::SortMenu(side)
            };
            self.execute(command);
            return true;
        }
        if (list.top..list.top + list.rows).contains(&row) {
            if self.right_click_menu(side, list, mouse) {
                return true;
            }
            self.start_drag(side, area, list, mouse);
            self.list_mouse(side, list, mouse);
            return true;
        }
        if row <= area.y + 1 {
            self.start_holding(side, -1, mouse);
            return true;
        }
        if row + 2 >= area.bottom() {
            self.start_holding(side, 1, mouse);
            return true;
        }
        false
    }

    /// A click, a double click or a drag over the entries.
    fn list_mouse(&mut self, side: Side, list: &ListArea, mouse: &Mouse) {
        self.active = side;
        if self.panels[side.index()].entries.is_empty() {
            return;
        }
        let moved = self.move_to_mouse(side, list, mouse);
        if mouse.buttons.left && mouse.action == MouseAction::DoubleClick {
            self.mouse_enter(mouse.modifiers);
            return;
        }
        if mouse.buttons.right && moved {
            let panel = &mut self.panels[side.index()];
            if is_button_event(mouse.action) {
                self.mouse.selecting = !panel.current_selected();
            }
            panel.set_current_selected(self.mouse.selecting);
            panel.selection_changed();
        }
    }

    /// What was held over the panels is over (something else is in front).
    fn forget_panels_mouse(&mut self) {
        self.mouse.hold = None;
        self.mouse.thumb = None;
        self.mouse.key_press = None;
        self.end_drag();
    }

    /// The pending Explorer's menu at the cell the buttons were let go (FAR's `delayedShowEMenu`, before a
    /// drag's end as in `filelist.cpp`).
    fn show_context_menu(&mut self, mouse: &Mouse) -> bool {
        let held = self.end_hold();
        let click = Some((mouse.column, mouse.row));
        self.context_menu_key(None, self.active, click).is_some() || held
    }

    /// FAR's right button with `RightClickSelect` off (`FileList::ProcessMouse`): the right button alone
    /// (with Shift or nothing) on an entry takes the cursor there, and Explorer's menu shows at the mouse
    /// once the buttons are let go - when a plugin gives it; false where the right button selects.
    fn right_click_menu(&mut self, side: Side, list: &ListArea, mouse: &Mouse) -> bool {
        let buttons = mouse.buttons;
        let alone = buttons.right && !buttons.left && !buttons.middle;
        let plain = !(mouse.modifiers).intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);
        let menu = (self.plugins.host)
            .prefix_plugin(super::plugins::CLICK_PREFIX)
            .is_some();
        let selects = self.panels[side.index()].options.right_click_select;
        if !alone || !plain || !menu || selects || self.mouse.drag.is_some() {
            return false;
        }
        self.active = side;
        if !self.move_to_mouse(side, list, mouse) {
            return false;
        }
        self.mouse.context_menu = true;
        true
    }

    /// FAR's `MoveToMouse`; false when the right button is on a column with no entries at all
    /// (FAR's `PanelRightClickRule` 2: the cursor stays and nothing is selected).
    pub(super) fn move_to_mouse(&mut self, side: Side, list: &ListArea, mouse: &Mouse) -> bool {
        let stripe = list
            .stripe_lefts
            .iter()
            .skip(1)
            .filter(|&&left| left <= mouse.column)
            .count();
        // A drop on the titles or the status lines: the nearest row, as FAR's `CorrectPosition`.
        let row = usize::from(
            mouse
                .row
                .saturating_sub(list.top)
                .min(list.rows.saturating_sub(1)),
        );
        let panel = &mut self.panels[side.index()];
        if panel.entries.is_empty() {
            return false;
        }
        let column_start = panel.offset + stripe * panel.column_height();
        if mouse.buttons.right && column_start >= panel.entries.len() {
            return false;
        }
        panel.cursor = (column_start + row).min(panel.entries.len() - 1);
        true
    }

    /// FAR's `ProcessEnter` from a double click: the entry is opened whatever the command line holds;
    /// Shift opens it in a new window, Ctrl+Alt as the administrator.
    fn mouse_enter(&mut self, modifiers: KeyModifiers) {
        let typed = std::mem::take(&mut self.command_line);
        if modifiers.contains(KeyModifiers::CONTROL | KeyModifiers::ALT) {
            super::keys::enter_or(self, App::run_as_administrator);
        } else if modifiers.contains(KeyModifiers::SHIFT) {
            // A folder too: Explorer opens it.
            self.run_in_new_window();
        } else {
            super::keys::enter_or(self, App::open_current_file);
        }
        self.command_line = typed;
    }

    pub(super) fn start_holding(&mut self, side: Side, step: isize, mouse: &Mouse) {
        self.active = side;
        let tree = self.tree_side() == Some(side);
        let empty = match &self.tree_panel {
            Some(panel) if tree => panel.items.is_empty(),
            _ => self.panels[side.index()].entries.is_empty(),
        };
        if empty {
            return;
        }
        let select = mouse.buttons.right && !mouse.buttons.left;
        self.hold_step(side, step, select);
        let (delay, _) = platform::input::keyboard_repeat();
        self.mouse.hold = Some(Hold {
            side,
            step,
            select,
            selected: select,
            next: Instant::now() + delay,
            bar: false,
        });
    }

    /// The left button down on a scroll bar's arrow: Up or Down, then again while held there.
    pub(super) fn start_holding_bar(&mut self, side: Side, step: isize, mouse: &Mouse) {
        self.start_holding(side, step, mouse);
        if let Some(hold) = &mut self.mouse.hold {
            hold.bar = true;
            hold.select = false;
            hold.selected = false;
        }
    }

    /// One step of a hold; true when the cursor or a mark changed. FAR sorts "selected first" once,
    /// when the button is let go (`end_hold`).
    fn hold_step(&mut self, side: Side, step: isize, select: bool) -> bool {
        if self.tree_side() == Some(side) {
            return self.tree_hold_step(step);
        }
        let selecting = self.mouse.selecting;
        let panel = &mut self.panels[side.index()];
        let before = (panel.cursor, panel.current_selected());
        panel.move_cursor(step);
        if select {
            panel.set_current_selected(selecting);
        }
        before != (panel.cursor, panel.current_selected())
    }

    /// The hold is over: what it selected is sorted in. True when that changed the screen.
    pub(super) fn end_hold(&mut self) -> bool {
        let Some(hold) = self.mouse.hold.take() else {
            return false;
        };
        if self.tree_side() == Some(hold.side) {
            self.tree_hold_ended();
            return true;
        }
        if hold.selected {
            self.panels[hold.side.index()].selection_changed();
        }
        hold.selected
    }

    /// The button held over a panel's edge: true while it stays there (FAR's `while_mouse_button_pressed`).
    fn go_on_holding(&mut self, mouse: &Mouse) -> bool {
        let Some(hold) = &self.mouse.hold else {
            return false;
        };
        if hold.bar {
            let (side, step) = (hold.side, hold.step);
            if mouse.buttons.left && self.still_on_arrow(side, step, mouse) {
                return true;
            }
            self.end_hold();
            return false;
        }
        let Some(hold) = &mut self.mouse.hold else {
            return false;
        };
        let still = self.mouse.areas[hold.side.index()]
            .as_ref()
            .is_some_and(|area| {
                if hold.step < 0 {
                    mouse.row <= area.y + 1
                } else {
                    mouse.row + 2 >= area.bottom()
                }
            });
        if still {
            hold.select = mouse.buttons.right && !mouse.buttons.left;
            hold.selected |= hold.select;
            return true;
        }
        self.end_hold();
        false
    }

    /// The button held over an edge moves the cursor on at the keyboard's repeat rate.
    pub(super) fn pump_mouse_hold(&mut self) -> bool {
        let Some(hold) = &self.mouse.hold else {
            return false;
        };
        // Something came over the panels, or the panel went.
        if !self.mouse_on_panels() || self.mouse.areas[hold.side.index()].is_none() {
            return self.end_hold();
        }
        let now = Instant::now();
        if now < hold.next {
            return false;
        }
        let (side, step, select) = (hold.side, hold.step, hold.select);
        let changed = self.hold_step(side, step, select);
        let (_, interval) = platform::input::keyboard_repeat();
        if let Some(hold) = &mut self.mouse.hold {
            hold.next = now + interval;
        }
        changed
    }

    /// Another window has the mouse: a button let go there is never heard of here.
    pub(super) fn mouse_focus_lost(&mut self) {
        self.mouse.context_menu = false;
        self.end_hold();
        self.mouse.key_press = None;
        self.mouse.last_double = None;
        self.mouse.menu_hold = None;
        self.mouse.menu_down = None;
        self.mouse.button_press = None;
    }
}

/// FAR's `IsMouseButtonEvent`: a button went down or up.
pub(super) fn is_button_event(action: MouseAction) -> bool {
    matches!(action, MouseAction::Button | MouseAction::DoubleClick)
}
