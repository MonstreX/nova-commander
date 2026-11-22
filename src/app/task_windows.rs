//! FAR's task list (Ctrl+W, `plist.cpp`): the windows Alt+Tab shows, each `PID │ title` (its image
//! with F2), its own process's or console's window checked; Enter brings the window to the front, Del
//! asks and ends its process, Ctrl+R lists them anew. A menu as FAR's `VMenu2` (wrapping, `&` shown,
//! automatic hotkeys) in the middle of the screen, over whatever was in front, which comes back after.

use ratatui::crossterm::event::KeyEvent;

use super::dialog::{ConfirmAction, Dialog};
use super::plugins::MenuOutcome;
use super::{App, Overlay, PluginMenu};
use crate::platform::process::{self as os, TaskWindow};
use crate::plugin_host::windows::{MenuLine, MenuRequest};

const TITLE: &str = "Task list";
const KILL_TITLE: &str = "Kill task";
/// FAR's `KeysToLocalizedText(KEY_DEL, KEY_F2, KEY_CTRLR)`, the break keys in their order.
const KEYS: [&str; 3] = ["Del", "F2", "Ctrl+R"];
const DEL: usize = 0;
/// FAR's `VMenu2` with `MaxHeight` `ScrY - 4`: the list takes the screen's rows but five.
pub const TASK_LIST_ROWS_LEFT: usize = 5;
const F2: usize = 1;

/// The windows listed, and whether the images are shown instead of the titles.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TaskWindows {
    pub windows: Vec<TaskWindow>,
    pub show_image: bool,
}

/// What stays of the task list while another window stands over it or it is to open again.
#[derive(Default)]
pub struct TaskListState {
    /// What was in front when Ctrl+W was pressed: it comes back once the list closes.
    behind: Option<Overlay>,
    /// The list opens again (FAR refills it after Del) with its images shown or not.
    reopen: Option<bool>,
}

/// FAR's `SetSelectPos` without a direction: the line asked for, past the end the last one.
fn kept(selected: usize, count: usize) -> usize {
    selected.min(count.saturating_sub(1))
}

/// FAR's `PidWidth`: the digits of the greatest process number.
fn pid_width(windows: &[TaskWindow]) -> usize {
    let most = windows.iter().map(|one| one.pid).max().unwrap_or(0);
    most.checked_ilog10()
        .map_or(1, |digits| digits as usize + 1)
}

/// FAR's `AddMenuItem` for each window: `pid │ title` (or its image, "???" when unknown), checked when
/// it is this program's (`own` its process and its console's window).
pub fn lines(
    list: &TaskWindows,
    image: &dyn Fn(u32) -> String,
    own: (u32, usize),
) -> Vec<MenuLine> {
    let width = pid_width(&list.windows);
    (list.windows.iter())
        .map(|one| {
            let text = if list.show_image {
                Some(image(one.pid))
                    .filter(|name| !name.is_empty())
                    .unwrap_or_else(|| "???".to_string())
            } else {
                one.title.clone()
            };
            MenuLine {
                text: format!("{:>width$} │ {text}", one.pid),
                checked: one.pid == own.0 || one.window == own.1,
                ..MenuLine::default()
            }
        })
        .collect()
}

/// The menu of `list` with the cursor on `selected`, `rows` the screen's.
pub fn menu(list: TaskWindows, selected: usize, rows: u16) -> PluginMenu {
    let own = (std::process::id(), os::console_window());
    let request = MenuRequest {
        title: TITLE.to_string(),
        bottom: KEYS.join(" "),
        lines: lines(&list, &os::process_image, own),
        selected,
        break_keys: KEYS.iter().map(|key| (*key).to_string()).collect(),
        auto_hotkeys: true,
        wrap: true,
        show_ampersand: true,
        // FAR's `ScrY - 4`.
        max_height: Some(usize::from(rows).saturating_sub(TASK_LIST_ROWS_LEFT)),
        ..MenuRequest::default()
    };
    let mut menu = PluginMenu::new(request);
    menu.tasks = Some(list);
    menu
}

impl App {
    /// Ctrl+W from any screen (FAR's `Manager`'s key): the task list over what is in front. Not over a
    /// running program's terminal (its keys are the program's), the work's waiting window, or itself.
    pub(super) fn task_list_key(&mut self, key: KeyEvent) -> bool {
        use ratatui::crossterm::event::{KeyCode, KeyModifiers};
        if key.code != KeyCode::Char('w') || key.modifiers != KeyModifiers::CONTROL {
            return false;
        }
        let busy = matches!(self.overlay, Some(Overlay::Dialog(Dialog::Counting(_))))
            || matches!(&self.overlay, Some(Overlay::PluginMenu(menu)) if menu.tasks.is_some());
        let program = self.overlay.is_none()
            && matches!(self.screen, super::Screen::Panels)
            && matches!(
                self.main_view,
                super::MainView::Guest
                    | super::MainView::Detached(_)
                    | super::MainView::CommandOutput
            );
        // A plugin's window waits for its answer (or tells it is busy): its pump would answer Esc for
        // it while the list's question stood in front.
        let plugin = self.plugin_window_waits();
        if busy
            || program
            || plugin
            || self.task_list.reopen.is_some()
            || self.task_list.behind.is_some()
        {
            return false;
        }
        // Taken even when the windows cannot be read (FAR's list then does not open).
        self.show_task_list();
        true
    }

    /// Commands > Task list.
    pub(super) fn show_task_list(&mut self) {
        let behind = self.overlay.take();
        if self.open_task_list(false, 0) {
            self.task_list.behind = behind;
        } else {
            self.overlay = behind;
        }
    }

    /// The windows read anew into the list; false when they cannot be (FAR closes it then).
    fn open_task_list(&mut self, show_image: bool, selected: usize) -> bool {
        let Ok(windows) = os::task_windows() else {
            return false;
        };
        let selected = kept(selected, windows.len());
        let list = TaskWindows {
            windows,
            show_image,
        };
        self.overlay = Some(Overlay::PluginMenu(Box::new(menu(
            list,
            selected,
            self.height,
        ))));
        true
    }

    /// A key in the task list: FAR's `ProcessList->Run` handler and what its exit code does.
    pub(super) fn task_list_menu_key(&mut self, mut menu: Box<PluginMenu>, key: KeyEvent) {
        let outcome = menu.handle_key(key);
        let list = menu.tasks.take().unwrap_or_default();
        match outcome {
            MenuOutcome::Stay => {
                menu.tasks = Some(list);
                self.overlay = Some(Overlay::PluginMenu(menu));
            }
            MenuOutcome::Esc => {}
            MenuOutcome::Chosen { item, key: None } => {
                if let Some(window) = item.and_then(|item| list.windows.get(item)) {
                    os::switch_to_window(window.window);
                }
            }
            MenuOutcome::Chosen {
                item,
                key: Some(DEL),
            } => match item.and_then(|item| list.windows.get(item)) {
                Some(window) => self.ask_kill_task(window, list.show_image),
                None => self.refill_task_list(list.show_image, 0),
            },
            MenuOutcome::Chosen { key: Some(F2), .. } => {
                self.refill_task_list(!list.show_image, menu.selected);
            }
            MenuOutcome::Chosen { .. } => self.refill_task_list(list.show_image, 0),
        }
    }

    fn refill_task_list(&mut self, show_image: bool, selected: usize) {
        if !self.open_task_list(show_image, selected) {
            self.overlay = None;
        }
    }

    /// FAR's question before Del ends the window's process; the list opens again whatever the answer.
    fn ask_kill_task(&mut self, window: &TaskWindow, show_image: bool) {
        self.task_list.reopen = Some(show_image);
        self.show_dialog(Dialog::Confirm {
            title: KILL_TITLE.to_string(),
            lines: vec![
                "Do you wish to kill selected task?".to_string(),
                window.title.clone(),
                "You will lose any unsaved information in this program".to_string(),
            ],
            buttons: vec!["Kill", "Cancel"],
            focused: 0,
            danger: true,
            action: ConfirmAction::KillTask(window.pid),
        });
    }

    /// "Kill": the process ended (FAR's `terminate_other`), its failure told.
    pub(super) fn kill_task(&mut self, pid: u32) {
        if let Err(error) = os::terminate_process(pid) {
            self.show_dialog(Dialog::error(
                KILL_TITLE,
                vec![
                    "Cannot kill the specified task".to_string(),
                    error.to_string(),
                ],
            ));
        }
    }

    /// Once nothing stands in front: the list opened again after Del, else what Ctrl+W covered.
    pub(super) fn resume_task_list(&mut self) -> bool {
        if self.overlay.is_some() {
            return false;
        }
        if let Some(show_image) = self.task_list.reopen.take()
            && self.open_task_list(show_image, 0)
        {
            return true;
        }
        match self.task_list.behind.take() {
            Some(behind) => {
                self.overlay = Some(behind);
                true
            }
            None => false,
        }
    }
}

#[cfg(test)]
#[path = "task_windows_tests.rs"]
mod tests;
