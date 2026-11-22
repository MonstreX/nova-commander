//! FAR's dialog with items (`far/dialog.cpp`): text, separators, edit lines, check boxes, radio
//! buttons, drop-down lists and buttons at FAR's coordinates. The program builds a form as FAR's
//! `MakeDialogItems` does, the dialog keys go here, drawing reads it (`ui::form`). Coordinates are
//! inside the dialog: a dialog `width` x `height` with its double box 3 columns and 1 row in.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::clipboard::Clipboard;
use super::line_edit;
use super::line_input::LineInput;
use super::masked_input::MaskedInput;

#[path = "form_history.rs"]
mod history;
#[path = "form_links.rs"]
mod links;
#[path = "form_list.rs"]
mod list;
#[path = "form_listbox.rs"]
mod list_box;
#[path = "form_values.rs"]
mod values;

pub use history::{EditHistory, HistoryChange, HistoryLine};
use list::list_key;
pub use list::{LIST_ROWS, OpenList};
pub use list_box::ListBox;

/// A line of a drop-down list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ListLine {
    /// `&` marks the hotkey letter.
    Choice(String),
    Separator,
    /// A separator with a title in its middle (FAR's `LIF_SEPARATOR` with a text: "System").
    Section(String),
    /// FAR's `LIF_CHECKED` line: choosing it toggles its mark and keeps the choice.
    Toggle {
        label: String,
        checked: bool,
    },
    /// FAR's `LIF_GRAYED` line: shown grey, the cursor stopping on it, never chosen (Find file's drives
    /// on a plugin's panel).
    Grayed(String),
    /// A line chosen as a [`ListLine::Choice`] is, with a mark Space and Ins put and take away (Find
    /// file's code pages).
    Marked {
        label: String,
        checked: bool,
    },
}

/// One item of a form. `&` in a label marks its hotkey letter (Alt+letter); in a `Text` it moves the
/// focus to the next item, as FAR does for the text before an edit line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Control {
    Text(String),
    /// Text shown as it is, `&` too (FAR's `DIF_SHOWAMPERSAND`): names, paths, messages.
    Label(String),
    /// A line across the box, joined to its sides (FAR's `DIF_SEPARATOR`).
    Separator,
    /// FAR's `DIF_SEPARATOR` or `DIF_SEPARATOR2` (`double`) with a title in its middle, or at the item's
    /// column when `left` (FAR's `X1` given, not -1).
    TitledSeparator {
        double: bool,
        title: String,
        left: bool,
    },
    Edit(LineInput),
    Check {
        label: String,
        checked: bool,
    },
    /// FAR's check box with `DIF_3STATE`: `[?]` (`None`) leaves the thing as it is. With `three` Space
    /// goes off, on, `[?]`; without it only off and on (a box that starts at `[?]` comes back to it).
    Tristate {
        label: String,
        state: Option<bool>,
        three: bool,
    },
    /// FAR's `DI_FIXEDIT` with a mask (`app::masked_input`).
    Masked(MaskedInput),
    /// FAR's `DI_VTEXT` with `DIF_SEPARATORUSER`: a line down this many rows, joined to the separators
    /// at its ends (`┬`, `│`, `┴`).
    VRule(u16),
    /// A line across this many columns from a vertical one to the box (`├`, `─`, `╢`).
    HRule(u16),
    /// FAR's `DI_SINGLEBOX` as wide as its item: a frame this many rows high with its title in its top's
    /// middle; one row high a line across with the title in it.
    Box {
        title: String,
        rows: u16,
    },
    /// `group` starts a new group of radio buttons (FAR's `DIF_GROUP`).
    Radio {
        label: String,
        checked: bool,
        group: bool,
    },
    /// FAR's `DI_COMBOBOX` with `DIF_DROPDOWNLIST`: the chosen line, a list opened by Alt+Down.
    DropDown {
        lines: Vec<ListLine>,
        chosen: usize,
        /// The list while it is open.
        open: Option<OpenList>,
        /// Shown in the field instead of the chosen line (FAR's "hard links (2)").
        caption: Option<String>,
        /// In the open list's bottom frame (FAR's `DM_LISTSETTITLES`: "Space Ins").
        bottom_title: Option<String>,
    },
    /// FAR's `DI_LISTBOX` (`form_listbox`).
    ListBox(ListBox),
    /// The default button is pressed by Enter anywhere but on another button; FAR draws it `{ Copy }`.
    Button {
        label: String,
        default: bool,
    },
}

impl Control {
    fn takes_focus(&self) -> bool {
        !matches!(
            self,
            Self::Text(_)
                | Self::Label(_)
                | Self::Separator
                | Self::TitledSeparator { .. }
                | Self::VRule(_)
                | Self::HRule(_)
                | Self::Box { .. }
        )
    }

    fn is_edit(&self) -> bool {
        matches!(self, Self::Edit(_) | Self::Masked(_))
    }

    fn label(&self) -> Option<&str> {
        match self {
            Self::Text(label)
            | Self::Check { label, .. }
            | Self::Tristate { label, .. }
            | Self::Radio { label, .. }
            | Self::Button { label, .. } => Some(label),
            _ => None,
        }
    }

    fn hotkey(&self) -> Option<char> {
        hotkey_of(self.label()?)
    }
}

/// The hotkey `&` marks, lower case (FAR's `HiTextBase`: `&&` an `&` shown, `&&&` the hotkey `&`).
pub fn hotkey_of(label: &str) -> Option<char> {
    super::vmenu_keys::marked_hotkey(label)
}

/// Where a button stands in its row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    /// FAR's `DIF_CENTERGROUP`: with the other buttons of its row, centred in the dialog.
    Centre,
    /// Centred with the others of its row between these margins from the dialog's left and right
    /// (FAR's "unofficial margins" of the Attributes dialog).
    Between(u16, u16),
    /// At its own column.
    At,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    /// Column inside the dialog; buttons in a centred group (FAR's `DIF_CENTERGROUP`) ignore it.
    pub x: u16,
    pub y: u16,
    /// Columns of an edit line or a drop-down list.
    pub width: u16,
    pub control: Control,
    pub disabled: bool,
    pub hidden: bool,
    /// FAR's `DIF_READONLY` edit line: it takes the focus, shows and copies its text, takes no typing.
    pub read_only: bool,
    /// FAR's `DI_PSWEDIT`: an edit line showing `*` for each character, its text never copied.
    pub password: bool,
    /// FAR's `DIF_DEFAULTBUTTON` on an edit line: Enter there closes the form with it.
    pub closes: bool,
    pub place: Place,
    /// FAR's `DIF_HISTORY` on an edit line (`form_history`).
    pub history: Option<Box<EditHistory>>,
    /// The check box it follows (`form_links`).
    pub follows: Option<usize>,
    /// Drawn in the dialog's highlight colour (FAR's `DN_CTLCOLORDLGITEM` giving it `Colors[1]`).
    pub highlighted: bool,
}

impl Item {
    pub fn new(x: u16, y: u16, control: Control) -> Self {
        Self {
            x,
            y,
            width: 0,
            control,
            disabled: false,
            hidden: false,
            read_only: false,
            password: false,
            closes: false,
            place: Place::Centre,
            history: None,
            follows: None,
            highlighted: false,
        }
    }

    pub fn password(mut self) -> Self {
        self.password = true;
        self
    }

    pub fn closes(mut self) -> Self {
        self.closes = true;
        self
    }

    pub fn read_only(mut self) -> Self {
        self.read_only = true;
        self
    }

    pub fn placed(mut self, place: Place) -> Self {
        self.place = place;
        self
    }

    pub fn hidden(mut self, hidden: bool) -> Self {
        self.hidden = hidden;
        self
    }

    pub fn wide(mut self, width: u16) -> Self {
        self.width = width;
        self
    }

    /// The title in the bottom frame of a drop-down field's open list.
    pub fn with_bottom_title(mut self, title: &str) -> Self {
        if let Control::DropDown { bottom_title, .. } = &mut self.control {
            *bottom_title = Some(title.to_string());
        }
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    fn focusable(&self) -> bool {
        self.control.takes_focus() && !self.disabled && !self.hidden
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Form {
    pub title: String,
    /// FAR's `DlgW` and `DlgH`: the whole dialog with the margin around the box.
    pub width: u16,
    pub height: u16,
    pub items: Vec<Item>,
    /// The index of the focused item.
    pub focus: usize,
    /// FAR's warning dialog (`MSG_WARNING`): the warning colours.
    pub warning: bool,
}

/// What a key did to a form.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormOutcome {
    Stay,
    Cancel,
    /// The pressed button, as the index of its item.
    Button(usize),
}

impl Form {
    /// The first focusable item has the focus.
    pub fn new(title: &str, width: u16, height: u16, items: Vec<Item>) -> Self {
        let mut form = Self {
            title: title.to_string(),
            width,
            height,
            items,
            focus: 0,
            warning: false,
        };
        form.apply_links();
        form.focus = form.items.iter().position(Item::focusable).unwrap_or(0);
        form
    }

    pub fn warning(mut self) -> Self {
        self.warning = true;
        self
    }

    pub fn control(&self, index: usize) -> Option<&Control> {
        self.items.get(index).map(|item| &item.control)
    }

    /// The focus on item `index`, when it takes the focus (a click).
    pub fn focus_item(&mut self, index: usize) {
        if self.items.get(index).is_some_and(Item::focusable) {
            self.focus = index;
        }
    }

    /// The line under the cursor of the focused drop-down list, or history list, while it is open.
    pub fn set_open_line(&mut self, line: usize) {
        let focus = self.focus;
        if let Some(list) = self
            .history_mut(focus)
            .and_then(|history| history.open.as_mut())
        {
            list.cursor = line;
        } else if let Some(Control::DropDown {
            open: Some(list), ..
        }) = self.control_mut(focus)
        {
            list.cursor = line;
        }
    }

    /// The text of edit line `index`, to place its cursor.
    pub fn edit_mut(&mut self, index: usize) -> Option<&mut LineInput> {
        match self.control_mut(index) {
            Some(Control::Edit(input)) => Some(input),
            _ => None,
        }
    }

    pub fn control_mut(&mut self, index: usize) -> Option<&mut Control> {
        self.items.get_mut(index).map(|item| &mut item.control)
    }

    fn key(&mut self, key: KeyEvent, clipboard: &mut dyn Clipboard) -> FormOutcome {
        if let Some(outcome) = self.history_key(key, clipboard) {
            return outcome;
        }
        if let Some(Control::DropDown {
            lines,
            chosen,
            open: Some(list),
            ..
        }) = self.items.get_mut(self.focus).map(|item| &mut item.control)
        {
            let close = list_key(lines, list, chosen, key);
            if close {
                self.close_list();
            }
            return FormOutcome::Stay;
        }
        if let Some(Control::ListBox(list)) =
            self.items.get_mut(self.focus).map(|item| &mut item.control)
            && list_box::key(list, key)
        {
            return FormOutcome::Stay;
        }
        let on_button = matches!(self.focused(), Some(Control::Button { .. }));
        let alt = key.modifiers.contains(KeyModifiers::ALT);
        match key.code {
            KeyCode::Esc => return FormOutcome::Cancel,
            KeyCode::Tab => self.move_focus(true),
            KeyCode::BackTab => self.move_focus(false),
            KeyCode::Down if alt || key.modifiers.contains(KeyModifiers::CONTROL) => {
                if !self.open_history_list() {
                    self.open_list();
                }
            }
            KeyCode::Down => self.move_focus(true),
            KeyCode::Up => self.move_focus(false),
            KeyCode::Right if on_button => self.move_focus(true),
            KeyCode::Left if on_button => self.move_focus(false),
            KeyCode::Enter => {
                return self.press(if on_button {
                    self.focus
                } else {
                    self.default_button()
                });
            }
            KeyCode::Char(ch) if alt => return self.hotkey(ch).unwrap_or(FormOutcome::Stay),
            // FAR's `ProcessHighlighting`: away from an edit line a letter is its item's hotkey.
            // Ctrl+letter is no hotkey (FAR's `IsKeyHighlighted`: the plain key or Alt).
            KeyCode::Char(ch)
                if !key.modifiers.contains(KeyModifiers::CONTROL)
                    && !self.focused().is_some_and(Control::is_edit) =>
            {
                if let Some(outcome) = self.hotkey(ch) {
                    return outcome;
                }
                return self.focused_key(key, clipboard);
            }
            _ => return self.focused_key(key, clipboard),
        }
        FormOutcome::Stay
    }

    /// Each empty `DIF_USELASTHISTORY` line takes the last line of its history (FAR's dialog init).
    pub fn fill_last_history(&mut self) {
        self.items.iter_mut().for_each(history::fill_last);
    }

    fn focused(&self) -> Option<&Control> {
        self.control(self.focus)
    }

    /// The key for the focused item: typing in an edit line, Space on a check box, radio button or
    /// button.
    fn focused_key(&mut self, key: KeyEvent, clipboard: &mut dyn Clipboard) -> FormOutcome {
        let focus = self.focus;
        let space = key.code == KeyCode::Char(' ');
        let read_only = self.items.get(focus).is_some_and(|item| item.read_only);
        let password = self.items.get(focus).is_some_and(|item| item.password);
        if password && line_edit::takes_out(key) {
            return FormOutcome::Stay;
        }
        match self.items.get_mut(focus).map(|item| &mut item.control) {
            Some(Control::Edit(input)) if !read_only || line_edit::only_looks(key) => {
                // A clipboard another program holds for longer than FAR waits only loses this key.
                let _ = line_edit::handle_key(input, key, clipboard);
            }
            Some(Control::Masked(input)) if !read_only => {
                input.handle_key(key);
            }
            Some(Control::Check { checked, .. }) if space => *checked = !*checked,
            Some(control @ Control::Tristate { .. }) if space => cycle(control),
            Some(Control::Radio { .. }) if space => self.select_radio(focus),
            Some(Control::Button { .. }) if space => return self.press(focus),
            _ => {}
        }
        FormOutcome::Stay
    }

    /// FAR's hotkeys: the letter acts on that item; a text's hotkey moves to the item after it. `None`
    /// when no item has it.
    fn hotkey(&mut self, ch: char) -> Option<FormOutcome> {
        let ch = super::vmenu_keys::lower(ch);
        let index = self
            .items
            .iter()
            .position(|item| !item.hidden && !item.disabled && item.control.hotkey() == Some(ch))?;
        let index = if matches!(self.items[index].control, Control::Text(_)) {
            match (index + 1..self.items.len()).find(|&next| self.items[next].focusable()) {
                Some(next) => next,
                None => return Some(FormOutcome::Stay),
            }
        } else {
            index
        };
        self.focus = index;
        match &mut self.items[index].control {
            Control::Check { checked, .. } => *checked = !*checked,
            control @ Control::Tristate { .. } => cycle(control),
            Control::Radio { .. } => self.select_radio(index),
            Control::Button { .. } => return Some(self.press(index)),
            _ => {}
        }
        Some(FormOutcome::Stay)
    }

    /// Checks the radio button at `index` and unchecks the others of its group.
    pub fn select_radio(&mut self, index: usize) {
        let start = (0..=index)
            .rev()
            .find(|&at| matches!(self.items[at].control, Control::Radio { group: true, .. }))
            .unwrap_or(index);
        for (at, item) in self.items.iter_mut().enumerate().skip(start) {
            match &mut item.control {
                Control::Radio { group: true, .. } if at > start => break,
                Control::Radio { checked, .. } => *checked = at == index,
                _ => break,
            }
        }
    }

    fn open_list(&mut self) {
        if let Some(Control::DropDown { chosen, open, .. }) =
            self.items.get_mut(self.focus).map(|item| &mut item.control)
        {
            *open = Some(OpenList::at(*chosen));
        }
    }

    fn close_list(&mut self) {
        if let Some(Control::DropDown { open, .. }) =
            self.items.get_mut(self.focus).map(|item| &mut item.control)
        {
            *open = None;
        }
    }

    fn default_button(&self) -> usize {
        self.items
            .iter()
            .position(|item| matches!(item.control, Control::Button { default: true, .. }))
            .unwrap_or(self.focus)
    }

    fn press(&self, index: usize) -> FormOutcome {
        match self.items.get(index) {
            Some(item)
                if item.focusable()
                    && (item.closes || matches!(item.control, Control::Button { .. })) =>
            {
                FormOutcome::Button(index)
            }
            _ => FormOutcome::Stay,
        }
    }

    /// The next (or previous) focusable item, round the end as FAR does.
    fn move_focus(&mut self, forward: bool) {
        let count = self.items.len();
        for step in 1..count {
            let index = if forward {
                (self.focus + step) % count
            } else {
                (self.focus + count - step) % count
            };
            if self.items[index].focusable() {
                self.focus = index;
                return;
            }
        }
    }
}

/// FAR's three-state box: off, on, `[?]` with `three`; off and on without.
fn cycle(control: &mut Control) {
    if let Control::Tristate { state, three, .. } = control {
        *state = match (*state, *three) {
            (Some(false), _) => Some(true),
            (Some(true), true) => None,
            (Some(true), false) | (None, _) => Some(false),
        };
    }
}

#[cfg(test)]
#[path = "form_tests.rs"]
mod tests;
