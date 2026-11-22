//! What a form's items hold: the text of an edit line, a check box, a three-state box, a masked
//! field, the chosen line of a drop-down list; read when the form closes, set as it changes.

use super::{Control, Form, LineInput, ListLine, MaskedInput};

impl Form {
    pub fn edit_text(&self, index: usize) -> &str {
        match self.control(index) {
            Some(Control::Edit(input)) => input.text(),
            _ => "",
        }
    }

    pub fn is_checked(&self, index: usize) -> bool {
        matches!(
            self.control(index),
            Some(Control::Check { checked: true, .. } | Control::Radio { checked: true, .. })
        )
    }

    /// A three-state box: `None` is `[?]`.
    pub fn tristate(&self, index: usize) -> Option<bool> {
        match self.control(index) {
            Some(Control::Tristate { state, .. }) => *state,
            _ => None,
        }
    }

    pub fn set_tristate(&mut self, index: usize, to: Option<bool>) {
        if let Some(Control::Tristate { state, .. }) = self.control_mut(index) {
            *state = to;
        }
    }

    /// Whether a three-state box goes round through `[?]` (FAR's `DM_SET3STATE`).
    pub fn set_three_state(&mut self, index: usize, to: bool) {
        if let Some(Control::Tristate { three, .. }) = self.control_mut(index) {
            *three = to;
        }
    }

    /// Replaces an edit line's text.
    pub fn set_edit_text(&mut self, index: usize, text: &str) {
        if let Some(Control::Edit(input)) = self.control_mut(index) {
            *input = LineInput::with_text(text);
        }
    }

    /// The text of a masked field.
    pub fn masked_text(&self, index: usize) -> String {
        match self.control(index) {
            Some(Control::Masked(input)) => input.text(),
            _ => String::new(),
        }
    }

    pub fn set_masked_text(&mut self, index: usize, text: &str) {
        if let Some(Control::Masked(input)) = self.control_mut(index) {
            input.set_text(text);
        }
    }

    pub fn masked_mut(&mut self, index: usize) -> Option<&mut MaskedInput> {
        match self.control_mut(index) {
            Some(Control::Masked(input)) => Some(input),
            _ => None,
        }
    }

    /// The chosen line of a drop-down list.
    pub fn chosen(&self, index: usize) -> usize {
        match self.control(index) {
            Some(Control::DropDown { chosen, .. }) => *chosen,
            _ => 0,
        }
    }

    /// Whether line `line` of a drop-down list carries its mark.
    pub fn line_checked(&self, index: usize, line: usize) -> bool {
        matches!(self.control(index), Some(Control::DropDown { lines, .. })
            if matches!(lines.get(line), Some(ListLine::Toggle { checked: true, .. }
                | ListLine::Marked { checked: true, .. })))
    }
}
