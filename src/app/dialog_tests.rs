//! The dialogs' keys: Esc, the buttons, their hotkeys, the input line, the lists.

use super::*;
use ratatui::crossterm::event::KeyModifiers;

fn press(dialog: &mut Dialog, code: KeyCode) -> DialogOutcome {
    handle_key(
        dialog,
        KeyEvent::new(code, KeyModifiers::NONE),
        &mut crate::app::clipboard::Memory::default(),
    )
}

fn confirm() -> Dialog {
    Dialog::Confirm {
        title: "Delete".to_string(),
        lines: vec![],
        buttons: vec!["Delete", "Cancel"],
        focused: 0,
        danger: true,
        action: ConfirmAction::Quit,
    }
}

#[test]
fn escape_always_cancels() {
    assert_eq!(press(&mut confirm(), KeyCode::Esc), DialogOutcome::Cancel);
}

#[test]
fn confirm_reports_focused_button() {
    let mut dialog = confirm();
    assert_eq!(press(&mut dialog, KeyCode::Right), DialogOutcome::Stay);
    assert_eq!(press(&mut dialog, KeyCode::Enter), DialogOutcome::Button(1));
}

#[test]
fn a_button_s_hotkey_presses_it() {
    let mut dialog = Dialog::Confirm {
        title: "Editor".to_string(),
        lines: vec![],
        buttons: vec!["&Yes", "&No", "&Cancel"],
        focused: 0,
        danger: false,
        action: ConfirmAction::CloseEditor,
    };
    assert_eq!(press(&mut dialog, KeyCode::Char('x')), DialogOutcome::Stay);
    assert_eq!(
        press(&mut dialog, KeyCode::Char('N')),
        DialogOutcome::Button(1)
    );
    assert_eq!(
        press(&mut dialog, KeyCode::Char('c')),
        DialogOutcome::Button(2)
    );
}

#[test]
fn input_submits_edited_text() {
    let mut dialog = Dialog::Input {
        title: "Make directory".to_string(),
        prompt: String::new(),
        input: LineInput::default(),
        action: InputAction::ApplyCommand,
    };
    press(&mut dialog, KeyCode::Char('a'));
    press(&mut dialog, KeyCode::Char('b'));
    press(&mut dialog, KeyCode::Backspace);
    assert_eq!(
        press(&mut dialog, KeyCode::Enter),
        DialogOutcome::Submit("a".to_string())
    );
}

#[test]
fn list_wraps_and_chooses() {
    let mut dialog = Dialog::List {
        title: "Themes".to_string(),
        items: vec!["far".to_string(), "dark".to_string()],
        cursor: 0,
        top: Default::default(),
        action: ListAction::Theme,
    };
    press(&mut dialog, KeyCode::Up);
    assert_eq!(press(&mut dialog, KeyCode::Enter), DialogOutcome::Choose(1));
}
