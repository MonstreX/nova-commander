//! The mouse in dialogs as FAR's `Dialog::ProcessMouse`: with the left button an edit line takes
//! the focus and the cursor where clicked (a double click selects the word), a drop-down list opens,
//! a check box or a radio button is Space, a button takes the focus and is pressed when the button is
//! let go on it; a click outside the dialog is Esc with either button (the owner's change: FAR's right
//! button there is Enter, `Dialogs.MouseButton`, which would answer a question nobody read). The
//! progress dialog of a task in front too, and Find file's results: a click puts the cursor on a line,
//! a double click is Enter on the list (Go to), the wheel moves the cursor.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::console_input::{Mouse, MouseAction};
use super::form::Control;
use super::line_input::LineInput;
use super::mouse_menus::Cells;
use super::{App, Dialog, Overlay};

/// What a part of a dialog does under the mouse.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlKind {
    Button,
    /// A check box or a radio button.
    Toggle,
    /// An edit line showing its text from character `first`.
    Edit {
        first: usize,
    },
    /// A field typed through a mask (FAR's `DI_FIXEDIT`).
    Masked,
    DropDown,
    /// A text: only a dialog's own rules take a click on it (FAR's `DN_CONTROLINPUT`); elsewhere it is
    /// a place to drag the dialog by.
    Text,
    /// Find file's results from their first row shown.
    List,
    /// A list in the dialog (FAR's `DI_LISTBOX`) from line `top` shown.
    ListBox {
        top: usize,
    },
}

/// A part of a dialog: the item it is (the button's number where the dialog has only buttons).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DialogControl {
    pub cells: Cells,
    pub index: usize,
    pub kind: ControlKind,
}

/// Where the renderer put the dialog in front and its parts that take the mouse.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DialogArea {
    pub frame: Cells,
    pub controls: Vec<DialogControl>,
}

/// A dialog button the left button went down on: the row and the columns FAR presses it in, and the
/// dialog it is on: a question that came up while the button was held must not get the release.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ButtonPress {
    index: usize,
    row: u16,
    from: u16,
    to: u16,
    target: Target,
    area: DialogArea,
}

/// The dialog the mouse acts on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Overlay,
    Task,
}

impl App {
    fn dialog_target(&self) -> Option<Target> {
        match &self.overlay {
            Some(Overlay::Dialog(Dialog::List { .. })) => None,
            Some(Overlay::Dialog(_) | Overlay::FindResults(_)) => Some(Target::Overlay),
            None if self.shown_task.is_some() => Some(Target::Task),
            _ => None,
        }
    }

    /// A record while a dialog is in front; `None` when none is.
    pub(super) fn dialog_mouse(&mut self, mouse: &Mouse) -> Option<bool> {
        let Some(target) = self.dialog_target() else {
            self.mouse.button_press = None;
            return None;
        };
        self.mouse.hold = None;
        self.mouse.key_press = None;
        self.end_drag();
        if target == Target::Overlay && self.dialog_drag_mouse(mouse) {
            return Some(true);
        }
        // Del's question in front of a history list takes keys only.
        if let Some(Overlay::Dialog(Dialog::Form { form, .. })) = &self.overlay
            && form
                .open_history()
                .is_some_and(|history| history.question.is_some())
        {
            return Some(false);
        }
        let left_or_right = mouse.buttons.left || mouse.buttons.right;
        // FAR waits for the button pressed on a button to be let go.
        if self.mouse.button_press.is_some() && left_or_right {
            return Some(false);
        }
        if let Some(press) = self.mouse.button_press.take() {
            let same_dialog =
                press.target == target && self.mouse.dialog.as_ref() == Some(&press.area);
            let on_it = mouse.row == press.row && (press.from..=press.to).contains(&mouse.column);
            if same_dialog && on_it {
                self.dialog_key(target, press.index, KeyCode::Enter);
            }
            return Some(same_dialog && on_it);
        }
        if let (MouseAction::Wheel(notches), Some(Overlay::FindResults(results))) =
            (mouse.action, &mut self.overlay)
        {
            results.wheel(notches);
            return Some(true);
        }
        let button_event = matches!(mouse.action, MouseAction::Button | MouseAction::DoubleClick);
        if !button_event || !left_or_right {
            return Some(false);
        }
        let Some(area) = self.mouse.dialog.clone() else {
            return Some(false);
        };
        if !area.frame.contains(mouse.column, mouse.row) {
            let pressed = mouse.pressed.left || mouse.pressed.right;
            if !pressed || !self.prefs.click_outside_dialog {
                return Some(false);
            }
            self.mouse.dialog = None;
            self.dispatch_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
            return Some(true);
        }
        if !mouse.buttons.left {
            return Some(false);
        }
        let control = area
            .controls
            .iter()
            .rev()
            .find(|control| control.cells.contains(mouse.column, mouse.row));
        match control {
            Some(&control) if control.kind == ControlKind::Text => {
                if self.text_click(target, control.index, mouse) {
                    return Some(true);
                }
            }
            Some(&control) => return Some(self.dialog_control(target, &area, control, mouse)),
            None => {}
        }
        // FAR drags a dialog by any place that is no item (the results' box stays where it is drawn).
        let fixed = matches!(self.overlay, Some(Overlay::FindResults(_)));
        if target == Target::Overlay && mouse.pressed.left && !fixed {
            self.start_dialog_drag(mouse);
            return Some(true);
        }
        Some(false)
    }

    /// A click on a text of the dialog in front, for its own rules: whether they took it.
    fn text_click(&mut self, target: Target, index: usize, mouse: &Mouse) -> bool {
        let Some(Overlay::Dialog(dialog)) = &mut self.overlay else {
            return false;
        };
        let double = mouse.action == MouseAction::DoubleClick;
        if target != Target::Overlay || !super::attributes::text_click(dialog, index, double) {
            return false;
        }
        super::attributes::follow(dialog);
        true
    }

    fn dialog_control(
        &mut self,
        target: Target,
        area: &DialogArea,
        control: DialogControl,
        mouse: &Mouse,
    ) -> bool {
        if control.kind == ControlKind::List {
            return self.results_click(control, mouse);
        }
        self.focus_dialog_item(target, control.index);
        match control.kind {
            ControlKind::Button => {
                // FAR presses it when let go on its row, up to `X1 + HiStrlen + 4`.
                self.mouse.button_press = Some(ButtonPress {
                    index: control.index,
                    row: control.cells.y,
                    from: area.frame.x,
                    to: control.cells.x + control.cells.width + 4,
                    target,
                    area: area.clone(),
                });
            }
            ControlKind::Toggle => self.dialog_key(target, control.index, KeyCode::Char(' ')),
            ControlKind::DropDown => {
                self.mouse.dialog = None;
                let was = self.set_dialog_passing(true);
                self.dispatch_key(KeyEvent::new(KeyCode::Down, KeyModifiers::ALT));
                self.set_dialog_passing(was);
            }
            ControlKind::Edit { first } => {
                let at = first + usize::from(mouse.column - control.cells.x);
                let mut last_double = self.mouse.last_double.take();
                if let Some(input) = self.dialog_line(target, control.index) {
                    input.place_cursor(at);
                    super::mouse_bars::line_clicks(input, &mut last_double, mouse);
                }
                self.mouse.last_double = last_double;
            }
            ControlKind::Masked => {
                let at = usize::from(mouse.column - control.cells.x);
                if let Some(Overlay::Dialog(Dialog::Form { form, .. })) = &mut self.overlay
                    && let Some(masked) = form.masked_mut(control.index)
                {
                    masked.set_cursor(at);
                }
            }
            ControlKind::ListBox { top } => self.list_box_click(target, control, top, mouse),
            // `dialog_mouse` gives a text to the dialog's own rules; `results_click` has the list.
            ControlKind::Text | ControlKind::List => {}
        }
        true
    }

    /// FAR's `VMenu::ProcessMouse` in a list box: the cursor on the line; a double click closes the dialog
    /// with the list itself (FAR's `m_ExitCode = I`, whatever the default button), not with
    /// `DIF_LISTNOCLOSE`.
    fn list_box_click(
        &mut self,
        target: Target,
        control: DialogControl,
        top: usize,
        mouse: &Mouse,
    ) {
        let line = top + usize::from(mouse.row - control.cells.y);
        let Some(Overlay::Dialog(Dialog::Form { form, .. })) = &mut self.overlay else {
            return;
        };
        let Some(Control::ListBox(list)) = form.control_mut(control.index) else {
            return;
        };
        if line >= list.lines.len() || list.lines[line].is_separator() {
            return;
        }
        list.list.cursor = line;
        let closes = !list.no_close && mouse.action == MouseAction::DoubleClick;
        if closes
            && target == Target::Overlay
            && let Some(Overlay::Dialog(dialog)) = self.overlay.take()
        {
            self.mouse.dialog = None;
            self.finish_dialog(dialog, super::dialog::DialogOutcome::Button(control.index));
        }
    }

    /// The left button on Find file's list: the cursor on the line, a double click Go to (FAR's list
    /// closing the dialog, `DN_CLOSE` with `FD_LISTBOX`).
    fn results_click(&mut self, control: DialogControl, mouse: &Mouse) -> bool {
        let Some(Overlay::FindResults(results)) = &mut self.overlay else {
            return false;
        };
        let on_line = results.click_row(usize::from(mouse.row - control.cells.y));
        if on_line && mouse.action == MouseAction::DoubleClick {
            self.dialog_key(Target::Overlay, super::find::results::GO_TO, KeyCode::Enter);
        }
        true
    }

    /// Focus on item `index` of the dialog (a button's number where it has only buttons).
    fn focus_dialog_item(&mut self, target: Target, index: usize) {
        match (target, &mut self.overlay) {
            (Target::Task, _) => {
                if let Some(shown) = &mut self.shown_task {
                    shown.focus = index;
                }
            }
            (Target::Overlay, Some(Overlay::Dialog(Dialog::Confirm { focused, .. }))) => {
                *focused = index;
            }
            (Target::Overlay, Some(Overlay::Dialog(Dialog::Form { form, .. }))) => {
                form.focus_item(index);
            }
            (Target::Overlay, Some(Overlay::FindResults(results))) => {
                results.form.focus_item(index);
            }
            _ => {}
        }
    }

    /// A key for item `index`, the focus on it first.
    fn dialog_key(&mut self, target: Target, index: usize, code: KeyCode) {
        self.focus_dialog_item(target, index);
        self.mouse.dialog = None;
        // A plugin's handler heard the click itself (`DN_CONTROLINPUT`), not the key it makes.
        let was = self.set_dialog_passing(true);
        self.dispatch_key(KeyEvent::new(code, KeyModifiers::NONE));
        self.set_dialog_passing(was);
    }

    /// The edit line of item `index`.
    fn dialog_line(&mut self, target: Target, index: usize) -> Option<&mut LineInput> {
        match (target, &mut self.overlay) {
            (Target::Overlay, Some(Overlay::Dialog(Dialog::Input { input, .. }))) => Some(input),
            (Target::Overlay, Some(Overlay::Dialog(Dialog::Form { form, .. }))) => {
                form.edit_mut(index)
            }
            _ => None,
        }
    }
}
