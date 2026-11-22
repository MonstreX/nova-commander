//! "Set" in the "Attributes" dialog: what the dialog asks for as a task (`jobs::attributes`) with FAR's
//! "Setting file attributes for" progress in front; each change that fails comes up as FAR's question
//! (Retry / Skip / Skip all / Cancel), one refused for want of rights as "Access denied" (ADR-0007).

use super::layout::{self, FIRST_FLAG, LINK_EDIT, OWNER, SUBFOLDERS};
use super::{MAIN_FLAGS, Session, times};
use crate::app::App;
use crate::app::delete::{Line, message_form};
use crate::app::dialog::{Dialog, FormAction};
use crate::app::drive_actions::MenuPlace;
use crate::app::elevation_question;
use crate::app::form::{Control, Form};
use crate::app::masked_input::MaskedInput;
use crate::app::transfer::{BACKGROUND, ShownTask};
use crate::jobs::attributes::{Question, Reply, Request, TimeEdit};
use crate::jobs::tasks::TaskId;
use crate::vfs::VfsPath;

/// The question's buttons, in order.
const REPLIES: [Reply; 4] = [Reply::Retry, Reply::Skip, Reply::SkipAll, Reply::Cancel];

/// A symbolic link's or a junction's target as typed, when it differs from the one it has, case aside
/// (FAR's `equal_icase`): unquoted, `expand` turns its environment variables into their values.
fn link_target(session: &Session, form: &Form, expand: &dyn Fn(&str) -> String) -> Option<String> {
    let link = session.link.as_ref().filter(|link| link.editable)?;
    let typed = form.edit_text(LINK_EDIT);
    let fold = crate::platform::files::fold_case;
    (fold(typed) != fold(&link.target))
        .then(|| expand(&crate::app::transfer_dialog::unquote(typed)))
}

/// What "Set" changes, from the dialog as the user left it.
pub fn request(session: &Session, form: &Form, expand: &dyn Fn(&str) -> String) -> Request {
    let mut wanted = [None; 20];
    for (at, state) in wanted.iter_mut().enumerate() {
        *state = match at.checked_sub(MAIN_FLAGS) {
            None => form.tristate(FIRST_FLAG + at),
            Some(more) => session.advanced[more],
        };
    }
    Request {
        items: (session.targets.iter())
            .map(|target| target.path.clone())
            .collect(),
        wanted,
        owner: form.edit_text(OWNER).trim().to_string(),
        times: std::array::from_fn(|row| time_edit(session, form, row)),
        utc: session.utc,
        format: session.format,
        subfolders: form.is_checked(SUBFOLDERS),
        link_target: link_target(session, form, expand),
    }
}

/// A line, and whether it is as it was first shown (the selected objects keep their own time then).
fn time_edit(session: &Session, form: &Form, row: usize) -> TimeEdit {
    let date = form.masked_text(layout::date(row));
    let time = form.masked_text(layout::time(row));
    let (shown_date, shown_time) = times::show(session.times[row], session.utc, session.format);
    let shown_date = MaskedInput::new(&session.format.date_mask(), &shown_date).text();
    let shown_time = MaskedInput::new(&session.format.time_mask(), &shown_time).text();
    let as_shown = (&date, &time) == (&shown_date, &shown_time);
    TimeEdit {
        date,
        time,
        as_shown,
    }
}

impl App {
    /// The task, its progress in front. From the panel its marks go as each object is done (FAR takes
    /// the selection off); from the drive menu the menu comes back when it ends.
    pub(in crate::app) fn start_attributes(&mut self, request: Request, menu: Option<MenuPlace>) {
        let id = self.tasks.add_attributes(request);
        match menu {
            Some(place) => self.work.attributes_menu = Some((id, place)),
            // What the tree takes is marked nowhere.
            None if self.tree_has_keys() => {}
            None => {
                self.work.task_marks.insert(id, self.active);
            }
        }
        self.shown_task = Some(ShownTask {
            id,
            focus: BACKGROUND,
            rest: Vec::new(),
        });
    }

    /// The question the task in front waits on, as FAR's error message.
    pub(in crate::app) fn attributes_question_dialog(&mut self, id: TaskId) -> Option<Dialog> {
        let form = match self.tasks.attributes_question(id)? {
            Question::Failed {
                failure,
                path,
                message,
            } => error_form(failure, path, message),
            Question::Elevate { path, verb } => {
                elevation_question::form(&VfsPath::local(path), *verb)
            }
        };
        Some(Dialog::Form {
            form,
            action: FormAction::AnswerAttributes { task: id },
        })
    }

    /// A button of the question (or Esc: `None`, Cancel) answers the task.
    pub(in crate::app) fn answer_attributes(
        &mut self,
        task: TaskId,
        form: &Form,
        button: Option<usize>,
    ) {
        if elevation_question::is_it(form) {
            elevation_question::keep_for_the_session(form, button);
            let for_all = form.is_checked(elevation_question::FOR_ALL);
            let reply = match button {
                Some(elevation_question::OK) => Reply::Elevate { for_all },
                Some(elevation_question::SKIP) => Reply::Refuse { for_all },
                _ => Reply::Cancel,
            };
            self.tasks.answer_attributes(task, reply);
            return;
        }
        let first = (form.items.iter())
            .position(|item| matches!(item.control, Control::Button { .. }))
            .unwrap_or(form.items.len());
        let reply = button
            .and_then(|index| index.checked_sub(first))
            .and_then(|position| REPLIES.get(position).copied())
            .unwrap_or(Reply::Cancel);
        self.tasks.answer_attributes(task, reply);
    }
}

/// FAR's `OperationFailed`: what could not be done, the name, the system's reason.
fn error_form(failure: &str, path: &std::path::Path, message: &str) -> Form {
    let lines = [
        Line::Centered(failure.to_string()),
        Line::Centered(path.display().to_string()),
        Line::Separator,
        Line::Centered(message.to_string()),
    ];
    let buttons = ["&Retry", "&Skip", "S&kip all", "&Cancel"];
    message_form("Error", &lines, &buttons).warning()
}
