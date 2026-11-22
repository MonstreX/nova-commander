//! The signals of background tasks outside the screen (ADR-0010 §5, FAR's `copy_progress` and
//! `taskbar`): `{45%} Copying` in the console title, the progress on the taskbar button (`OSC 9;4`,
//! Windows Terminal and ConEmu; other consoles ignore it), and a flash of the button when a task ends
//! while the window is elsewhere. The title the console had is put back when no task runs and when the
//! program ends. Also the console's cursor, sized as FAR sizes it (`SetConsoleCursorInfo`, not a VT
//! shape): FAR's while the panels and their lines have it, the one the console had while a command or
//! the guest shell fills the console and when the program ends (FAR's `InitialCursorInfo`).

use std::io::{self, Write};

use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::SetTitle;

use crate::app::{App, MainView, TaskTitle};
use crate::platform;

/// The program's name after the task in the title, as FAR puts `- Far`.
const PROGRAM: &str = "nova-commander";

pub struct Signals {
    /// The console's title before the first task; `None` when it could not be read.
    original: Option<String>,
    /// What the `title` command asked for: shown while no task runs, gone with the program.
    user: Option<String>,
    /// The screen's own title shown (`App::screen_title`), over the `title` command's.
    screen: Option<String>,
    shown: Option<TaskTitle>,
    /// FAR's cursor is shown (`None`: nothing set yet).
    far_cursor: Option<bool>,
}

impl Signals {
    pub fn new() -> Self {
        Self {
            original: platform::process::console_title(),
            user: None,
            screen: None,
            shown: None,
            far_cursor: None,
        }
    }

    /// Called once per loop of the event loop: changes the title and the taskbar progress when they
    /// change; flashes the taskbar button for a task that ended while the console was not in front.
    pub fn update(&mut self, app: &mut App, focused: bool) -> io::Result<()> {
        self.update_cursor(far_cursor(app.main_view));
        if app.take_task_ended() && !focused {
            platform::process::flash_console_window();
        }
        let user = app.take_user_title();
        let screen = app.screen_title();
        let base_changed = user.is_some() || screen != self.screen;
        if let Some(title) = user {
            // FAR: an empty title is none.
            self.user = (!title.is_empty()).then_some(title);
        }
        self.screen = screen;
        let wanted = app.task_title();
        if wanted == self.shown {
            if base_changed && self.shown.is_none() {
                self.show_base()?;
            }
            return Ok(());
        }
        match &wanted {
            Some(title) => {
                execute!(io::stdout(), SetTitle(title_text(title)))?;
                let (state, percent) = taskbar_state(title);
                progress(state, percent)?;
            }
            None => {
                self.show_base()?;
                progress(0, 0)?;
            }
        }
        self.shown = wanted;
        Ok(())
    }

    /// The title while no task runs: the screen's, the `title` command's, or the console's own.
    fn show_base(&self) -> io::Result<()> {
        let screen = (self.screen.as_ref()).map(|title| format!("{title} - {PROGRAM}"));
        match screen
            .as_ref()
            .or(self.user.as_ref())
            .or(self.original.as_ref())
        {
            Some(title) => execute!(io::stdout(), SetTitle(title)),
            None => Ok(()),
        }
    }

    /// FAR's `console_session::activate`: a command gets the console's own cursor
    /// (`SetInitialCursorType`), the panels FAR's. A size the console does not take changes nothing
    /// else: it is tried again on the next loop.
    fn update_cursor(&mut self, far: bool) {
        if self.far_cursor == Some(far) {
            return;
        }
        let set = if far {
            set_far_cursor_size()
        } else {
            restore_cursor_size()
        };
        if set.is_ok() {
            self.far_cursor = Some(far);
        }
    }

    /// The title the console had when the program started.
    pub fn original_title(&self) -> Option<String> {
        self.original.clone()
    }

    /// The console's own title back and no progress on the taskbar button.
    pub fn restore(&self) -> io::Result<()> {
        if self.shown.is_none() && self.user.is_none() && self.screen.is_none() {
            return Ok(());
        }
        if let Some(original) = &self.original {
            execute!(io::stdout(), SetTitle(original))?;
        }
        progress(0, 0)
    }
}

/// Whether the console shows FAR's cursor: while the panels and their lines have it, not while a
/// command, the guest shell or a command brought back from the background fills the console.
fn far_cursor(view: MainView) -> bool {
    view == MainView::Panels
}

/// FAR's `CursorSize1`: the cursor 15% of the cell high, a line under the character, as its insert
/// mode has it in a window.
const FAR_CURSOR_SIZE: u32 = 15;

/// The height the console's cursor had when the program started (FAR's `InitialCursorInfo`).
static INITIAL_CURSOR_SIZE: std::sync::OnceLock<Option<u32>> = std::sync::OnceLock::new();

/// Takes the size the console's cursor has now as the one to put back: called first, before the
/// screen is switched (FAR's `InitConsole`).
pub fn remember_cursor_size() -> Option<u32> {
    *INITIAL_CURSOR_SIZE.get_or_init(platform::process::console_cursor_size)
}

fn set_far_cursor_size() -> io::Result<()> {
    remember_cursor_size();
    platform::process::set_console_cursor_size(FAR_CURSOR_SIZE)
}

/// The cursor the console had back.
pub fn restore_cursor_size() -> io::Result<()> {
    match remember_cursor_size() {
        Some(size) => platform::process::set_console_cursor_size(size),
        None => Ok(()),
    }
}

/// After a panic: the title the console had, no progress on the taskbar button. Nothing here may fail.
pub fn restore_after_panic(original: Option<&str>) {
    if let Some(title) = original {
        execute!(io::stdout(), SetTitle(title)).ok();
    }
    restore_cursor_size().ok();
    progress(0, 0).ok();
}

/// `{45%} Copying - program`; while a total is counted there is no percent to give.
fn title_text(title: &TaskTitle) -> String {
    match title.percent {
        Some(percent) => format!("{{{percent}%}} {} - {PROGRAM}", title.text),
        None => format!("{} - {PROGRAM}", title.text),
    }
}

/// State 1 normal, 3 indeterminate (FAR's `taskbar::indeterminate`), 4 paused (yellow: nothing moves).
fn taskbar_state(title: &TaskTitle) -> (u8, u64) {
    match (title.stalled, title.percent) {
        (true, percent) => (4, percent.unwrap_or(0)),
        (false, Some(percent)) => (1, percent),
        (false, None) => (3, 0),
    }
}

/// `OSC 9;4;state;percent`: the taskbar button's progress (state 0 removes it).
fn progress(state: u8, percent: u64) -> io::Result<()> {
    let mut out = io::stdout();
    write!(out, "\x1b]9;4;{state};{percent}\x07")?;
    out.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fars_cursor_is_the_panels_and_a_command_gets_the_consoles() {
        assert!(far_cursor(MainView::Panels));
        for view in [
            MainView::CommandOutput,
            MainView::Guest,
            MainView::Detached(0),
        ] {
            assert!(!far_cursor(view), "{view:?}");
        }
    }

    fn title(percent: Option<u64>, stalled: bool) -> TaskTitle {
        TaskTitle {
            text: "Copying".to_string(),
            percent,
            stalled,
        }
    }

    #[test]
    fn with_a_total_still_counted_there_is_no_percent_and_the_button_is_indeterminate() {
        assert_eq!(title_text(&title(None, false)), "Copying - nova-commander");
        assert_eq!(taskbar_state(&title(None, false)), (3, 0));
        assert_eq!(taskbar_state(&title(None, true)), (4, 0));
    }

    #[test]
    fn with_the_total_known_the_percent_is_shown() {
        assert_eq!(
            title_text(&title(Some(45), false)),
            "{45%} Copying - nova-commander"
        );
        assert_eq!(taskbar_state(&title(Some(45), false)), (1, 45));
        assert_eq!(taskbar_state(&title(Some(45), true)), (4, 45));
    }
}
