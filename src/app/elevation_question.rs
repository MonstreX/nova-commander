//! FAR's "Access denied" (`elevation.cpp`, `ElevationApproveDlg`, ADR-0007 §2): why the administrator's
//! rights are needed, the object, "Do this for all current objects", "Do not ask again".

use std::path::PathBuf;

use super::App;
use super::dialog::{ConfirmAction, Dialog};
use super::form::{Control, Form, Item};
use crate::jobs::elevation::Verb;
use crate::platform;
use crate::vfs::VfsPath;
use crate::vfs::local::elevated_session::{self, Policy};

const TITLE: &str = "Access denied";
const WIDTH: u16 = 56;
/// The item of "Do this for all current objects".
pub const FOR_ALL: usize = 3;
const NOT_AGAIN: usize = 4;
/// The items of the buttons.
pub const OK: usize = 6;
pub const SKIP: usize = 7;

fn reason(verb: Verb) -> &'static str {
    match verb {
        Verb::Create => "to create this object:",
        Verb::Open => "to open this object:",
        Verb::SetOwner => "to set owner of this object:",
        Verb::Encrypt => "to encrypt this object:",
        Verb::Decrypt => "to decrypt this object:",
        Verb::Replace => "to replace this object:",
        Verb::SetAttributes => "to set attributes of this object:",
        Verb::Delete => "to delete this object:",
        Verb::Copy => "to copy this object:",
        Verb::Move => "to move this object:",
        Verb::Recycle => "to move this object to recycle bin:",
        Verb::HardLink => "to create this hard link:",
        Verb::SymbolicLink => "to create this symbolic link:",
    }
}

/// The name, cut in the middle to fit.
fn fit(name: &str) -> String {
    crate::app::text_cut::truncate_center(name, usize::from(WIDTH) - 10)
}

pub fn form(path: &VfsPath, verb: Verb) -> Form {
    let line = |text: &str, y| Item::new(5, y, Control::Label(text.to_string()));
    let check = |label: &str, checked, y| {
        Item::new(
            5,
            y,
            Control::Check {
                label: label.to_string(),
                checked,
            },
        )
    };
    let button = |label: &str, default| {
        Item::new(
            0,
            9,
            Control::Button {
                label: label.to_string(),
                default,
            },
        )
    };
    let items = vec![
        line("You need to provide administrator permission", 2),
        line(reason(verb), 3),
        line(&fit(&path.to_string()), 4),
        check("Do this for &all current objects", true, 6),
        check("Do not ask again in the current session", false, 7),
        Item::new(0, 8, Control::Separator),
        button("&Ok", true),
        button("&Skip", false),
    ];
    Form::new(TITLE, WIDTH, 12, items).warning()
}

/// Whether `form` is this question.
pub fn is_it(form: &Form) -> bool {
    form.title == TITLE
}

/// "Do not ask again": what the button pressed means for the rest of the session.
pub fn keep_for_the_session(form: &Form, button: Option<usize>) {
    if !form.is_checked(NOT_AGAIN) {
        return;
    }
    match button {
        Some(OK) => elevated_session::set_policy(Policy::Always),
        Some(SKIP) => elevated_session::set_policy(Policy::Never),
        // Esc only stops this operation.
        _ => {}
    }
}

impl App {
    /// F7 where the rights are missing (ADR-0007): FAR's question, unless the session has its answer.
    pub(super) fn make_dir_denied(
        &mut self,
        parent: PathBuf,
        name: String,
        error: &std::io::Error,
    ) {
        match denied_step(
            platform::elevation::can_elevate(),
            elevated_session::policy(),
        ) {
            DeniedStep::Report => self.show_error("Make directory", error),
            DeniedStep::Elevate => self.make_dir_elevated(parent, name, error.to_string()),
            DeniedStep::Ask => self.ask_make_dir_elevated(parent, name, error.to_string()),
        }
    }

    fn ask_make_dir_elevated(&mut self, parent: PathBuf, name: String, error: String) {
        let object = parent.join(name.trim()).display().to_string();
        self.show_dialog(Dialog::Confirm {
            title: "Access denied".to_string(),
            lines: vec![
                "You need to provide administrator permission".to_string(),
                "to create this object:".to_string(),
                object,
            ],
            buttons: vec!["Ok", "Skip"],
            focused: 0,
            danger: false,
            action: ConfirmAction::MakeDirElevated {
                parent,
                name,
                error,
            },
        });
    }

    /// `error`: why it failed before, what stands if the helper cannot be started (ADR-0007 §3).
    pub(super) fn make_dir_elevated(&mut self, parent: PathBuf, name: String, error: String) {
        let tree_cache = self.tasks.tree_cache.clone();
        self.wait_for(Vec::new(), move || {
            let made = super::fileops::make_dir_keeping(
                tree_cache.as_ref(),
                &parent,
                &name,
                elevated_session::make_dir,
            );
            Box::new(move |app: &mut App| match made {
                Err(failed) if elevated_session::is_not_started(&failed) => {
                    app.show_dialog(Dialog::error("Make directory", vec![error]));
                }
                made => app.made_dir(&parent, made),
            })
        });
        self.label_waiting("Making the folder as administrator…");
    }
}

/// What F7 does where the rights are missing.
#[derive(Debug, PartialEq, Eq)]
enum DeniedStep {
    Report,
    Elevate,
    Ask,
}

fn denied_step(can_elevate: bool, policy: elevated_session::Policy) -> DeniedStep {
    match (can_elevate, policy) {
        (false, _) | (_, elevated_session::Policy::Never) => DeniedStep::Report,
        (true, elevated_session::Policy::Always) => DeniedStep::Elevate,
        (true, elevated_session::Policy::Ask) => DeniedStep::Ask,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Overlay;
    use crate::app::fileops::tests::app_in;
    use ratatui::crossterm::event::{KeyCode, KeyEvent};

    #[test]
    fn it_says_why_and_what_as_far_does() {
        let form = form(&VfsPath::local(r"E:\AI.Tools\README.md"), Verb::Move);
        let label = |index: usize| match &form.items[index].control {
            Control::Label(text) => text.clone(),
            other => panic!("{other:?}"),
        };
        assert_eq!(label(0), "You need to provide administrator permission");
        assert_eq!(label(1), "to move this object:");
        assert_eq!(label(2), r"E:\AI.Tools\README.md");
        assert!(form.is_checked(FOR_ALL), "on by default, as FAR");
        assert!(!form.is_checked(NOT_AGAIN));
        assert!(is_it(&form));
        assert!(matches!(form.items[OK].control, Control::Button { .. }));
        assert!(matches!(form.items[SKIP].control, Control::Button { .. }));
    }

    #[test]
    fn a_long_name_keeps_both_ends() {
        let long = format!(r"C:\{}\end.txt", "x".repeat(200));
        let fitted = fit(&long);
        assert_eq!(fitted.chars().count(), usize::from(WIDTH) - 10);
        assert!(fitted.starts_with(r"C:\") && fitted.ends_with("end.txt"));
    }

    #[test]
    fn nothing_is_kept_without_the_tick() {
        let form = form(&VfsPath::local(r"C:\a"), Verb::Delete);
        keep_for_the_session(&form, Some(OK));
        assert_eq!(elevated_session::policy(), Policy::Ask);
    }

    fn ticked() -> Form {
        let mut question = form(&VfsPath::local(r"C:\a"), Verb::Delete);
        let Control::Check { checked, .. } = &mut question.items[NOT_AGAIN].control else {
            panic!("the box of \"Do not ask again\"");
        };
        *checked = true;
        question
    }

    #[test]
    fn the_buttons_are_what_their_indices_say() {
        let question = form(&VfsPath::local(r"C:\a"), Verb::Delete);
        let button = |index: usize| match &question.items[index].control {
            Control::Button { label, .. } => label.clone(),
            other => panic!("{other:?}"),
        };
        assert_eq!(button(OK), "&Ok", "Ok is the yes that the tick keeps");
        assert_eq!(button(SKIP), "&Skip");
        let Control::Check { label, .. } = &question.items[NOT_AGAIN].control else {
            panic!("{:?}", question.items[NOT_AGAIN].control);
        };
        assert_eq!(label, "Do not ask again in the current session");
    }

    #[test]
    fn the_tick_keeps_ok_as_yes_and_skip_as_no() {
        keep_for_the_session(&ticked(), Some(OK));
        assert_eq!(elevated_session::policy(), Policy::Always);
        elevated_session::set_policy(Policy::Ask);

        keep_for_the_session(&ticked(), Some(SKIP));
        assert_eq!(elevated_session::policy(), Policy::Never);
        elevated_session::set_policy(Policy::Ask);
    }

    #[test]
    fn the_tick_keeps_nothing_for_esc_or_for_anything_but_the_buttons() {
        let question = ticked();
        keep_for_the_session(&question, None);
        assert_eq!(elevated_session::policy(), Policy::Ask, "Esc");
        keep_for_the_session(&question, Some(FOR_ALL));
        assert_eq!(elevated_session::policy(), Policy::Ask, "not a button");
        keep_for_the_session(&question, Some(NOT_AGAIN));
        assert_eq!(elevated_session::policy(), Policy::Ask, "the box itself");
    }

    #[test]
    fn skip_without_the_tick_keeps_nothing_either() {
        let question = form(&VfsPath::local(r"C:\a"), Verb::Delete);
        keep_for_the_session(&question, Some(SKIP));
        assert_eq!(elevated_session::policy(), Policy::Ask);
    }

    #[test]
    fn f7_where_the_rights_are_missing_follows_the_sessions_answer() {
        use elevated_session::Policy::{Always, Ask, Never};
        assert_eq!(denied_step(true, Ask), DeniedStep::Ask);
        assert_eq!(denied_step(true, Always), DeniedStep::Elevate);
        assert_eq!(denied_step(true, Never), DeniedStep::Report);
        assert_eq!(denied_step(false, Ask), DeniedStep::Report);
        assert_eq!(denied_step(false, Always), DeniedStep::Report);
    }

    fn asked_about_f7(app: &mut App, parent: &std::path::Path) {
        app.ask_make_dir_elevated(
            parent.to_path_buf(),
            "new".to_string(),
            "denied".to_string(),
        );
        let Some(Overlay::Dialog(Dialog::Confirm { title, lines, .. })) = &app.overlay else {
            panic!("no question");
        };
        assert_eq!(title, "Access denied");
        assert_eq!(lines[1], "to create this object:");
    }

    fn shown_error(app: &App) -> Vec<String> {
        let Some(Overlay::Dialog(Dialog::Message { lines, .. })) = &app.overlay else {
            panic!("no message");
        };
        lines.clone()
    }

    #[test]
    fn f7s_question_ok_tries_the_helper_and_its_failure_to_start_leaves_the_error() {
        let temp = tempfile::tempdir().unwrap();
        let mut app = app_in(temp.path());
        asked_about_f7(&mut app, temp.path());
        app.handle_key(KeyEvent::from(KeyCode::Enter));
        crate::app::waiting::finish_waiting(&mut app);
        // No test starts the helper: this is the prompt refused.
        assert_eq!(shown_error(&app), ["denied"]);
        assert!(!temp.path().join("new").exists());
    }

    #[test]
    fn f7s_question_skip_shows_the_error_and_esc_nothing() {
        let temp = tempfile::tempdir().unwrap();
        let mut app = app_in(temp.path());
        asked_about_f7(&mut app, temp.path());
        app.handle_key(KeyEvent::from(KeyCode::Right));
        app.handle_key(KeyEvent::from(KeyCode::Enter));
        assert_eq!(shown_error(&app), ["denied"]);

        app.overlay = None;
        asked_about_f7(&mut app, temp.path());
        app.handle_key(KeyEvent::from(KeyCode::Esc));
        assert!(app.overlay.is_none());
    }
}
