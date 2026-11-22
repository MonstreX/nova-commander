//! Editing keys of a text field, as FAR's `Edit::ProcessKey`: movement, Shift selection, word and
//! line deletion, and the clipboard (Ctrl+Ins / Shift+Ins / Shift+Del and Ctrl+C / Ctrl+V / Ctrl+X).

use std::io;

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::clipboard::Clipboard;
use super::editing::is_text_input;
use super::line_input::LineInput;

const NONE: KeyModifiers = KeyModifiers::NONE;
const SHIFT: KeyModifiers = KeyModifiers::SHIFT;
const CTRL: KeyModifiers = KeyModifiers::CONTROL;
const CTRL_SHIFT: KeyModifiers = KeyModifiers::CONTROL.union(KeyModifiers::SHIFT);
const ALT_SHIFT: KeyModifiers = KeyModifiers::ALT.union(KeyModifiers::SHIFT);

/// Handles an editing key; `Ok(false)` for keys that do not edit.
pub fn handle_key(
    input: &mut LineInput,
    key: KeyEvent,
    clipboard: &mut dyn Clipboard,
) -> io::Result<bool> {
    if !key.modifiers.contains(SHIFT) {
        input.end_marking();
    }
    if let Some(handled) = clipboard_key(input, key, clipboard)? {
        return Ok(handled);
    }
    if let KeyCode::Char(ch) = key.code
        && is_text_input(key.modifiers)
    {
        input.insert(ch);
        return Ok(true);
    }
    Ok(edit_key(input, key))
}

/// Ctrl+C, Ctrl+Ins, Ctrl+X, Shift+Del: what would put a line's text on the clipboard.
pub(super) fn takes_out(key: KeyEvent) -> bool {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let shift = key.modifiers.contains(KeyModifiers::SHIFT);
    (ctrl
        && matches!(
            key.code,
            KeyCode::Insert | KeyCode::Char('c' | 'x' | 'C' | 'X')
        ))
        || (shift && key.code == KeyCode::Delete)
}

/// Keys that move in or copy from a read-only edit line and change nothing.
pub(super) fn only_looks(key: KeyEvent) -> bool {
    let copy = key.modifiers.contains(KeyModifiers::CONTROL)
        && matches!(key.code, KeyCode::Insert | KeyCode::Char('c'));
    copy || matches!(
        key.code,
        KeyCode::Left | KeyCode::Right | KeyCode::Home | KeyCode::End
    )
}

/// Ctrl+Ins / Ctrl+C copy the block, or the whole line without one; Shift+Ins / Ctrl+V paste;
/// Shift+Del / Ctrl+X cut the block. `None` for other keys.
fn clipboard_key(
    input: &mut LineInput,
    key: KeyEvent,
    clipboard: &mut dyn Clipboard,
) -> io::Result<Option<bool>> {
    match (key.code, key.modifiers) {
        (KeyCode::Insert, CTRL) | (KeyCode::Char('c'), CTRL) => {
            copy(input, clipboard)?;
        }
        (KeyCode::Insert, SHIFT) | (KeyCode::Char('v'), CTRL) => {
            if let Some(text) = clipboard.text()? {
                input.paste(&text);
            }
        }
        (KeyCode::Delete, SHIFT) | (KeyCode::Char('x'), CTRL) => {
            if let Some(text) = input.selected_text() {
                clipboard.set_text(&text)?;
                input.delete_selection();
            }
        }
        _ => return Ok(None),
    }
    Ok(Some(true))
}

/// Ctrl+Ins: the block, or the whole line when nothing is selected (an empty line copies nothing).
pub fn copy(input: &LineInput, clipboard: &mut dyn Clipboard) -> io::Result<()> {
    let text = input
        .selected_text()
        .unwrap_or_else(|| input.text().to_string());
    if text.is_empty() {
        return Ok(());
    }
    clipboard.set_text(&text)
}

fn edit_key(input: &mut LineInput, key: KeyEvent) -> bool {
    match (key.code, key.modifiers) {
        (KeyCode::Backspace, NONE | SHIFT) => input.backspace(),
        (KeyCode::Backspace, CTRL) => input.delete_word_left(),
        (KeyCode::Backspace, CTRL_SHIFT) => input.delete_to_start(),
        (KeyCode::Delete, NONE) => input.delete(),
        (KeyCode::Delete, CTRL) => input.delete_word_right(),
        (KeyCode::Left, NONE) | (KeyCode::Char('s'), CTRL) => input.left(),
        (KeyCode::Right, NONE) | (KeyCode::Char('d'), CTRL) => input.right(),
        (KeyCode::Left, CTRL) => input.word_left(),
        (KeyCode::Right, CTRL) => input.word_right(),
        (KeyCode::Home, NONE | CTRL) => input.home(),
        (KeyCode::End, NONE | CTRL | CTRL_SHIFT) => input.end(),
        (KeyCode::Left, SHIFT | ALT_SHIFT) => input.select_with(LineInput::left),
        (KeyCode::Right, SHIFT | ALT_SHIFT) => input.select_with(LineInput::right),
        (KeyCode::Home, SHIFT | ALT_SHIFT) => input.select_with(LineInput::home),
        (KeyCode::End, SHIFT | ALT_SHIFT) => input.select_with(LineInput::end),
        (KeyCode::Left, CTRL_SHIFT) => input.select_word_left(),
        (KeyCode::Right, CTRL_SHIFT) => input.select_word_right(),
        (KeyCode::Char('a'), CTRL) => input.select_all(),
        (KeyCode::Char('y'), CTRL) => input.clear(),
        (KeyCode::Char('k'), CTRL) => input.delete_to_end(),
        _ => return false,
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::clipboard::Memory;

    fn press(
        input: &mut LineInput,
        clipboard: &mut Memory,
        code: KeyCode,
        modifiers: KeyModifiers,
    ) {
        assert!(handle_key(input, KeyEvent::new(code, modifiers), clipboard).unwrap());
    }

    #[test]
    fn copy_cut_and_paste_as_in_far() {
        let mut clipboard = Memory::default();
        let mut input = LineInput::with_text("copy a.txt");
        press(&mut input, &mut clipboard, KeyCode::Insert, CTRL);
        assert_eq!(clipboard.contents().text.as_deref(), Some("copy a.txt"));

        press(&mut input, &mut clipboard, KeyCode::Left, CTRL_SHIFT);
        press(&mut input, &mut clipboard, KeyCode::Left, CTRL_SHIFT);
        press(&mut input, &mut clipboard, KeyCode::Char('x'), CTRL);
        assert_eq!(clipboard.contents().text.as_deref(), Some("a.txt"));
        assert_eq!(input.text(), "copy ");

        clipboard.put_text("b\r\nc\r\n");
        press(&mut input, &mut clipboard, KeyCode::Insert, SHIFT);
        assert_eq!(input.text(), "copy b c");
        press(&mut input, &mut clipboard, KeyCode::Char('a'), CTRL);
        press(&mut input, &mut clipboard, KeyCode::Char('v'), CTRL);
        assert_eq!(input.text(), "b c", "pasting replaces the block");
    }

    #[test]
    fn shift_selection_ends_with_a_key_without_shift() {
        let mut clipboard = Memory::default();
        let mut input = LineInput::with_text("abc");
        press(&mut input, &mut clipboard, KeyCode::Left, SHIFT);
        press(&mut input, &mut clipboard, KeyCode::Home, ALT_SHIFT);
        assert_eq!(input.selected_text().as_deref(), Some("abc"));
        press(&mut input, &mut clipboard, KeyCode::Char('c'), CTRL);
        press(&mut input, &mut clipboard, KeyCode::Right, SHIFT);
        assert_eq!(
            input.selected_text().as_deref(),
            Some("a"),
            "a new block after Ctrl+C"
        );
        press(&mut input, &mut clipboard, KeyCode::Char('z'), NONE);
        assert_eq!(input.text(), "zbc");
        press(&mut input, &mut clipboard, KeyCode::Char('y'), CTRL);
        assert!(input.is_empty());
    }
}
