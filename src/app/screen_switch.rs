//! Ctrl+Tab and Ctrl+Shift+Tab (FAR's `Manager::SwitchWindow`): the next or the previous screen of the
//! Screens list comes to the front - the panels, a task's progress dialog, a search's results, a command in
//! the background - and the one in front goes behind as "Background" would send it. A task that has ended
//! is no screen to go to: its report is opened from the list (Enter there removes it). The viewers and
//! the editors are screens too (`windows`). Only a screen in front switches (FAR's `GetCanLoseFocus`):
//! a dialog or a menu, the command line's list, a modal viewer or editor and a command running on the
//! console keep the key.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use super::screens::ScreenEntry;
use super::transfer::{BACKGROUND, ShownTask};
use super::{App, MainView, Overlay, Screen};
use crate::jobs::tasks::TaskId;

/// Ctrl+Tab: forward; Ctrl+Shift+Tab: back.
fn direction(key: KeyEvent) -> Option<bool> {
    let tab = matches!(key.code, KeyCode::Tab | KeyCode::BackTab);
    if key.modifiers == KeyModifiers::CONTROL && key.code == KeyCode::Tab {
        Some(true)
    } else if key.modifiers == KeyModifiers::CONTROL | KeyModifiers::SHIFT && tab {
        Some(false)
    } else {
        None
    }
}

impl App {
    /// Ctrl+Tab / Ctrl+Shift+Tab while a screen is in front; whether the key was taken.
    pub(super) fn switch_screen_key(&mut self, key: KeyEvent) -> bool {
        let Some(forward) = direction(key) else {
            return false;
        };
        if !self.front_is_screen() {
            return false;
        }
        // Ctrl+Shift+Tab still held after it sent a running command to the background.
        if key.kind == KeyEventKind::Repeat && !forward {
            return true;
        }
        let current = self.leave_screen();
        let screens = self.screens_to_switch();
        let count = screens.len();
        let at = (screens.iter().position(|screen| *screen == current)).unwrap_or(0);
        let next = if forward {
            (at + 1) % count
        } else {
            (at + count - 1) % count
        };
        self.enter_screen(screens[next]);
        true
    }

    fn front_is_screen(&self) -> bool {
        let front = matches!(self.overlay, None | Some(Overlay::FindResults(_)));
        let panels = matches!(self.screen, Screen::Panels)
            && matches!(self.main_view, MainView::Panels | MainView::Detached(_));
        // A viewer or an editor that can lose the focus (FAR's `GetCanLoseFocus`).
        let window = !matches!(self.screen, Screen::Panels)
            && self.overlay.is_none()
            && self.front_can_lose_focus();
        front && !self.completion_shown() && (panels || window)
    }

    /// The screen in front goes behind; which it was. A viewer or an editor stays in front until the
    /// next one comes (FAR's windows keep the order they were last in front in).
    fn leave_screen(&mut self) -> ScreenEntry {
        if !matches!(self.screen, Screen::Panels) {
            return ScreenEntry::Window(self.front_window());
        }
        if matches!(self.overlay, Some(Overlay::FindResults(_)))
            && let Some(Overlay::FindResults(results)) = self.overlay.take()
        {
            return ScreenEntry::Search(self.send_find_to_background(results));
        }
        if let Some(id) = self.task_to_background() {
            return ScreenEntry::Task(id);
        }
        if let MainView::Detached(index) = self.main_view {
            self.main_view = MainView::Panels;
            return ScreenEntry::Command(index);
        }
        ScreenEntry::Panels
    }

    /// The screens in the Screens list's order: the panels, the viewers and the editors, the tasks
    /// that run or wait, the searches, the commands.
    fn screens_to_switch(&self) -> Vec<ScreenEntry> {
        let windows = (self.window_lines().into_iter()).map(|line| ScreenEntry::Window(line.id));
        let tasks = (self.tasks.iter())
            .filter(|task| task.is_active())
            .map(|task| ScreenEntry::Task(task.id));
        let searches = (0..self.background_searches().len()).map(ScreenEntry::Search);
        let commands = (0..self.detached.len()).map(ScreenEntry::Command);
        (std::iter::once(ScreenEntry::Panels).chain(windows))
            .chain(tasks)
            .chain(searches)
            .chain(commands)
            .collect()
    }

    /// `screen` to the front (Enter in the Screens list, Ctrl+Tab); a task, a search and a command come
    /// over the panels.
    pub(super) fn enter_screen(&mut self, screen: ScreenEntry) {
        let window = match screen {
            ScreenEntry::Window(id) => id,
            _ => super::windows::PANELS,
        };
        self.bring_window(window);
        match screen {
            ScreenEntry::Panels | ScreenEntry::Window(_) => {}
            ScreenEntry::Task(id) => {
                self.shown_task = Some(ShownTask {
                    id,
                    focus: BACKGROUND,
                    rest: Vec::new(),
                });
            }
            ScreenEntry::Search(index) => self.bring_back_find(index),
            ScreenEntry::Command(index) => self.show_detached(index),
        }
    }

    /// The task in front goes to the background ("Background", F2, Ctrl+Tab); ending there, it brings
    /// no menu back later. Which it was.
    pub(super) fn task_to_background(&mut self) -> Option<TaskId> {
        let id = self.shown_task.take()?.id;
        self.work.attributes_menu.take_if(|(task, _)| *task == id);
        Some(id)
    }
}

#[cfg(test)]
#[path = "screen_switch_tests.rs"]
mod tests;
