//! The mouse in the editor (S2d.6f; FAR's `Editor::ProcessMouse`, `Edit::ProcessMouse` and the wheel's
//! `KEY_MSWHEEL_*`): the wheel scrolls as Ctrl+Up / Ctrl+Down (with Alt a line a notch), the tilt
//! wheel moves the cursor left or right; a button on the text puts the cursor where it is (a block
//! that is not persistent let go), a double click marks the word there and a click on the same place
//! soon after it the line; on the scroll bar its arrows scroll a line and the rest goes to the line as
//! far down the file.

use std::time::Instant;

use ratatui::crossterm::event::KeyModifiers;

use super::console_input::{Mouse, MouseAction};
use super::{App, Screen};
use crate::editor::view::edit_view::Step;

impl App {
    /// A mouse record over the editor; true when it was the editor's.
    pub(super) fn editor_mouse(&mut self, mouse: &Mouse) -> bool {
        if !self.mouse.enabled
            || self.overlay.is_some()
            || self.shown_task.is_some()
            || self.backdrop
        {
            return false;
        }
        let Screen::Editor(editor) = &mut self.screen else {
            return false;
        };
        let text = &mut editor.text;
        let (steps, each) = match (mouse.action, mouse.modifiers) {
            (MouseAction::Wheel(notches), KeyModifiers::NONE | KeyModifiers::ALT) => {
                let each = per_notch(mouse, crate::platform::input::wheel_scroll_lines());
                let step = if notches > 0 {
                    Step::ScrollUp
                } else {
                    Step::ScrollDown
                };
                ((step, notches.unsigned_abs() as usize), each)
            }
            (MouseAction::HorizontalWheel(notches), KeyModifiers::NONE | KeyModifiers::ALT) => {
                let each = per_notch(mouse, crate::platform::input::wheel_scroll_chars());
                let step = if notches > 0 { Step::Right } else { Step::Left };
                ((step, notches.unsigned_abs() as usize), each)
            }
            _ => return self.editor_buttons(mouse),
        };
        let (step, notches) = steps;
        for _ in 0..notches * each {
            text.press(step.clone());
        }
        true
    }

    /// The left or right button over the editor's text or its scroll bar.
    fn editor_buttons(&mut self, mouse: &Mouse) -> bool {
        if !(mouse.buttons.left || mouse.buttons.right) {
            return false;
        }
        let last_double = &mut self.mouse.last_double;
        let Screen::Editor(editor) = &mut self.screen else {
            return false;
        };
        let text = &mut editor.text;
        let ((x, y), (width, height)) = (text.origin.get(), text.room.get());
        if !(y..y + height).contains(&mouse.row) {
            return false;
        }
        let row = usize::from(mouse.row - y);
        if text.scrollbar && mouse.column == x + width {
            if let Some(step) = bar_step(text, row, usize::from(height), mouse) {
                text.press(step);
            }
            return true;
        }
        if !(x..x + width).contains(&mouse.column) {
            return false;
        }
        let here = (mouse.column, mouse.row);
        let again = last_double.is_some_and(|(at, place)| {
            place == here && at.elapsed() <= crate::platform::input::double_click_time()
        });
        let line = text.view().and_then(|view| view.cursor_line().ok());
        match (mouse.buttons.left, mouse.action, line) {
            (true, MouseAction::DoubleClick, Some((chars, at))) => {
                *last_double = Some((Instant::now(), here));
                let word = super::line_input::word_around(&chars, at);
                text.press(
                    word.map_or(Step::Unselect, |word| Step::MarkChars(word.start, word.end)),
                );
            }
            (true, MouseAction::Button, Some((chars, _))) if again => {
                *last_double = None;
                text.press(Step::MarkChars(0, chars.len()));
            }
            _ => {
                *last_double = None;
                text.press(Step::Click(row, usize::from(mouse.column - x)));
            }
        }
        true
    }
}

/// The lines or characters a wheel's notch moves: the system's, one with Alt (FAR's).
fn per_notch(mouse: &Mouse, system: u32) -> usize {
    match mouse.modifiers {
        KeyModifiers::ALT => 1,
        _ => usize::try_from(system).unwrap_or(1).max(1),
    }
}

/// FAR's scroll bar in the editor: its top arrow Ctrl+Up, its bottom one Ctrl+Down, the rest the line
/// as far down the file as the row is down the bar (lines not counted: nothing).
fn bar_step(
    text: &super::editor_text::EditorText,
    row: usize,
    height: usize,
    mouse: &Mouse,
) -> Option<Step> {
    let pressed = mouse.pressed.left || mouse.pressed.right;
    match row {
        0 if pressed => return Some(Step::ScrollUp),
        _ if row + 1 == height && pressed => return Some(Step::ScrollDown),
        0 => return None,
        _ if row + 1 == height => return None,
        _ => {}
    }
    if !mouse.buttons.left {
        return None;
    }
    let (_, total) = text.lines();
    if total == 0 {
        return None;
    }
    let line = (total - 1) * row as u64 / (height.max(2) - 1) as u64;
    let col = text.view().map_or(0, |view| view.cursor().1);
    Some(Step::GoTo(line, col))
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::platform::input::MouseButtons;

    /// The editor on `text`, read, its rows from the screen's second row.
    fn opened(text: &str) -> (tempfile::TempDir, App) {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("m.txt");
        std::fs::write(&path, text).unwrap();
        let mut app = crate::app::fileops::tests::app_in(temp.path());
        app.mouse.enabled = true;
        app.edit_file(&path, false);
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            app.tick();
            let Screen::Editor(editor) = &mut app.screen else {
                panic!("the editor");
            };
            editor.text.room.set((40, 5));
            editor.text.origin.set((0, 1));
            if editor.text.place().is_some() && !app.is_waiting() {
                return (temp, app);
            }
            assert!(Instant::now() < deadline, "never read");
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    fn view(app: &App) -> &crate::editor::view::edit_view::EditView {
        match &app.screen {
            Screen::Editor(editor) => editor.text.view().unwrap(),
            _ => panic!("the editor"),
        }
    }

    fn record(action: MouseAction, (column, row): (u16, u16), left: bool) -> Mouse {
        let buttons = MouseButtons {
            left,
            right: false,
            middle: false,
        };
        let pressed = match action {
            MouseAction::Button => buttons,
            _ => MouseButtons {
                left: false,
                ..buttons
            },
        };
        Mouse {
            column,
            row,
            buttons,
            pressed,
            action,
            modifiers: KeyModifiers::NONE,
        }
    }

    #[test]
    fn a_click_puts_the_cursor_there_and_a_double_click_marks_the_word_then_the_line() {
        let (_temp, mut app) = opened("one two\r\nthree\r\nfour\r\n");
        assert!(app.handle_mouse(record(MouseAction::Button, (2, 2), true)));
        assert_eq!(
            view(&app).cursor(),
            (9, 2),
            "the second row's line, its column"
        );
        assert!(app.handle_mouse(record(MouseAction::Button, (5, 1), true)));
        assert!(app.handle_mouse(record(MouseAction::DoubleClick, (5, 1), true)));
        assert_eq!(view(&app).block().unwrap(), Some((4, 7)), "the word");
        assert!(app.handle_mouse(record(MouseAction::Button, (5, 1), true)));
        assert_eq!(view(&app).block().unwrap(), Some((0, 7)), "the line");
        assert!(app.handle_mouse(record(MouseAction::Button, (0, 3), true)));
        assert_eq!(view(&app).block().unwrap(), None, "not persistent: let go");
        assert_eq!(view(&app).cursor(), (16, 0));
        assert!(
            !app.handle_mouse(record(MouseAction::Button, (0, 9), true)),
            "below the text"
        );
    }

    #[test]
    fn the_wheel_scrolls_as_ctrl_up_and_ctrl_down() {
        let (_temp, mut app) = opened("a\r\nb\r\nc\r\nd\r\n");
        let mut wheel = record(MouseAction::Wheel(-2), (0, 1), false);
        wheel.modifiers = KeyModifiers::ALT;
        assert!(app.handle_mouse(wheel));
        assert_eq!(view(&app).cursor().0, 6, "two lines down");
        wheel.action = MouseAction::Wheel(1);
        assert!(app.handle_mouse(wheel));
        assert_eq!(view(&app).cursor().0, 3);
    }
}
