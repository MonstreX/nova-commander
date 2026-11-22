//! The main loop's work between input events (review R53): every step in one list, in the order each
//! must see what the ones before it did, and each saying whether the screen is to be drawn again
//! (review R35: only on a change; a running task's progress as often as FAR's `RedrawTimeout`).

use std::time::{Duration, Instant};

use super::App;

/// One step of the loop: true when the screen is to be drawn again.
type Pump = fn(&mut App) -> bool;

/// FAR's `RedrawTimeout`: how often a running task's progress (its times and speed too) is drawn.
const PROGRESS_REDRAW: Duration = Duration::from_millis(200);

/// The steps in their order: problems found at the start reported first; the command line's output, the
/// tasks and the panels before the work that waits on them; what was remembered before what reads it;
/// the dialogs held back once the work that held them has ended; the guest's screen last.
const PUMPS: [Pump; 28] = [
    |app| {
        app.report_theme_problems();
        false
    },
    App::pump_user_screen,
    App::pump_detached,
    App::pump_completion,
    |app| {
        app.report_history_failure();
        false
    },
    App::pump_transfer,
    App::pump_panels,
    App::pump_delete_count,
    App::pump_folder_count,
    App::pump_find,
    // After the tasks, the searches and the commands: the Screens list says how they are now.
    App::refresh_screens_list,
    App::pump_waiting,
    // After the work: a copy edited goes back once its editor has closed.
    App::pump_extracted,
    |app| {
        app.remember_drive_folder();
        app.remember_folders();
        app.resume_dialogs_behind() | app.pump_menu_run()
    },
    |app| {
        app.remove_list_files(false);
        app.pump_devices()
    },
    App::pump_info_panel,
    App::pump_tree_panel,
    App::pump_quick_view,
    // The schemes of the file under the cursor read ahead for F3 / F4.
    App::read_syntax_ahead,
    App::pump_viewer,
    App::pump_editor,
    // After the editor's save: quitting goes on once the window it asked about is answered.
    App::pump_quitting,
    App::pump_plugins,
    App::pump_mouse_hold,
    App::pump_menu_hold,
    |_| crate::platform::locale::take_locale_change(),
    // Heard by the devices' window (`pump_devices` starts it): nothing on screen changes.
    App::pump_environment,
    App::pump_guest,
];

impl App {
    /// Periodic work between input events: command and shell output, the tasks, the panels, the work
    /// the keys wait for, the plugins. Returns whether the screen needs to be drawn again.
    pub fn tick(&mut self) -> bool {
        PUMPS
            .iter()
            .fold(false, |changed, pump| pump(self) | changed)
    }

    /// Whether a running task's progress is due to be drawn again (FAR's `RedrawTimeout`).
    pub(super) fn progress_due(&mut self) -> bool {
        let now = Instant::now();
        let due = now.duration_since(self.work.progress_drawn) >= PROGRESS_REDRAW;
        if due {
            self.work.progress_drawn = now;
        }
        due
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::fileops::tests::app_in;

    #[test]
    fn a_running_tasks_progress_is_drawn_as_often_as_fars() {
        let temp = tempfile::tempdir().unwrap();
        let mut app = app_in(temp.path());
        app.work.progress_drawn = Instant::now() - PROGRESS_REDRAW;
        assert!(app.progress_due());
        assert!(!app.progress_due(), "not again at once");
        app.work.progress_drawn -= PROGRESS_REDRAW;
        assert!(app.progress_due(), "again after FAR's interval");
    }

    #[test]
    fn a_quiet_tick_draws_nothing() {
        let temp = tempfile::tempdir().unwrap();
        let mut app = app_in(temp.path());
        let deadline = Instant::now() + Duration::from_secs(20);
        // The panels' first reads and the start's reports done.
        while app.tick() {
            assert!(Instant::now() < deadline, "the screen never settles");
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(!app.tick(), "nothing changed: nothing drawn");
    }
}
