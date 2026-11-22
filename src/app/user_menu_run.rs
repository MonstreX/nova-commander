//! Running a user menu item as FAR's `ProcessSingleMenu` does: its commands one after another, each
//! once the one before has ended, `REM` and `::` lines left out; a command sent to the background stops
//! what is left. Each line has its special symbols replaced as its turn comes (a `cd` before it moves
//! the folder they speak of), but the name under the cursor is the one the item was chosen on (FAR's
//! `subst_context`). Its questions are asked in one dialog titled with the item's label, and Cancel
//! stops what is left. The files `!@!` made stay a while, as FAR's `delayed_deleter` keeps them, since a
//! program started by `start` reads them after its line has ended; they go after five minutes or when the
//! program ends.

use std::collections::VecDeque;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use super::dialog::{Dialog, FormAction};
use super::form::{Control, EditHistory, Form, Item as FormItem};
use super::line_input::LineInput;
use super::metasymbols::{Expanded, PanelFacts, expand_command};
use super::{App, MainView, Screen};
use crate::terminal;

/// FAR's `delayed_deleter`: how long a list file is kept.
const LIST_FILE_LIFE: Duration = Duration::from_secs(5 * 60);

/// The commands of a user menu item or of Apply command still to run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuRun {
    lines: VecDeque<RunLine>,
    /// The line whose questions are asked: what is done when it runs.
    asking: Option<RunLine>,
    /// The run the line running now came from: Cancel or the background stops it alone.
    group: u64,
    next_group: u64,
}

/// A line to run, and the file its `!.!` and its kin speak of.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RunLine {
    pub(super) text: String,
    /// The name under the cursor when the item was chosen, or the file Apply command runs on.
    pub(super) current: String,
    /// The folder the line runs in and its symbols speak of, when not the panel's (an association of
    /// a file Find file found: FAR's `ProcessLocalFileTypes` with its `CurrentDirectory`).
    pub(super) dir: Option<PathBuf>,
    /// Apply command: the file leaves the selection once its line runs (FAR's `ClearLastGetSelection`).
    pub(super) unselect: bool,
    /// An association's Enter: the line as it runs goes into the command history (FAR's
    /// `ProcessLocalFileTypes`).
    pub(super) note: bool,
    /// The item's label, its questions' title; set by `queue_run`.
    pub(super) title: String,
    /// Which run it belongs to; set by `queue_run`.
    pub(super) group: u64,
}

/// FAR's comment lines.
fn is_comment(line: &str) -> bool {
    let rem = line
        .get(..3)
        .is_some_and(|start| start.eq_ignore_ascii_case("rem"))
        && line[3..].chars().next().is_none_or(char::is_whitespace);
    rem || line.starts_with("::")
}

/// FAR's `MakeListFile`: the panel's selected names, one a line, in a file in `dir`; `F` full paths,
/// `Q` in quotes, `S` with `/`, `W` in UTF-16 (else UTF-8), no signature.
fn list_file(panel: &PanelFacts, modifiers: &str, dir_of_lists: &Path) -> io::Result<PathBuf> {
    static MADE: AtomicUsize = AtomicUsize::new(0);
    let dir = if panel.dir.ends_with('\\') {
        panel.dir.clone()
    } else {
        format!("{}\\", panel.dir)
    };
    let lines: Vec<String> = (panel.selected.iter())
        .map(|name| {
            let mut line = if modifiers.contains('F') {
                format!("{dir}{name}")
            } else {
                name.clone()
            };
            if modifiers.contains('S') {
                line = line.replace('\\', "/");
            }
            if modifiers.contains('Q') {
                line = format!("\"{line}\"");
            }
            line
        })
        .collect();
    let text = lines.join("\r\n") + "\r\n";
    let bytes: Vec<u8> = if modifiers.contains('W') {
        text.encode_utf16().flat_map(u16::to_le_bytes).collect()
    } else {
        text.into_bytes()
    };
    let number = MADE.fetch_add(1, Ordering::Relaxed);
    let path = dir_of_lists.join(format!("vc{}-{number}.tmp", std::process::id()));
    std::fs::write(&path, bytes)?;
    Ok(path)
}

/// The questions of a line: a title and an edit line each, OK and Cancel.
fn questions_form(title: &str, expanded: &Expanded) -> Form {
    let mut items = Vec::new();
    let mut y = 2;
    for (index, prompt) in expanded.prompts.iter().enumerate() {
        items.push(FormItem::new(5, y, Control::Text(prompt.title.clone())));
        // FAR's `DIF_HISTORY | DIF_USELASTHISTORY`.
        let history = EditHistory::new(&prompt.history_name(index)).use_last();
        let field = FormItem::new(5, y + 1, Control::Edit(LineInput::with_text(&prompt.init)));
        items.push(field.wide(66).with_history(history));
        y += 2;
    }
    items.push(FormItem::new(0, y, Control::Separator));
    items.push(FormItem::new(
        0,
        y + 1,
        Control::Button {
            label: "OK".to_string(),
            default: true,
        },
    ));
    items.push(FormItem::new(
        0,
        y + 1,
        Control::Button {
            label: "Cancel".to_string(),
            default: false,
        },
    ));
    Form::new(title, 76, y + 4, items)
}

impl App {
    /// Enter on an item: its commands, from the first.
    pub(super) fn run_menu_commands(&mut self, title: &str, commands: &[String]) {
        let current = self.menu_facts().active.current;
        let lines = (commands.iter())
            .filter(|line| !is_comment(line))
            .map(|text| RunLine {
                dir: None,
                text: text.clone(),
                current: current.clone(),
                unselect: false,
                note: false,
                title: String::new(),
                group: 0,
            })
            .collect();
        self.queue_run(title, lines);
    }

    /// Lines to run; while others run, after them.
    pub(super) fn queue_run(&mut self, title: &str, lines: Vec<RunLine>) {
        let run = self.user_menu.run.get_or_insert_with(|| MenuRun {
            lines: VecDeque::new(),
            asking: None,
            group: 0,
            next_group: 0,
        });
        let group = run.next_group;
        run.next_group += 1;
        run.lines.extend(lines.into_iter().map(|line| RunLine {
            title: title.to_string(),
            group,
            ..line
        }));
        self.pump_menu_run();
    }

    /// Cancel of a line's questions, or its command sent to the background: what is left of its run
    /// goes; runs asked for after it go on.
    pub(super) fn stop_menu_run(&mut self) {
        if let Some(run) = &mut self.user_menu.run {
            let group = run.group;
            run.lines.retain(|line| line.group != group);
            run.asking = None;
        }
    }

    /// Ctrl+G: FAR's question for the command, the last one given in it.
    pub(super) fn ask_apply_command(&mut self) {
        let has_files = !self.menu_facts().active.selected.is_empty();
        if !has_files {
            return;
        }
        self.show_dialog(Dialog::Input {
            title: "Apply command".to_string(),
            prompt: "Enter command to process selected files".to_string(),
            input: LineInput::with_text(&self.user_menu.apply_last),
            action: super::dialog::InputAction::ApplyCommand,
        });
    }

    /// FAR's `FileList::ApplyCommand`: `command` once for each selected file (or the one under the
    /// cursor), its special symbols speaking of that file, one after another.
    pub(super) fn apply_command(&mut self, command: &str) {
        self.user_menu.apply_last = command.to_string();
        let command = command.trim_start();
        if command.is_empty() {
            return;
        }
        let files = self.menu_facts().active.selected;
        // FAR's `SaveSelection`: Ctrl+M brings it back.
        self.active_panel_mut().save_selection();
        let lines = (files.into_iter())
            .map(|current| RunLine {
                dir: None,
                text: command.to_string(),
                current,
                unselect: true,
                note: false,
                title: String::new(),
                group: 0,
            })
            .collect();
        self.queue_run("Apply command", lines);
    }

    /// The next command once the screen is free (the one before has ended); true when one began.
    pub(super) fn pump_menu_run(&mut self) -> bool {
        let free = self.overlay.is_none()
            && self.waiting.is_none()
            && self.main_view == MainView::Panels
            && matches!(self.screen, Screen::Panels);
        if !free {
            return false;
        }
        let Some(run) = &mut self.user_menu.run else {
            return false;
        };
        let Some(line) = run.lines.pop_front() else {
            self.end_menu_run();
            self.reread_panels();
            return true;
        };
        run.group = line.group;
        let mut facts = self.menu_facts();
        facts.active.current = line.current.clone();
        if let Some(dir) = &line.dir {
            facts.active.dir = dir.display().to_string();
        }
        let mut made = Vec::new();
        let lists = self
            .user_menu
            .list_dir
            .clone()
            .unwrap_or_else(std::env::temp_dir);
        let mut make = |panel: &PanelFacts, modifiers: &str| -> io::Result<String> {
            let path = list_file(panel, modifiers, &lists)?;
            let name = path.display().to_string();
            made.push(path);
            Ok(name)
        };
        let expanded = expand_command(&line.text, &facts, &mut make);
        let now = Instant::now();
        (self.user_menu.list_files).extend(made.into_iter().map(|file| (file, now)));
        match expanded {
            Err(error) => {
                let title = if line.title.is_empty() {
                    "Error"
                } else {
                    &line.title
                };
                self.stop_menu_run();
                self.show_error(title, &error);
            }
            Ok(expanded) if expanded.prompts.is_empty() => {
                self.run_queued_line(&line, &expanded.fill(&[]));
            }
            Ok(expanded) => {
                let form = questions_form(&line.title, &expanded);
                if let Some(run) = &mut self.user_menu.run {
                    run.asking = Some(line);
                }
                self.show_dialog(Dialog::Form {
                    form,
                    action: FormAction::MenuInput(Box::new(expanded)),
                });
            }
        }
        true
    }

    /// The questions answered: OK runs the line with the answers, Cancel stops the item.
    pub(super) fn finish_menu_input(
        &mut self,
        form: &Form,
        button: Option<usize>,
        expanded: &Expanded,
    ) {
        if button != Some(form.items.len() - 2) {
            self.stop_menu_run();
            return;
        }
        let answers: Vec<String> = (0..expanded.prompts.len())
            .map(|index| form.edit_text(index * 2 + 1).to_string())
            .collect();
        let asked = self
            .user_menu
            .run
            .as_mut()
            .and_then(|run| run.asking.take());
        match asked {
            Some(line) => self.run_queued_line(&line, &expanded.fill(&answers)),
            None => self.run_menu_line(&expanded.fill(&answers)),
        }
    }

    /// A line of the queue as it runs: its file leaves the selection, it goes into the history as
    /// it asks (FAR clears the selection and notes the line after its symbols are replaced).
    fn run_queued_line(&mut self, line: &RunLine, text: &str) {
        if line.unselect {
            self.active_panel_mut().selected.remove(&line.current);
        }
        if line.note && !text.trim().is_empty() {
            self.push_history(text.trim());
        }
        self.run_menu_line_in(text, line.dir.clone());
    }

    /// One line as the command line runs it: the program's own commands (`cd`, a drive, `set`)
    /// here, the rest in the user screen.
    fn run_menu_line(&mut self, line: &str) {
        self.run_menu_line_in(line, None);
    }

    /// As [`Self::run_menu_line`], in `dir` when given: whether it was carried out or started.
    pub(in crate::app) fn run_menu_line_in(&mut self, line: &str, dir: Option<PathBuf>) -> bool {
        let line = line.trim();
        if line.is_empty() {
            return false;
        }
        let dir = dir.unwrap_or_else(|| self.active_folder());
        match terminal::parse_builtin(line) {
            Some(builtin) => {
                self.carry_out(builtin, &dir, line);
                true
            }
            None => self.started_in_user_screen(&dir, line),
        }
    }

    /// The item is done, or stopped.
    pub(super) fn end_menu_run(&mut self) {
        self.user_menu.run = None;
    }

    /// List files older than FAR keeps them go; `all`: every one (the program ends).
    pub(super) fn remove_list_files(&mut self, all: bool) {
        let now = Instant::now();
        self.user_menu.list_files.retain(|(file, made)| {
            let old = all || now.duration_since(*made) >= LIST_FILE_LIFE;
            if old {
                // A file another program still holds stays in the temporary folder.
                let _ = std::fs::remove_file(file);
            }
            !old
        });
    }

    /// The program ends: the list files go.
    pub fn close_user_menu(&mut self) {
        self.remove_list_files(true);
    }
}

#[cfg(test)]
#[path = "user_menu_run_tests.rs"]
mod tests;
