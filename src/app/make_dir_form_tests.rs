//! F7's "Make folder": FAR's dialog, its names one or several, the target open with a link type, a
//! junction and a symbolic link made, no name FAR's warning, the choice of several names kept.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::*;
use crate::app::dialog::{DialogOutcome, FormAction};
use crate::app::fileops::tests::{app_in, submit_dialog};
use crate::app::{App, Dialog, Overlay};

fn press(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
    app.handle_key(KeyEvent::new(code, modifiers));
}

fn form_of(app: &mut App) -> &mut Form {
    match &mut app.overlay {
        Some(Overlay::Dialog(Dialog::Form {
            form,
            action: FormAction::MakeDir(_),
        })) => form,
        _ => panic!("the dialog"),
    }
}

/// F7, `name` typed, the link type `link` with `target`, several names or not, OK; the work done.
fn make(app: &mut App, name: &str, (link, target): (usize, &str), multiple: bool) {
    press(app, KeyCode::F(7), KeyModifiers::NONE);
    let form = form_of(app);
    if let Some(Control::Edit(input)) = form.control_mut(NAME) {
        *input = LineInput::with_text(name);
    }
    if let Some(Control::Edit(input)) = form.control_mut(TARGET) {
        *input = LineInput::with_text(target);
    }
    if let Some(Control::DropDown { chosen, .. }) = form.control_mut(LINK_TYPE) {
        *chosen = link;
    }
    if let Some(Control::Check { checked, .. }) = form.control_mut(MULTIPLE) {
        *checked = multiple;
    }
    submit_dialog(app, DialogOutcome::Button(OK));
}

#[test]
fn the_dialog_is_far_s_and_the_target_opens_with_a_link_type() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    press(&mut app, KeyCode::F(7), KeyModifiers::NONE);
    let form = form_of(&mut app);
    assert_eq!(form.title, "Make folder");
    assert_eq!(form.focus, NAME);
    let history = |form: &Form, at: usize| form.items[at].history.as_ref().unwrap().name.clone();
    assert_eq!(history(form, NAME), "NewFolder");
    assert_eq!(history(form, TARGET), "NewFolderLinkTarget");
    assert!(form.items[TARGET].disabled, "no link: no target");
    assert!(!form.is_checked(MULTIPLE));
    if let Some(Control::DropDown { chosen, .. }) = form.control_mut(LINK_TYPE) {
        *chosen = 1;
    }
    let mut dialog = app.overlay.take().map(|overlay| match overlay {
        Overlay::Dialog(dialog) => dialog,
        _ => panic!("the dialog"),
    });
    follow(dialog.as_mut().unwrap());
    let Some(Dialog::Form { form, .. }) = &dialog else {
        panic!("the form");
    };
    assert!(!form.items[TARGET].disabled, "a junction: its target");
}

#[test]
fn the_names_one_or_several() {
    assert_eq!(names(r#" "a, b" "#, false), ["a, b"]);
    assert_eq!(names("a;b , c", false), ["a;b , c"]);
    assert_eq!(names(r#"a; "b; c" ,d,, "#, true), ["a", "b; c", "d"]);
    assert!(names(" ; , ", true).is_empty());
    assert!(names("  ", false).is_empty());
}

/// Several names made, the folders on their way too; the cursor on the last name's first folder; the
/// choice kept for the next F7 and the settings.
#[test]
fn several_names_are_made_and_the_choice_is_kept() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    make(&mut app, r"one; two\deep, three", (0, ""), true);
    for made in ["one", r"two\deep", "three"] {
        assert!(temp.path().join(made).is_dir(), "{made}");
    }
    let current = app.active_panel().current().map(|entry| entry.name.clone());
    assert_eq!(current.as_deref(), Some("three"));
    assert!(app.prefs.multi_make_dir);
    press(&mut app, KeyCode::F(7), KeyModifiers::NONE);
    assert!(form_of(&mut app).is_checked(MULTIPLE), "kept");
    app.overlay = None;
    make(&mut app, "a;b", (0, ""), false);
    assert!(temp.path().join("a;b").is_dir(), "one name with a `;`");
}

/// No name: FAR's "Incorrect folders list", nothing made.
#[test]
fn no_name_is_far_s_warning() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    make(&mut app, " ;, ", (0, ""), true);
    let Some(Overlay::Dialog(Dialog::Message { title, lines, .. })) = &app.overlay else {
        panic!("the warning");
    };
    assert_eq!(
        (title.as_str(), lines[0].as_str()),
        ("Warning", "Incorrect folders list")
    );
    assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
}

/// A junction and a symbolic link made with their targets (the folders on the way made).
#[cfg(windows)]
#[test]
fn a_junction_and_a_symbolic_link_are_made() {
    let temp = tempfile::tempdir().unwrap();
    let target = temp.path().join("target");
    std::fs::create_dir(&target).unwrap();
    std::fs::write(target.join("in.txt"), "in").unwrap();
    let mut app = app_in(temp.path());
    let quoted = format!("\"{}\"", target.display());
    make(&mut app, r"links\j", (1, &quoted), false);
    let junction = temp.path().join("links").join("j");
    assert!(
        junction
            .symlink_metadata()
            .unwrap()
            .file_type()
            .is_symlink()
            || junction.is_dir()
    );
    assert_eq!(
        std::fs::read_to_string(junction.join("in.txt")).unwrap(),
        "in"
    );
    make(&mut app, "s", (2, "target"), false);
    let link = temp.path().join("s");
    match std::fs::read_link(&link) {
        // Symbolic links need the right or developer mode: its error then, nothing left.
        Ok(to) => assert_eq!(to, Path::new("target")),
        Err(_) => assert!(!link.exists()),
    }
}
