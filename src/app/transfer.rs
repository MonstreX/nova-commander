//! Copies and moves as background tasks (ADR-0009, ADR-0010): each destination is a task; the progress
//! dialog shows one of them and can send it to the background (Background, F2); its questions are shown
//! when it is the one in front; Esc asks FAR's "Operation has been interrupted". The task list and its
//! queue live in `jobs::tasks`; `ui` draws the dialog and the status row from them.

use std::time::Instant;

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::dialog::{ConfirmAction, Dialog, FormAction};
use super::form::Form;
use super::transfer_dialog::{Operation, TransferAnswers};
use super::transfer_question::{
    RENAME_LINE, RENAME_OK, RENAME_REMEMBER, question_dialog, rename_form, reply_for,
};
use super::{App, Overlay};
use crate::jobs::tasks::{Status, Task, TaskId};
use crate::jobs::transfer::{Answer, Existing, Question, Reply, Request};
use crate::platform::files::exact_path;
use crate::vfs::VfsPath;

/// The progress dialog's buttons, in order.
pub const BACKGROUND: usize = 0;
pub const PAUSE: usize = 1;
pub const CANCEL: usize = 2;

/// The task whose progress dialog is in front, its focused button, and the other destinations of the
/// same operation ("Process multiple destinations"): shown in turn, cancelled together.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShownTask {
    pub id: TaskId,
    pub focus: usize,
    pub rest: Vec<TaskId>,
}

impl App {
    /// "Retry skipped" in a report: what errors left out, as a task in front.
    pub(super) fn retry_skipped(&mut self, request: Request) {
        self.start_in_front(request);
    }

    /// `request` as a task in front (its progress dialog shown): "Retry skipped", a plugin's copy handed
    /// to the program (API 9).
    pub(super) fn start_in_front(&mut self, request: Request) {
        // Its volumes asked off the screen's thread: the share that failed may not answer (review R19).
        self.wait_to_start(None, move || {
            let volumes = crate::jobs::task_volumes::of_transfer(&request);
            Box::new(move |app: &mut App| app.retry_skipped_on(request, volumes))
        });
        self.label_waiting("Looking at the disks…");
    }

    fn retry_skipped_on(
        &mut self,
        request: Request,
        volumes: Vec<crate::platform::volumes::VolumeId>,
    ) {
        let id = self.tasks.add_on(request, volumes);
        self.shown_task = Some(ShownTask {
            id,
            focus: BACKGROUND,
            rest: Vec::new(),
        });
    }

    /// The F5 / F6 dialog was answered: a task per destination; the first one is shown.
    pub(super) fn start_transfer(
        &mut self,
        sources: &[VfsPath],
        answers: &TransferAnswers,
        operation: Operation,
    ) {
        let title = operation.title();
        let move_ = operation == Operation::Move;
        let base = self.operation_base(sources);
        let mut requests = Vec::new();
        for target in &answers.targets {
            // The dialog dealt with quotes and spaces: what is left is the name exactly.
            let target = base.resolve_exact(target);
            requests.push(Request {
                sources: sources.to_vec(),
                target,
                move_,
                existing: existing_mode(answers.existing),
                ask_read_only: answers.ask_read_only,
                keep: answers.keep(),
                link: answers.link,
                retrying: Vec::new(),
                filter: self.copy_filter(answers.use_filter),
            });
        }
        let several = sources.len() > 1;
        // The disks asked off the screen's thread, a window and Esc past 300 ms (ADR-0019, R19).
        self.wait_to_start(None, move || {
            // Several items need an existing folder: otherwise the first would take the target's name
            // and the rest would land inside it. Behind a provider the job finds that out (only a job
            // calls a provider).
            let missing = |folder: &std::path::Path| !exact_path(folder).is_dir();
            let lacking = (requests.iter())
                .find(|request| several && request.target.as_local().is_some_and(missing))
                .map(|request| request.target.clone());
            if let Some(target) = lacking {
                let line = format!(
                    "{target} is not an existing directory; several items can only go into a directory."
                );
                return Box::new(move |app: &mut App| app.show_dialog(Dialog::error(title, vec![line])));
            }
            let planned: Vec<(Request, Vec<crate::platform::volumes::VolumeId>)> = (requests.into_iter())
                .map(|request| {
                    let volumes = crate::jobs::task_volumes::of_transfer(&request);
                    (request, volumes)
                })
                .collect();
            Box::new(move |app: &mut App| app.start_planned_transfers(planned))
        });
        self.label_waiting("Looking at the disks…");
    }

    /// The tasks of an F5 / F6 whose volumes were worked out: the first one is shown.
    fn start_planned_transfers(
        &mut self,
        planned: Vec<(Request, Vec<crate::platform::volumes::VolumeId>)>,
    ) {
        let mut ids: Vec<TaskId> = planned
            .into_iter()
            .map(|(request, volumes)| self.tasks.add_on(request, volumes))
            .collect();
        // What the tree takes is marked nowhere.
        if let Some(&last) = ids.last()
            && !self.tree_has_keys()
        {
            self.work.task_marks.insert(last, self.active);
        }
        if !ids.is_empty() {
            let id = ids.remove(0);
            self.shown_task = Some(ShownTask {
                id,
                focus: BACKGROUND,
                rest: ids,
            });
        }
    }

    /// Between input events: tasks start, end and ask. The task in front shows its question; a task
    /// that ended in front closes its dialog, reports, and the next destination of its operation comes
    /// to the front; one in the background rereads the panels. True when the screen must be drawn
    /// again: a task's progress is due, or the list or the dialogs changed.
    pub(super) fn pump_transfer(&mut self) -> bool {
        let count = self.tasks.len();
        let shown_before = self.shown_task.as_ref().map(|shown| shown.id);
        let interrupted = self.interrupted_task();
        self.tasks.hold_only(interrupted);
        let ended = self.tasks.pump(Instant::now());
        self.take_off_finished_marks();
        self.work.task_ended_signal |= !ended.is_empty();
        if !ended.is_empty() {
            let places = (ended.iter())
                .filter_map(|&id| self.tasks.get(id).map(|task| task.target.clone()))
                .collect();
            self.tree_files_changed(places);
        }
        for &id in &ended {
            self.task_ended(id, interrupted == Some(id));
        }
        let asked = self.show_question();
        self.quit_if_idle();
        let shown_now = self.shown_task.as_ref().map(|shown| shown.id);
        // Progress moves: drawn as often as FAR's (review R35).
        (self.tasks.active() > 0 && self.progress_due())
            || !ended.is_empty()
            || asked
            || self.tasks.len() != count
            || shown_now != shown_before
    }

    /// FAR takes a mark off as soon as its object is done, not at the end of the operation.
    fn take_off_finished_marks(&mut self) {
        let mut changed = [false; 2];
        for (id, path) in self.tasks.take_finished_objects() {
            if let Some(side) = self.work.task_marks.get(&id) {
                changed[side.index()] |= self.panels[side.index()].unmark(&path);
            }
        }
        for (panel, changed) in self.panels.iter_mut().zip(changed) {
            if changed {
                panel.selection_changed();
            }
        }
        let tasks = &self.tasks;
        self.work
            .task_marks
            .retain(|id, _| tasks.get(*id).is_some_and(Task::is_active));
    }

    /// The question of the task in front, when no other dialog is up.
    fn show_question(&mut self) -> bool {
        let Some(id) = self.shown_task.as_ref().map(|shown| shown.id) else {
            return false;
        };
        if self.screen_busy() {
            return false;
        }
        let Some(question) = self.tasks.question(id).cloned() else {
            let dialog = match self.delete_question_dialog(id) {
                Some(dialog) => Some(dialog),
                None => self.attributes_question_dialog(id),
            };
            let Some(dialog) = dialog else {
                return false;
            };
            self.show_dialog(dialog);
            return true;
        };
        let dialog = question_dialog(&question);
        self.show_dialog(Dialog::Form {
            form: dialog.form,
            action: FormAction::Answer {
                task: id,
                replies: dialog.replies,
                remember: dialog.remember,
            },
        });
        true
    }

    /// Back (or Esc) from the question about tasks that do not stop: the program stays, nothing waits
    /// to quit.
    pub(super) fn stay_after_not_stopping(&mut self) {
        self.work.quit_when_idle = false;
        self.tasks.forget_stop();
    }

    /// Tasks cancelled at F10 still run (a dead share, a plugin): ADR-0019's Wait / Abandon, and Back.
    fn ask_quit_not_stopping(&mut self) {
        let count = self.tasks.active();
        self.show_dialog(Dialog::Confirm {
            title: "Quit".to_string(),
            lines: vec![
                format!("{count} cancelled task(s) do not stop:"),
                "a disk or a plugin does not answer.".to_string(),
                "Quitting anyway may leave a file being written incomplete.".to_string(),
            ],
            buttons: vec!["Wait", "Quit anyway", "Back"],
            focused: 0,
            danger: true,
            action: ConfirmAction::QuitNotStopping,
        });
    }

    /// F10 > Wait: quit once nothing runs and no report waits to be read; a task that failed in the
    /// background is shown in the Screens list instead, and the program stays.
    fn quit_if_idle(&mut self) {
        if !self.work.quit_when_idle || self.screen_busy() {
            return;
        }
        if self.tasks.not_stopping() {
            return self.ask_quit_not_stopping();
        }
        if self.tasks.active() > 0 {
            return;
        }
        self.work.quit_when_idle = false;
        if self
            .tasks
            .iter()
            .any(|task| task.status() == Status::Failed)
        {
            self.show_screens();
        } else {
            self.finish_quit();
        }
    }

    fn interrupted_task(&self) -> Option<TaskId> {
        match &self.overlay {
            Some(Overlay::Dialog(Dialog::Confirm {
                action: ConfirmAction::StopTransfer(id),
                ..
            })) => Some(*id),
            _ => None,
        }
    }

    fn task_ended(&mut self, id: TaskId, interrupted: bool) {
        let Some(shown) = self.shown_task.take_if(|shown| shown.id == id) else {
            // A task gone from the list was dismissed after it may have written something.
            if self.tasks.get(id).is_none_or(Task::has_started) {
                self.reread_panels();
            }
            self.work.attributes_menu.take_if(|(task, _)| *task == id);
            return;
        };
        // Cancelled (Esc > Yes, or Cancel in one of its questions): the whole operation stops, as in
        // FAR. Otherwise the next destination that still runs comes to the front.
        let cancelled = self
            .tasks
            .get(id)
            .is_some_and(|task| task.status() == Status::Cancelled);
        let mut rest = shown.rest;
        if cancelled {
            self.cancel_group(&rest);
            rest.clear();
        }
        rest.retain(|&other| self.tasks.get(other).is_some_and(Task::is_active));
        if !rest.is_empty() {
            let next = rest.remove(0);
            self.shown_task = Some(ShownTask {
                id: next,
                focus: BACKGROUND,
                rest,
            });
        }
        if interrupted {
            self.overlay = None;
        }
        let Some(task) = self.tasks.get(id) else {
            return;
        };
        let title = task.kind.title();
        let report = task.report().cloned().unwrap_or_default();
        let retry = task.retry_request();
        // Seen ending in front: it needs no place in the Screens list.
        self.tasks.remove(id);
        self.finish_operation(title, &report, retry);
        self.go_to_renamed(report.renamed.as_ref());
        if let Some((_, place)) = self.work.attributes_menu.take_if(|(task, _)| *task == id)
            && !self.screen_busy()
        {
            self.back_to_drive_menu(place);
        }
    }

    /// Keys while a progress dialog is in front: its buttons (Background, Pause, Cancel), F2 sends it to
    /// the background, Esc asks whether to stop. The panels get no keys meanwhile.
    pub(super) fn handle_transfer_key(&mut self, key: KeyEvent) {
        let Some(shown) = self.shown_task.as_mut() else {
            return;
        };
        let letter = match key.code {
            KeyCode::Char(ch) if key.modifiers.is_empty() || key.modifiers == KeyModifiers::ALT => {
                Some(ch.to_ascii_lowercase())
            }
            _ => None,
        };
        let pressed = match (key.code, letter) {
            (KeyCode::Esc, _) => Some(CANCEL),
            (KeyCode::F(2), _) | (_, Some('b')) => Some(BACKGROUND),
            (_, Some('p' | 'o')) => Some(PAUSE),
            (_, Some('c')) => Some(CANCEL),
            (KeyCode::Enter | KeyCode::Char(' '), _) => Some(shown.focus),
            (KeyCode::Left | KeyCode::BackTab, _) => {
                shown.focus = (shown.focus + 2) % 3;
                None
            }
            (KeyCode::Right | KeyCode::Tab, _) => {
                shown.focus = (shown.focus + 1) % 3;
                None
            }
            _ => None,
        };
        let id = shown.id;
        match pressed {
            Some(BACKGROUND) => {
                self.task_to_background();
            }
            Some(PAUSE) => {
                let paused = self.tasks.get(id).is_some_and(Task::is_paused);
                self.tasks.set_paused(id, !paused);
            }
            Some(_) => self.ask_stop(id),
            None => {}
        }
    }

    fn ask_stop(&mut self, id: TaskId) {
        if !self.prefs.confirm_interrupt {
            return self.stop_transfer(id);
        }
        self.show_dialog(Dialog::Confirm {
            title: "Operation has been interrupted".to_string(),
            lines: vec!["Do you really want to cancel it?".to_string()],
            buttons: vec!["Yes", "No"],
            focused: 0,
            danger: true,
            action: ConfirmAction::StopTransfer(id),
        });
    }

    /// Yes in "Operation has been interrupted": the task stops, and with it the other destinations of
    /// its operation (FAR stops the whole operation); those that never started leave the list.
    pub(super) fn stop_transfer(&mut self, id: TaskId) {
        self.tasks.cancel(id);
        let rest = match &mut self.shown_task {
            Some(shown) if shown.id == id => std::mem::take(&mut shown.rest),
            _ => Vec::new(),
        };
        self.cancel_group(&rest);
    }

    /// Cancels the other destinations of an operation; they leave the list once they have ended.
    fn cancel_group(&mut self, rest: &[TaskId]) {
        for &other in rest {
            self.tasks.dismiss(other);
        }
    }

    /// A question's button (or Esc: `None`) answers the task.
    pub(super) fn answer_transfer(
        &mut self,
        task: TaskId,
        form: &Form,
        replies: &[Reply],
        remember: Option<usize>,
        button: Option<usize>,
    ) {
        if super::elevation_question::is_it(form) {
            super::elevation_question::keep_for_the_session(form, button);
        }
        let reply = reply_for((form, replies, remember), button);
        // FAR's Rename without "Remember choice" asks the name first.
        if let (Answer::Rename, false, Some(Question::Exists { target, free, .. })) =
            (reply.answer, reply.for_all, self.tasks.question(task))
        {
            let (target, form) = (target.path.clone(), rename_form(free));
            self.show_dialog(Dialog::Form {
                form,
                action: FormAction::RenameAnswer { task, target },
            });
            return;
        }
        self.tasks.answer(task, reply);
    }

    /// "Rename" closed: OK gives the task the name (its variables expanded, FAR's `FIB_EXPANDENV`).
    pub(super) fn finish_rename(
        &mut self,
        form: &Form,
        button: Option<usize>,
        (task, target): (TaskId, VfsPath),
    ) {
        if button != Some(RENAME_OK) {
            return;
        }
        let typed = form.edit_text(RENAME_LINE);
        // Nothing typed: as Cancel, the question comes back.
        if typed.trim().is_empty() {
            return;
        }
        let text = self.environment.expand(typed);
        let typed = VfsPath::parse(&text);
        let name = match &typed {
            VfsPath::Local(path) if !path.has_root() => target
                .parent()
                .unwrap_or_else(|| target.clone())
                .join(&text),
            _ => typed,
        };
        let reply = Reply::renamed(name, form.is_checked(RENAME_REMEMBER));
        self.tasks.answer(task, reply);
    }
}

/// The line chosen in "Already existing files".
fn existing_mode(line: usize) -> Existing {
    match line {
        1 => Existing::Overwrite,
        2 => Existing::Skip,
        3 => Existing::Rename,
        4 => Existing::Append,
        5 => Existing::OnlyNewer,
        _ => Existing::Ask,
    }
}

#[cfg(test)]
mod tests {
    use super::super::dialog::DialogOutcome;
    use super::super::fileops::tests::{
        answers, app_in, finish_transfer, select_all_files, submit_dialog,
    };
    use super::*;
    use std::time::{Duration, Instant};

    /// `src/a.txt` ("new") to copy into the folder above, which has an `a.txt` ("old"): the task asks.
    fn conflicting_copy() -> (tempfile::TempDir, App) {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("src");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::write(source.join("a.txt"), "new").unwrap();
        std::fs::write(temp.path().join("a.txt"), "old").unwrap();
        let mut app = app_in(&source);
        select_all_files(&mut app);
        let sources = app.active_panel().targets();
        app.start_transfer(&sources, &answers(".."), Operation::Copy);
        (temp, app)
    }

    fn tick_until(app: &mut App, mut done: impl FnMut(&App) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !done(app) {
            assert!(Instant::now() < deadline, "waited too long");
            app.tick();
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn first_status(app: &App) -> Option<Status> {
        app.tasks
            .iter()
            .next()
            .map(crate::jobs::tasks::Task::status)
    }

    #[test]
    fn f2_sends_a_task_to_the_background_and_f12_brings_it_and_its_question_back() {
        let (temp, mut app) = conflicting_copy();
        assert!(app.shown_task.is_some());
        assert!(
            !app.status_row_shown(),
            "the operation in front has its dialog; the panels stay where they are"
        );
        app.handle_transfer_key(KeyEvent::from(KeyCode::F(2)));
        assert!(app.shown_task.is_none(), "the panels take keys again");
        tick_until(&mut app, |app| first_status(app) == Some(Status::Waiting));
        assert!(app.overlay.is_none(), "a question in the background waits");
        assert!(app.status_row_shown());

        app.show_screens();
        let Some(Overlay::Dialog(Dialog::List { items, .. })) = &app.overlay else {
            panic!("the Screens list");
        };
        assert!(items[1].contains("Copy question!"), "{items:?}");
        submit_dialog(&mut app, DialogOutcome::Choose(1));
        assert!(app.shown_task.is_some());
        assert!(
            !app.status_row_shown(),
            "back in front, no task is left behind"
        );
        tick_until(&mut app, |app| app.overlay.is_some());
        // "File already exists": its first button, Overwrite.
        submit_dialog(&mut app, DialogOutcome::Button(8));
        finish_transfer(&mut app);

        assert_eq!(
            std::fs::read_to_string(temp.path().join("a.txt")).unwrap(),
            "new"
        );
        assert!(
            app.tasks.is_empty(),
            "seen ending in front, it leaves the list"
        );
        assert!(!app.status_row_shown());
    }

    #[test]
    fn pause_holds_the_task_and_continue_lets_it_go() {
        let (temp, mut app) = conflicting_copy();
        tick_until(&mut app, |app| app.overlay.is_some());
        app.handle_transfer_key(KeyEvent::from(KeyCode::Char('p')));
        assert!(app.tasks.iter().next().is_some_and(|task| task.is_paused()));
        submit_dialog(&mut app, DialogOutcome::Button(8));
        std::thread::sleep(Duration::from_millis(200));
        app.tick();
        assert_eq!(first_status(&app), Some(Status::Paused));
        assert_eq!(
            std::fs::read_to_string(temp.path().join("a.txt")).unwrap(),
            "old"
        );
        app.handle_transfer_key(KeyEvent::from(KeyCode::Char('o')));
        finish_transfer(&mut app);
        assert_eq!(
            std::fs::read_to_string(temp.path().join("a.txt")).unwrap(),
            "new"
        );
    }

    #[test]
    fn quitting_with_a_task_asks_and_cancel_all_quits_once_it_stopped() {
        let (temp, mut app) = conflicting_copy();
        app.handle_transfer_key(KeyEvent::from(KeyCode::F(2)));
        // The task runs and waits for its answer when the user quits.
        tick_until(&mut app, |app| first_status(app) == Some(Status::Waiting));
        app.ask_quit();
        assert!(matches!(
            app.overlay,
            Some(Overlay::Dialog(Dialog::Confirm {
                action: ConfirmAction::QuitWithTasks,
                ..
            }))
        ));
        submit_dialog(&mut app, DialogOutcome::Button(1));
        tick_until(&mut app, App::should_quit);
        assert_eq!(
            std::fs::read_to_string(temp.path().join("a.txt")).unwrap(),
            "old"
        );
    }
}
