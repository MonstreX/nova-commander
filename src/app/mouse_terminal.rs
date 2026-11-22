//! The mouse while a program owns the screen (a command running, the guest shell, a command brought
//! back from the background): told to the program in the terminal's protocol when it asked for the
//! mouse (FAR gives the program its console and with it the mouse), and to nobody otherwise. A button
//! the program was told of is always told let go, whatever came in between.

use ratatui::crossterm::event::KeyModifiers;

use super::console_input::{Mouse, MouseAction};
use super::mouse_menus::Cells;
use super::{App, MainView, Screen};
use crate::platform::input::MouseButtons;
use crate::terminal::ShellView;
use crate::terminal::mouse::{self, MouseButton, MouseEvent, MouseReport};

/// Whether a button is among those held.
type IsHeld = fn(MouseButtons) -> bool;

const BUTTONS: [(MouseButton, IsHeld); 3] = [
    (MouseButton::Left, |buttons| buttons.left),
    (MouseButton::Middle, |buttons| buttons.middle),
    (MouseButton::Right, |buttons| buttons.right),
];

/// What a program was last told: the buttons it believes held and where.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Told {
    buttons: MouseButtons,
    column: u16,
    row: u16,
}

impl Told {
    fn set(&mut self, button: MouseButton, held: bool) {
        match button {
            MouseButton::Left => self.buttons.left = held,
            MouseButton::Middle => self.buttons.middle = held,
            MouseButton::Right => self.buttons.right = held,
        }
    }
}

/// What a record is in the terminal's terms; `told`: the buttons the program believes held. A button
/// let go is told first, whatever the record is (it may have been let go in another window).
pub(super) fn events_of(mouse: &Mouse, told: MouseButtons) -> Vec<MouseEvent> {
    let mut events: Vec<MouseEvent> = BUTTONS
        .iter()
        .filter(|(_, is)| is(told) && !is(mouse.buttons))
        .map(|(button, _)| MouseEvent::Release(*button))
        .collect();
    let times =
        |notches: i32, event| vec![event; usize::try_from(notches.unsigned_abs()).unwrap_or(0)];
    match mouse.action {
        MouseAction::Wheel(notches) if notches > 0 => {
            events.extend(times(notches, MouseEvent::WheelUp));
        }
        MouseAction::Wheel(notches) => events.extend(times(notches, MouseEvent::WheelDown)),
        MouseAction::HorizontalWheel(notches) if notches > 0 => {
            events.extend(times(notches, MouseEvent::WheelRight));
        }
        MouseAction::HorizontalWheel(notches) => {
            events.extend(times(notches, MouseEvent::WheelLeft));
        }
        MouseAction::Moved => {
            let held = BUTTONS.iter().find(|(_, is)| is(mouse.buttons) && is(told));
            events.push(held.map_or(MouseEvent::Move, |(button, _)| MouseEvent::Drag(*button)));
        }
        MouseAction::Button | MouseAction::DoubleClick => events.extend(
            BUTTONS
                .iter()
                .filter(|(_, is)| is(mouse.pressed))
                .map(|(button, _)| MouseEvent::Press(*button)),
        ),
    }
    events
}

/// The bytes for the program and what it then believes. Outside its screen only a release is told, at
/// the nearest cell, so that a button does not stay down there.
fn reports(view: &ShellView, area: Cells, mouse: &Mouse, told: Told) -> (Vec<Vec<u8>>, Told) {
    let mut now = told;
    let mut bytes = Vec::new();
    let inside = area.contains(mouse.column, mouse.row);
    let last_column = area.x + area.width.saturating_sub(1);
    let last_row = area.y + area.height.saturating_sub(1);
    let column = mouse.column.clamp(area.x, last_column) - area.x;
    let shown = usize::from(mouse.row.clamp(area.y, last_row) - area.y);
    let here = view.rows.get(shown).copied();
    for event in events_of(mouse, told.buttons) {
        let row = match event {
            MouseEvent::Release(button) => {
                now.set(button, false);
                here.or(view.rows.last().copied())
            }
            _ if inside => here,
            _ => None,
        };
        let Some(row) = row else {
            continue;
        };
        let report = MouseReport {
            column,
            row,
            event,
            shift: mouse.modifiers.contains(KeyModifiers::SHIFT),
            alt: mouse.modifiers.contains(KeyModifiers::ALT),
            ctrl: mouse.modifiers.contains(KeyModifiers::CONTROL),
        };
        let Some(sequence) = mouse::encode(view.screen, report) else {
            continue;
        };
        bytes.push(sequence);
        now.column = column;
        now.row = row;
        if let MouseEvent::Press(button) = event {
            now.set(button, true);
        }
    }
    (bytes, now)
}

/// The releases of what `told` holds, at the last place the program heard of.
fn releases(view: &ShellView, told: Told) -> Vec<Vec<u8>> {
    BUTTONS
        .iter()
        .filter(|(_, is)| is(told.buttons))
        .filter_map(|(button, _)| {
            mouse::encode(
                view.screen,
                MouseReport {
                    column: told.column,
                    row: told.row,
                    event: MouseEvent::Release(*button),
                    shift: false,
                    alt: false,
                    ctrl: false,
                },
            )
        })
        .collect()
}

impl App {
    /// The view of the program in front, when one is.
    fn program_in_front(&self) -> Option<MainView> {
        let in_front = self.main_view != MainView::Panels
            && matches!(self.screen, Screen::Panels)
            && self.overlay.is_none()
            && self.shown_task.is_none()
            && self.waiting.is_none();
        in_front.then_some(self.main_view)
    }

    /// A record while a program owns the screen; `None` when none does.
    pub(super) fn terminal_mouse(&mut self, mouse: &Mouse) -> Option<bool> {
        let front = self.program_in_front();
        self.let_go_where_not_in_front(front);
        let view = front?;
        let told = self.mouse.told.map_or_else(Told::default, |(_, told)| told);
        let (Some(area), Some(shell)) = (self.mouse.terminal, self.program_view(view)) else {
            return Some(false);
        };
        let (bytes, now) = reports(&shell, area, mouse, told);
        self.mouse.told = now.buttons.any().then_some((view, now));
        for sequence in bytes {
            self.send_to_program(view, &sequence);
        }
        Some(false)
    }

    /// A program that left the front with a button down hears it let go.
    fn let_go_where_not_in_front(&mut self, front: Option<MainView>) {
        let Some((view, told)) = self.mouse.told.filter(|(view, _)| Some(*view) != front) else {
            return;
        };
        self.mouse.told = None;
        let bytes = self
            .program_view(view)
            .map_or_else(Vec::new, |shell| releases(&shell, told));
        for sequence in bytes {
            self.send_to_program(view, &sequence);
        }
    }

    fn program_view(&self, view: MainView) -> Option<ShellView<'_>> {
        match view {
            MainView::Panels => None,
            MainView::CommandOutput => self.user_screen.as_ref().map(|screen| screen.view()),
            MainView::Guest => self.guest.as_ref().map(|guest| guest.view()),
            MainView::Detached(index) => self.detached_view(index),
        }
    }

    /// Bytes for the program; a console that is gone is the keys' to report, the mouse says nothing.
    fn send_to_program(&mut self, view: MainView, bytes: &[u8]) {
        let sent = match view {
            MainView::Panels => Ok(()),
            MainView::CommandOutput => self
                .user_screen
                .as_mut()
                .map_or(Ok(()), |screen| screen.send(bytes)),
            MainView::Guest => self
                .guest
                .as_mut()
                .map_or(Ok(()), |guest| guest.send(bytes)),
            MainView::Detached(index) => self.send_to_detached(index, bytes),
        };
        sent.ok();
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU16;

    use super::*;

    const LEFT: MouseButtons = MouseButtons {
        left: true,
        right: false,
        middle: false,
    };
    const RIGHT: MouseButtons = MouseButtons {
        left: false,
        right: true,
        middle: false,
    };
    const NONE: MouseButtons = MouseButtons {
        left: false,
        right: false,
        middle: false,
    };
    /// The program's screen on the console: one row below the top, as the guest's.
    const AREA: Cells = Cells {
        x: 0,
        y: 1,
        width: 80,
        height: 10,
    };

    fn mouse(buttons: MouseButtons, pressed: MouseButtons, action: MouseAction) -> Mouse {
        Mouse {
            column: 3,
            row: 4,
            buttons,
            pressed,
            action,
            modifiers: KeyModifiers::NONE,
        }
    }

    fn at(column: u16, row: u16, mouse: Mouse) -> Mouse {
        Mouse {
            column,
            row,
            ..mouse
        }
    }

    /// A screen whose program asked for the mouse with SGR reports.
    fn parser() -> vt100::Parser {
        let (rows, cols) = (NonZeroU16::new(24).unwrap(), NonZeroU16::new(80).unwrap());
        let mut parser = vt100::Parser::new(rows, cols, 0);
        parser.process(b"\x1b[?1002h\x1b[?1006h");
        parser
    }

    fn text(bytes: &[Vec<u8>]) -> Vec<String> {
        bytes
            .iter()
            .map(|sequence| String::from_utf8(sequence.clone()).unwrap())
            .collect()
    }

    #[test]
    fn a_press_and_a_release_name_their_button() {
        let down = mouse(LEFT, LEFT, MouseAction::Button);
        assert_eq!(
            events_of(&down, NONE),
            [MouseEvent::Press(MouseButton::Left)]
        );
        let up = mouse(NONE, NONE, MouseAction::Button);
        assert_eq!(
            events_of(&up, RIGHT),
            [MouseEvent::Release(MouseButton::Right)]
        );
    }

    #[test]
    fn a_double_click_is_a_press_to_the_program() {
        let double = mouse(LEFT, LEFT, MouseAction::DoubleClick);
        assert_eq!(
            events_of(&double, NONE),
            [MouseEvent::Press(MouseButton::Left)]
        );
    }

    #[test]
    fn a_move_is_a_drag_with_a_button_the_program_knows_of() {
        let drag = mouse(RIGHT, NONE, MouseAction::Moved);
        assert_eq!(
            events_of(&drag, RIGHT),
            [MouseEvent::Drag(MouseButton::Right)]
        );
        assert_eq!(events_of(&drag, NONE), [MouseEvent::Move]);
        let idle = mouse(NONE, NONE, MouseAction::Moved);
        assert_eq!(events_of(&idle, NONE), [MouseEvent::Move]);
    }

    #[test]
    fn a_button_let_go_in_another_window_is_told_with_the_next_move() {
        let idle = mouse(NONE, NONE, MouseAction::Moved);
        assert_eq!(
            events_of(&idle, LEFT),
            [MouseEvent::Release(MouseButton::Left), MouseEvent::Move]
        );
    }

    #[test]
    fn each_notch_of_the_wheel_is_told() {
        let down = mouse(NONE, NONE, MouseAction::Wheel(-2));
        assert_eq!(events_of(&down, NONE), [MouseEvent::WheelDown; 2]);
        let up = mouse(NONE, NONE, MouseAction::Wheel(1));
        assert_eq!(events_of(&up, NONE), [MouseEvent::WheelUp]);
        let tilt = mouse(NONE, NONE, MouseAction::HorizontalWheel(1));
        assert_eq!(events_of(&tilt, NONE), [MouseEvent::WheelRight]);
    }

    #[test]
    fn the_row_is_the_programs_own_when_lines_are_hidden() {
        let parser = parser();
        // The second row of the screen is the program's own `cd`, not shown.
        let view = ShellView {
            screen: parser.screen(),
            rows: vec![0, 2, 3],
        };
        let down = at(5, 2, mouse(LEFT, LEFT, MouseAction::Button));
        let (bytes, told) = reports(&view, AREA, &down, Told::default());
        assert_eq!(
            text(&bytes),
            ["\x1b[<0;6;3M"],
            "shown row 1 is screen row 2"
        );
        assert!(told.buttons.left);
    }

    #[test]
    fn a_release_below_the_shown_lines_is_told_at_the_last_of_them() {
        let parser = parser();
        let view = ShellView {
            screen: parser.screen(),
            rows: vec![0, 2],
        };
        let told = Told {
            buttons: LEFT,
            column: 5,
            row: 2,
        };
        // On the frame under the screen.
        let up = at(7, 20, mouse(NONE, NONE, MouseAction::Button));
        let (bytes, now) = reports(&view, AREA, &up, told);
        assert_eq!(text(&bytes), ["\x1b[<0;8;3m"]);
        assert!(!now.buttons.any());
    }

    #[test]
    fn outside_its_screen_a_program_hears_no_press_and_no_release_of_it() {
        let parser = parser();
        let view = ShellView {
            screen: parser.screen(),
            rows: vec![0, 1],
        };
        let down = at(5, 0, mouse(LEFT, LEFT, MouseAction::Button));
        let (bytes, told) = reports(&view, AREA, &down, Told::default());
        assert!(bytes.is_empty());
        assert!(!told.buttons.any());
        let up = at(5, 2, mouse(NONE, NONE, MouseAction::Button));
        assert!(reports(&view, AREA, &up, told).0.is_empty());
    }

    #[test]
    fn what_is_held_is_told_let_go_at_the_last_place() {
        let parser = parser();
        let view = ShellView {
            screen: parser.screen(),
            rows: vec![0, 1],
        };
        let told = Told {
            buttons: RIGHT,
            column: 9,
            row: 1,
        };
        assert_eq!(text(&releases(&view, told)), ["\x1b[<2;10;2m"]);
    }

    fn app_with_guest_view() -> (tempfile::TempDir, App) {
        let temp = tempfile::tempdir().unwrap();
        let mut app = App::new(temp.path(), 80, 24, &crate::settings::Settings::default());
        app.main_view = MainView::Guest;
        (temp, app)
    }

    #[test]
    fn a_program_in_front_takes_the_mouse_and_a_dialog_over_it_does_not_let_it() {
        let (_temp, mut app) = app_with_guest_view();
        let down = mouse(LEFT, LEFT, MouseAction::Button);
        assert_eq!(app.terminal_mouse(&down), Some(false));
        app.show_dialog(super::super::Dialog::error("x", vec!["y".to_string()]));
        assert_eq!(app.terminal_mouse(&down), None);
        app.overlay = None;
        app.main_view = MainView::Panels;
        assert_eq!(app.terminal_mouse(&down), None);
    }
}
