//! Delete from the panels, as FAR's `delete.cpp`: F8 and Del (the command line empty) to the Recycle Bin
//! or past it as the settings say, Shift+F8 the file under the cursor only, Shift+Del past the Recycle
//! Bin. FAR's question comes first (`show_confirmation`); then the deletion runs as a task (ADR-0010)
//! with FAR's "Deleting" progress, and its questions come up as warning dialogs.

use super::dialog::{Dialog, FormAction};
use super::elevation_question;
use super::form::{Control, Form, Item};
use super::{App, Overlay};
use crate::app::text_cut::truncate_center;
use crate::jobs::delete::{Answer, Counter, Mode, Question, Reply, Request, Tally};
use crate::jobs::tasks::TaskId;
use crate::platform;
use crate::vfs::{Entry, VfsPath};

#[path = "delete_disks.rs"]
mod disks;
#[cfg(test)]
use disks::all_can_trash;
#[cfg(test)]
use disks::first_recyclable;
use disks::{Asked, recyclable};

/// The narrowest question dialog.
const MIN_WIDTH: u16 = 40;
/// The widest: FAR keeps 6 columns free at each side of an 80-column screen.
const MAX_WIDTH: u16 = 68;
/// The count line's widest text: the question is made wide enough for it from the start.
const WIDEST_TALLY: &str = "9 999 999 files and 999 999 folders, 999.9 GB…";

/// The count of files and folders shown in the open question before deleting (owner's addition):
/// it grows while the question waits; Delete hands it to the deletion.
#[derive(Debug)]
pub struct QuestionCount {
    counter: Counter,
    /// The form item showing it.
    line: usize,
    shown: (Tally, bool),
}

/// FAR's settings for deleting (their defaults, FAR's, are in `settings`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeleteSettings {
    /// System > "Delete to Recycle Bin".
    pub to_recycle_bin: bool,
    /// Confirmations > "Delete".
    pub confirm: bool,
    /// Confirmations > "Delete non-empty folders".
    pub confirm_folders: bool,
    /// Confirmations > "Delete read-only files" (FAR's `Confirm.RO`).
    pub confirm_read_only: bool,
    /// FAR's `DelShowTotal`: the progress counts everything (beside the deletion, owner).
    pub show_total: bool,
    /// FAR's `DelShowSelected`: names listed in the question.
    pub show_selected: usize,
}

impl DeleteSettings {
    /// As the settings file keeps them.
    pub fn saved(settings: &crate::settings::Settings) -> Self {
        Self {
            to_recycle_bin: settings.delete_to_recycle_bin,
            confirm: settings.confirm_delete,
            confirm_folders: settings.confirm_delete_folder,
            confirm_read_only: settings.confirm_read_only,
            show_total: settings.delete_show_total,
            show_selected: settings.delete_show_selected,
        }
    }
}

impl App {
    /// F8 / Del (`current_only` for Shift+F8), Shift+Del (`permanently`): FAR's question, then the task.
    /// What the disks say of the items (the tree's folder looked at, the Recycle Bins asked) is asked off
    /// the screen's thread, a window and Esc past 300 ms (ADR-0019, review R11).
    pub(super) fn ask_delete(&mut self, current_only: bool, permanently: bool) {
        // A plugin that deletes itself asks itself (FAR's `PluginDelete` with `DeleteFilesW`).
        if self.tree_folder_below_root().is_none() && self.delete_by_plugin(current_only) {
            return;
        }
        let to_bin = !permanently && self.config.delete_settings.to_recycle_bin;
        let panel = self.active_panel();
        let asked = match self.tree_folder_below_root() {
            Some(folder) => Asked::Tree(folder),
            None => {
                let items = panel.delete_items(current_only);
                // A plugin's real files go as their disks' do (ADR-0016 §14): from anywhere.
                let real = panel.path.as_local().is_none()
                    && (items.iter()).any(|item| item.path.as_local().is_some());
                match real {
                    true => Asked::Real(items),
                    false => Asked::Panel(items, panel.can_trash()),
                }
            }
        };
        self.wait_to_start(None, move || {
            let (items, can_trash) = asked.looked_at();
            // FAR asks `can_recycle` before its question: where there is no Recycle Bin it says
            // "delete"; the first item asked of the shell itself (`delete.cpp:722-737`); and, over FAR, a
            // file bigger than its disk's bin, which the shell would delete for good, is deleted only
            // after the question says so.
            let trash = to_bin && can_trash && recyclable(&items);
            Box::new(move |app: &mut App| app.ask_delete_of(items, trash))
        });
        self.label_waiting("Looking at the disks…");
    }

    /// FAR's question for `items` (to the Recycle Bin when `trash`), then the task.
    fn ask_delete_of(&mut self, items: Vec<crate::jobs::delete::Item>, trash: bool) {
        let settings = self.config.delete_settings;
        if items.is_empty() {
            return;
        }
        let request = Request {
            items,
            mode: if trash { Mode::Trash } else { Mode::Remove },
            ask_folders: settings.confirm_folders,
            ask_read_only: settings.confirm_read_only,
            count_totals: settings.show_total,
        };
        if !settings.confirm {
            self.start_delete(request, None);
            return;
        }
        // FAR's `ScrY / 2` names at most; the one name highlighted when it is not under the cursor.
        let shown = settings
            .show_selected
            .min(usize::from(self.height.saturating_sub(1)) / 2);
        let current = self
            .active_panel()
            .current()
            .map(|entry| entry.name.clone());
        let selected = match request.items.as_slice() {
            [item] => current.as_deref() != Some(item.entry.name.as_str()),
            _ => true,
        };
        let (mut form, yes, line) = confirmation(&request, shown, selected);
        self.work.delete_count = line.map(|line| {
            let shown = (Tally::default(), false);
            set_centered(&mut form, line, &tally_text(shown));
            QuestionCount {
                counter: Counter::start(request.items.clone()),
                line,
                shown,
            }
        });
        self.show_dialog(Dialog::Form {
            form,
            action: FormAction::Delete { request, yes },
        });
    }

    /// The open question's count, taken from it: to go on in the deletion, or dropped to stop.
    pub(super) fn take_delete_count(&mut self) -> Option<Counter> {
        self.work.delete_count.take().map(|count| count.counter)
    }

    /// The count in the open question: its line follows the tally. True when it changed. A question
    /// gone some other way takes its count with it.
    pub(super) fn pump_delete_count(&mut self) -> bool {
        let Some(count) = &mut self.work.delete_count else {
            return false;
        };
        let Some(Overlay::Dialog(Dialog::Form {
            form,
            action: FormAction::Delete { .. },
        })) = &mut self.overlay
        else {
            self.work.delete_count = None;
            return false;
        };
        let now = count.counter.now();
        if now == count.shown {
            return false;
        }
        count.shown = now;
        set_centered(form, count.line, &tally_text(now));
        true
    }

    /// The question a deletion in front waits on, as a dialog.
    pub(super) fn delete_question_dialog(&mut self, id: TaskId) -> Option<Dialog> {
        let question = self.tasks.delete_question(id)?.clone();
        let (form, replies) = question_dialog(&question);
        Some(Dialog::Form {
            form,
            action: FormAction::AnswerDelete { task: id, replies },
        })
    }

    /// A question's button (or Esc: `None`, Cancel) answers the task.
    pub(super) fn answer_delete(
        &mut self,
        task: TaskId,
        form: &Form,
        replies: &[Reply],
        button: Option<usize>,
    ) {
        let first = form
            .items
            .iter()
            .position(|item| matches!(item.control, Control::Button { .. }))
            .unwrap_or(form.items.len());
        let mut reply = button
            .and_then(|index| index.checked_sub(first))
            .and_then(|position| replies.get(position).copied())
            .unwrap_or(Reply::once(Answer::Cancel));
        if elevation_question::is_it(form) {
            elevation_question::keep_for_the_session(form, button);
            reply.for_all = form.is_checked(elevation_question::FOR_ALL);
        }
        self.tasks.answer_delete(task, reply);
    }
}

/// FAR's question before deleting (`show_confirmation`): one object by name ("a link to" its target
/// for a link), several as a list; the Delete button's item index with the form, and the item of the
/// count line (a folder, or several objects: owner's addition, filled as the count grows).
fn confirmation(
    request: &Request,
    show_selected: usize,
    highlight: bool,
) -> (Form, usize, Option<usize>) {
    let trash = request.mode == Mode::Trash;
    let what = match request.items.as_slice() {
        [item] if item.entry.is_dir => "the folder",
        [_] => "the file",
        _ => "the objects",
    };
    let question = if trash {
        format!("Do you wish to move {what} to the Recycle Bin?")
    } else {
        format!("Do you wish to delete {what}?")
    };
    let mut lines = vec![Line::Centered(question)];
    let mut count_line = None;
    // FAR's `DelHighlightSelected`: the names in the highlight colour.
    let mut names = 1..2;
    if let [item] = request.items.as_slice() {
        lines.push(Line::Centered(quote_outer_space(&item.entry.name)));
        if item.entry.is_dir && !item.entry.points_elsewhere() {
            count_line = Some(lines.len());
            lines.push(Line::Centered(WIDEST_TALLY.to_string()));
        }
        if let Some(facts) = file_facts(&item.entry) {
            lines.push(Line::Centered(facts));
        }
        if item.entry.points_elsewhere()
            && let Some(target) = link_target(&item.path, item.entry.link_target.as_deref())
        {
            lines.push(Line::Centered("a link to".to_string()));
            lines.push(Line::Centered(target));
        }
    } else {
        count_line = Some(lines.len());
        lines.push(Line::Centered(WIDEST_TALLY.to_string()));
        lines.push(Line::Separator);
        let first = lines.len();
        lines.extend(
            listed_names(request, show_selected)
                .into_iter()
                .map(Line::Left),
        );
        names = first..lines.len();
    }
    let button = if trash { "Move" } else { "Delete" };
    let mut form = message_form("Delete", &lines, &[button, "Cancel"]);
    if highlight {
        for item in form.items.iter_mut().take(names.end).skip(names.start) {
            item.highlighted = true;
        }
    }
    let yes = form.items.len() - 2;
    let form = if trash { form } else { form.warning() };
    (form, yes, count_line)
}

/// "120 files and 5 folders, 1.25 GB", `…` after it while counting ("Counting…" before anything is
/// counted). The size is the owner's addition.
fn tally_text((tally, known): (Tally, bool)) -> String {
    if tally.objects() == 0 && !known {
        return "Counting…".to_string();
    }
    let separator = platform::locale::number_format().thousands_separator;
    let amount = |count: u64, one: &str, many: &str| {
        let unit = if count == 1 { one } else { many };
        format!(
            "{} {unit}",
            platform::locale::group_digits(count, separator)
        )
    };
    let files = amount(tally.files, "file", "files");
    let folders = amount(tally.folders, "folder", "folders");
    let text = match (tally.files, tally.folders) {
        (_, 0) => files,
        (0, _) => folders,
        _ => format!("{files} and {folders}"),
    };
    let more = if known { "" } else { "…" };
    if tally.files == 0 {
        // Folders only: no size to speak of.
        return format!("{text}{more}");
    }
    format!("{text}, {}{more}", readable_size(tally.bytes))
}

/// A size as people read it, three digits at most as Explorer shows it: "512 bytes", "12.5 KB",
/// "1.25 GB" (the decimal separator of the locale).
fn readable_size(bytes: u64) -> String {
    const UNITS: [&str; 6] = ["KB", "MB", "GB", "TB", "PB", "EB"];
    if bytes < 1024 {
        let unit = if bytes == 1 { "byte" } else { "bytes" };
        return format!("{bytes} {unit}");
    }
    let mut value = bytes as f64 / 1024.0;
    let mut unit = 0;
    while value >= 999.5 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    let decimals = if value < 9.995 {
        2
    } else if value < 99.95 {
        1
    } else {
        0
    };
    let separator = platform::locale::number_format().decimal_separator;
    let number = format!("{value:.decimals$}").replace('.', &separator.to_string());
    format!("{number} {}", UNITS[unit])
}

/// A centred label of a message form gets new text, centred again (the lines are message_form's,
/// one per item from the top).
fn set_centered(form: &mut Form, index: usize, text: &str) {
    let text_width = usize::from(form.width) - 10;
    let text = truncate_center(text, text_width);
    let x = 5 + (text_width - text.chars().count()) / 2;
    if let Some(item) = form.items.get_mut(index) {
        item.x = u16::try_from(x).unwrap_or(5);
        item.control = Control::Label(text);
    }
}

/// A file's size and last write time, under its name in the question (the owner's addition, as
/// Explorer and Total Commander show them); nothing for a folder or a link.
fn file_facts(entry: &Entry) -> Option<String> {
    if entry.is_dir || entry.link.is_some() {
        return None;
    }
    let separator = platform::locale::number_format().thousands_separator;
    let size = platform::locale::group_digits(entry.size, separator);
    let unit = if entry.size == 1 { "byte" } else { "bytes" };
    let Some((date, time)) = entry
        .modified
        .map(|modified| platform::locale::date_time(modified, false, 5))
    else {
        return Some(format!("{size} {unit}"));
    };
    Some(format!("{size} {unit}, {date} {time}"))
}

/// FAR's list of the selected names: `show_selected` of them, then "…and N more" when more than one
/// is left (one more is shown instead).
fn listed_names(request: &Request, show_selected: usize) -> Vec<String> {
    let count = request.items.len();
    let shown = show_selected.max(1).min(count);
    let more = count - shown;
    let names = request
        .items
        .iter()
        .map(|item| quote_outer_space(&item.entry.name));
    if more > 1 {
        let mut lines: Vec<String> = names.take(shown).collect();
        lines.push(format!("…and {more} more"));
        lines
    } else {
        names.collect()
    }
}

/// FAR's questions during a deletion, as its warning messages; each button's reply in order.
fn question_dialog(question: &Question) -> (Form, Vec<Reply>) {
    if let Question::Elevate { path, verb } = question {
        return (
            elevation_question::form(path, *verb),
            vec![Reply::once(Answer::Elevate), Reply::once(Answer::Skip)],
        );
    }
    let (title, text, error) = question_text(question);
    // As FAR's `Message` with an error: the system's text last, under a separator.
    let mut lines: Vec<Line> = text.into_iter().map(Line::Centered).collect();
    if let Some(error) = error {
        lines.extend([Line::Separator, Line::Centered(error)]);
    }
    let buttons = question_buttons(question);
    let labels: Vec<&str> = buttons.iter().map(|(label, _)| *label).collect();
    let form = message_form(title, &lines, &labels).warning();
    (form, buttons.into_iter().map(|(_, reply)| reply).collect())
}

/// The title, the lines and the system's error text of a question.
fn question_text(question: &Question) -> (&'static str, Vec<String>, Option<String>) {
    match question {
        Question::NotEmpty { path, mode } => {
            let text = if *mode == Mode::Trash {
                "The following folder will be moved to the Recycle Bin:"
            } else {
                "The following folder will be deleted:"
            };
            (
                "Delete folder",
                vec![text.to_string(), path.to_string()],
                None,
            )
        }
        Question::Elevate { .. } => ("Access denied", Vec::new(), None),
        Question::ReadOnly { path } => (
            "Warning",
            vec![
                "The file is read only".to_string(),
                path.to_string(),
                "Do you wish to delete it?".to_string(),
            ],
            None,
        ),
        Question::CannotTrash {
            path,
            folder,
            message,
        } => (
            "Error",
            vec![
                cannot("move", *folder, " to the Recycle Bin"),
                quote_outer_space(&path.to_string()),
                "Try to delete it permanently?".to_string(),
            ],
            Some(message.clone()),
        ),
        Question::Error {
            path,
            folder,
            message,
            held,
            ..
        } => {
            let mut lines = vec![cannot("delete", *folder, ""), path.to_string()];
            lines.extend(held.iter().cloned());
            ("Error", lines, Some(message.clone()))
        }
    }
}

/// FAR's buttons of a question, each with its reply.
fn question_buttons(question: &Question) -> Vec<(&'static str, Reply)> {
    let delete = Reply::once(Answer::Delete);
    let all = Reply::always(Answer::Delete);
    let skip = Reply::once(Answer::Skip);
    let skip_all = Reply::always(Answer::Skip);
    let cancel = Reply::once(Answer::Cancel);
    match question {
        Question::NotEmpty { mode, .. } => {
            let first = if *mode == Mode::Trash {
                "Move"
            } else {
                "&Delete"
            };
            vec![
                (first, delete),
                ("&All", all),
                ("&Skip", skip),
                ("&Cancel", cancel),
            ]
        }
        // `question_dialog` makes this one's form itself.
        Question::Elevate { .. } => Vec::new(),
        Question::ReadOnly { .. } => vec![
            ("&Delete", delete),
            ("&All", all),
            ("&Skip", skip),
            ("S&kip all", skip_all),
            ("&Cancel", cancel),
        ],
        Question::CannotTrash { .. } => vec![
            ("&Delete", delete),
            ("&Skip", skip),
            ("S&kip all", skip_all),
            ("&Cancel", cancel),
        ],
        Question::Error { .. } => vec![
            ("&Retry", Reply::once(Answer::Retry)),
            ("&Skip", skip),
            ("S&kip all", skip_all),
            ("&Cancel", cancel),
        ],
    }
}

/// "Cannot delete the file", "Cannot move folder to the Recycle Bin" (FAR's wording).
fn cannot(verb: &str, folder: bool, rest: &str) -> String {
    match (verb, folder) {
        ("delete", true) => "Cannot delete the folder".to_string(),
        ("delete", false) => "Cannot delete the file".to_string(),
        (_, true) => format!("Cannot {verb} folder{rest}"),
        (_, false) => format!("Cannot {verb} file{rest}"),
    }
}

/// One line of a message.
pub(super) enum Line {
    Centered(String),
    Left(String),
    Separator,
}

/// FAR's `Message` / `DialogBuilder` look: the lines, a separator, the buttons in a centred row; as
/// wide as the longest line within FAR's limits, long lines cut in the middle.
pub(super) fn message_form(title: &str, lines: &[Line], buttons: &[&str]) -> Form {
    let buttons_width: usize = buttons.iter().map(|label| label.len() + 5).sum();
    let longest = lines
        .iter()
        .map(|line| match line {
            Line::Centered(text) | Line::Left(text) => text.chars().count(),
            Line::Separator => 0,
        })
        .chain([buttons_width, title.len() + 4])
        .max()
        .unwrap_or(0);
    let width = u16::try_from(longest + 10)
        .unwrap_or(MAX_WIDTH)
        .clamp(MIN_WIDTH, MAX_WIDTH);
    let text_width = usize::from(width) - 10;
    let mut items = Vec::new();
    let mut y = 2u16;
    for line in lines {
        items.push(match line {
            Line::Centered(text) => {
                let text = truncate_center(text, text_width);
                let x = 5 + (text_width - text.chars().count()) / 2;
                Item::new(u16::try_from(x).unwrap_or(5), y, Control::Label(text))
            }
            Line::Left(text) => Item::new(5, y, Control::Label(truncate_center(text, text_width))),
            Line::Separator => Item::new(0, y, Control::Separator),
        });
        y += 1;
    }
    // No buttons: the box closes under the lines (a plugin's window kept while it works).
    if buttons.is_empty() {
        return Form::new(title, width, y + 2, items);
    }
    items.push(Item::new(0, y, Control::Separator));
    items.extend(buttons.iter().enumerate().map(|(index, label)| {
        Item::new(
            0,
            y + 1,
            Control::Button {
                label: (*label).to_string(),
                default: index == 0,
            },
        )
    }));
    Form::new(title, width, y + 4, items)
}

/// FAR's `QuoteOuterSpace`: a name with a space at either end is shown in quotes.
fn quote_outer_space(name: &str) -> String {
    if name.starts_with(' ') || name.ends_with(' ') {
        format!("\"{name}\"")
    } else {
        name.to_string()
    }
}

/// Where a link points, as the question names it: read for a local link (FAR reads it there too).
fn link_target(path: &VfsPath, listed: Option<&str>) -> Option<String> {
    if let Some(target) = listed {
        return Some(target.to_string());
    }
    let local = path.as_local()?;
    platform::files::link_target(local)
        .ok()
        .map(|target| target.display().to_string())
}

#[cfg(test)]
#[path = "delete_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "delete_names_tests.rs"]
mod names_tests;
