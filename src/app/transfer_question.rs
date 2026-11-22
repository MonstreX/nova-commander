//! FAR's questions during a copy or move, as warning dialogs: "File already exists" (`copy.cpp`,
//! `WarnCopyDlg`), the read-only file, the low-space warning (ADR-0009 §5) and the error with Retry.
//! Each button carries the reply it gives.

use super::elevation_question;
use super::form::{Control, Form, Item};
use crate::jobs::transfer::{Answer, FileFacts, Question, Reply};
use crate::platform;
use crate::vfs::VfsPath;

/// FAR's `DlgW` for these dialogs.
const WIDTH: u16 = 76;
/// The text width inside the box: from column 5 to `DlgW - 6`.
const TEXT_WIDTH: usize = WIDTH as usize - 10;
/// "Remember choice" in the question about an existing file.
const REMEMBER: usize = 6;

/// The question's dialog, the reply of each button in order, and the item that turns a reply into
/// one for the rest of the operation.
pub struct QuestionDialog {
    pub form: Form,
    pub replies: Vec<Reply>,
    pub remember: Option<usize>,
}

pub fn question_dialog(question: &Question) -> QuestionDialog {
    match question {
        Question::Exists {
            source,
            target,
            append,
            ..
        } => exists(source, target, *append),
        Question::ReadOnly { target } => message(
            "Warning",
            &[
                "The file is read only".to_string(),
                target.to_string(),
                "Do you wish to overwrite it?".to_string(),
            ],
            &[
                ("&Overwrite", Reply::once(Answer::Overwrite)),
                ("&All", Reply::always(Answer::Overwrite)),
                ("&Skip", Reply::once(Answer::Skip)),
                ("S&kip all", Reply::always(Answer::Skip)),
                ("&Cancel", Reply::once(Answer::Cancel)),
            ],
        ),
        Question::LowSpace {
            target,
            new_size,
            old_size,
            free,
        } => message(
            "Warning",
            &[
                "Not enough free space to overwrite safely".to_string(),
                target.to_string(),
                format!("New: {new_size} bytes, existing: {old_size} bytes, free: {free} bytes"),
                "Overwrite in place? If the copy fails, the old file is lost.".to_string(),
            ],
            &[
                ("&In place", Reply::once(Answer::InPlace)),
                ("&All in place", Reply::always(Answer::InPlace)),
                ("&Skip", Reply::once(Answer::Skip)),
                ("&Cancel", Reply::once(Answer::Cancel)),
            ],
        ),
        Question::Elevate { path, verb } => QuestionDialog {
            form: elevation_question::form(path, *verb),
            replies: vec![Reply::once(Answer::Elevate), Reply::once(Answer::Skip)],
            remember: Some(elevation_question::FOR_ALL),
        },
        Question::Error {
            path,
            message: text,
            held,
            retry,
        } => {
            let mut buttons = vec![
                ("&Skip", Reply::once(Answer::Skip)),
                ("S&kip all", Reply::always(Answer::Skip)),
                ("&Cancel", Reply::once(Answer::Cancel)),
            ];
            if *retry {
                buttons.insert(0, ("&Retry", Reply::once(Answer::Retry)));
            }
            // As FAR's `Message`: the system's text last.
            let mut lines = vec![path.to_string()];
            lines.extend(held.iter().cloned());
            lines.push(text.clone());
            message("Error", &lines, &buttons)
        }
    }
}

/// The reply for the pressed button (its item index), or Cancel for Esc.
pub fn reply_for(dialog: (&Form, &[Reply], Option<usize>), button: Option<usize>) -> Reply {
    let (form, replies, remember) = dialog;
    let first_button = form
        .items
        .iter()
        .position(|item| matches!(item.control, Control::Button { .. }))
        .unwrap_or(form.items.len());
    let Some(mut reply) = button
        .and_then(|index| index.checked_sub(first_button))
        .and_then(|position| replies.get(position).cloned())
    else {
        return Reply::once(Answer::Cancel);
    };
    if remember.is_some_and(|index| form.is_checked(index)) {
        reply.for_all = true;
    }
    reply
}

/// FAR's "File already exists": the name, both files' size and time, "Remember choice"; no Append
/// for a link (FAR hides it).
fn exists(source: &FileFacts, target: &FileFacts, append: bool) -> QuestionDialog {
    let text = |s: String| Control::Label(s);
    let button = |label: &str, default| Control::Button {
        label: label.to_string(),
        default,
    };
    let title = "File already exists";
    let mut items = vec![
        Item::new(centered(title), 2, text(title.to_string())),
        Item::new(5, 3, text(fit_path(&target.path))),
        Item::new(0, 4, Control::Separator),
        Item::new(5, 5, text(facts_line("New", source))),
        Item::new(5, 6, text(facts_line("Existing", target))),
        Item::new(0, 7, Control::Separator),
        Item::new(
            5,
            8,
            Control::Check {
                label: "&Remember choice".to_string(),
                checked: false,
            },
        ),
        Item::new(0, 9, Control::Separator),
        Item::new(0, 10, button("&Overwrite", true)),
        Item::new(0, 10, button("&Skip", false)),
        Item::new(0, 10, button("R&ename", false)),
        Item::new(0, 10, button("A&ppend", false)),
        Item::new(0, 10, button("&Cancel", false)),
    ];
    let mut replies = vec![
        Answer::Overwrite,
        Answer::Skip,
        Answer::Rename,
        Answer::Append,
        Answer::Cancel,
    ];
    if !append {
        items.retain(
            |item| !matches!(&item.control, Control::Button { label, .. } if label == "A&ppend"),
        );
        replies.retain(|answer| *answer != Answer::Append);
    }
    QuestionDialog {
        form: Form::new("Warning", WIDTH, 13, items).warning(),
        replies: replies.into_iter().map(Reply::once).collect(),
        remember: Some(REMEMBER),
    }
}

/// FAR's "Rename" after the question's Rename (`GetString` with `FIB_BUTTONS | FIB_CHECKBOX`): the
/// free name in the line, "Remember choice", OK and Cancel.
pub fn rename_form(free: &VfsPath) -> Form {
    let mut input = super::line_input::LineInput::with_text(&free.to_string());
    input.select_all();
    let button = |label: &str, default| Control::Button {
        label: label.to_string(),
        default,
    };
    let remember = Control::Check {
        label: "&Remember choice".to_string(),
        checked: false,
    };
    let items = vec![
        Item::new(5, 2, Control::Text("&New name:".to_string())),
        Item::new(5, 3, Control::Edit(input)).wide(66),
        Item::new(0, 4, Control::Separator),
        Item::new(5, 5, remember),
        Item::new(0, 6, Control::Separator),
        Item::new(0, 7, button("OK", true)),
        Item::new(0, 7, button("Cancel", false)),
    ];
    Form::new("Rename", WIDTH, 10, items)
}

/// The items of [`rename_form`].
pub const RENAME_LINE: usize = 1;
pub const RENAME_REMEMBER: usize = 3;
pub const RENAME_OK: usize = 5;

/// FAR's `{:26} {:20} {date} {time}` line.
fn facts_line(label: &str, facts: &FileFacts) -> String {
    let (date, time) = facts
        .modified
        .map(|time| platform::locale::date_time(time, false, 8))
        .unwrap_or_default();
    format!("{label:26} {:20} {date} {time}", facts.size)
}

/// FAR's `Message` with the warning colours: centred lines, a separator, the buttons.
fn message(title: &str, lines: &[String], buttons: &[(&str, Reply)]) -> QuestionDialog {
    let rows = u16::try_from(lines.len()).unwrap_or(0);
    let mut items: Vec<Item> = lines
        .iter()
        .zip(2u16..)
        .map(|(line, y)| {
            let line = fit_text(line);
            Item::new(centered(&line), y, Control::Label(line))
        })
        .collect();
    items.push(Item::new(0, 2 + rows, Control::Separator));
    items.extend(buttons.iter().enumerate().map(|(index, (label, _))| {
        Item::new(
            0,
            3 + rows,
            Control::Button {
                label: (*label).to_string(),
                default: index == 0,
            },
        )
    }));
    QuestionDialog {
        form: Form::new(title, WIDTH, rows + 6, items).warning(),
        replies: buttons.iter().map(|(_, reply)| reply.clone()).collect(),
        remember: None,
    }
}

/// The column that centres `text` in the box.
fn centered(text: &str) -> u16 {
    let length = text.chars().count().min(TEXT_WIDTH);
    u16::try_from(5 + (TEXT_WIDTH - length) / 2).unwrap_or(5)
}

fn fit_path(path: &VfsPath) -> String {
    fit_text(&path.to_string())
}

/// Cuts a long line in the middle (FAR's `truncate_center`).
fn fit_text(text: &str) -> String {
    crate::app::text_cut::truncate_center(text, TEXT_WIDTH)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts(path: &str, size: u64) -> FileFacts {
        FileFacts {
            path: VfsPath::local(path),
            size,
            modified: None,
        }
    }

    #[test]
    fn the_exists_question_answers_by_button_and_remember_choice() {
        let mut dialog = question_dialog(&Question::Exists {
            source: facts("C:\\a\\x.txt", 10),
            target: facts("D:\\x.txt", 7),
            append: true,
            free: VfsPath::local("D:\\x (2).txt"),
        });
        assert!(dialog.form.warning);
        assert_eq!(dialog.form.focus, REMEMBER, "FAR focuses Remember choice");
        let skip = 9;
        let reply = |dialog: &QuestionDialog, button| {
            reply_for((&dialog.form, &dialog.replies, dialog.remember), button)
        };
        assert_eq!(reply(&dialog, Some(skip)), Reply::once(Answer::Skip));
        if let Control::Check { checked, .. } = &mut dialog.form.items[REMEMBER].control {
            *checked = true;
        }
        assert_eq!(reply(&dialog, Some(skip)), Reply::always(Answer::Skip));
        assert_eq!(reply(&dialog, None), Reply::once(Answer::Cancel));
        assert!(
            matches!(&dialog.form.items[4].control, Control::Label(line) if line.starts_with("Existing") && line.contains(" 7"))
        );
    }

    #[test]
    fn the_error_question_offers_retry_only_when_it_can_help() {
        let question = |retry| Question::Error {
            path: VfsPath::local("D:\\x"),
            message: "access denied".to_string(),
            held: Vec::new(),
            retry,
        };
        let with = question_dialog(&question(true));
        let without = question_dialog(&question(false));
        assert_eq!(with.replies[0], Reply::once(Answer::Retry));
        assert_eq!(without.replies[0], Reply::once(Answer::Skip));
        assert_eq!(without.replies.len(), 3);
    }

    #[test]
    fn the_error_question_names_who_holds_the_file() {
        let dialog = question_dialog(&Question::Error {
            path: VfsPath::local("D:\\x"),
            message: "in use".to_string(),
            held: vec![
                "Object is being opened in:".to_string(),
                "a.exe (PID 1)".to_string(),
            ],
            retry: true,
        });
        let labels: Vec<&str> = dialog
            .form
            .items
            .iter()
            .filter_map(|item| match &item.control {
                Control::Label(text) => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(
            labels,
            [
                "D:\\x",
                "Object is being opened in:",
                "a.exe (PID 1)",
                "in use"
            ]
        );
    }

    #[test]
    fn the_exists_question_for_a_link_has_no_append() {
        let dialog = question_dialog(&Question::Exists {
            source: facts("C:\\a\\x.txt", 10),
            target: facts("D:\\x.txt", 7),
            append: false,
            free: VfsPath::local("D:\\x (2).txt"),
        });
        let labels: Vec<&str> = (dialog.form.items.iter())
            .filter_map(|item| match &item.control {
                Control::Button { label, .. } => Some(label.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(labels, ["&Overwrite", "&Skip", "R&ename", "&Cancel"]);
        let cancel = dialog.form.items.len() - 1;
        let reply = reply_for(
            (&dialog.form, &dialog.replies, dialog.remember),
            Some(cancel),
        );
        assert_eq!(reply, Reply::once(Answer::Cancel));
    }

    #[test]
    fn long_paths_are_cut_in_the_middle() {
        let long = format!("C:\\{}\\end.txt", "x".repeat(100));
        let cut = fit_text(&long);
        assert_eq!(cut.chars().count(), TEXT_WIDTH);
        assert!(cut.starts_with("C:\\x") && cut.ends_with("end.txt") && cut.contains('…'));
    }
}
