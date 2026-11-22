//! A drive that cannot be read: the question, Retry with the letter typed over it, Cancel.

use super::*;
use crate::app::Overlay;
use crate::app::dialog::DialogOutcome;
use crate::app::fileops::tests::{app_in, submit_dialog};
use crate::app::waiting::finish_waiting;
use crate::platform::volumes::DriveKind;

/// A letter with no drive on this machine.
fn free_letter() -> char {
    ('B'..='Z')
        .find(|letter| platform::volumes::drive_kind(&format!("{letter}:\\")) == DriveKind::NoRoot)
        .unwrap()
}

fn the_form(app: &App) -> &Form {
    match &app.overlay {
        Some(Overlay::Dialog(Dialog::Form { form, .. })) => form,
        _ => panic!("no dialog"),
    }
}

fn texts(form: &Form) -> Vec<String> {
    (form.items.iter())
        .filter_map(|item| match &item.control {
            Control::Text(text) => Some(text.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn a_drive_that_cannot_be_read_asks_and_retry_takes_the_letter_typed() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    let free = free_letter();
    app.go_to_drive(Side::Left, format!("{free}:\\"), true, DriveKind::NoRoot);
    finish_waiting(&mut app);
    let form = the_form(&app);
    assert!(form.warning, "FAR's warning colours");
    assert_eq!(form.title, "Error");
    assert_eq!(texts(form)[..2], [CANNOT_READ.to_string(), ":".to_string()]);
    assert_eq!(form.edit_text(LETTER), free.to_string());
    assert_eq!(
        app.panels[0].path.as_local(),
        Some(temp.path()),
        "the panel stays"
    );
    // Cancel: nothing more.
    submit_dialog(&mut app, DialogOutcome::Cancel);
    assert!(app.overlay.is_none());
    assert_eq!(app.panels[0].path.as_local(), Some(temp.path()));
    // Retry with the temporary folder's drive typed over the letter: the panel goes there.
    app.go_to_drive(Side::Left, format!("{free}:\\"), true, DriveKind::NoRoot);
    finish_waiting(&mut app);
    let own = temp.path().to_string_lossy().chars().next().unwrap();
    let Some(Overlay::Dialog(Dialog::Form { form, .. })) = &mut app.overlay else {
        panic!("no dialog");
    };
    if let Some(Control::Edit(input)) = form.items.get_mut(LETTER).map(|item| &mut item.control) {
        *input = LineInput::with_text(&own.to_ascii_lowercase().to_string());
    }
    let retry = (form.items.iter())
        .position(
            |item| matches!(&item.control, Control::Button { label, .. } if label == "&Retry"),
        )
        .unwrap();
    submit_dialog(&mut app, DialogOutcome::Button(retry));
    assert!(app.overlay.is_none(), "{:?}", app.overlay.is_some());
    let path = app.panels[0].path.as_local().unwrap().to_path_buf();
    assert!(
        path.to_string_lossy()
            .to_ascii_uppercase()
            .starts_with(&format!("{}:", own.to_ascii_uppercase())),
        "{path:?}"
    );
}

#[test]
fn a_root_without_a_letter_is_named_whole() {
    let temp = tempfile::tempdir().unwrap();
    let mut app = app_in(temp.path());
    let error = io::Error::from(io::ErrorKind::NotFound);
    app.ask_drive_retry(Side::Right, r"\\?\Volume{0}\".to_string(), &error);
    let form = the_form(&app);
    assert_eq!(texts(form)[0], format!(r"{CANNOT_READ} \\?\Volume{{0}}\"));
    assert!(!(form.items.iter()).any(|item| matches!(item.control, Control::Edit(_))));
}

#[test]
fn reachable_falls_back_to_the_root_and_gives_its_error() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().to_string_lossy().into_owned();
    let gone = VfsPath::local(temp.path().join("gone"));
    assert_eq!(
        reachable(gone, &root).unwrap(),
        VfsPath::local(temp.path()),
        "FAR's FarChDir(D:\\) after FarChDir(D:)"
    );
    let free = format!("{}:\\", free_letter());
    assert!(reachable(VfsPath::local(&free), &free).is_err());
}
