//! FAR's Screens list (F12, `manager.cpp` `WindowsMenu`) with the background tasks (ADR-0010): the
//! panels, the viewers and the editors (`windows`), then each task, each search and each command in the
//! background; Enter brings a window to the front, a task's progress
//! dialog (or question) to the front, or shows the report of one that ended, and a search's results;
//! Del removes an ended one. F10 while tasks run asks Wait / Cancel all / Back.

use crate::app::text_cut::{truncate_path, truncate_right};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::dialog::{ConfirmAction, Dialog, ListAction};
use super::fileops::{report_dialog, report_lines};
use super::find::search_label;
use super::{App, Overlay};
use crate::jobs::tasks::{Kind, Status, Task, TaskId};

/// What a line of the Screens list stands for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreenEntry {
    Panels,
    /// A viewer or an editor, by its window's number (`windows`).
    Window(u64),
    Task(TaskId),
    /// A search in the background, by its place in the list of them.
    Search(usize),
    /// A command in the background, by its place in the list of them.
    Command(usize),
}

/// How much of a task is done, 0 to 100: by bytes, a deletion by objects. `None` while its total is not
/// known (still being counted, or a deletion told not to count): a percent of a total that grows would
/// jump back and forth.
pub fn task_percent(task: &Task) -> Option<u64> {
    let (done, total) = task_amounts(task)?;
    Some(percent_of(u128::from(done.min(total)), u128::from(total)))
}

/// What a task has done and has to do, once that is known.
fn task_amounts(task: &Task) -> Option<(u64, u64)> {
    if task.kind == Kind::Attributes {
        // Nothing is counted: FAR shows the name only.
        return None;
    }
    if task.kind == Kind::Delete {
        let progress = task.deletion();
        return progress
            .totals_known
            .then_some((progress.done, progress.total));
    }
    let progress = task.progress();
    progress.totals_known.then(|| progress.amounts())
}

fn percent_of(done: u128, total: u128) -> u64 {
    if total == 0 {
        return 0;
    }
    u64::try_from(done * 100 / total).unwrap_or(100)
}

/// What a task line says: `Copy 45%`, `Copy …` while its total is counted, `Move queued`, `Copy question!`, `Copy 2 failed`.
pub fn task_label(task: &Task) -> String {
    let kind = task.kind.title();
    let percent = match task.kind {
        // Nothing is counted: no percent and no "…" waiting for a total.
        Kind::Attributes => String::new(),
        _ => percent_text(task_percent(task)),
    };
    let state = match task.status() {
        Status::Queued => "queued".to_string(),
        Status::Running | Status::Paused if percent.is_empty() => {
            let paused = task.status() == Status::Paused;
            return if paused {
                format!("{kind} paused")
            } else {
                kind.to_string()
            };
        }
        Status::Running => percent,
        Status::Paused => format!("paused {percent}"),
        Status::Waiting => "question!".to_string(),
        Status::Done => "done".to_string(),
        Status::Failed => {
            let count = task.report().map_or(0, |report| report.failures.len());
            format!("{count} failed")
        }
        Status::Cancelled => "cancelled".to_string(),
    };
    format!("{kind} {state}")
}

fn percent_text(percent: Option<u64>) -> String {
    percent.map_or("…".to_string(), |percent| format!("{percent}%"))
}

/// What the console title and the taskbar button show while tasks run (FAR's `{35%} Copying`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskTitle {
    /// `Copying`, `Moving`, with `(+n)` for more tasks.
    pub text: String,
    /// How much of the started tasks is done, 0 to 100; `None` while that cannot be told.
    pub percent: Option<u64>,
    /// Nothing moves: every active task is paused, queued or waiting for an answer.
    pub stalled: bool,
}

/// A started task's part in the title.
struct Share {
    /// Counted by objects, not by bytes.
    deletion: bool,
    amounts: Option<(u64, u64)>,
}

/// The started tasks together. Bytes and objects do not add up: copies and moves speak for all, deletions
/// only when nothing else runs.
fn combined_percent(shares: &[Share]) -> Option<u64> {
    let transfers = shares.iter().any(|share| !share.deletion);
    let counted = shares.iter().filter(|share| !transfers || !share.deletion);
    let amounts: Option<Vec<(u64, u64)>> = counted.map(|share| share.amounts).collect();
    let (done, total) = amounts?.iter().fold((0, 0), |(done, total), (now, all)| {
        let all = u128::from(*all);
        (done + u128::from(*now).min(all), total + all)
    });
    Some(percent_of(done, total))
}

impl App {
    /// The title while tasks are active; `None` when none is.
    pub fn task_title(&self) -> Option<TaskTitle> {
        let active: Vec<&Task> = self.tasks.iter().filter(|task| task.is_active()).collect();
        let first = active.first()?;
        // A change of attributes counts nothing: it has no part in the percent.
        let shares: Vec<Share> = (active.iter())
            .filter(|task| task.status() != Status::Queued && task.kind != Kind::Attributes)
            .map(|task| Share {
                deletion: task.kind == Kind::Delete,
                amounts: task_amounts(task),
            })
            .collect();
        // Only changes of attributes run: nothing is counted, no percent.
        let counting =
            !shares.is_empty() || active.iter().all(|task| task.kind != Kind::Attributes);
        let percent = counting.then(|| combined_percent(&shares)).flatten();
        let verb = match first.kind {
            Kind::Copy => "Copying",
            Kind::Move => "Moving",
            Kind::Link => "Linking",
            Kind::Delete => "Deleting",
            // FAR's console title while it sets attributes (`MSetAttrTitle`).
            Kind::Attributes => "Attributes",
        };
        let text = match active.len() {
            1 => verb.to_string(),
            count => format!("{verb} (+{})", count - 1),
        };
        let stalled = active.iter().all(|task| task.status() != Status::Running);
        Some(TaskTitle {
            text,
            percent,
            stalled,
        })
    }

    /// Whether a task ended since the last call (the taskbar button flashes when the window is
    /// elsewhere).
    pub fn take_task_ended(&mut self) -> bool {
        std::mem::take(&mut self.work.task_ended_signal)
    }

    /// F12: the cursor on the window in front (FAR's `GetBottomWindow` selected).
    pub(super) fn show_screens(&mut self) {
        let front = match self.screen {
            super::Screen::Panels => ScreenEntry::Panels,
            _ => ScreenEntry::Window(self.front_window()),
        };
        let at = (self.screen_list().0.iter()).position(|entry| *entry == front);
        self.show_screens_at(at.unwrap_or(0));
    }

    /// The Screens list with the cursor on the line `cursor` (or the last one).
    fn show_screens_at(&mut self, cursor: usize) {
        let (ids, items) = self.screen_list();
        self.show_dialog(Dialog::List {
            title: "Screens".to_string(),
            cursor: cursor.min(items.len() - 1),
            items,
            top: Default::default(),
            action: ListAction::Screens(ids),
        });
    }

    /// While the Screens list is open its lines follow the tasks, searches and commands; the cursor
    /// stays on what it stood on.
    pub(super) fn refresh_screens_list(&mut self) -> bool {
        let open = |overlay: &Option<Overlay>| {
            matches!(
                overlay,
                Some(Overlay::Dialog(Dialog::List {
                    action: ListAction::Screens(_),
                    ..
                }))
            )
        };
        if !open(&self.overlay) {
            return false;
        }
        let (now_ids, now_items) = self.screen_list();
        let Some(Overlay::Dialog(Dialog::List {
            items,
            cursor,
            action: ListAction::Screens(ids),
            ..
        })) = &mut self.overlay
        else {
            return false;
        };
        if *items == now_items && *ids == now_ids {
            return false;
        }
        let same = ids
            .get(*cursor)
            .and_then(|entry| now_ids.iter().position(|now| now == entry));
        *cursor = same.unwrap_or_else(|| (*cursor).min(now_items.len() - 1));
        (*items, *ids) = (now_items, now_ids);
        true
    }

    /// The Screens list's lines and what each stands for.
    fn screen_list(&self) -> (Vec<ScreenEntry>, Vec<String>) {
        let width = list_width(self.width);
        let mut ids = vec![ScreenEntry::Panels];
        let mut items = vec![screen_line(0, "Panels", "", width)];
        // FAR's `WindowMenu` line: the type, `*` when the file is modified, the name.
        for window in self.window_lines() {
            let mark = if window.modified { '*' } else { ' ' };
            // A long path loses its middle, not the file's name.
            let room = width.saturating_sub(NAME_AT + 2);
            let name = format!("{mark} {}", truncate_path(&window.name, room));
            items.push(screen_line(ids.len(), window.kind, &name, width));
            ids.push(ScreenEntry::Window(window.id));
        }
        for task in self.tasks.iter() {
            let target = task.target.display().to_string();
            items.push(screen_line(
                ids.len(),
                &task_label(task),
                &format!("-> {target}"),
                width,
            ));
            ids.push(ScreenEntry::Task(task.id));
        }
        for (index, results) in self.background_searches().iter().enumerate() {
            let label = search_label(results);
            items.push(screen_line(ids.len(), &label, &results.subject(), width));
            ids.push(ScreenEntry::Search(index));
        }
        for (index, command) in self.detached.iter().enumerate() {
            let kind = if command.ended {
                "Command ended"
            } else {
                "Command"
            };
            let columns = command_columns(&command.line, width.saturating_sub(NAME_AT));
            items.push(screen_line(ids.len(), kind, &columns, width));
            ids.push(ScreenEntry::Command(index));
        }
        (ids, items)
    }

    /// Enter in the Screens list.
    pub(super) fn choose_screen(&mut self, ids: &[ScreenEntry], index: usize) {
        let id = match ids.get(index).copied() {
            Some(ScreenEntry::Task(id)) => id,
            Some(entry) => return self.enter_screen(entry),
            None => return,
        };
        let Some(task) = self.tasks.get(id) else {
            return;
        };
        if task.is_active() {
            return self.enter_screen(ScreenEntry::Task(id));
        }
        let title = task_label(task);
        let mut lines = vec![task.target.display().to_string()];
        if let Some(report) = task.report() {
            lines.extend(report_lines(report));
        }
        let failed = task.status() == Status::Failed;
        let retry = task.retry_request();
        self.tasks.remove(id);
        self.show_dialog(report_dialog(&title, lines, failed, retry));
    }

    /// Del in the Screens list: an ended task leaves; the list stays open.
    pub(super) fn remove_screen(&mut self, ids: &[ScreenEntry], index: usize) {
        match ids.get(index).copied() {
            Some(ScreenEntry::Task(id)) => self.tasks.remove(id),
            Some(ScreenEntry::Search(search)) => self.forget_find(search),
            Some(ScreenEntry::Command(command)) => self.forget_detached(command),
            // FAR's list closes no window.
            Some(ScreenEntry::Panels | ScreenEntry::Window(_)) | None => {}
        }
        self.show_screens_at(index);
    }

    /// F10: FAR's question, or Wait / Cancel all / Back while tasks run or wait.
    pub(super) fn ask_quit(&mut self) {
        // A "Cancel all" given before is not this question's.
        self.tasks.forget_stop();
        // An `exit` given up earlier leaves no code behind.
        self.environment.ask_exit_code(0);
        let running = self.tasks.active();
        let searches = self.finds_running();
        if running == 0 {
            let mut lines = vec!["Do you want to quit nova-commander?".to_string()];
            let commands = self.detached_running();
            lines.extend(background_lines(commands, searches));
            // FAR's Confirmations > Exit; running commands and searches are always said.
            if !self.prefs.confirm_exit && commands == 0 && searches == 0 {
                return self.finish_confirm(ConfirmAction::Quit, 0);
            }
            self.show_dialog(Dialog::Confirm {
                title: "Quit".to_string(),
                lines,
                buttons: vec!["Yes", "No"],
                focused: 0,
                danger: false,
                action: ConfirmAction::Quit,
            });
            return;
        }
        self.show_dialog(Dialog::Confirm {
            title: "Quit".to_string(),
            lines: quit_with_tasks_lines(running, self.detached_running(), searches),
            buttons: vec!["Wait", "Cancel all", "Back"],
            focused: 0,
            danger: true,
            action: ConfirmAction::QuitWithTasks,
        });
    }
}

/// The question of F10 while tasks run; commands and searches in the background end with the program.
fn quit_with_tasks_lines(tasks: usize, commands: usize, searches: usize) -> Vec<String> {
    let mut lines = vec![
        format!("{tasks} task(s) are running or queued."),
        "Wait for them to end, or cancel them?".to_string(),
    ];
    lines.extend(background_lines(commands, searches));
    lines
}

/// What ends with the program: the commands and the searches in the background that go on.
fn background_lines(commands: usize, searches: usize) -> Vec<String> {
    let mut lines = Vec::new();
    if commands > 0 {
        lines.push(format!(
            "{commands} command(s) in the background will be ended."
        ));
    }
    if searches > 0 {
        lines.push(format!(
            "{searches} search(es) in the background will be stopped."
        ));
    }
    lines
}

/// Where the name begins in a line of the list: after the hotkey and the type.
const NAME_AT: usize = 20;
/// The program's column at most, for a command in the background.
const PROGRAM_COLUMN: usize = 24;

/// The list's lines: the window is at most half the console (owner), its frame and margins taken off.
fn list_width(console: u16) -> usize {
    (usize::from(console) / 2).saturating_sub(6).clamp(30, 110)
}

/// The program of a command line, as typed, and its arguments.
fn program_and_arguments(line: &str) -> (&str, &str) {
    let line = line.trim();
    let end = match line.strip_prefix('"') {
        Some(rest) => rest.find('"').map_or(line.len(), |quote| quote + 2),
        None => line.find([' ', '\t']).unwrap_or(line.len()),
    };
    let (program, arguments) = line.split_at(end);
    (program, arguments.trim_start())
}

/// A command in two columns, program and arguments, in `room` characters.
fn command_columns(line: &str, room: usize) -> String {
    let (program, arguments) = program_and_arguments(line);
    if arguments.is_empty() {
        return truncate_right(program, room);
    }
    let column = PROGRAM_COLUMN.min(room / 2);
    let program = truncate_right(program, column);
    let arguments = truncate_right(arguments, room.saturating_sub(column + 1));
    format!("{program:<column$} {arguments}")
}

/// The line a hotkey of the list stands for (FAR's `VMenu` hotkeys, Alt with them too): 0-9, then A-Z.
pub(super) fn hotkey_line(key: KeyEvent) -> Option<usize> {
    let KeyCode::Char(ch) = key.code else {
        return None;
    };
    let modifiers = key.modifiers - KeyModifiers::SHIFT;
    if !modifiers.is_empty() && modifiers != KeyModifiers::ALT {
        return None;
    }
    // Base 36: the digits, then the letters from 10, either case; the character itself first, then the
    // key it is typed with in another layout.
    let latin = (super::keyboard::SYSTEM.key_for_char)(ch).map_or(ch, |(latin, _)| latin);
    (ch.to_digit(36).or_else(|| latin.to_digit(36))).and_then(|line| usize::try_from(line).ok())
}

/// FAR's line: the hotkey (0-9, then A-Z), the type, the name; all the lines as wide as the list.
fn screen_line(index: usize, kind: &str, name: &str, width: usize) -> String {
    let hotkey = match index {
        0..=9 => char::from(b'0' + u8::try_from(index).unwrap_or(0)),
        10..=35 => char::from(b'A' + u8::try_from(index - 10).unwrap_or(0)),
        _ => ' ',
    };
    format!(
        "{:<width$}",
        truncate_right(&format!("{hotkey}  {kind:<16} {name}"), width)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn transfer(amounts: Option<(u64, u64)>) -> Share {
        Share {
            deletion: false,
            amounts,
        }
    }

    fn deletion(amounts: Option<(u64, u64)>) -> Share {
        Share {
            deletion: true,
            amounts,
        }
    }

    #[test]
    fn tasks_together_are_weighed_by_their_bytes() {
        let shares = [transfer(Some((10, 100))), transfer(Some((90, 100)))];
        assert_eq!(combined_percent(&shares), Some(50));
        assert_eq!(combined_percent(&[transfer(Some((5, 0)))]), Some(0));
        assert_eq!(combined_percent(&[]), Some(0));
    }

    #[test]
    fn one_total_still_counted_leaves_no_percent() {
        let shares = [transfer(Some((10, 100))), transfer(None)];
        assert_eq!(combined_percent(&shares), None);
    }

    #[test]
    fn objects_are_not_added_to_bytes() {
        let shares = [transfer(Some((1, 4))), deletion(Some((4000, 5000)))];
        assert_eq!(combined_percent(&shares), Some(25));
    }

    #[test]
    fn a_deletion_that_is_not_counted_leaves_a_copy_its_percent() {
        let shares = [transfer(Some((1, 2))), deletion(None)];
        assert_eq!(combined_percent(&shares), Some(50));
    }

    #[test]
    fn deletions_alone_go_by_their_objects() {
        assert_eq!(combined_percent(&[deletion(Some((1, 4)))]), Some(25));
        assert_eq!(combined_percent(&[deletion(None)]), None);
    }

    #[test]
    fn a_line_says_the_percent_or_that_it_is_not_known() {
        assert_eq!(percent_text(Some(45)), "45%");
        assert_eq!(percent_text(None), "…");
    }

    #[test]
    fn screen_lines_number_like_fars() {
        assert_eq!(screen_line(0, "Panels", "", 20), "0  Panels           ");
        assert!(screen_line(10, "Copy 5%", "-> D:\\", 40).starts_with("A  Copy 5% "));
    }
}

#[cfg(test)]
mod quit_tests {
    use super::*;

    #[test]
    fn a_command_is_split_into_program_and_arguments() {
        assert_eq!(
            program_and_arguments("ping -n 30 127.0.0.1"),
            ("ping", "-n 30 127.0.0.1")
        );
        assert_eq!(
            program_and_arguments(r#""C:\Program Files\x.exe" -a"#),
            (r#""C:\Program Files\x.exe""#, "-a")
        );
        assert_eq!(program_and_arguments("tool"), ("tool", ""));
        assert_eq!(
            program_and_arguments(r#""C:\no end"#),
            (r#""C:\no end"#, "")
        );
    }

    #[test]
    fn the_columns_fit_and_are_cut_each_on_its_own() {
        assert_eq!(
            command_columns("ping -n 30", 60),
            format!("{:<24} -n 30", "ping")
        );
        let long = format!("{} {}", "p".repeat(40), "a".repeat(80));
        let shown = command_columns(&long, 60);
        assert_eq!(shown.chars().count(), 60);
        assert_eq!(shown.matches('…').count(), 2, "{shown}");
        assert_eq!(command_columns("only-program", 60), "only-program");
    }

    #[test]
    fn every_line_of_the_list_is_as_wide_as_the_list() {
        for name in ["", "short", &"x".repeat(200)] {
            assert_eq!(screen_line(1, "Command", name, 70).chars().count(), 70);
        }
        assert!(screen_line(1, "Command", &"x".repeat(200), 70).ends_with('…'));
    }

    #[test]
    fn the_list_follows_the_console_within_limits() {
        assert_eq!(list_width(100), 44, "half the console with its frame");
        assert_eq!(list_width(20), 30);
        assert_eq!(list_width(250), 110);
    }

    #[test]
    fn the_question_with_tasks_says_commands_in_the_background_will_end() {
        assert_eq!(quit_with_tasks_lines(2, 0, 0).len(), 2);
        let lines = quit_with_tasks_lines(2, 3, 0);
        assert_eq!(lines[2], "3 command(s) in the background will be ended.");
        let lines = quit_with_tasks_lines(2, 3, 1);
        assert_eq!(lines[3], "1 search(es) in the background will be stopped.");
        assert_eq!(
            quit_with_tasks_lines(1, 0, 2)[2],
            "2 search(es) in the background will be stopped."
        );
    }
}
